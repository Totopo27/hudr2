use crate::config::resolve_opencode_db_path;
use crate::models::{AccountQuota, AttributedChanges, HerdrPane, OpenCodeSession};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct DbSessionRecord {
    pub id: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub directory: Option<String>,
    pub title: Option<String>,
    pub time_updated: Option<i64>,
    pub time_archived: Option<i64>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub cost: Option<f64>,
}

pub fn clean_opencode_terminal_title(title: &str) -> String {
    let re = regex::Regex::new(r"(?i)^(?:OC|opencode)\s*\|\s*").unwrap();
    re.replace(title, "").trim().to_string()
}

pub fn clean_model_name(model_raw: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(model_raw)
        && let Some(id_val) = parsed.get("id").and_then(|i| i.as_str())
    {
        return id_val.to_string();
    }
    model_raw.to_string()
}

pub fn resolve_model_context_limit(model_id: &str) -> i64 {
    let lower = model_id.to_lowercase();
    if lower.contains("qwen") {
        128_000
    } else if lower.contains("claude-3-5")
        || lower.contains("claude-3.5")
        || lower.contains("haiku")
    {
        200_000
    } else {
        1_000_000
    }
}

fn extract_tool_path(tool_obj: &serde_json::Value) -> Option<String> {
    if let Some(state_val) = tool_obj.get("state") {
        if let Some(obj) = state_val.as_object() {
            if let Some(p) = obj
                .get("input")
                .and_then(|i| i.get("path").or_else(|| i.get("file_path")))
                .and_then(|p| p.as_str())
            {
                return Some(p.to_string());
            }
            if let Some(p) = obj.get("path").and_then(|p| p.as_str()) {
                return Some(p.to_string());
            }
        } else if let Some(s_str) = state_val.as_str()
            && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s_str)
        {
            if let Some(p) = parsed
                .get("input")
                .and_then(|i| i.get("path").or_else(|| i.get("file_path")))
                .and_then(|p| p.as_str())
            {
                return Some(p.to_string());
            }
            if let Some(p) = parsed.get("path").and_then(|p| p.as_str()) {
                return Some(p.to_string());
            }
        }
    }
    if let Some(input_val) = tool_obj.get("input") {
        if let Some(p) = input_val
            .get("path")
            .or_else(|| input_val.get("file_path"))
            .and_then(|p| p.as_str())
        {
            return Some(p.to_string());
        }
        if let Some(s_str) = input_val.as_str()
            && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s_str)
            && let Some(p) = parsed
                .get("path")
                .or_else(|| parsed.get("file_path"))
                .and_then(|p| p.as_str())
        {
            return Some(p.to_string());
        }
    }
    None
}

