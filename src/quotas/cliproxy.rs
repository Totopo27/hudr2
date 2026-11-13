//! Adaptador CLIProxyAPI para cuotas y cuentas.
//!
//! Maneja el parseo de credenciales en `resolve_auths_dir()`,
//! la normalización de prefijos (`antigravity-*`, `google-*`, `oauth-*`),
//! y la consulta HTTP a `QUOTA_URL`.

use crate::config::{QUOTA_URL, USER_AGENT};
use crate::models::{AccountQuota, AgentSession, HerdrPane, OrchestratorProfile, QuotaPool};
use crate::paths::{
    cliproxy_config_candidates_for, gentle_ai_profiles_candidates, opencode_config_candidates,
    resolve_auths_dir_for,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Marcador explícito para cuentas descubiertas en disco cuyas cuotas aún no se han consultado.
/// Jamás es producido por [`format_relative_reset`], evitando falsos positivos cuando una ventana
/// real coincide exactamente con `4h 59m` o `4d 23h`.
pub const UNFETCHED_RESET: &str = "...";

/// Segundos de enfriamiento preventivo para cuentas inactivas tras recibir HTTP 429 de Google.
const RATE_LIMIT_COOLDOWN_SECS: i64 = 90;
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

static RATE_LIMIT_UNTIL_EPOCH: AtomicI64 = AtomicI64::new(0);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct CachedQuotaRecord {
    fetched_at: i64,
    auth_mtime: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_body: Option<serde_json::Value>,
}

#[derive(Default)]
struct QuotaCacheState {
    loaded_path: Option<PathBuf>,
    loaded_mtime: Option<SystemTime>,
    entries: HashMap<String, CachedQuotaRecord>,
}

static QUOTA_CACHE_STATE: LazyLock<Mutex<QuotaCacheState>> =
    LazyLock::new(|| Mutex::new(QuotaCacheState::default()));

/// Devuelve la ruta de `quotas_cache.json` hermana al archivo de configuración `hudr2.json`.
pub fn quota_cache_path_for(config_path: &Path) -> PathBuf {
    config_path.with_file_name("quotas_cache.json")
}

/// Limpia el estado de caché en memoria y el freno de rate-limit (útil para aislamiento en tests).
pub fn clear_quota_runtime_cache() {
    RATE_LIMIT_UNTIL_EPOCH.store(0, Ordering::SeqCst);
    if let Ok(mut guard) = QUOTA_CACHE_STATE.lock() {
        *guard = QuotaCacheState::default();
    }
}

/// Indica si el freno preventivo por HTTP 429 está activo para pausar barridos en segundo plano.
pub fn is_rate_limit_cooling_down() -> bool {
    chrono::Utc::now().timestamp() < RATE_LIMIT_UNTIL_EPOCH.load(Ordering::SeqCst)
}

/// Activa el freno preventivo ante HTTP 429 durante `secs` segundos.
pub fn trigger_rate_limit_cooldown(secs: i64) {
    let until = chrono::Utc::now().timestamp() + secs.max(1);
    RATE_LIMIT_UNTIL_EPOCH.store(until, Ordering::SeqCst);
}

fn file_mtime_secs(path_str: &str) -> u64 {
    if path_str.is_empty() {
        return 0;
    }
    let actual_path = if let Some((p, _)) = path_str.split_once('#') {
        p
    } else {
        path_str
    };
    Path::new(actual_path)
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sync_disk_cache_if_needed(state: &mut QuotaCacheState, cache_path: &Path) {
    let disk_mtime = cache_path.metadata().and_then(|m| m.modified()).ok();
    if state.loaded_path.as_deref() == Some(cache_path)
        && state.loaded_mtime == disk_mtime
        && disk_mtime.is_some()
    {
        return;
    }

    if state.loaded_path.as_deref() != Some(cache_path) {
        state.entries.clear();
    }

    if let Ok(bytes) = crate::paths::read_config_bytes(cache_path)
        && let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Some(accs_obj) = val.get("accounts").and_then(|a| a.as_object())
    {
        for (k, v) in accs_obj {
            if let Ok(rec) = serde_json::from_value::<CachedQuotaRecord>(v.clone()) {
                let key = k.to_lowercase();
                let should_update = state
                    .entries
                    .get(&key)
                    .is_none_or(|existing| rec.fetched_at >= existing.fetched_at);
                if should_update {
                    state.entries.insert(key, rec);
                }
            }
        }
    }

    state.loaded_path = Some(cache_path.to_path_buf());
    state.loaded_mtime = disk_mtime;
}

fn persist_disk_cache(state: &mut QuotaCacheState, cache_path: &Path) {
    let payload = serde_json::json!({
        "version": 1,
        "accounts": &state.entries,
    });
    let Ok(body) = serde_json::to_vec(&payload) else {
        return;
    };
    if crate::paths::atomic_write_bytes(cache_path, &body).is_ok() {
        state.loaded_path = Some(cache_path.to_path_buf());
        state.loaded_mtime = cache_path.metadata().and_then(|m| m.modified()).ok();
    }
}

fn error_retry_ttl_secs(status: u16) -> i64 {
    match status {
        401 | 403 => 300,
        _ => 120,
    }
}

fn has_expired_bucket_window(data: &serde_json::Value, fetched_at: i64, now_epoch: i64) -> bool {
    let Some(groups) = data.get("groups").and_then(|g| g.as_array()) else {
        return false;
    };
    for grp in groups {
        let Some(buckets) = grp.get("buckets").and_then(|b| b.as_array()) else {
            continue;
        };
        for b in buckets {
            if let Some(rt) = b.get("resetTime").and_then(|r| r.as_str())
                && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(rt)
            {
                let reset_ts = dt.timestamp();
                if fetched_at < reset_ts && now_epoch >= reset_ts {
                    return true;
                }
                if reset_ts <= fetched_at && now_epoch - fetched_at >= 120 {
                    return true;
                }
            }
        }
    }
    false
}

/// Hidrata cuentas descubiertas desde `quotas_cache.json` (si se provee) y recalcula al instante
/// los contadores relativos (`en Xh Ym`) y `pace` respecto a la hora actual sin llamadas HTTP.
pub fn hydrate_accounts_from_cache(accounts: &mut [AccountQuota], cache_path: Option<&Path>) {
    let Ok(mut guard) = QUOTA_CACHE_STATE.lock() else {
        return;
    };
    if let Some(cp) = cache_path {
        sync_disk_cache_if_needed(&mut guard, cp);
    }

    let mut stale_keys = Vec::new();
    for acc in accounts.iter_mut() {
        if is_api_key(acc) {
            let key = acc.prefix.to_lowercase();
            if guard.entries.remove(&key).is_some()
                && let Some(cp) = cache_path
            {
                persist_disk_cache(&mut guard, cp);
            }
            continue;
        }

        let key = acc.prefix.to_lowercase();
        let Some(rec) = guard.entries.get(&key).cloned() else {
            continue;
        };
        let cur_mtime = file_mtime_secs(&acc.auth_file);
        if cur_mtime > 0 && rec.auth_mtime > 0 && cur_mtime != rec.auth_mtime {
            stale_keys.push(key);
            acc.has_error = false;
            acc.error_msg.clear();
            acc.reset_5h = UNFETCHED_RESET.to_string();
            acc.reset_wk = UNFETCHED_RESET.to_string();
            continue;
        }

        let now_epoch = chrono::Utc::now().timestamp();
        if let Some(ref data) = rec.data {
            if is_codex_account(acc) {
                if !apply_codex_quota_data(acc, data) {
                    mark_quota_unknown(acc, "respuesta sin cuota");
                }
            } else {
                apply_quota_data(acc, data);
            }
        }
        if let Some(status) = rec.error_status {
            let ttl = error_retry_ttl_secs(status);
            if rec.data.is_none()
                || matches!(status, 401 | 403)
                || (now_epoch - rec.fetched_at) < ttl
            {
                if status == 0 {
                    acc.has_error = true;
                    acc.error_msg = "sin red".to_string();
                } else {
                    apply_quota_error(
                        acc,
                        status,
                        rec.error_body.as_ref().unwrap_or(&serde_json::Value::Null),
                    );
                }
            }
        }
    }

    for k in stale_keys {
        guard.entries.remove(&k);
    }
}

/// Registra una respuesta válida de cuotas en la caché (memoria + disco opcional) con marca temporal explícita.
pub fn record_cached_quota_at(
    acc: &mut AccountQuota,
    data: &serde_json::Value,
    fetched_at: i64,
    cache_path: Option<&Path>,
) {
    apply_quota_data(acc, data);
    let rec = CachedQuotaRecord {
        fetched_at,
        auth_mtime: file_mtime_secs(&acc.auth_file),
        data: Some(data.clone()),
        error_status: None,
        error_body: None,
    };
    if let Ok(mut guard) = QUOTA_CACHE_STATE.lock() {
        if let Some(cp) = cache_path {
            sync_disk_cache_if_needed(&mut guard, cp);
        }
        guard.entries.insert(acc.prefix.to_lowercase(), rec);
        if let Some(cp) = cache_path {
            persist_disk_cache(&mut guard, cp);
        }
    }
}

/// Registra un estado de error HTTP o de red (`status == 0`) en la caché con TTL de reintento,
/// preservando los últimos datos conocidos en memoria para no borrar el aviso ante `DataChanged`.
pub fn record_cached_quota_error_at(
    acc: &mut AccountQuota,
    status: u16,
    err_val: &serde_json::Value,
    fetched_at: i64,
    cache_path: Option<&Path>,
) {
    if status == 0 {
        acc.has_error = true;
        acc.error_msg = "sin red".to_string();
    } else {
        apply_quota_error(acc, status, err_val);
    }

    if let Ok(mut guard) = QUOTA_CACHE_STATE.lock() {
        if let Some(cp) = cache_path {
            sync_disk_cache_if_needed(&mut guard, cp);
        }
        let key = acc.prefix.to_lowercase();
        let prev_data = guard.entries.get(&key).and_then(|r| r.data.clone());
        let rec = CachedQuotaRecord {
            fetched_at,
            auth_mtime: file_mtime_secs(&acc.auth_file),
            data: prev_data,
            error_status: Some(status),
            error_body: (!err_val.is_null()).then(|| err_val.clone()),
        };
        guard.entries.insert(key, rec);
        if let Some(cp) = cache_path
            && matches!(status, 401 | 403)
        {
            persist_disk_cache(&mut guard, cp);
        }
    }
}

/// Determina si una cuenta inactiva necesita consulta remota por ser nueva (`unfetched`),
/// haber cambiado su archivo de credenciales, haber cumplido el TTL de error o haber superado `resetTime`.
pub fn needs_refresh(acc: &AccountQuota) -> bool {
    if is_api_key(acc) {
        return false;
    }
    if is_unfetched(acc) {
        return true;
    }
    let Ok(guard) = QUOTA_CACHE_STATE.lock() else {
        return false;
    };
    let Some(rec) = guard.entries.get(&acc.prefix.to_lowercase()) else {
        return false;
    };
    let cur_mtime = file_mtime_secs(&acc.auth_file);
    if cur_mtime > 0 && rec.auth_mtime > 0 && cur_mtime != rec.auth_mtime {
        return true;
    }
    let now_epoch = chrono::Utc::now().timestamp();
    if let Some(status) = rec.error_status {
        return (now_epoch - rec.fetched_at) >= error_retry_ttl_secs(status);
    }
    if is_codex_account(acc) {
        return now_epoch - rec.fetched_at >= 60;
    }
    if let Some(ref data) = rec.data {
        has_expired_bucket_window(data, rec.fetched_at, now_epoch)
    } else {
        false
    }
}

/// Comprueba si una cuenta candidata fue verificada recientemente (`<= max_age_secs`) y sigue vigente.
pub fn is_fresh_within(acc: &AccountQuota, max_age_secs: i64) -> bool {
    if is_api_key(acc) {
        return !acc.has_error;
    }
    if is_unfetched(acc) || acc.has_error || needs_refresh(acc) {
        return false;
    }
    let Ok(guard) = QUOTA_CACHE_STATE.lock() else {
        return false;
    };
    let Some(rec) = guard.entries.get(&acc.prefix.to_lowercase()) else {
        return false;
    };
    rec.error_status.is_none()
        && rec.data.is_some()
        && (chrono::Utc::now().timestamp() - rec.fetched_at) <= max_age_secs
}

/// Indica si una cuenta aún no ha recibido ninguna respuesta (ni datos ni error) del endpoint de cuotas.
pub fn is_unfetched(acc: &AccountQuota) -> bool {
    !is_api_key(acc) && !acc.has_error && (acc.pools.is_empty() || acc.reset_5h == UNFETCHED_RESET)
}

/// Comprueba si una cuenta corresponde a una suscripción o autenticación OAuth de OpenAI Codex
pub fn is_codex_account(acc: &AccountQuota) -> bool {
    acc.prefix.to_lowercase().contains("codex")
        || acc.auth_file.to_lowercase().contains("codex")
        || matches!(
            acc.reset_5h.as_str(),
            "plus" | "pro" | "team" | "free" | "codex"
        )
}

/// Comprueba si una cuenta corresponde a un perfil configurado vía API Key (prepago/saldo por tokens)
/// en contraposición a una credencial OAuth / cuenta de suscripción periódica en auths/.
pub fn is_api_key(acc: &AccountQuota) -> bool {
    acc.auth_file.contains('#')
        || acc.auth_file.ends_with(".yaml")
        || acc.auth_file.ends_with(".yml")
        || acc.reset_5h == "api-key"
}

fn read_valid_credential_json(path: &Path) -> Option<(String, serde_json::Value)> {
    let file_name = path.file_name().and_then(|f| f.to_str())?;
    if !file_name.ends_with(".json") || file_name.starts_with('.') {
        return None;
    }
    let content = crate::paths::read_config_string(path).ok()?;
    let val = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    let has_creds = val.get("access_token").is_some()
        || val.get("email").is_some()
        || val.get("token").is_some()
        || val.get("type").is_some();
    has_creds.then(|| (file_name.to_string(), val))
}

/// Parsea cuentas y prefijos configurados bajo `api-keys:` en un archivo YAML de CLIProxy o CPAMC
pub fn parse_api_key_accounts_from_yaml(content: &str) -> Vec<(String, String)> {
    let mut accounts = Vec::new();
    let mut in_api_keys = false;
    let mut api_keys_indent = 0;
    let mut current_provider = "Claude".to_string();
    let mut provider_indent: Option<usize> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        if trimmed == "api-keys:" || trimmed.starts_with("api-keys:") {
            in_api_keys = true;
            api_keys_indent = indent;
            provider_indent = None;
            continue;
        }

        if in_api_keys {
            if indent <= api_keys_indent && !trimmed.starts_with('-') {
                in_api_keys = false;
                provider_indent = None;
                continue;
            }

            if trimmed.ends_with(':') && !trimmed.starts_with('-') {
                let is_provider_header = match provider_indent {
                    None => indent > api_keys_indent,
                    Some(pi) => indent <= pi && indent > api_keys_indent,
                };

                if is_provider_header {
                    let provider_raw = trimmed.trim_end_matches(':').trim();
                    let lower = provider_raw.to_ascii_lowercase();
                    if !matches!(
                        lower.as_str(),
                        "keys" | "models" | "model" | "params" | "options" | "headers" | "settings"
                    ) {
                        provider_indent = Some(indent);
                        current_provider = match lower.as_str() {
                            "claude" | "anthropic" => "Claude".to_string(),
                            "gemini" | "google" => "Gemini".to_string(),
                            "interactions" | "interactions-api" => "Interactions".to_string(),
                            "openai" | "gpt" | "codex" => "GPT".to_string(),
                            "deepseek" => "DeepSeek".to_string(),
                            "qwen" => "Qwen".to_string(),
                            "kimi" | "moonshot" => "Kimi".to_string(),
                            "xai" | "grok" => "xAI".to_string(),
                            "muse" | "meta" => "Muse".to_string(),
                            "vertex" => "Vertex".to_string(),
                            "openai-compatibility" | "openai-compatible" => "OpenAI".to_string(),
                            "fennoai" => "FennoAI".to_string(),
                            "qiniu" => "Qiniu".to_string(),
                            "alcodemirror" => "Alcodemirror".to_string(),
                            other => {
                                let mut c = other.chars();
                                match c.next() {
                                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                                    None => "API".to_string(),
                                }
                            }
                        };
                        continue;
                    }
                }
            }

            if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim().trim_start_matches('-').trim();
                if key.eq_ignore_ascii_case("prefix") {
                    let clean_pfx = v.trim().trim_matches(['"', '\'', ' ']);
                    if !clean_pfx.is_empty() && !accounts.iter().any(|(p, _)| p == clean_pfx) {
                        accounts.push((clean_pfx.to_string(), current_provider.clone()));
                    }
                }
            }
        }
    }
    accounts
}

