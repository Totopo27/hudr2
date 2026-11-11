//! Adaptador de telemetría para Pi y Gentle Shell.
//!
//! Lee de forma incremental (por byte-offset) los archivos de sesión `.jsonl`
//! de Pi / Gentle Shell, resuelve límites de contexto desde `cpamc-models-cache.json`
//! / `models-store.json` y aplica fallback a `settings.json` cuando la sesión es nueva.

use crate::models::{AccountQuota, AgentSession, AttributedChanges, HerdrPane, SessionTodo};
use crate::opencode::{
    clean_model_name, extract_prefix, resolve_model_context_limit, resolve_pool_account_prefixes,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Debug, Default)]
pub struct PiTail {
    pub session: AgentSession,
    pub pending: Vec<u8>,
    pub offset: u64,
    pub limit_model: Option<String>,
    pub modified_files: HashSet<String>,
    pub raw_model_id: String,
    pub raw_provider: String,
}

static PI_TAIL_CACHE: LazyLock<Mutex<HashMap<String, PiTail>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn clear_pi_tail_cache() {
    if let Ok(mut guard) = PI_TAIL_CACHE.lock() {
        guard.clear();
    }
}

fn json_str<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn parse_todos(tasks: &[Value]) -> Vec<SessionTodo> {
    tasks
        .iter()
        .filter_map(|t| {
            let title = t.get("title").and_then(Value::as_str)?.trim();
            if title.is_empty() {
                return None;
            }
            let status = match json_str(t, "status") {
                "done" => "done",
                "in_progress" => "in_progress",
                _ => "pending",
            };
            Some(SessionTodo {
                title: title.to_string(),
                status: status.to_string(),
            })
        })
        .collect()
}

/// Busca recursivamente `contextWindow` para un `id` de modelo en `cpamc-models-cache.json` o `models-store.json`.
pub fn find_context_window_in_json(v: &Value, model_id: &str) -> Option<i64> {
    if model_id.is_empty() {
        return None;
    }
    let short_target = model_id.rsplit('/').next().unwrap_or(model_id);
    if let Some(id) = v.get("id").and_then(Value::as_str)
        && (id.eq_ignore_ascii_case(model_id) || id.eq_ignore_ascii_case(short_target))
        && let Some(cw) = v.get("contextWindow").and_then(Value::as_i64)
        && cw > 0
    {
        return Some(cw);
    }
    match v {
        Value::Object(map) => map
            .values()
            .find_map(|child| find_context_window_in_json(child, model_id)),
        Value::Array(arr) => arr
            .iter()
            .find_map(|child| find_context_window_in_json(child, model_id)),
        _ => None,
    }
}

/// Obtiene el directorio raíz del agente Pi / Gentle Shell a partir de la ruta de sesión `<agent>/sessions/.../*.jsonl`.
pub fn agent_dir_from_session_path(session_path: &Path) -> Option<PathBuf> {
    let norm = session_path.to_string_lossy().replace('\\', "/");
    norm.split_once("/sessions/")
        .map(|(prefix, _)| PathBuf::from(prefix))
}