pub fn resolve_session_for_pane(conn: &Connection, pane: &HerdrPane) -> Option<DbSessionRecord> {
    if let Some(ref ses_id) = pane.agent_session_id
        && let Ok(mut stmt) = conn.prepare(
            "SELECT id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost FROM session_v2 WHERE id = ?"
        )
            && let Ok(rec) = stmt.query_row([ses_id], |row| {
                Ok(DbSessionRecord {
                    id: row.get(0)?,
                    agent: row.get(1)?,
                    model: row.get(2)?,
                    directory: row.get(3)?,
                    title: row.get(4)?,
                    time_updated: row.get(5)?,
                    time_archived: row.get(6)?,
                    tokens_input: row.get(7)?,
                    tokens_output: row.get(8)?,
                    cost: row.get(9)?,
                })
            }) {
                return Some(rec);
            }

    if let Some(ref raw_cwd) = pane.cwd {
        let norm_cwd = raw_cwd.replace('\\', "/");
        let raw_title = pane
            .terminal_title_stripped
            .as_deref()
            .or(pane.terminal_title.as_deref())
            .unwrap_or("");
        let title_clean = clean_opencode_terminal_title(raw_title);

        // 1. Coincidencia exacta por título y directorio (sin filtrar por time_archived, ya que el pane está vivo)
        if !title_clean.is_empty()
            && let Ok(mut stmt) = conn.prepare(
                "SELECT id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost FROM session_v2 WHERE (directory = ? OR directory = ?) AND title = ? ORDER BY time_updated DESC LIMIT 1"
            )
            && let Ok(rec) = stmt.query_row(rusqlite::params![raw_cwd, norm_cwd, title_clean], |row| {
                Ok(DbSessionRecord {
                    id: row.get(0)?,
                    agent: row.get(1)?,
                    model: row.get(2)?,
                    directory: row.get(3)?,
                    title: row.get(4)?,
                    time_updated: row.get(5)?,
                    time_archived: row.get(6)?,
                    tokens_input: row.get(7)?,
                    tokens_output: row.get(8)?,
                    cost: row.get(9)?,
                })
            }) {
                return Some(rec);
            }

        // 2. Coincidencia por directorio en sesiones activas (no archivadas o actualizadas después de archivarse)
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost FROM session_v2 WHERE (time_archived IS NULL OR (time_updated IS NOT NULL AND time_archived IS NOT NULL AND time_updated > time_archived)) AND (directory = ? OR directory = ?) ORDER BY time_updated DESC LIMIT 1"
        )
            && let Ok(rec) = stmt.query_row(rusqlite::params![raw_cwd, norm_cwd], |row| {
                Ok(DbSessionRecord {
                    id: row.get(0)?,
                    agent: row.get(1)?,
                    model: row.get(2)?,
                    directory: row.get(3)?,
                    title: row.get(4)?,
                    time_updated: row.get(5)?,
                    time_archived: row.get(6)?,
                    tokens_input: row.get(7)?,
                    tokens_output: row.get(8)?,
                    cost: row.get(9)?,
                })
            }) {
                return Some(rec);
            }
    }

    None
}

pub fn resolve_pool_account_prefixes(model_str: &str, known_prefixes: &[String]) -> Vec<String> {
    if model_str.is_empty() {
        return Vec::new();
    }
    let model_id = clean_model_name(model_str);
    let clean = model_id
        .to_lowercase()
        .replace("cliproxy/", "")
        .replace("opencode/", "");

    let re = regex::Regex::new(r"(?i)(?:^|[/\\'\\])pool-0?([0-9]+)").unwrap();
    if let Some(caps) = re.captures(&clean)
        && let Some(num_match) = caps.get(1)
        && let Ok(pool_num) = num_match.as_str().parse::<u32>()
    {
        let base_prefix = if pool_num == 1 {
            "agy".to_string()
        } else {
            format!("agy{pool_num}")
        };
        let mut matching = Vec::new();
        for pfx in known_prefixes {
            let p_low = pfx.to_lowercase();
            if pool_num == 1 {
                if p_low == "agy" || p_low.starts_with("agy_") {
                    matching.push(p_low);
                }
            } else if p_low == base_prefix || p_low.starts_with(&format!("{base_prefix}_")) {
                matching.push(p_low);
            }
        }
        if matching.is_empty() {
            matching.push(base_prefix);
        }
        return matching;
    }
    Vec::new()
}