/// Extrae prefijos de cuenta y modelo configurados en `opencode.json` (`providers.<proxy>.models` y `agents`)
pub fn parse_api_key_accounts_from_opencode_json(content: &str) -> Vec<(String, String)> {
    let mut accounts = Vec::new();
    let Ok(val) = serde_json::from_str::<serde_json::Value>(content) else {
        return accounts;
    };

    // 1. providers.<proxy>.models
    for section in ["providers", "provider"] {
        if let Some(prov_map) = val.get(section).and_then(|p| p.as_object()) {
            for prov_val in prov_map.values() {
                if let Some(models_map) = prov_val.get("models").and_then(|m| m.as_object()) {
                    for (model_key, model_entry) in models_map {
                        if let Some((pfx, model_part)) = model_key.split_once('/') {
                            if pfx.is_empty()
                                || matches!(
                                    pfx.to_ascii_lowercase().as_str(),
                                    "cliproxy" | "opencode" | "cpamc" | "relay"
                                )
                            {
                                continue;
                            }
                            let name_str = model_entry
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("");
                            let family = resolve_model_family(model_part)
                                .or_else(|| resolve_model_family(name_str))
                                .unwrap_or("Claude");
                            if !accounts.iter().any(|(p, _)| p.eq_ignore_ascii_case(pfx)) {
                                accounts.push((pfx.to_string(), family.to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    accounts
}

/// Extrae prefijos de cuenta y modelo configurados en `profiles.json` de Gentle AI / Pi
pub fn parse_api_key_accounts_from_profiles_json(content: &str) -> Vec<(String, String)> {
    let mut accounts = Vec::new();
    let Ok(val) = serde_json::from_str::<serde_json::Value>(content) else {
        return accounts;
    };
    if let Some(profiles_map) = val.get("profiles").and_then(|p| p.as_object()) {
        for profile in profiles_map.values() {
            if let Some(model_str) = profile
                .get("orchestrator")
                .and_then(|o| o.get("model"))
                .and_then(|m| m.as_str())
            {
                let clean = crate::opencode::normalize_model_str(model_str);
                if let Some((pfx, model_part)) = clean.split_once('/')
                    && !pfx.is_empty()
                    && !matches!(
                        pfx.to_ascii_lowercase().as_str(),
                        "cliproxy" | "opencode" | "cpamc" | "relay"
                    )
                {
                    let family = resolve_model_family(model_part).unwrap_or("Claude");
                    if !accounts.iter().any(|(p, _)| p.eq_ignore_ascii_case(pfx)) {
                        accounts.push((pfx.to_string(), family.to_string()));
                    }
                }
            }
        }
    }
    accounts
}

/// Comprueba si el directorio de autenticación existe y contiene credenciales JSON válidas
/// o si existen archivos de configuración con perfiles API Key activos.
pub fn is_available() -> bool {
    is_available_for(".")
}

/// Comprueba si el directorio de autenticación para `target_dir` contiene credenciales JSON válidas
/// o si existen archivos de configuración con perfiles API Key activos.
pub fn is_available_for(target_dir: &str) -> bool {
    let auths_dir = resolve_auths_dir_for(target_dir);
    if auths_dir.exists()
        && let Ok(entries) = fs::read_dir(&auths_dir)
        && entries
            .flatten()
            .any(|entry| read_valid_credential_json(&entry.path()).is_some())
    {
        return true;
    }

    // Comprobar candidatos config.yaml con api-keys
    for cfg in cliproxy_config_candidates_for(target_dir) {
        if let Ok(c) = crate::paths::read_config_string(&cfg)
            && !parse_api_key_accounts_from_yaml(&c).is_empty()
        {
            return true;
        }
    }

    // Comprobar modelos en opencode.json si no hay entorno de auths explícito
    let has_explicit_auths =
        std::env::var("CLIPROXY_AUTHS_DIR").is_ok() || std::env::var("CPAMC_AUTHS_DIR").is_ok();
    if !has_explicit_auths || std::env::var("OPENCODE_CONFIG_PATH").is_ok() {
        for cfg in opencode_config_candidates(target_dir) {
            if let Ok(c) = crate::paths::read_config_string(&cfg)
                && !parse_api_key_accounts_from_opencode_json(&c).is_empty()
            {
                return true;
            }
        }
    }

    false
}

/// Obtiene el balance inicial o presupuesto definido por el usuario para una cuenta API Key.
/// Retorna `None` si el usuario aún no ha configurado ningún saldo para esta cuenta.
pub fn get_user_configured_budget(prefix: &str) -> Option<f64> {
    let cfg_path = crate::paths::resolve_hudr2_config_path();
    if cfg_path.exists()
        && let Ok(content) = crate::paths::read_config_string(&cfg_path)
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&content)
    {
        if let Some(custom) = v.get("account_budgets").and_then(|ab| ab.get(prefix)) {
            if let Some(b) = custom.as_f64() {
                return Some(b);
            }
            if let Some(b) = custom.get("budget").and_then(|b| b.as_f64()) {
                return Some(b);
            }
        }
        if let Some(global) = v.get("api_key_budget").and_then(|b| b.as_f64()) {
            return Some(global);
        }
    }
    None
}

/// Devuelve la URL oficial de facturación y consulta de saldo según el proveedor o prefijo.
pub fn provider_billing_url(provider_or_prefix: &str) -> &'static str {
    let lower = provider_or_prefix.to_ascii_lowercase();
    if lower.contains("claude") || lower.contains("anthropic") {
        "https://platform.claude.com/spending"
    } else if lower.contains("deepseek") {
        "https://platform.deepseek.com/top_up"
    } else if lower.contains("gemini") || lower.contains("google") || lower.contains("aistudio") {
        "https://aistudio.google.com/"
    } else if lower.contains("opencode") || lower.contains("zen") {
        "https://opencode.ai/zen"
    } else if lower.contains("kimi") || lower.contains("moonshot") {
        "https://platform.moonshot.cn/console/pay"
    } else if lower.contains("codex") {
        "https://chatgpt.com/#settings/Subscription"
    } else if lower.contains("openai") || lower.contains("gpt") {
        "https://platform.openai.com/settings/organization/billing/overview"
    } else if lower.contains("openrouter") {
        "https://openrouter.ai/credits"
    } else if lower.contains("groq") {
        "https://console.groq.com/settings/billing"
    } else if lower.contains("together") {
        "https://api.together.ai/settings/billing"
    } else if lower.contains("mistral") {
        "https://console.mistral.ai/billing/"
    } else if lower.contains("perplexity") || lower.contains("pplx") {
        "https://www.perplexity.ai/settings/api"
    } else if lower.contains("cerebras") {
        "https://cloud.cerebras.ai/"
    } else if lower.contains("fireworks") {
        "https://fireworks.ai/account/billing"
    } else if lower.contains("xai") || lower.contains("grok") {
        "https://console.x.ai/"
    } else if lower.contains("cohere") {
        "https://dashboard.cohere.com/billing"
    } else if lower.contains("muse") || lower.contains("meta") {
        "https://developers.facebook.com/"
    } else if lower.contains("vertex") {
        "https://console.cloud.google.com/billing"
    } else if lower.contains("fennoai") || lower.contains("fenno") {
        "https://fenno.ai/"
    } else if lower.contains("qiniu") {
        "https://portal.qiniu.com/"
    } else if lower.contains("alcodemirror") {
        "https://alcodemirror.com/"
    } else {
        "https://platform.claude.com/spending"
    }
}

/// Devuelve la etiqueta legible de la consola o portal oficial de facturación según el proveedor.
pub fn provider_billing_label(provider_or_prefix: &str) -> &'static str {
    let lower = provider_or_prefix.to_ascii_lowercase();
    if lower.contains("claude") || lower.contains("anthropic") {
        "Anthropic Console"
    } else if lower.contains("deepseek") {
        "DeepSeek Platform"
    } else if lower.contains("gemini") || lower.contains("google") || lower.contains("aistudio") {
        "Google AI Studio"
    } else if lower.contains("opencode") || lower.contains("zen") {
        "OpenCode Zen"
    } else if lower.contains("kimi") || lower.contains("moonshot") {
        "Moonshot Platform"
    } else if lower.contains("codex") {
        "ChatGPT Subscription"
    } else if lower.contains("openai") || lower.contains("gpt") {
        "OpenAI Platform"
    } else if lower.contains("openrouter") {
        "OpenRouter Credits"
    } else if lower.contains("groq") {
        "Groq Console"
    } else if lower.contains("together") {
        "Together AI"
    } else if lower.contains("mistral") {
        "Mistral Console"
    } else if lower.contains("perplexity") || lower.contains("pplx") {
        "Perplexity API"
    } else if lower.contains("cerebras") {
        "Cerebras Cloud"
    } else if lower.contains("fireworks") {
        "Fireworks AI"
    } else if lower.contains("xai") || lower.contains("grok") {
        "xAI Console"
    } else if lower.contains("cohere") {
        "Cohere Dashboard"
    } else if lower.contains("muse") || lower.contains("meta") {
        "Meta Developers"
    } else if lower.contains("vertex") {
        "Google Cloud Billing"
    } else if lower.contains("fennoai") || lower.contains("fenno") {
        "FennoAI Console"
    } else if lower.contains("qiniu") {
        "Qiniu Cloud"
    } else if lower.contains("alcodemirror") {
        "Alcodemirror Console"
    } else {
        "Billing Console"
    }
}

fn api_account_matches(prefix: &str, local_prefix: Option<&str>, model: &str) -> bool {
    if local_prefix.is_some_and(|p| p.eq_ignore_ascii_case(prefix)) {
        return true;
    }

    let prefix = prefix.to_ascii_lowercase();
    let model = model.to_ascii_lowercase();
    model.starts_with(&format!("{prefix}/"))
        || model.contains(&format!("/{prefix}/"))
        || model.contains(&format!("{prefix}:"))
}

fn pane_uses_api_account(pane: &HerdrPane, prefix: &str) -> bool {
    let pane_prefix = pane
        .tokens
        .get("local_prefix")
        .or_else(|| pane.tokens.get("account"))
        .or_else(|| pane.tokens.get("prefix"))
        .or_else(|| pane.tokens.get("profile"))
        .map(String::as_str);
    let pane_model = pane.tokens.get("model").map(String::as_str).unwrap_or("");
    api_account_matches(prefix, pane_prefix, pane_model)
}

/// Determina si una cuenta API Key está siendo utilizada ahora por la sesión
/// principal o por un subagente observable en la telemetría de ejecución.
pub fn is_api_account_active(
    acc: &AccountQuota,
    session: &AgentSession,
    _orchestrators: &[OrchestratorProfile],
    panes: &[HerdrPane],
) -> bool {
    if session.agent_status == "working"
        && api_account_matches(
            &acc.prefix,
            session.local_prefix.as_deref(),
            session.effective_model(),
        )
    {
        return true;
    }

    if session.subagent_invocations.iter().any(|invocation| {
        invocation.status == "working"
            && api_account_matches(
                &acc.prefix,
                invocation.local_prefix.as_deref(),
                &invocation.model,
            )
    }) {
        return true;
    }

    panes.iter().any(|pane| {
        (pane.agent_status.as_deref() == Some("working")
            || pane.tokens.get("agent_status").map(String::as_str) == Some("working"))
            && pane_uses_api_account(pane, &acc.prefix)
    })
}

/// Devuelve únicamente costo y turnos que pueden atribuirse a la cuenta dada.
/// Si un subagente no publica costo propio, no se reutiliza el costo global del
/// orquestador: es preferible omitir una proyección antes que inventarla.
pub fn api_account_attributed_usage(
    acc: &AccountQuota,
    session: &AgentSession,
    panes: &[HerdrPane],
) -> (f64, usize) {
    if api_account_matches(
        &acc.prefix,
        session.local_prefix.as_deref(),
        session.effective_model(),
    ) {
        return (session.cost, session.turn_count);
    }

    let (child_cost, child_tasks) = session
        .subagent_invocations
        .iter()
        .filter(|invocation| {
            api_account_matches(
                &acc.prefix,
                invocation.local_prefix.as_deref(),
                &invocation.model,
            )
        })
        .fold((0.0, 0usize), |(cost, tasks), invocation| {
            (
                cost + invocation.cost,
                tasks + usize::from(invocation.cost > 0.0),
            )
        });
    if child_cost > 0.0 || child_tasks > 0 {
        return (child_cost, child_tasks);
    }

    panes
        .iter()
        .filter(|pane| pane_uses_api_account(pane, &acc.prefix))
        .fold((0.0, 0usize), |(cost, turns), pane| {
            let pane_cost = pane
                .tokens
                .get("cost")
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0);
            let pane_turns = pane
                .tokens
                .get("turn_count")
                .or_else(|| pane.tokens.get("requests"))
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            (cost + pane_cost, turns + pane_turns)
        })
}

/// Calcula el saldo remanente y porcentaje disponible para una cuenta API Key.
/// Si el usuario configuró el saldo con línea base, solo descuenta el costo incurrido
/// desde ese momento en la sesión, protegiendo el saldo de consumos históricos previos.
/// Retorna: `(budget, rem_balance, rem_pct, cost_since_set)`
pub fn calculate_api_key_balance(
    prefix: &str,
    session_cost: f64,
    session_id: Option<&str>,
) -> (Option<f64>, Option<f64>, u8, f64) {
    let cfg_path = crate::paths::resolve_hudr2_config_path();
    if cfg_path.exists()
        && let Ok(content) = crate::paths::read_config_string(&cfg_path)
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&content)
    {
        if let Some(custom) = v.get("account_budgets").and_then(|ab| ab.get(prefix)) {
            let (budget, baseline, saved_sid) = if let Some(b) = custom.as_f64() {
                (b, 0.0, None)
            } else if let Some(b) = custom.get("budget").and_then(|v| v.as_f64()) {
                let base = custom
                    .get("baseline_cost")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let sid = custom.get("session_id").and_then(|v| v.as_str());
                (b, base, sid)
            } else {
                return (None, None, 100, 0.0);
            };

            let same_session = match (saved_sid, session_id) {
                (Some(s1), Some(s2)) => s1 == s2,
                _ => true,
            };

            let cost_since_set = if same_session {
                (session_cost - baseline).max(0.0)
            } else {
                session_cost.max(0.0)
            };

            let rem_balance = (budget - cost_since_set).max(0.0);
            let rem_pct = if budget > 0.0 {
                ((rem_balance / budget) * 100.0).clamp(0.0, 100.0) as u8
            } else {
                0
            };
            return (Some(budget), Some(rem_balance), rem_pct, cost_since_set);
        }

        if let Some(global) = v.get("api_key_budget").and_then(|b| b.as_f64()) {
            let rem_balance = (global - session_cost).max(0.0);
            let rem_pct = if global > 0.0 {
                ((rem_balance / global) * 100.0).clamp(0.0, 100.0) as u8
            } else {
                0
            };
            return (Some(global), Some(rem_balance), rem_pct, session_cost);
        }
    }

    (None, None, 100, 0.0)
}

fn register_or_update_api_key_account(
    target_list: &mut Vec<AccountQuota>,
    prefix: &str,
    provider: &str,
    auth_file_str: &str,
) {
    let user_budget = get_user_configured_budget(prefix);
    let saldo_pace = match user_budget {
        Some(b) => format!("${:.2}", b),
        None => "[definir saldo]".to_string(),
    };
    let pools = vec![
        QuotaPool {
            label: format!("{provider} API"),
            percent: 100,
            reset: "api-key".to_string(),
            pace: "$0.000".to_string(),
        },
        QuotaPool {
            label: format!("{provider} Saldo"),
            percent: 100,
            reset: "api-key".to_string(),
            pace: saldo_pace,
        },
    ];

    if let Some(existing) = target_list
        .iter_mut()
        .find(|a| a.auth_file == auth_file_str || a.prefix.eq_ignore_ascii_case(prefix))
    {
        existing.prefix = prefix.to_string();
        existing.auth_file = auth_file_str.to_string();
        if existing.pools.is_empty() {
            existing.pools = pools;
        }
    } else {
        target_list.push(AccountQuota {
            prefix: prefix.to_string(),
            auth_file: auth_file_str.to_string(),
            rem_5h: 100,
            reset_5h: "api-key".to_string(),
            rem_wk: 100,
            reset_wk: "api-key".to_string(),
            is_critical: false,
            is_warning: false,
            has_error: false,
            error_msg: String::new(),
            pools,
        });
    }
}

/// Descubre y registra cuentas desde los archivos de credenciales de CLIProxy.
pub fn discover_accounts(target_list: &mut Vec<AccountQuota>) {
    discover_accounts_for(".", target_list);
}

/// Descubre y registra cuentas considerando el directorio de proyecto activo (`target_dir`).
pub fn discover_accounts_for(target_dir: &str, target_list: &mut Vec<AccountQuota>) {
    let mut seen_files = std::collections::HashSet::new();
    let mut seen_prefixes = std::collections::HashSet::new();

    // 1. Descubrimiento de cuentas OAuth (archivos JSON en auths_dir)
    let auths_dir = resolve_auths_dir_for(target_dir);
    if auths_dir.exists()
        && let Ok(entries) = fs::read_dir(&auths_dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some((file_name, val)) = read_valid_credential_json(&path) {
                let email = val
                    .get("email")
                    .and_then(|e| e.as_str())
                    .unwrap_or("")
                    .to_string();
                let raw_stem = file_name.trim_end_matches(".json");
                let clean_stem = raw_stem
                    .strip_prefix("antigravity-")
                    .or_else(|| raw_stem.strip_prefix("google-"))
                    .or_else(|| raw_stem.strip_prefix("oauth-"))
                    .unwrap_or(raw_stem);

                let cred_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let is_antigravity = cred_type.eq_ignore_ascii_case("antigravity")
                    || raw_stem.to_ascii_lowercase().contains("antigravity")
                    || raw_stem.to_ascii_lowercase().contains("google")
                    || clean_stem.to_ascii_lowercase().starts_with("agy");

                let prefix = val
                    .get("prefix")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        if !is_antigravity {
                            let base_cand = if !cred_type.is_empty() {
                                cred_type.to_lowercase()
                            } else {
                                clean_stem.to_lowercase()
                            };
                            let mut cand = base_cand.clone();
                            let mut idx = 2;
                            while seen_prefixes.contains(&cand.to_lowercase()) {
                                cand = format!("{base_cand}{idx}");
                                idx += 1;
                            }
                            cand
                        } else if !clean_stem.is_empty() {
                            clean_stem.to_string()
                        } else if !email.is_empty() {
                            email.split('@').next().unwrap_or("acc").to_string()
                        } else {
                            "acc".to_string()
                        }
                    });

                let path_str = path.to_string_lossy().to_string();
                seen_files.insert(path_str.clone());
                seen_prefixes.insert(prefix.to_lowercase());

                if !is_antigravity {
                    // Cuentas OAuth de suscripción o plataforma (Codex, Claude OAuth, Kimi, Muse, xAI, Devin, Vertex AI)
                    let provider_name = match cred_type.to_ascii_lowercase().as_str() {
                        "codex" => "Codex".to_string(),
                        "claude" => "Claude".to_string(),
                        "kimi" => "Kimi".to_string(),
                        "xai" => "xAI".to_string(),
                        "muse" => "Muse".to_string(),
                        "devin" => "Devin".to_string(),
                        "vertex" => "Vertex AI".to_string(),
                        other if !other.is_empty() => {
                            let mut c = other.chars();
                            match c.next() {
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                                None => "OAuth".to_string(),
                            }
                        }
                        _ => {
                            if raw_stem.to_ascii_lowercase().contains("codex") {
                                "Codex".to_string()
                            } else if raw_stem.to_ascii_lowercase().contains("claude") {
                                "Claude".to_string()
                            } else if raw_stem.to_ascii_lowercase().contains("kimi") {
                                "Kimi".to_string()
                            } else {
                                "OAuth".to_string()
                            }
                        }
                    };

                    let plan = val.get("plan_type").and_then(|p| p.as_str()).unwrap_or(
                        if provider_name == "Codex" {
                            "plus"
                        } else {
                            "sub"
                        },
                    );

                    let badge = if plan.is_empty() {
                        provider_name.to_lowercase()
                    } else {
                        plan.to_lowercase()
                    };

                    let plan_display = match badge.as_str() {
                        "plus" => "Plus",
                        "pro" => "Pro",
                        "team" => "Team",
                        "free" => "Free",
                        "sub" => "Sub",
                        _ => plan,
                    };

                    let pools = vec![QuotaPool {
                        label: format!("{provider_name} {plan_display}"),
                        percent: 0,
                        reset: UNFETCHED_RESET.to_string(),
                        pace: "sin consultar".to_string(),
                    }];

                    let is_disabled = val
                        .get("disabled")
                        .and_then(|d| d.as_bool())
                        .unwrap_or(false);
                    let is_expired = val
                        .get("expired")
                        .and_then(|e| e.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.timestamp() < chrono::Utc::now().timestamp())
                        .unwrap_or(false);

                    let (has_error, error_msg) = if is_disabled {
                        (true, "deshabilitado".to_string())
                    } else if is_expired {
                        (true, "expirado".to_string())
                    } else {
                        (false, String::new())
                    };

                    if let Some(existing) = target_list
                        .iter_mut()
                        .find(|a| a.auth_file == path_str || a.prefix.eq_ignore_ascii_case(&prefix))
                    {
                        existing.prefix = prefix;
                        existing.auth_file = path_str;
                        existing.rem_5h = 0;
                        existing.rem_wk = 0;
                        existing.reset_5h = UNFETCHED_RESET.to_string();
                        existing.reset_wk = UNFETCHED_RESET.to_string();
                        existing.has_error = has_error;
                        existing.error_msg = error_msg;
                        if existing.pools.is_empty()
                            || existing.pools[0].label.starts_with("Gemini")
                        {
                            existing.pools = pools;
                        }
                    } else {
                        target_list.push(AccountQuota {
                            prefix,
                            auth_file: path_str,
                            rem_5h: 0,
                            reset_5h: UNFETCHED_RESET.to_string(),
                            rem_wk: 0,
                            reset_wk: UNFETCHED_RESET.to_string(),
                            is_critical: false,
                            is_warning: false,
                            has_error,
                            error_msg,
                            pools,
                        });
                    }
                    continue;
                }

                if let Some(existing) = target_list
                    .iter_mut()
                    .find(|a| a.auth_file == path_str || a.prefix.eq_ignore_ascii_case(&prefix))
                {
                    existing.prefix = prefix;
                    existing.auth_file = path_str;
                } else {
                    target_list.push(AccountQuota {
                        prefix,
                        auth_file: path_str,
                        rem_5h: 100,
                        reset_5h: UNFETCHED_RESET.to_string(),
                        rem_wk: 100,
                        reset_wk: UNFETCHED_RESET.to_string(),
                        is_critical: false,
                        is_warning: false,
                        has_error: false,
                        error_msg: String::new(),
                        pools: vec![
                            QuotaPool {
                                label: "Gemini Wk".to_string(),
                                percent: 100,
                                reset: UNFETCHED_RESET.to_string(),
                                pace: "~0%".to_string(),
                            },
                            QuotaPool {
                                label: "Gemini 5h".to_string(),
                                percent: 100,
                                reset: UNFETCHED_RESET.to_string(),
                                pace: "~0%".to_string(),
                            },
                            QuotaPool {
                                label: "Claude Wk".to_string(),
                                percent: 100,
                                reset: UNFETCHED_RESET.to_string(),
                                pace: "~0%".to_string(),
                            },
                            QuotaPool {
                                label: "Claude 5h".to_string(),
                                percent: 100,
                                reset: UNFETCHED_RESET.to_string(),
                                pace: "~0%".to_string(),
                            },
                        ],
                    });
                }
            }
        }
    }

    // 2. Descubrimiento de cuentas / perfiles por API Key (config.yaml de CLIProxy/CPAMC)
    for cfg_path in cliproxy_config_candidates_for(target_dir) {
        if let Ok(content) = crate::paths::read_config_string(&cfg_path) {
            let api_accounts = parse_api_key_accounts_from_yaml(&content);
            for (prefix, provider) in api_accounts {
                let p_low = prefix.to_lowercase();
                let auth_file_str = format!("{}#{}", cfg_path.display(), prefix);
                seen_files.insert(auth_file_str.clone());

                if seen_prefixes.contains(&p_low) {
                    continue;
                }
                seen_prefixes.insert(p_low);

                register_or_update_api_key_account(target_list, &prefix, &provider, &auth_file_str);
            }
        }
    }

    // 3. Descubrimiento de perfiles de orquestador / modelos en opencode.json
    let has_explicit_auths =
        std::env::var("CLIPROXY_AUTHS_DIR").is_ok() || std::env::var("CPAMC_AUTHS_DIR").is_ok();
    if !has_explicit_auths || std::env::var("OPENCODE_CONFIG_PATH").is_ok() {
        for cfg_path in opencode_config_candidates(target_dir) {
            if let Ok(content) = crate::paths::read_config_string(&cfg_path) {
                let api_accounts = parse_api_key_accounts_from_opencode_json(&content);
                for (prefix, provider) in api_accounts {
                    let p_low = prefix.to_lowercase();
                    let auth_file_str = format!("{}#{}", cfg_path.display(), prefix);
                    seen_files.insert(auth_file_str.clone());

                    if seen_prefixes.contains(&p_low) {
                        continue;
                    }
                    seen_prefixes.insert(p_low);

                    register_or_update_api_key_account(
                        target_list,
                        &prefix,
                        &provider,
                        &auth_file_str,
                    );
                }
            }
        }
    }

    // 4. Descubrimiento de perfiles de orquestador en profiles.json (Gentle AI / Pi)
    if !has_explicit_auths {
        for cfg_path in gentle_ai_profiles_candidates(target_dir) {
            if let Ok(content) = crate::paths::read_config_string(&cfg_path) {
                let api_accounts = parse_api_key_accounts_from_profiles_json(&content);
                for (prefix, provider) in api_accounts {
                    let p_low = prefix.to_lowercase();
                    let auth_file_str = format!("{}#{}", cfg_path.display(), prefix);
                    seen_files.insert(auth_file_str.clone());

                    if seen_prefixes.contains(&p_low) {
                        continue;
                    }
                    seen_prefixes.insert(p_low);

                    register_or_update_api_key_account(
                        target_list,
                        &prefix,
                        &provider,
                        &auth_file_str,
                    );
                }
            }
        }
    }

    // Purgar cuentas huérfanas cuyos archivos JSON o configuraciones fueron eliminados
    target_list.retain(|a| seen_files.contains(&a.auth_file));
    target_list.sort_by(|a, b| a.prefix.cmp(&b.prefix));
}

/// Prefijos de cuenta conocidos en minúsculas, ordenados de mayor a menor longitud
/// (para que `agy3_1` gane sobre `agy3`).
/// Si `accounts_list` está vacío y `target_dir` es provisto, descubre cuentas del disco.
pub fn sorted_known_prefixes(
    accounts_list: &[AccountQuota],
    target_dir: Option<&str>,
) -> Vec<String> {
    let mut known: Vec<String> = accounts_list
        .iter()
        .map(|a| a.prefix.to_lowercase())
        .collect();
    if known.is_empty()
        && let Some(dir) = target_dir
    {
        let mut discovered = Vec::new();
        discover_accounts_for(dir, &mut discovered);
        known = discovered
            .into_iter()
            .map(|a| a.prefix.to_lowercase())
            .collect();
    }
    known.sort_by_key(|a| std::cmp::Reverse(a.len()));
    known
}

/// Consulta el endpoint de cuotas y actualiza la caché compartida (`quotas_cache.json`) + freno 429.
pub async fn fetch_quota_for_account_with_cache(
    acc: &mut AccountQuota,
    cache_path: Option<&Path>,
) -> u16 {
    // 1. Cuentas de tipo API Key: no consultan endpoint remoto
    if is_api_key(acc) {
        acc.has_error = false;
        acc.error_msg.clear();
        acc.rem_5h = 100;
        acc.rem_wk = 100;
        if acc.reset_5h.is_empty() || acc.reset_5h == UNFETCHED_RESET {
            acc.reset_5h = "api-key".to_string();
            acc.reset_wk = "api-key".to_string();
        }
        return 200;
    }

    // 2. Suscripciones Codex: consultar las ventanas reales de uso de OpenAI.
    if is_codex_account(acc) {
        let raw = match crate::paths::read_config_string(Path::new(&acc.auth_file)) {
            Ok(raw) => raw,
            Err(_) => {
                mark_quota_unknown(acc, "credencial no disponible");
                return 0;
            }
        };
        let credential: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(_) => {
                mark_quota_unknown(acc, "credencial inválida");
                return 0;
            }
        };
        if credential.get("disabled").and_then(|v| v.as_bool()) == Some(true) {
            mark_quota_unknown(acc, "deshabilitado");
            return 200;
        }
        if let Some(exp_str) = credential.get("expired").and_then(|v| v.as_str())
            && let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(exp_str)
            && expiry.timestamp() < chrono::Utc::now().timestamp()
        {
            mark_quota_unknown(acc, "expirado");
            return 200;
        }

        let token = credential
            .get("access_token")
            .or_else(|| credential.pointer("/tokens/access_token"))
            .and_then(|v| v.as_str());
        let Some(token) = token else {
            mark_quota_unknown(acc, "token no disponible");
            return 0;
        };
        let account_id = credential
            .get("account_id")
            .or_else(|| credential.pointer("/tokens/account_id"))
            .and_then(|v| v.as_str());

        let mut request = crate::config::HTTP_CLIENT
            .get(CODEX_USAGE_URL)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/json")
            .header("OpenAI-Beta", "codex-1")
            .header("originator", "codex_cli_rs")
            .header("User-Agent", USER_AGENT);
        if let Some(account_id) = account_id {
            request = request.header("ChatGPT-Account-ID", account_id);
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                if response.status().is_success() {
                    match response.json::<serde_json::Value>().await {
                        Ok(data) if apply_codex_quota_data(acc, &data) => {
                            record_cached_codex_quota_at(
                                acc,
                                &data,
                                chrono::Utc::now().timestamp(),
                                cache_path,
                            );
                        }
                        _ => mark_quota_unknown(acc, "respuesta sin cuota"),
                    }
                } else {
                    let message = match status {
                        401 => "expirado",
                        403 => "acceso denegado",
                        429 => "rate limited",
                        _ => "cuota no disponible",
                    };
                    mark_quota_unknown(acc, message);
                    record_cached_quota_error_at(
                        acc,
                        status,
                        &serde_json::Value::Null,
                        chrono::Utc::now().timestamp(),
                        cache_path,
                    );
                }
                return status;
            }
            Err(_) => {
                mark_quota_unknown(acc, "sin red");
                record_cached_quota_error_at(
                    acc,
                    0,
                    &serde_json::Value::Null,
                    chrono::Utc::now().timestamp(),
                    cache_path,
                );
                return 0;
            }
        }
    }

    // 3. Cuentas OAuth de plataforma / suscripción (Claude OAuth, Kimi, Muse, xAI, etc.):
    // Tienen pools asignados y reset propio (no UNFETCHED_RESET), validan vigencia en disco
    // sin enviar tokens a QUOTA_URL de Google
    if !is_antigravity_account(acc) && (!acc.pools.is_empty() && acc.reset_5h != UNFETCHED_RESET) {
        if Path::new(&acc.auth_file).exists()
            && !acc.auth_file.contains('#')
            && let Ok(content) = crate::paths::read_config_string(&acc.auth_file)
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(&content)
        {
            if v.get("disabled").and_then(|d| d.as_bool()) == Some(true) {
                acc.has_error = true;
                acc.error_msg = "deshabilitado".to_string();
                return 200;
            }
            if let Some(exp_str) = v.get("expired").and_then(|e| e.as_str())
                && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(exp_str)
                && dt.timestamp() < chrono::Utc::now().timestamp()
            {
                acc.has_error = true;
                acc.error_msg = "expirado".to_string();
                return 200;
            }
        }
        acc.has_error = false;
        acc.error_msg.clear();
        acc.rem_5h = 100;
        acc.rem_wk = 100;
        return 200;
    }

    // 4. Cuentas Google Antigravity (y mocks de prueba) que consultan QUOTA_URL de Google
    if !Path::new(&acc.auth_file).exists() {
        return 0;
    }

    let raw = match crate::paths::read_config_string(&acc.auth_file) {
        Ok(r) => r,
        Err(_) => return 0,
    };

    let val: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    let token = match val.get("access_token").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return 0,
    };

    let project_id = val.get("project_id").and_then(|p| p.as_str()).unwrap_or("");

    let client = &*crate::config::HTTP_CLIENT;

    let body = serde_json::json!({ "project": project_id });
    if let Ok(res) = client
        .post(QUOTA_URL)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", USER_AGENT)
        .json(&body)
        .send()
        .await
    {
        let status = res.status().as_u16();
        if res.status().is_success() {
            if let Ok(data) = res.json::<serde_json::Value>().await {
                let now = chrono::Utc::now().timestamp();
                record_cached_quota_at(acc, &data, now, cache_path);
            }
            return status;
        }

        let err_val = res
            .json::<serde_json::Value>()
            .await
            .unwrap_or(serde_json::Value::Null);
        let now = chrono::Utc::now().timestamp();
        record_cached_quota_error_at(acc, status, &err_val, now, cache_path);

        if status == 429 || acc.error_msg == "rate limited" {
            trigger_rate_limit_cooldown(RATE_LIMIT_COOLDOWN_SECS);
        }
        return status;
    }

    let now = chrono::Utc::now().timestamp();
    record_cached_quota_error_at(acc, 0, &serde_json::Value::Null, now, cache_path);
    0
}