impl PiTail {
    pub fn new() -> Self {
        Self {
            session: AgentSession {
                provider_name: "pi".to_string(),
                agent: "pi".to_string(),
                model: String::new(),
                model_short: String::new(),
                context_limit: 200_000,
                agent_status: "idle".to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    pub fn ingest(&mut self, bytes: &[u8], reset: bool) {
        if reset {
            *self = Self::new();
        }
        self.offset += bytes.len() as u64;
        self.pending.extend_from_slice(bytes);

        let end = self
            .pending
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        if end == 0 {
            return;
        }

        let complete: Vec<u8> = self.pending.drain(..end).collect();
        for line in complete.split(|&b| b == b'\n') {
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<Value>(line) else {
                continue;
            };
            match json_str(&v, "type") {
                "model_change" => {
                    let mid = json_str(&v, "modelId");
                    let prov = json_str(&v, "provider");
                    if !mid.is_empty() {
                        self.raw_model_id = mid.to_string();
                    }
                    if !prov.is_empty() {
                        self.raw_provider = prov.to_string();
                    }
                    self.sync_model_fields();
                }
                "thinking_level_change" => {
                    let lvl = json_str(&v, "thinkingLevel");
                    if !lvl.is_empty() {
                        self.session.thinking_level = Some(lvl.to_string());
                    }
                }
                "message"
                    if json_str(&v["message"], "role") == "toolResult"
                        && json_str(&v["message"], "toolName") == "todo" =>
                {
                    if let Some(tasks) = v["message"]["details"]["gentleTodo"]["tasks"].as_array() {
                        self.session.todos = parse_todos(tasks);
                    }
                }
                "message" if json_str(&v["message"], "role") == "assistant" => {
                    let m = &v["message"];
                    let u = &m["usage"];
                    let inp = u["input"].as_i64().unwrap_or(0);
                    let outp = u["output"].as_i64().unwrap_or(0);
                    let c_read = u["cacheRead"].as_i64().unwrap_or(0);
                    let c_write = u["cacheWrite"].as_i64().unwrap_or(0);

                    self.session.tokens_input += inp;
                    self.session.tokens_output += outp;
                    if let Some(tot) = u["totalTokens"].as_i64() {
                        self.session.active_turn_tokens = tot;
                    } else if inp + outp + c_read + c_write > 0 {
                        self.session.active_turn_tokens = inp + outp + c_read + c_write;
                    }
                    let step_cost = u["cost"]["total"].as_f64().unwrap_or(0.0);
                    self.session.cost += step_cost;
                    if step_cost > 0.0 {
                        self.session.last_turn_cost = step_cost;
                    }
                    self.session.turn_count += 1;

                    if let Some(mod_str) = m["model"].as_str().filter(|x| !x.is_empty()) {
                        self.raw_model_id = mod_str.to_string();
                    }
                    if let Some(prov_str) = m["provider"].as_str().filter(|x| !x.is_empty()) {
                        self.raw_provider = prov_str.to_string();
                    }
                    self.sync_model_fields();

                    if let Some(content_arr) = m["content"].as_array() {
                        for item in content_arr {
                            if json_str(item, "type") == "toolCall" {
                                let name = json_str(item, "name").trim().to_lowercase();
                                if name.is_empty() {
                                    continue;
                                }
                                self.session.tool_total += 1;
                                if name == "read" || name == "grep" || name == "glob" {
                                    self.session.tool_reads += 1;
                                } else if name == "edit" || name == "write" {
                                    self.session.tool_writes += 1;
                                    if let Some(p) = item["arguments"]["path"]
                                        .as_str()
                                        .or_else(|| item["arguments"]["file_path"].as_str())
                                        .filter(|p| !p.is_empty())
                                    {
                                        self.modified_files.insert(p.to_string());
                                    }
                                } else if name == "bash" || name == "shell" {
                                    self.session.tool_bash += 1;
                                } else if name.starts_with("mem_") || name.contains("engram") {
                                    self.session.tool_engram += 1;
                                }
                            }
                        }
                    }

                    self.session.attributed_changes = AttributedChanges {
                        files_count: self.modified_files.len(),
                        additions: 0,
                    };
                }
                _ => {}
            }
        }
    }

    fn sync_model_fields(&mut self) {
        if self.raw_model_id.is_empty() {
            return;
        }
        let combined = if !self.raw_provider.is_empty() && !self.raw_model_id.contains('/') {
            format!("{}/{}", self.raw_provider, self.raw_model_id)
        } else {
            self.raw_model_id.clone()
        };
        let clean = clean_model_name(&combined);
        self.session.model_short = clean.rsplit('/').next().unwrap_or(&clean).to_string();
        self.session.model = clean;
    }
}

/// Lee incrementalmente un archivo de sesión `.jsonl` de Pi / Gentle Shell,
/// usando `PI_TAIL_CACHE` para leer únicamente los bytes nuevos desde la última llamada.
pub fn read_pi_session_file(
    jsonl_path: &Path,
    accounts_list: &[AccountQuota],
    agent_status: &str,
) -> Option<AgentSession> {
    let mut f = fs::File::open(jsonl_path).ok()?;
    let file_len = f.metadata().ok()?.len();
    let key = crate::paths::normalize_path(&jsonl_path.to_string_lossy());

    let mut tail = {
        let guard = PI_TAIL_CACHE.lock().ok()?;
        guard.get(&key).cloned().unwrap_or_else(PiTail::new)
    };

    if file_len < tail.offset {
        tail = PiTail::new();
    }

    if file_len > tail.offset {
        f.seek(SeekFrom::Start(tail.offset)).ok()?;
        let mut bytes = Vec::with_capacity((file_len - tail.offset) as usize);
        f.read_to_end(&mut bytes).ok()?;
        tail.ingest(&bytes, false);
    }

    let agent_dir = agent_dir_from_session_path(jsonl_path);

    // Determinar si es gentle-shell o pi según la ruta del agente
    if let Some(ref ad) = agent_dir {
        let ad_str = ad.to_string_lossy().to_lowercase();
        if ad_str.contains("gentle-shell") {
            tail.session.provider_name = "gentle-shell".to_string();
            tail.session.agent = "gentle-shell".to_string();
        } else {
            tail.session.provider_name = "pi".to_string();
            tail.session.agent = "pi".to_string();
        }
    }

    // Fallback a settings.json si la sesión es recién creada y aún no tiene turno de asistente
    if tail.session.model.is_empty()
        && let Some(ref ad) = agent_dir
        && let Ok(raw) = fs::read(ad.join("settings.json"))
        && let Ok(settings) = serde_json::from_slice::<Value>(&raw)
    {
        let def_model = json_str(&settings, "defaultModel");
        let def_prov = json_str(&settings, "defaultProvider");
        let def_think = json_str(&settings, "defaultThinkingLevel");
        if !def_model.is_empty() {
            tail.raw_model_id = def_model.to_string();
            tail.raw_provider = def_prov.to_string();
            tail.sync_model_fields();
        }
        if tail.session.thinking_level.is_none() && !def_think.is_empty() {
            tail.session.thinking_level = Some(def_think.to_string());
        }
    }

    // Resolver contextWindow desde cpamc-models-cache.json -> models-store.json -> fallback
    if !tail.session.model.is_empty()
        && tail.limit_model.as_deref() != Some(tail.session.model.as_str())
    {
        tail.limit_model = Some(tail.session.model.clone());
        let mut found_limit = None;
        if let Some(ref ad) = agent_dir {
            for store_name in ["cpamc-models-cache.json", "models-store.json"] {
                if let Ok(raw) = fs::read(ad.join(store_name))
                    && let Ok(v) = serde_json::from_slice::<Value>(&raw)
                    && let Some(cw) = find_context_window_in_json(&v, &tail.raw_model_id)
                        .or_else(|| find_context_window_in_json(&v, &tail.session.model_short))
                {
                    found_limit = Some(cw);
                    break;
                }
            }
        }
        tail.session.context_limit =
            found_limit.unwrap_or_else(|| resolve_model_context_limit(&tail.session.model));
    }

    if tail.session.model.is_empty() {
        tail.session.model = "sin-modelo".to_string();
        tail.session.model_short = "sin-modelo".to_string();
    }

    tail.session.agent_status = if agent_status.is_empty() || agent_status == "unknown" {
        "idle".to_string()
    } else {
        agent_status.to_string()
    };

    // Atribución de prefijos de cuenta (si usa cuentas de Cliproxy/CPAMC)
    let known_prefixes = crate::quotas::sorted_known_prefixes(accounts_list, None);

    if let Some(pfx) = extract_prefix(&tail.session.agent, &tail.session.model, &known_prefixes) {
        tail.session.local_prefix = Some(pfx.clone());
        tail.session.all_active_prefixes.insert(pfx);
    }
    for pp in resolve_pool_account_prefixes(&tail.session.model, &known_prefixes) {
        tail.session.all_active_prefixes.insert(pp);
    }

    let result = tail.session.clone();
    if let Ok(mut guard) = PI_TAIL_CACHE.lock() {
        if guard.len() >= 64 {
            guard.clear();
        }
        guard.insert(key, tail);
    }

    Some(result)
}

/// Sincroniza en caliente los `metadata_tokens` emitidos vía IPC de Herdr
/// sobre la sesión activa, permitiendo telemetría en memoria con 0 lecturas de disco cuando el agente los publica.
pub fn apply_pane_metadata_tokens(
    session: &mut AgentSession,
    tokens: &HashMap<String, String>,
    accounts_list: &[AccountQuota],
) {
    if tokens.is_empty() {
        return;
    }

    if let Some(profile) = tokens
        .get("profile")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        let clean = clean_model_name(profile);
        session.model_short = clean.rsplit('/').next().unwrap_or(&clean).to_string();
        session.model = clean;
    }

    if let Some(win_str) = tokens.get("ctx_win")
        && let Ok(win) = win_str.parse::<i64>()
        && win > 0
    {
        session.context_limit = win;
    } else if !session.model.is_empty()
        && session.model != "sin-modelo"
        && session.context_limit <= 0
    {
        session.context_limit = resolve_model_context_limit(&session.model);
    }

    if let Some(tok_str) = tokens.get("ctx_tok")
        && let Ok(tok) = tok_str.parse::<i64>()
        && tok >= 0
    {
        session.active_turn_tokens = tok;
    } else if let Some(pct_str) = tokens.get("ctx_pct")
        && let Ok(pct) = pct_str.parse::<f64>()
        && session.context_limit > 0
        && session.active_turn_tokens == 0
    {
        session.active_turn_tokens = ((pct / 100.0) * session.context_limit as f64).round() as i64;
    }

    if let Some(tin_str) = tokens.get("tok_in")
        && let Ok(tin) = tin_str.parse::<i64>()
    {
        session.tokens_input = tin;
    }
    if let Some(tout_str) = tokens.get("tok_out")
        && let Ok(tout) = tout_str.parse::<i64>()
    {
        session.tokens_output = tout;
    }
    if let Some(cost_str) = tokens.get("cost")
        && let Ok(cost) = cost_str.parse::<f64>()
    {
        session.cost = cost;
    }
    if let Some(files_str) = tokens.get("git_files")
        && let Ok(fc) = files_str.parse::<usize>()
    {
        session.attributed_changes.files_count = session.attributed_changes.files_count.max(fc);
    }
    if let Some(lines_str) = tokens.get("git_lines")
        && let Ok(la) = lines_str.parse::<usize>()
    {
        session.attributed_changes.additions = session.attributed_changes.additions.max(la);
    }

    // Parsear TODOs publicados como "todo_1" => "done:Título" | "prog:Título" | "open:Título"
    let mut indexed_todos: Vec<(usize, SessionTodo)> = Vec::new();
    for (k, v) in tokens {
        if let Some(idx_str) = k.strip_prefix("todo_")
            && let Some((prefix, raw_title)) = v.split_once(':')
        {
            let title = raw_title.trim();
            if title.is_empty() {
                continue;
            }
            let status = match prefix.trim() {
                "done" => "done",
                "prog" | "in_progress" => "in_progress",
                _ => "pending",
            };
            let idx = idx_str.parse::<usize>().unwrap_or(usize::MAX);
            indexed_todos.push((
                idx,
                SessionTodo {
                    title: title.to_string(),
                    status: status.to_string(),
                },
            ));
        }
    }
    if !indexed_todos.is_empty() {
        indexed_todos.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.title.cmp(&b.1.title)));
        session.todos = indexed_todos.into_iter().map(|(_, t)| t).collect();
    }