pub fn extract_prefix(agent: &str, model: &str, sorted_prefixes: &[String]) -> Option<String> {
    // 1. Del agente (ej: sdd-orchestrator-xen1, sdd-apply-gianni, sdd-orchestrator-nina)
    if !agent.is_empty() {
        let re_agent = regex::Regex::new(r"(?i)sdd-[a-z0-9]+-([a-z0-9_-]+)").unwrap();
        if let Some(caps) = re_agent.captures(agent)
            && let Some(c_match) = caps.get(1)
        {
            let candidate = c_match.as_str().to_lowercase();
            if sorted_prefixes.is_empty()
                || sorted_prefixes
                    .iter()
                    .any(|p| p.to_lowercase() == candidate)
            {
                return Some(candidate);
            }
        }

        let lower_agent = agent.to_lowercase();
        for pfx in sorted_prefixes {
            let p_low = pfx.to_lowercase();
            let pat_dash = format!("-{p_low}");
            let pat_under = format!("_{p_low}");
            if lower_agent == p_low
                || lower_agent.ends_with(&pat_dash)
                || lower_agent.ends_with(&pat_under)
            {
                return Some(p_low);
            }
        }
    }

    // 2. Del modelo (ej: "nina/gemini-3.8-flash-high", "cliproxy/nina/...")
    if !model.is_empty() {
        let clean_model = clean_model_name(model)
            .to_lowercase()
            .replace("cliproxy/", "")
            .replace("opencode/", "");

        let pool_prefixes = resolve_pool_account_prefixes(&clean_model, sorted_prefixes);
        if let Some(first_pool) = pool_prefixes.into_iter().next() {
            return Some(first_pool);
        }

        let re_model = regex::Regex::new(
            r#"(?i)(?:^|[/"'\\\\])([a-z0-9_-]+)/(?:gemini|claude|antigravity|[a-z0-9.-]+)"#,
        )
        .unwrap();
        if let Some(caps) = re_model.captures(&clean_model)
            && let Some(c_match) = caps.get(1)
        {
            let candidate = c_match.as_str().to_lowercase();
            if candidate != "cliproxy"
                && candidate != "opencode"
                && (sorted_prefixes.is_empty()
                    || sorted_prefixes
                        .iter()
                        .any(|p| p.to_lowercase() == candidate))
            {
                return Some(candidate);
            }
        }

        for pfx in sorted_prefixes {
            let p_low = pfx.to_lowercase();
            if clean_model.starts_with(&format!("{p_low}/"))
                || clean_model.contains(&format!("/{p_low}/"))
                || clean_model.contains(&format!(":{p_low}/"))
                || clean_model.contains(&format!("\"{p_low}/"))
                || clean_model.ends_with(&format!("/{p_low}"))
            {
                return Some(p_low);
            }
        }
    }

    None
}