/// Registra estados de error HTTP (401 expirado, 403 TOS/verificación, 429 rate limit) en la cuenta.
pub fn apply_quota_error(acc: &mut AccountQuota, status: u16, err_val: &serde_json::Value) {
    acc.has_error = true;
    let msg = err_val
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let err_status = err_val
        .get("error")
        .and_then(|e| e.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    let has_reason = |target: &str| -> bool {
        err_val
            .get("error")
            .and_then(|e| e.get("details"))
            .and_then(|d| d.as_array())
            .is_some_and(|arr| {
                arr.iter()
                    .any(|item| item.get("reason").and_then(|r| r.as_str()) == Some(target))
            })
    };

    if status == 401 || msg.contains("expired") || err_status == "UNAUTHENTICATED" {
        acc.error_msg = "expirado".to_string();
    } else if has_reason("TOS_VIOLATION") || msg.contains("Terms of Service") {
        acc.error_msg = "tos suspendida".to_string();
    } else if has_reason("VALIDATION_REQUIRED") || msg.contains("Verify your account") {
        acc.error_msg = "verif. requerida".to_string();
    } else if status == 429 || err_status == "RESOURCE_EXHAUSTED" {
        acc.error_msg = "rate limited".to_string();
    } else {
        acc.error_msg = format!("err {status}");
    }
}

fn mark_quota_unknown(acc: &mut AccountQuota, message: &str) {
    acc.rem_5h = 0;
    acc.rem_wk = 0;
    acc.reset_5h = UNFETCHED_RESET.to_string();
    acc.reset_wk = UNFETCHED_RESET.to_string();
    acc.is_critical = false;
    acc.is_warning = false;
    acc.has_error = true;
    acc.error_msg = message.to_string();
    for pool in &mut acc.pools {
        pool.percent = 0;
        pool.reset = UNFETCHED_RESET.to_string();
        pool.pace = "desconocida".to_string();
    }
}

fn codex_plan_label(data: &serde_json::Value, acc: &AccountQuota) -> String {
    let raw = data
        .get("plan_type")
        .and_then(|value| value.as_str())
        .or_else(|| {
            acc.pools.first().and_then(|pool| {
                pool.label
                    .strip_prefix("Codex ")
                    .and_then(|rest| rest.split_whitespace().next())
            })
        })
        .unwrap_or("Codex");
    match raw.to_ascii_lowercase().as_str() {
        "plus" => "Plus".to_string(),
        "pro" => "Pro".to_string(),
        "team" => "Team".to_string(),
        "free" => "Free".to_string(),
        _ => raw.replace('_', " "),
    }
}

fn codex_reset_label(window: &serde_json::Value) -> String {
    if let Some(reset_at) = window.get("reset_at").and_then(|value| value.as_i64()) {
        let now = chrono::Utc::now().timestamp();
        let remaining = reset_at.saturating_sub(now);
        let days = remaining / 86_400;
        let hours = (remaining % 86_400) / 3_600;
        return if days > 0 {
            format!("{days}d {hours}h")
        } else {
            format!("{hours}h")
        };
    }
    if let Some(seconds) = window
        .get("reset_after_seconds")
        .and_then(|value| value.as_i64())
    {
        let days = seconds / 86_400;
        let hours = (seconds % 86_400) / 3_600;
        return if days > 0 {
            format!("{days}d {hours}h")
        } else {
            format!("{hours}h")
        };
    }
    UNFETCHED_RESET.to_string()
}

/// Aplica la respuesta de `wham/usage`, clasificando cada ventana por su duración real.
/// Codex puede devolver una cuenta semanal-only en `primary_window`, por lo que la posición
/// de la ventana no determina si es de sesión o semanal.
pub fn apply_codex_quota_data(acc: &mut AccountQuota, data: &serde_json::Value) -> bool {
    let rate_limit = data.get("rate_limit").unwrap_or(data);
    let plan = codex_plan_label(data, acc);
    let mut five_hour: Option<(u8, String)> = None;
    let mut weekly: Option<(u8, String)> = None;

    for slot in ["primary_window", "secondary_window"] {
        let Some(window) = rate_limit.get(slot).filter(|value| value.is_object()) else {
            continue;
        };
        let Some(used) = window.get("used_percent").and_then(|value| value.as_f64()) else {
            continue;
        };
        let remaining = (100.0 - used).clamp(0.0, 100.0).round() as u8;
        let reset = codex_reset_label(window);
        let duration = window
            .get("limit_window_seconds")
            .and_then(|value| value.as_i64());
        match duration {
            Some(seconds) if seconds >= 86_400 => weekly = Some((remaining, reset)),
            Some(_) => five_hour = Some((remaining, reset)),
            None if slot == "secondary_window" => weekly = Some((remaining, reset)),
            None => five_hour = Some((remaining, reset)),
        }
    }

    if five_hour.is_none() && weekly.is_none() {
        return false;
    }

    let summary = five_hour.as_ref().or(weekly.as_ref()).unwrap();
    let weekly_summary = weekly.as_ref().unwrap_or(summary);
    acc.rem_5h = summary.0;
    acc.reset_5h = summary.1.clone();
    acc.rem_wk = weekly_summary.0;
    acc.reset_wk = weekly_summary.1.clone();
    let worst_remaining = acc.rem_5h.min(acc.rem_wk);
    acc.is_critical = worst_remaining <= 15;
    acc.is_warning = !acc.is_critical && worst_remaining <= 30;
    acc.has_error = false;
    acc.error_msg.clear();

    let mut pools = Vec::new();
    if let Some((remaining, reset)) = five_hour {
        pools.push(QuotaPool {
            label: format!("Codex {plan} 5h"),
            percent: remaining,
            reset,
            pace: "cuota real".to_string(),
        });
    }
    if let Some((remaining, reset)) = weekly {
        pools.push(QuotaPool {
            label: format!("Codex {plan} Wk"),
            percent: remaining,
            reset,
            pace: "cuota real".to_string(),
        });
    }
    acc.pools = pools;
    true
}

fn record_cached_codex_quota_at(
    acc: &mut AccountQuota,
    data: &serde_json::Value,
    fetched_at: i64,
    cache_path: Option<&Path>,
) {
    let rec = CachedQuotaRecord {
        fetched_at,
        auth_mtime: file_mtime_secs(&acc.auth_file),
        data: Some(data.clone()),
        error_status: None,
        error_body: None,
    };
    if let Ok(mut guard) = QUOTA_CACHE_STATE.lock() {
        if let Some(cp) = cache_path {
            sync_disk_cache_if_needed(&mut guard, cp);
        }
        guard.entries.insert(acc.prefix.to_lowercase(), rec);
        if let Some(cp) = cache_path {
            persist_disk_cache(&mut guard, cp);
        }
    }
}

/// Comprueba si una cuenta es de Antigravity (Google / agy*).
/// Para estas cuentas, el dato soberano y principal es `GEMINI MODELS` (Flash/Pro).
/// Los pools secundarios (Argon) y terciarios (Claude/GPT) son desglosados en `Cuota de Modelos`.
pub fn is_antigravity_account(acc: &AccountQuota) -> bool {
    let lower_auth = acc.auth_file.to_lowercase();
    let lower_pfx = acc.prefix.to_lowercase();
    lower_auth.contains("antigravity")
        || lower_auth.contains("google")
        || lower_pfx.starts_with("agy")
}

/// Métricas efectivas de salud de una cuenta evaluadas dinámicamente para la familia del modelo activo.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveAccountHealth {
    pub rem_5h: u8,
    pub reset_5h: String,
    pub rem_wk: u8,
    pub reset_wk: String,
    pub is_critical: bool,
    pub is_warning: bool,
    pub family: &'static str,
}

