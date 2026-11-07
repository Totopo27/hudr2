//! Adaptador CLIProxyAPI para cuotas y cuentas.
//!
//! Maneja el parseo de credenciales en `resolve_auths_dir()`,
//! la normalización de prefijos (`antigravity-*`, `google-*`, `oauth-*`),
//! y la consulta HTTP a `QUOTA_URL`.

use crate::config::{QUOTA_URL, USER_AGENT};
use crate::models::{AccountQuota, QuotaPool};
use crate::paths::resolve_auths_dir_for;
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
    Path::new(path_str)
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

    if let Ok(bytes) = fs::read(cache_path)
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
            apply_quota_data(acc, data);
        }
        if let Some(status) = rec.error_status {
            let ttl = error_retry_ttl_secs(status);
            if rec.data.is_none() || matches!(status, 401 | 403) || (now_epoch - rec.fetched_at) < ttl
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
    if let Some(ref data) = rec.data {
        has_expired_bucket_window(data, rec.fetched_at, now_epoch)
    } else {
        false
    }
}

/// Comprueba si una cuenta candidata fue verificada recientemente (`<= max_age_secs`) y sigue vigente.
pub fn is_fresh_within(acc: &AccountQuota, max_age_secs: i64) -> bool {
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
    !acc.has_error && (acc.pools.is_empty() || acc.reset_5h == UNFETCHED_RESET)
}

fn read_valid_credential_json(path: &Path) -> Option<(String, serde_json::Value)> {
    let file_name = path.file_name().and_then(|f| f.to_str())?;
    if !file_name.ends_with(".json") || file_name.starts_with('.') {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let val = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    let has_creds = val.get("access_token").is_some()
        || val.get("email").is_some()
        || val.get("token").is_some()
        || val.get("type").is_some();
    has_creds.then(|| (file_name.to_string(), val))
}

/// Comprueba si el directorio de autenticación existe y contiene credenciales JSON válidas.
pub fn is_available() -> bool {
    is_available_for(".")
}

/// Comprueba si el directorio de autenticación para `target_dir` contiene credenciales JSON válidas.
pub fn is_available_for(target_dir: &str) -> bool {
    let auths_dir = resolve_auths_dir_for(target_dir);
    if !auths_dir.exists() {
        return false;
    }

    let Ok(entries) = fs::read_dir(auths_dir) else {
        return false;
    };

    entries
        .flatten()
        .any(|entry| read_valid_credential_json(&entry.path()).is_some())
}

/// Descubre y registra cuentas desde los archivos de credenciales de CLIProxy.
pub fn discover_accounts(target_list: &mut Vec<AccountQuota>) {
    discover_accounts_for(".", target_list);
}

/// Descubre y registra cuentas considerando el directorio de proyecto activo (`target_dir`).
pub fn discover_accounts_for(target_dir: &str, target_list: &mut Vec<AccountQuota>) {
    let auths_dir = resolve_auths_dir_for(target_dir);
    if !auths_dir.exists() {
        target_list.clear();
        return;
    }

    let mut seen_files = std::collections::HashSet::new();

    if let Ok(entries) = fs::read_dir(auths_dir) {
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

                let prefix = val
                    .get("prefix")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        if !clean_stem.is_empty() {
                            clean_stem.to_string()
                        } else if !email.is_empty() {
                            email.split('@').next().unwrap_or("acc").to_string()
                        } else {
                            "acc".to_string()
                        }
                    });

                let path_str = path.to_string_lossy().to_string();
                seen_files.insert(path_str.clone());
                if let Some(existing) = target_list
                    .iter_mut()
                    .find(|a| a.auth_file == path_str || a.prefix == prefix)
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

    // Purgar cuentas huérfanas cuyos archivos JSON fueron eliminados del directorio auths
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
    if !Path::new(&acc.auth_file).exists() {
        return 0;
    }

    let raw = match fs::read_to_string(&acc.auth_file) {
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
        acc.error_msg = "token expirado".to_string();
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
    } else if lower.contains("gemini") || lower.contains("gemma") {
        Some("Gemini")
    } else {
        None
    }
}

/// Evalúa la salud y los porcentajes `5h` / `Wk` de una cuenta según el modelo activo en la sesión.
/// Si el usuario usa Claude, GPT, DeepSeek, Qwen o Gemini, toma los pools de esa familia;
/// si no hay modelo específico o el pool no existe, usa las métricas base de la cuenta (Gemini / default).
pub fn evaluate_account_for_model(
    acc: &AccountQuota,
    active_model: &str,
) -> EffectiveAccountHealth {
    if let Some(family) = resolve_model_family(active_model) {
        let find_family_pools = |fam_prefix: &str| -> Option<(&QuotaPool, &QuotaPool)> {
            let fam_lower = fam_prefix.to_lowercase();
            let p_5h = acc.pools.iter().find(|p| {
                let l = p.label.to_lowercase();
                l.starts_with(&fam_lower) && l.contains("5h")
            });
            let p_wk = acc.pools.iter().find(|p| {
                let l = p.label.to_lowercase();
                l.starts_with(&fam_lower) && l.contains("wk")
            });
            match (p_5h, p_wk) {
                (Some(h), Some(w)) => Some((h, w)),
                _ => None,
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
            let rem_5h = pool_5h.percent;
            let rem_wk = pool_wk.percent;
            let worst_used =
                std::cmp::max(100u8.saturating_sub(rem_5h), 100u8.saturating_sub(rem_wk));
            let is_critical = !acc.has_error && worst_used >= 85;
            let is_warning = !acc.has_error && (70..85).contains(&worst_used);
            return EffectiveAccountHealth {
                rem_5h,
                reset_5h: pool_5h.reset.clone(),
                rem_wk,
                reset_wk: pool_wk.reset.clone(),
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

        // Pools adicionales dinámicos (GPT dedicado, DeepSeek, Qwen, etc.)
        struct ExtraFamilyPool {
            family: &'static str,
            rem_5h: u8,
            reset_5h: String,
            iso_5h: Option<String>,
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

                    if name.contains("gemini") {
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

                    let extra_family = if name.contains("gpt") && !name.contains("claude") {
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
                                rem_5h: 100,
                                reset_5h: "4h 59m".to_string(),
                                iso_5h: None,
                                rem_wk: 100,
                                reset_wk: "6d 23h".to_string(),
                                iso_wk: None,
                            });
                            extra_families.last_mut().unwrap()
                        };

                        if window == "5h" || window.contains('5') {
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
        let worst_used =
            std::cmp::max(100u8.saturating_sub(gem_5h), 100u8.saturating_sub(gem_wk));
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
            let wk_pace = calculate_quota_pace(extra.rem_wk, true, extra.iso_wk.as_deref());
            let h5_pace = calculate_quota_pace(extra.rem_5h, false, extra.iso_5h.as_deref());
            pools.push(QuotaPool {
                label: format!("{} Wk", extra.family),
                percent: extra.rem_wk,
                reset: extra.reset_wk,
                pace: wk_pace,
            });
            pools.push(QuotaPool {
                label: format!("{} 5h", extra.family),
                percent: extra.rem_5h,
                reset: extra.reset_5h,
                pace: h5_pace,
            });
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