pub fn get_sdd_subagent_account_prefixes(
    orchestrator_agent: &str,
    target_dir: &str,
    known_prefixes: &[String],
) -> Vec<String> {
    let mut prefixes = Vec::new();
    let candidates = crate::paths::opencode_config_candidates(target_dir);

    for c in candidates {
        if !c.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&c) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        // 1. Tabla explícita en system o prompt (gentle-ai:sdd-model-assignments)
        let agent_sections = [val.get("agent"), val.get("agents")];
        for sec in agent_sections.into_iter().flatten() {
            if let Some(orch) = sec.get(orchestrator_agent) {
                let sys_text = orch
                    .get("system")
                    .or_else(|| orch.get("prompt"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                if let Some(start_idx) = sys_text.find("<!-- gentle-ai:sdd-model-assignments -->")
                    && let Some(end_idx) = sys_text.find("<!-- /gentle-ai:sdd-model-assignments -->")
                {
                    let table_part = &sys_text[start_idx..end_idx];
                    for line in table_part.lines() {
                        if !line.contains('|') {
                            continue;
                        }
                        let cols: Vec<&str> = line.split('|').map(|col| col.trim()).collect();
                        if cols.len() >= 3
                            && !cols[1].is_empty()
                            && !cols[1].starts_with("Phase")
                            && !cols[1].starts_with("---")
                        {
                            let model_str = cols[2];
                            let clean = model_str
                                .replace("cliproxy/", "")
                                .replace("opencode/", "");
                            if let Some(slash_idx) = clean.find('/') {
                                let pfx = clean[..slash_idx].to_lowercase();
                                if !prefixes.contains(&pfx) {
                                    prefixes.push(pfx);
                                }
                            }
                        }
                    }
                }
            }

            // 2. Mapeo de subagentes por prefijo en la sección agents
            if prefixes.is_empty() {
                let agent_prefix = orchestrator_agent
                    .strip_prefix("sdd-orchestrator-")
                    .map(|s| s.to_lowercase());

                if let Some(obj) = sec.as_object() {
                    for (name, def) in obj {
                        let lower = name.to_lowercase();
                        let is_match = if let Some(ref pfx) = agent_prefix {
                            lower.starts_with("sdd-")
                                && lower.ends_with(&format!("-{pfx}"))
                                && !lower.contains("orchestrator")
                        } else {
                            lower.starts_with("sdd-")
                                && !lower.contains("orchestrator")
                                && !lower.contains('-')
                        };

                        if is_match {
                            let model_str = def
                                .get("model")
                                .and_then(|m| m.as_str().or_else(|| m.get("id").and_then(|i| i.as_str())))
                                .unwrap_or("");
                            let clean = model_str
                                .replace("cliproxy/", "")
                                .replace("opencode/", "");
                            if let Some(slash_idx) = clean.find('/') {
                                let pfx = clean[..slash_idx].to_lowercase();
                                if !prefixes.contains(&pfx) {
                                    prefixes.push(pfx);
                                }
                            }
                        }
                    }
                }
            }
        }
        if !prefixes.is_empty() {
            break;
        }
    }

    // Filtrar solo los que realmente pertenecen a cuentas conocidas
    prefixes
        .into_iter()
        .filter(|p| {
            known_prefixes.is_empty()
                || known_prefixes.iter().any(|k| k.to_lowercase() == *p)
        })
        .collect()
}

pub fn read_opencode_session(
    target_dir: &str,
    herdr_panes: &[HerdrPane],
    accounts_list: &[AccountQuota],
    my_pane_id: Option<&str>,
    my_tab_id: Option<&str>,
) -> OpenCodeSession {
    let default_model = "gemini-3.8-flash-high".to_string();
    let default_limit = resolve_model_context_limit(&default_model);
    let mut session = OpenCodeSession {
        agent: "sdd-orchestrator".to_string(),
        model: default_model,
        model_short: "gemini-3.8-flash-h".to_string(),
        context_limit: default_limit,
        agent_status: "idle".to_string(),
        ..Default::default()
    };

    let db_path = resolve_opencode_db_path();
    if !db_path.exists() {
        return session;
    }

    let conn = match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(c) => c,
        Err(_) => return session,
    };

    let _ = conn.busy_timeout(std::time::Duration::from_millis(1500));

    let norm_target = target_dir
        .replace('\\', "/")
        .to_lowercase()
        .trim_end_matches('/')
        .to_string();

    let mut known_prefixes: Vec<String> = accounts_list
        .iter()
        .map(|a| a.prefix.to_lowercase())
        .collect();
    if known_prefixes.is_empty() {
        let mut discovered = Vec::new();
        crate::quotas::QuotaCollector::discover_accounts(&mut discovered);
        known_prefixes = discovered
            .into_iter()
            .map(|a| a.prefix.to_lowercase())
            .collect();
    }
    known_prefixes.sort_by_key(|a| std::cmp::Reverse(a.len()));

    // 1. Recolectar TODAS las cuentas activas desde los panes vivos de Herdr
    for p in herdr_panes {
        if let Some(s) = resolve_session_for_pane(&conn, p) {
            let is_alive = s.time_archived.is_none()
                || (s.time_updated.unwrap_or(0) > s.time_archived.unwrap_or(0))
                || p.agent_status.as_deref() == Some("working")
                || (my_tab_id.is_some() && p.tab_id.as_deref() == my_tab_id);

            if is_alive {
                if let Some(pfx) = extract_prefix(
                    s.agent.as_deref().unwrap_or(""),
                    s.model.as_deref().unwrap_or(""),
                    &known_prefixes,
                ) {
                    session.all_active_prefixes.insert(pfx);
                }

                let model_str = s.model.as_deref().unwrap_or("");
                let pool_prefixes = resolve_pool_account_prefixes(model_str, &known_prefixes);
                for pp in pool_prefixes {
                    session.all_active_prefixes.insert(pp);
                }
            }
        }
    }

    // 2. Determinar el pane / sesión prioritaria para este espacio
    let is_target_pane = |p: &HerdrPane| -> bool {
        p.agent_session_id.is_some()
            || p.agent.as_deref() == Some("opencode")
            || p.cwd.as_deref().map(|c| {
                c.replace('\\', "/")
                    .to_lowercase()
                    .trim_end_matches('/')
                    .to_string()
            }) == Some(norm_target.clone())
    };

    let sibling_pane = my_tab_id.and_then(|tid| {
        herdr_panes.iter().find(|p| {
            p.tab_id.as_deref() == Some(tid)
                && my_pane_id.map(|mp| mp != p.pane_id).unwrap_or(true)
                && is_target_pane(p)
        })
    });
    let focused_pane = herdr_panes.iter().find(|p| p.focused && is_target_pane(p));
    let working_pane = herdr_panes
        .iter()
        .find(|p| p.agent_status.as_deref() == Some("working") && is_target_pane(p));
    let current_cwd_pane = herdr_panes.iter().find(|p| {
        p.cwd.as_deref().map(|c| {
            c.replace('\\', "/")
                .to_lowercase()
                .trim_end_matches('/')
                .to_string()
        }) == Some(norm_target.clone())
            && is_target_pane(p)
    });

    let primary_pane = sibling_pane
        .or(focused_pane)
        .or(working_pane)
        .or(current_cwd_pane);

    let mut target_session_id: Option<String> = None;

    if let Some(pp) = primary_pane {
        if let Some(status) = &pp.agent_status
            && status != "unknown"
        {
            session.agent_status = status.clone();
        }
        if let Some(target_s) = resolve_session_for_pane(&conn, pp) {
            target_session_id = Some(target_s.id.clone());
            if let Some(a) = target_s.agent {
                session.agent = a;
            }
            if let Some(m) = target_s.model {
                let clean = clean_model_name(&m);
                session.model = clean.clone();
                session.model_short = clean.split('/').next_back().unwrap_or(&clean).to_string();
                session.context_limit = resolve_model_context_limit(&clean);
            }
            if let Some(ti) = target_s.tokens_input {
                session.tokens_input = ti;
            }
            if let Some(to) = target_s.tokens_output {
                session.tokens_output = to;
            }
            if let Some(c) = target_s.cost {
                session.cost = c;
            }
        }
    }

    // Fallback: si no se encontró sesión a través de los panes, consultar la sesión activa más reciente en DB
    if target_session_id.is_none()
        && let Ok(mut stmt) = conn.prepare(
            "SELECT id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost FROM session_v2 WHERE (time_archived IS NULL OR (time_updated IS NOT NULL AND time_archived IS NOT NULL AND time_updated > time_archived)) ORDER BY time_updated DESC LIMIT 30"
        )
            && let Ok(rows) = stmt.query_map([], |row| {
                Ok(DbSessionRecord {
                    id: row.get(0)?,
                    agent: row.get(1)?,
                    model: row.get(2)?,
                    directory: row.get(3)?,
                    title: row.get(4)?,
                    time_updated: row.get(5)?,
                    time_archived: row.get(6)?,
                    tokens_input: row.get(7)?,
                    tokens_output: row.get(8)?,
                    cost: row.get(9)?,
                })
            }) {
                let sessions: Vec<DbSessionRecord> = rows.flatten().collect();
                // 1) Coincidencia por directorio
                for s in &sessions {
                    if let Some(ref d) = s.directory {
                        let d_clean = d.replace('\\', "/").to_lowercase().trim_end_matches('/').to_string();
                        if d_clean == norm_target || norm_target.ends_with(&format!("/{d_clean}")) || d_clean.ends_with(&format!("/{norm_target}")) {
                            target_session_id = Some(s.id.clone());
                            if let Some(ref a) = s.agent { session.agent = a.clone(); }
                            if let Some(ref m) = s.model {
                                let clean = clean_model_name(m);
                                session.model = clean.clone();
                                session.model_short = clean.split('/').next_back().unwrap_or(&clean).to_string();
                                session.context_limit = resolve_model_context_limit(&clean);
                            }
                            if let Some(ti) = s.tokens_input { session.tokens_input = ti; }
                            if let Some(to) = s.tokens_output { session.tokens_output = to; }
                            if let Some(c) = s.cost { session.cost = c; }
                            break;
                        }
                    }
                }
                // 2) Si aún no hay, la primera de la lista
                if target_session_id.is_none()
                    && let Some(first_s) = sessions.first() {
                        target_session_id = Some(first_s.id.clone());
                        if let Some(ref a) = first_s.agent { session.agent = a.clone(); }
                        if let Some(ref m) = first_s.model {
                            let clean = clean_model_name(m);
                            session.model = clean.clone();
                            session.model_short = clean.split('/').next_back().unwrap_or(&clean).to_string();
                            session.context_limit = resolve_model_context_limit(&clean);
                        }
                        if let Some(ti) = first_s.tokens_input { session.tokens_input = ti; }
                        if let Some(to) = first_s.tokens_output { session.tokens_output = to; }
                        if let Some(c) = first_s.cost { session.cost = c; }
                    }
            }

    // Prefijo de la cuenta local activa
    if let Some(local_pfx) = extract_prefix(&session.agent, &session.model, &known_prefixes) {
        session.local_prefix = Some(local_pfx.clone());
        session.all_active_prefixes.insert(local_pfx);
    }

    // Agregar prefijos de subagentes SDD si es un orquestador
    let sub_prefixes = get_sdd_subagent_account_prefixes(&session.agent, target_dir, &known_prefixes);
    for sp in sub_prefixes {
        session.all_active_prefixes.insert(sp);
    }

    // Si encontramos sesión, calcular tokens y herramientas
    if let Some(ref session_id) = target_session_id {
        let mut modified_files = HashSet::new();
        let mut found_active_tokens = false;

        if let Ok(mut msg_stmt) = conn.prepare(
            "SELECT data FROM session_message WHERE session_id = ? AND type = 'assistant' ORDER BY time_created DESC"
        )
            && let Ok(msg_rows) = msg_stmt.query_map([session_id], |row| {
                row.get::<_, String>(0)
            }) {
                for data_raw in msg_rows.flatten() {
                    let Ok(val) = serde_json::from_str::<serde_json::Value>(&data_raw) else {
                        continue;
                    };

                    // Active turn tokens: input + output + reasoning + cache.read + cache.write
                    if !found_active_tokens
                        && let Some(tokens_obj) = val.get("tokens") {
                            let inp = tokens_obj.get("input").and_then(|v| v.as_i64()).unwrap_or(0);
                            let outp = tokens_obj.get("output").and_then(|v| v.as_i64()).unwrap_or(0);
                            let reasoning = tokens_obj.get("reasoning").and_then(|v| v.as_i64()).unwrap_or(0);
                            let cache_read = tokens_obj
                                .get("cache")
                                .and_then(|c| c.get("read"))
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0);
                            let cache_write = tokens_obj
                                .get("cache")
                                .and_then(|c| c.get("write"))
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0);

                            session.active_turn_tokens = inp + outp + reasoning + cache_read + cache_write;
                            found_active_tokens = true;
                        }

                    // Tool usage in content array
                    if let Some(content_arr) = val.get("content").and_then(|c| c.as_array()) {
                        for item in content_arr {
                            if item.get("type").and_then(|t| t.as_str()) == Some("tool") {
                                session.tool_total += 1;
                                let tool_name = item
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .or_else(|| item.get("tool").and_then(|t| t.as_str()))
                                    .unwrap_or("");
                                let lower = tool_name.to_lowercase();
                                if lower.contains("read") || lower.contains("glob") || lower.contains("grep") {
                                    session.tool_reads += 1;
                                } else if lower.contains("write") || lower.contains("edit") {
                                    session.tool_writes += 1;
                                    if let Some(path_str) = extract_tool_path(item) {
                                        modified_files.insert(path_str);
                                    }
                                } else if lower.contains("bash") || lower.contains("shell") {
                                    session.tool_bash += 1;
                                } else if lower.contains("engram") || lower.contains("mem_") {
                                    session.tool_engram += 1;
                                } else {
                                    session.tool_other += 1;
                                }
                            }
                        }
                    }
                }
            }

        session.attributed_changes = AttributedChanges {
            files_count: modified_files.len(),
            additions: 0,
            deletions: 0,
        };
    }

    session
}