/// Detecta la familia de modelo (`"Gemini"`, `"Claude"`, `"GPT"`, `"DeepSeek"`, `"Qwen"`)
/// a partir del identificador del modelo activo en la sesión.
pub fn resolve_model_family(model_str: &str) -> Option<&'static str> {
    let lower = model_str.to_lowercase();
    if lower.is_empty() {
        return None;
    }
    if lower.contains("claude")
        || lower.contains("sonnet")
        || lower.contains("opus")
        || lower.contains("haiku")
        || lower.contains("anthropic")
    {
        Some("Claude")
    } else if lower.contains("deepseek") {
        Some("DeepSeek")
    } else if lower.contains("gpt")
        || lower.contains("openai")
        || lower.contains("chatgpt")
        || lower.contains("codex")
        || lower.contains("/o1")
        || lower.starts_with("o1")
        || lower.contains("/o3")
        || lower.starts_with("o3")
        || lower.contains("/o4")
        || lower.starts_with("o4")
    {
        Some("GPT")
    } else if lower.contains("qwen") || lower.contains("qwq") {
        Some("Qwen")
    } else if lower.contains("argon") {
        Some("Argon")
    } else if lower.contains("gemini") || lower.contains("gemma") {
        Some("Gemini")
    } else {
        None
    }
}