    // Atribución de prefijos de cuenta (Cliproxy/CPAMC) si el modelo provino de profile
    if !session.model.is_empty() && session.model != "sin-modelo" {
        let known_prefixes = crate::quotas::sorted_known_prefixes(accounts_list, None);

        if let Some(pfx) = extract_prefix(&session.agent, &session.model, &known_prefixes) {
            session.local_prefix = Some(pfx.clone());
            session.all_active_prefixes.insert(pfx);
        }
        for pp in resolve_pool_account_prefixes(&session.model, &known_prefixes) {
            session.all_active_prefixes.insert(pp);
        }
    }
}

/// Intenta resolver una sesión activa de Pi / Gentle Shell a partir de los panes de Herdr
/// (incluyendo telemetría en vivo vía `metadata_tokens`) o de la variable de entorno `PI_SESSION_PATH`.
pub fn try_read_pi_session(
    target_dir: &str,
    herdr_panes: &[HerdrPane],
    accounts_list: &[AccountQuota],
    my_pane_id: Option<&str>,
    my_tab_id: Option<&str>,
) -> Option<(PathBuf, AgentSession)> {
    // 1. Variable explícita PI_SESSION_PATH (útil para pruebas o lanzamiento directo)
    if let Ok(env_path) = std::env::var("PI_SESSION_PATH") {
        let p = PathBuf::from(env_path.trim());
        if p.exists() && p.extension().is_some_and(|e| e == "jsonl") {
            let ses = read_pi_session_file(&p, accounts_list, "idle")?;
            return Some((p, ses));
        }
    }

    let norm_target = crate::paths::normalize_path(target_dir);

    let is_pi_pane = |p: &HerdrPane| -> bool {
        let has_jsonl = p
            .agent_session_id
            .as_deref()
            .is_some_and(|s| s.ends_with(".jsonl"));
        let is_pi_agent = matches!(
            p.agent.as_deref(),
            Some("pi") | Some("gentle-shell") | Some("gs2")
        );
        has_jsonl || is_pi_agent || !p.tokens.is_empty()
    };

    let is_same_cwd = |p: &HerdrPane| -> bool {
        p.cwd.as_deref().map(crate::paths::normalize_path).as_ref() == Some(&norm_target)
    };

    let sibling = my_tab_id.and_then(|tid| {
        herdr_panes.iter().find(|p| {
            p.tab_id.as_deref() == Some(tid)
                && my_pane_id.map(|mp| mp != p.pane_id).unwrap_or(true)
                && is_pi_pane(p)
        })
    });
    let cwd_focused = herdr_panes.iter().find(|p| is_same_cwd(p) && p.focused && is_pi_pane(p));
    let cwd_working = herdr_panes.iter().find(|p| is_same_cwd(p) && p.agent_status.as_deref() == Some("working") && is_pi_pane(p));
    let cwd_any = herdr_panes.iter().find(|p| is_same_cwd(p) && is_pi_pane(p));

    let global_focused = herdr_panes.iter().find(|p| p.focused && is_pi_pane(p));
    let global_working = herdr_panes
        .iter()
        .find(|p| p.agent_status.as_deref() == Some("working") && is_pi_pane(p));

    let target_pane = sibling
        .or(cwd_focused)
        .or(cwd_working)
        .or(cwd_any)
        .or(global_focused)
        .or(global_working)?;
    let status = target_pane.agent_status.as_deref().unwrap_or("idle");

    if let Some(ref raw_path) = target_pane.agent_session_id {
        let path = PathBuf::from(raw_path);
        if path.exists() {
            // Si hay un archivo hermano más reciente en el mismo directorio (ej. tras `/new` en Pi),
            // usar el más reciente.
            let newest = newest_jsonl_in_same_dir(&path).unwrap_or(path);
            let mut ses = read_pi_session_file(&newest, accounts_list, status)?;
            apply_pane_metadata_tokens(&mut ses, &target_pane.tokens, accounts_list);
            return Some((newest, ses));
        }
    }

    // 2. Si aún no existe archivo .jsonl en disco pero el pane emite `metadata_tokens` en memoria (TelarDev IPC):
    if !target_pane.tokens.is_empty() {
        let agent_name = target_pane.agent.as_deref().unwrap_or("pi");
        let mut ses = AgentSession {
            provider_name: agent_name.to_string(),
            agent: agent_name.to_string(),
            model: "sin-modelo".to_string(),
            model_short: "sin-modelo".to_string(),
            context_limit: 200_000,
            agent_status: if status.is_empty() || status == "unknown" {
                "idle".to_string()
            } else {
                status.to_string()
            },
            ..Default::default()
        };
        apply_pane_metadata_tokens(&mut ses, &target_pane.tokens, accounts_list);
        let fallback_path = target_pane
            .agent_session_id
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("memory://herdr-metadata-tokens"));
        return Some((fallback_path, ses));
    }

    None
}

fn newest_jsonl_in_same_dir(current: &Path) -> Option<PathBuf> {
    let parent = current.parent()?;
    let entries = fs::read_dir(parent).ok()?;
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
}