/// Evalúa la salud y los porcentajes `5h` / `Wk` de una cuenta según el modelo activo en la sesión.
/// Si el usuario usa Claude, GPT, DeepSeek, Qwen, Argon o Gemini, toma los pools de esa familia;
/// si no hay modelo específico o el pool no existe, usa las métricas base de la cuenta (Gemini / default).
pub fn evaluate_account_for_model(
    acc: &AccountQuota,
    active_model: &str,
) -> EffectiveAccountHealth {
    if is_api_key(acc) {
        let rem = acc.rem_5h;
        let is_critical = !acc.has_error && rem <= 8;
        let is_warning = !acc.has_error && (rem > 8 && rem <= 20);
        let family = resolve_model_family(active_model).unwrap_or("Claude");
        return EffectiveAccountHealth {
            rem_5h: rem,
            reset_5h: acc.reset_5h.clone(),
            rem_wk: acc.rem_wk,
            reset_wk: acc.reset_wk.clone(),
            is_critical,
            is_warning,
            family,
        };
    }

    if is_codex_account(acc) {
        if is_unfetched(acc) {
            return EffectiveAccountHealth {
                rem_5h: acc.rem_5h,
                reset_5h: acc.reset_5h.clone(),
                rem_wk: acc.rem_wk,
                reset_wk: acc.reset_wk.clone(),
                is_critical: false,
                is_warning: false,
                family: "GPT",
            };
        }
        let worst_remaining = acc.rem_5h.min(acc.rem_wk);
        let is_critical = !acc.has_error && worst_remaining <= 15;
        let is_warning = !acc.has_error && (worst_remaining > 15 && worst_remaining <= 30);
        let family = resolve_model_family(active_model).unwrap_or("GPT");
        return EffectiveAccountHealth {
            rem_5h: acc.rem_5h,
            reset_5h: acc.reset_5h.clone(),
            rem_wk: acc.rem_wk,
            reset_wk: acc.reset_wk.clone(),
            is_critical,
            is_warning,
            family,
        };
    }

    // Para cuentas de Antigravity (Google / agy*), la cuota soberana y principal es GEMINI MODELS.
    // Los datos secundarios (Argon) y terciarios (Claude/GPT) son desglosados en la card
    // 'Cuota de Modelos' y no deben sobreescribir la salud global en Integraciones.
    if is_antigravity_account(acc) {
        return EffectiveAccountHealth {
            rem_5h: acc.rem_5h,
            reset_5h: acc.reset_5h.clone(),
            rem_wk: acc.rem_wk,
            reset_wk: acc.reset_wk.clone(),
            is_critical: acc.is_critical,
            is_warning: acc.is_warning,
            family: "Gemini",
        };
    }

    if let Some(family) = resolve_model_family(active_model) {
        let find_family_pools =
            |fam_prefix: &str| -> Option<(Option<&QuotaPool>, Option<&QuotaPool>)> {
                let fam_lower = fam_prefix.to_lowercase();
                let p_5h = acc.pools.iter().find(|p| {
                    let l = p.label.to_lowercase();
                    l.starts_with(&fam_lower) && l.contains("5h")
                });
                let p_wk = acc.pools.iter().find(|p| {
                    let l = p.label.to_lowercase();
                    l.starts_with(&fam_lower) && l.contains("wk")
                });
                if p_5h.is_some() || p_wk.is_some() {
                    Some((p_5h, p_wk))
                } else {
                    None
                }
            };

        // Para GPT, si el proveedor agrupa Claude/GPT bajo "Claude *", hacer fallback a Claude
        let matched = find_family_pools(family).or_else(|| {
            if family == "GPT" {
                find_family_pools("Claude")
            } else {
                None
            }
        });

        if let Some((pool_5h, pool_wk)) = matched {
            let rem_5h = pool_5h.map(|p| p.percent).unwrap_or(100);
            let reset_5h = pool_5h
                .map(|p| p.reset.clone())
                .unwrap_or_else(|| acc.reset_5h.clone());
            let rem_wk = pool_wk.map(|p| p.percent).unwrap_or(100);
            let reset_wk = pool_wk
                .map(|p| p.reset.clone())
                .unwrap_or_else(|| acc.reset_wk.clone());
            let worst_used =
                std::cmp::max(100u8.saturating_sub(rem_5h), 100u8.saturating_sub(rem_wk));
            let is_critical = !acc.has_error && worst_used >= 85;
            let is_warning = !acc.has_error && (70..85).contains(&worst_used);
            return EffectiveAccountHealth {
                rem_5h,
                reset_5h,
                rem_wk,
                reset_wk,
                is_critical,
                is_warning,
                family,
            };
        }
    }

    EffectiveAccountHealth {
        rem_5h: acc.rem_5h,
        reset_5h: acc.reset_5h.clone(),
        rem_wk: acc.rem_wk,
        reset_wk: acc.reset_wk.clone(),
        is_critical: acc.is_critical,
        is_warning: acc.is_warning,
        family: "Gemini",
    }
}

/// Sincroniza los campos de resumen (`rem_5h`, `rem_wk`, `is_critical`, `is_warning`) de la cuenta
/// en función del modelo activo en la sesión.
pub fn sync_account_health_for_model(acc: &mut AccountQuota, active_model: &str) {
    let eff = evaluate_account_for_model(acc, active_model);
    acc.rem_5h = eff.rem_5h;
    acc.reset_5h = eff.reset_5h;
    acc.rem_wk = eff.rem_wk;
    acc.reset_wk = eff.reset_wk;
    acc.is_critical = eff.is_critical;
    acc.is_warning = eff.is_warning;
}

/// Aplica los grupos y buckets recibidos de CLIProxy a la estructura `AccountQuota`.
pub fn apply_quota_data(acc: &mut AccountQuota, data: &serde_json::Value) {
    acc.has_error = false;
    acc.error_msg.clear();

    if let Some(groups) = data.get("groups").and_then(|g| g.as_array()) {
        let mut gem_5h = 100;
        let mut gem_wk = 100;
        let mut gem_5h_reset = "4h 59m".to_string();
        let mut gem_wk_reset = "4d 23h".to_string();
        let mut gem_5h_iso: Option<String> = None;
        let mut gem_wk_iso: Option<String> = None;
        let mut cla_5h = 100;
        let mut cla_wk = 100;
        let mut cla_5h_reset = "4h 59m".to_string();
        let mut cla_wk_reset = "6d 23h".to_string();
        let mut cla_5h_iso: Option<String> = None;
        let mut cla_wk_iso: Option<String> = None;
        let mut has_explicit_claude = false;

        // Pools adicionales dinámicos (Argon, GPT dedicado, DeepSeek, Qwen, etc.)
        struct ExtraFamilyPool {
            family: &'static str,
            has_5h: bool,
            rem_5h: u8,
            reset_5h: String,
            iso_5h: Option<String>,
            has_wk: bool,
            rem_wk: u8,
            reset_wk: String,
            iso_wk: Option<String>,
        }
        let mut extra_families: Vec<ExtraFamilyPool> = Vec::new();

        for grp in groups {
            let name = grp
                .get("displayName")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_lowercase();
            if name.contains("claude") {
                has_explicit_claude = true;
            }
            if let Some(buckets) = grp.get("buckets").and_then(|b| b.as_array()) {
                for b in buckets {
                    let window = b.get("window").and_then(|w| w.as_str()).unwrap_or("");
                    let frac = b
                        .get("remainingFraction")
                        .and_then(|f| f.as_f64())
                        .unwrap_or(1.0);
                    let pct = (frac * 100.0).clamp(0.0, 100.0).round() as u8;
                    let reset_time = b.get("resetTime").and_then(|r| r.as_str());

                    if name.contains("gemini") && !name.contains("argon") {
                        if window == "5h" || window.contains('5') {
                            gem_5h = pct;
                            if let Some(rt) = reset_time {
                                gem_5h_reset = format_relative_reset(rt);
                                gem_5h_iso = Some(rt.to_string());
                            }
                        } else if window == "weekly" || window.contains("week") {
                            gem_wk = pct;
                            if let Some(rt) = reset_time {
                                gem_wk_reset = format_relative_reset(rt);
                                gem_wk_iso = Some(rt.to_string());
                            }
                        }
                    } else if name.contains("claude") {
                        if window == "5h" || window.contains('5') {
                            cla_5h = pct;
                            if let Some(rt) = reset_time {
                                cla_5h_reset = format_relative_reset(rt);
                                cla_5h_iso = Some(rt.to_string());
                            }
                        } else if window == "weekly" || window.contains("week") {
                            cla_wk = pct;
                            if let Some(rt) = reset_time {
                                cla_wk_reset = format_relative_reset(rt);
                                cla_wk_iso = Some(rt.to_string());
                            }
                        }
                    }

                    let extra_family = if name.contains("argon") {
                        Some("Argon")
                    } else if name.contains("gpt") && !name.contains("claude") {
                        Some("GPT")
                    } else if name.contains("deepseek") {
                        Some("DeepSeek")
                    } else if name.contains("qwen") {
                        Some("Qwen")
                    } else {
                        None
                    };

                    if let Some(fam) = extra_family {
                        let entry = if let Some(existing) =
                            extra_families.iter_mut().find(|e| e.family == fam)
                        {
                            existing
                        } else {
                            extra_families.push(ExtraFamilyPool {
                                family: fam,
                                has_5h: false,
                                rem_5h: 100,
                                reset_5h: "4h 59m".to_string(),
                                iso_5h: None,
                                has_wk: false,
                                rem_wk: 100,
                                reset_wk: "6d 23h".to_string(),
                                iso_wk: None,
                            });
                            extra_families.last_mut().unwrap()
                        };

                        if window == "5h" || window.contains('5') {
                            entry.has_5h = true;
                            entry.rem_5h = pct;
                            if let Some(rt) = reset_time {
                                entry.reset_5h = format_relative_reset(rt);
                                entry.iso_5h = Some(rt.to_string());
                            }
                            if fam == "GPT" && !has_explicit_claude {
                                cla_5h = pct;
                                cla_5h_reset = entry.reset_5h.clone();
                                cla_5h_iso = entry.iso_5h.clone();
                            }
                        } else if window == "weekly" || window.contains("week") {
                            entry.has_wk = true;
                            entry.rem_wk = pct;
                            if let Some(rt) = reset_time {
                                entry.reset_wk = format_relative_reset(rt);
                                entry.iso_wk = Some(rt.to_string());
                            }
                            if fam == "GPT" && !has_explicit_claude {
                                cla_wk = pct;
                                cla_wk_reset = entry.reset_wk.clone();
                                cla_wk_iso = entry.iso_wk.clone();
                            }
                        }
                    }
                }
            }
        }

        acc.rem_5h = gem_5h;
        acc.rem_wk = gem_wk;
        acc.reset_5h = gem_5h_reset.clone();
        acc.reset_wk = gem_wk_reset.clone();

        // Por defecto (sin sesión o con Gemini), las métricas base reflejan Gemini;
        // evaluate_account_for_model / sync_account_health_for_model las adapta dinámicamente
        // cuando la sesión activa usa Claude, GPT, DeepSeek, Qwen u otro modelo.
        let worst_used = std::cmp::max(100u8.saturating_sub(gem_5h), 100u8.saturating_sub(gem_wk));
        acc.is_critical = worst_used >= 85;
        acc.is_warning = (70..85).contains(&worst_used);

        let gem_wk_pace = calculate_quota_pace(gem_wk, true, gem_wk_iso.as_deref());
        let gem_5h_pace = calculate_quota_pace(gem_5h, false, gem_5h_iso.as_deref());
        let cla_wk_pace = calculate_quota_pace(cla_wk, true, cla_wk_iso.as_deref());
        let cla_5h_pace = calculate_quota_pace(cla_5h, false, cla_5h_iso.as_deref());

        let mut pools = vec![
            QuotaPool {
                label: "Gemini Wk".to_string(),
                percent: gem_wk,
                reset: gem_wk_reset,
                pace: gem_wk_pace,
            },
            QuotaPool {
                label: "Gemini 5h".to_string(),
                percent: gem_5h,
                reset: gem_5h_reset,
                pace: gem_5h_pace,
            },
            QuotaPool {
                label: "Claude Wk".to_string(),
                percent: cla_wk,
                reset: cla_wk_reset,
                pace: cla_wk_pace,
            },
            QuotaPool {
                label: "Claude 5h".to_string(),
                percent: cla_5h,
                reset: cla_5h_reset,
                pace: cla_5h_pace,
            },
        ];

        for extra in extra_families {
            if extra.has_wk {
                let wk_pace = calculate_quota_pace(extra.rem_wk, true, extra.iso_wk.as_deref());
                pools.push(QuotaPool {
                    label: format!("{} Wk", extra.family),
                    percent: extra.rem_wk,
                    reset: extra.reset_wk,
                    pace: wk_pace,
                });
            }
            if extra.has_5h {
                let h5_pace = calculate_quota_pace(extra.rem_5h, false, extra.iso_5h.as_deref());
                pools.push(QuotaPool {
                    label: format!("{} 5h", extra.family),
                    percent: extra.rem_5h,
                    reset: extra.reset_5h,
                    pace: h5_pace,
                });
            }
        }

        acc.pools = pools;
    }
}

/// Calcula el ritmo de consumo (pacing) comparando el % usado frente al % de tiempo transcurrido en la ventana.
pub fn calculate_quota_pace(rem_percent: u8, is_weekly: bool, iso_time: Option<&str>) -> String {
    let Some(iso) = iso_time else {
        return "~0%".to_string();
    };
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return "~0%".to_string();
    };
    let now = chrono::Utc::now();
    let remaining_secs = (dt.with_timezone(&chrono::Utc) - now).num_seconds();
    let window_secs: i64 = if is_weekly { 7 * 24 * 3600 } else { 5 * 3600 };
    if remaining_secs <= 0 || remaining_secs > window_secs {
        return "~0%".to_string();
    }
    let elapsed_secs = window_secs - remaining_secs;
    let elapsed_pct = (elapsed_secs as f64 / window_secs as f64) * 100.0;
    let used_pct = 100.0 - f64::from(rem_percent);
    let pace_diff = (used_pct - elapsed_pct).round() as i32;
    if pace_diff > 5 {
        format!("↓{pace_diff}%")
    } else if pace_diff < -5 {
        format!("↑{}%", pace_diff.abs())
    } else {
        "~0%".to_string()
    }
}

/// Formatea marcas temporales ISO en expresiones relativas breves (ej. `4h 59m`, `4d 23h`).
pub fn format_relative_reset(iso_time: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso_time) {
        let now = chrono::Utc::now();
        let dur = dt.with_timezone(&chrono::Utc) - now;
        let total_mins = dur.num_minutes();
        if total_mins <= 0 {
            return "reseteando…".to_string();
        }
        if total_mins < 60 {
            return format!("{}m", total_mins);
        }
        let hours = total_mins / 60;
        let mins = total_mins % 60;
        if hours < 24 {
            return if mins > 0 {
                format!("{}h {}m", hours, mins)
            } else {
                format!("{}h", hours)
            };
        }
        let days = hours / 24;
        let rem_hours = hours % 24;
        if rem_hours > 0 {
            format!("{}d {}h", days, rem_hours)
        } else {
            format!("{}d", days)
        }
    } else {
        "?".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unfetched_marker_transitions_cleanly_even_when_real_reset_is_4h59m() {
        let mut acc = AccountQuota {
            prefix: "agy1".to_string(),
            reset_5h: UNFETCHED_RESET.to_string(),
            reset_wk: UNFETCHED_RESET.to_string(),
            pools: vec![QuotaPool {
                label: "Gemini 5h".to_string(),
                percent: 100,
                reset: UNFETCHED_RESET.to_string(),
                pace: "~0%".to_string(),
            }],
            ..Default::default()
        };
        assert!(is_unfetched(&acc));

        // Incluso cuando la respuesta real coincide con el valor por defecto "4h 59m",
        // ya NO debe considerarse sin consultar.
        apply_quota_data(
            &mut acc,
            &json!({
                "groups": [{
                    "displayName": "Gemini",
                    "buckets": [{"window": "5h", "remainingFraction": 1.0}]
                }]
            }),
        );
        assert_eq!(acc.reset_5h, "4h 59m");
        assert!(!is_unfetched(&acc));

        // Y si una cuenta nueva recibe error HTTP, tampoco queda marcada como pendiente.
        let mut err_acc = AccountQuota {
            prefix: "agy2".to_string(),
            reset_5h: UNFETCHED_RESET.to_string(),
            ..Default::default()
        };
        assert!(is_unfetched(&err_acc));
        apply_quota_error(&mut err_acc, 401, &json!({}));
        assert!(!is_unfetched(&err_acc));
    }
}
