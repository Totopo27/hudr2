//! Telemetría de OpenCode: lee `opencode.db` (solo lectura) para resolver la sesión
//! activa del espacio, su modelo/tokens/herramientas y las cuentas en uso.
//!
//! Submódulos:
//! - [`model_info`]: normalización de nombres de modelo y límites de contexto.
//! - [`prefix`]: atribución de sesiones a cuentas (prefijos y pools).
//! - [`session`]: consultas a `session_v2` y resolución de sesión por pane.

mod model_info;
mod prefix;
mod session;

pub use model_info::{
    clean_model_name, clean_opencode_terminal_title, estimate_tokens_cost,
    extract_model_name_from_val, is_opencode_terminal_title, resolve_model_context_limit,
};
pub use prefix::{extract_prefix, normalize_model_str, resolve_pool_account_prefixes};
pub use session::{
    DbSessionRecord, has_column, hydrate_session_from_messages, paired_message_and_part_tables,
    resolve_session_for_pane, resolve_session_table_name, session_select_columns, table_exists,
};

use crate::models::{AccountQuota, AgentSession, AttributedChanges, HerdrPane};
use crate::paths::resolve_opencode_db_path;
use rusqlite::{Connection, OpenFlags};
use session::{extract_tool_path, recent_alive_sessions};
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Debug)]
struct MessageStatsCacheEntry {
    time_updated: Option<i64>,
    active_turn_tokens: i64,
    tokens_input: Option<i64>,
    tokens_output: Option<i64>,
    cost: Option<f64>,
    last_turn_cost: f64,
    turn_count: usize,
    tool_total: usize,
    tool_reads: usize,
    tool_writes: usize,
    tool_bash: usize,
    tool_engram: usize,
    attributed_changes: AttributedChanges,
}

static MESSAGE_STATS_CACHE: LazyLock<Mutex<HashMap<String, MessageStatsCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn clear_message_stats_cache() {
    if let Ok(mut guard) = MESSAGE_STATS_CACHE.lock() {
        guard.clear();
    }
}

pub fn read_opencode_session(
    target_dir: &str,
    herdr_panes: &[HerdrPane],
    accounts_list: &[AccountQuota],
    my_pane_id: Option<&str>,
    my_tab_id: Option<&str>,
) -> AgentSession {
    let default_model = "gemini-3.8-flash-high".to_string();
    let default_limit = resolve_model_context_limit(&default_model);
    let mut session = AgentSession {
        provider_name: "opencode".to_string(),
        agent: "orchestrator".to_string(),
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

    let norm_target = crate::paths::normalize_path(target_dir);
    let known_prefixes = crate::quotas::sorted_known_prefixes(accounts_list, Some(""));

    // 1. Recolectar TODAS las cuentas activas desde los panes vivos de Herdr
    for p in herdr_panes {
        // Solo considerar panes que realmente estén ejecutando OpenCode o pertenezcan al espacio actual
        let is_agent_pane = p.agent_session_id.is_some()
            || p.agent.as_deref() == Some("opencode")
            || p.agent_status.as_deref() == Some("working")
            || p.terminal_title
                .as_deref()
                .is_some_and(is_opencode_terminal_title)
            || (p.cwd.as_deref().map(crate::paths::normalize_path).as_ref() == Some(&norm_target));

        if !is_agent_pane {
            continue;
        }

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
    let is_same_cwd = |p: &HerdrPane| -> bool {
        p.cwd.as_deref().map(crate::paths::normalize_path).as_ref() == Some(&norm_target)
    };

    let sibling_pane = my_tab_id.and_then(|tid| {
        herdr_panes.iter().find(|p| {
            p.tab_id.as_deref() == Some(tid)
                && my_pane_id.map(|mp| mp != p.pane_id).unwrap_or(true)
                && (p.agent_session_id.is_some() || p.agent.as_deref() == Some("opencode") || is_same_cwd(p))
        })
    });

    // Prioridad para este espacio: panes que pertenezcan exactamente a este cwd (norm_target)
    let cwd_focused = herdr_panes.iter().find(|p| is_same_cwd(p) && p.focused);
    let cwd_working = herdr_panes.iter().find(|p| is_same_cwd(p) && p.agent_status.as_deref() == Some("working"));
    let cwd_any = herdr_panes.iter().find(|p| is_same_cwd(p));

    // Fallback global sólo si ningún pane está en este directorio
    let global_focused = herdr_panes.iter().find(|p| p.focused);
    let global_working = herdr_panes.iter().find(|p| p.agent_status.as_deref() == Some("working"));

    let primary_pane = sibling_pane
        .or(cwd_focused)
        .or(cwd_working)
        .or(cwd_any)
        .or(global_focused)
        .or(global_working);

    let mut target_session_id: Option<String> = None;
    let mut target_time_updated: Option<i64> = None;
    let mut target_raw_model: Option<String> = None;
    let mut target_raw_agent: Option<String> = None;

    if let Some(pp) = primary_pane {
        if let Some(status) = &pp.agent_status
            && status != "unknown"
        {
            session.agent_status = status.clone();
        }
        if let Some(target_s) = resolve_session_for_pane(&conn, pp) {
            target_session_id = Some(target_s.id.clone());
            target_time_updated = target_s.time_updated;
            target_raw_model = target_s.model.clone();
            target_raw_agent = target_s.agent.clone();
            target_s.apply_to(&mut session);
        }
    }

    // Fallback: si no se encontró sesión a través de los panes, consultar la sesión activa más reciente en DB
    if target_session_id.is_none() {
        let sessions = recent_alive_sessions(&conn);
        // 1) Coincidencia por directorio; 2) si aún no hay, la primera de la lista
        let by_dir = sessions.iter().find(|s| {
            s.directory.as_deref().is_some_and(|d| {
                let d_clean = crate::paths::normalize_path(d);
                d_clean == norm_target
                    || norm_target.ends_with(&format!("/{d_clean}"))
                    || d_clean.ends_with(&format!("/{norm_target}"))
            })
        });
        if let Some(s) = by_dir.or(sessions.first()) {
            target_session_id = Some(s.id.clone());
            target_time_updated = s.time_updated;
            target_raw_model = s.model.clone();
            target_raw_agent = s.agent.clone();
            s.apply_to(&mut session);
        }
    }

    // Prefijo de la cuenta local activa
    let agent_to_check = target_raw_agent.as_deref().unwrap_or(&session.agent);
    let model_to_check = target_raw_model.as_deref().unwrap_or(&session.model);
    if let Some(local_pfx) = extract_prefix(agent_to_check, model_to_check, &known_prefixes) {
        session.local_prefix = Some(local_pfx.clone());
        session.all_active_prefixes.insert(local_pfx);
    }
    for pp in resolve_pool_account_prefixes(model_to_check, &known_prefixes) {
        session.all_active_prefixes.insert(pp);
    }

    // Si encontramos sesión, calcular tokens y herramientas
    if let Some(ref session_id) = target_session_id {
        collect_message_stats(&conn, session_id, target_time_updated, &mut session);
    }

    session
}

fn parse_tokens_from_value(val: &serde_json::Value) -> Option<i64> {
    let tokens_obj = val.get("tokens").or_else(|| val.get("usage"))?;
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
    let total = inp + outp + reasoning + cache_read + cache_write;
    if total > 0 { Some(total) } else { None }
}

fn record_tool_item(
    tool_name: &str,
    item: &serde_json::Value,
    session: &mut AgentSession,
    modified_files: &mut HashSet<String>,
) {
    session.tool_total += 1;
    let lower = tool_name.to_lowercase();
    if lower.contains("read") || lower.contains("glob") || lower.contains("grep") {
        session.tool_reads += 1;
    } else if lower.contains("write") || lower.contains("edit") || lower.contains("apply") {
        session.tool_writes += 1;
        if let Some(path_str) = extract_tool_path(item) {
            modified_files.insert(path_str);
        }
    } else if lower.contains("bash") || lower.contains("shell") || lower.contains("exec") || lower.contains("cmd") {
        session.tool_bash += 1;
    } else if lower.contains("engram") || lower.contains("mem_") {
        session.tool_engram += 1;
    }
}

/// Recorre los mensajes del asistente y partes de la sesión para calcular los tokens del turno
/// activo, el uso de herramientas por categoría y los archivos modificados.
/// Compatible con esquemas legacy (`session_v2` + `session_message` / `session_part`)
/// y esquemas modernos de OpenCode 1.19+ (`session` + `message` / `part`).
/// Si time_updated no ha cambiado, reutiliza el resultado cacheado para evitar queries
/// pesadas y deserialización de JSON repetitiva.
fn collect_message_stats(
    conn: &Connection,
    session_id: &str,
    time_updated: Option<i64>,
    session: &mut AgentSession,
) {
    let session_table = resolve_session_table_name(conn);
    let resolved_time_updated = time_updated.or_else(|| {
        let sql = format!("SELECT time_updated FROM {session_table} WHERE id = ?");
        conn.query_row(&sql, [session_id], |row| row.get(0)).ok()
    });

    if let Some(t_up) = resolved_time_updated
        && let Ok(guard) = MESSAGE_STATS_CACHE.lock()
        && let Some(cached) = guard.get(session_id)
        && cached.time_updated == Some(t_up)
    {
        session.active_turn_tokens = cached.active_turn_tokens;
        if let Some(ti) = cached.tokens_input {
            session.tokens_input = ti;
        }
        if let Some(to) = cached.tokens_output {
            session.tokens_output = to;
        }
        if let Some(c) = cached.cost {
            session.cost = c;
        }
        session.last_turn_cost = cached.last_turn_cost;
        session.turn_count = cached.turn_count;
        session.tool_total = cached.tool_total;
        session.tool_reads = cached.tool_reads;
        session.tool_writes = cached.tool_writes;
        session.tool_bash = cached.tool_bash;
        session.tool_engram = cached.tool_engram;
        session.attributed_changes = cached.attributed_changes.clone();
        return;
    }

    let mut modified_files = HashSet::new();
    let mut found_active_tokens = false;
    let mut msg_tokens_in: i64 = 0;
    let mut msg_tokens_out: i64 = 0;
    let mut msg_cost: f64 = 0.0;
    let mut msg_turns: usize = 0;
    let mut msg_last_cost: Option<f64> = None;
    let mut part_tokens_in: i64 = 0;
    let mut part_tokens_out: i64 = 0;
    let mut part_cost: f64 = 0.0;
    let mut part_turns: usize = 0;
    let mut part_last_cost: Option<f64> = None;

    let (msg_table, part_table) = paired_message_and_part_tables(conn, session_table);

    // 1. Consultar tabla de mensajes emparejada con la tabla de sesiones activa
    if let Some(tbl) = msg_table
        && has_column(conn, tbl, "data")
    {
        let (type_filter, sql_filtered_assistant) = if has_column(conn, tbl, "type") {
            ("AND type = 'assistant'", true)
        } else if has_column(conn, tbl, "role") {
            ("AND role = 'assistant'", true)
        } else {
            ("", false)
        };
        let order_col = if has_column(conn, tbl, "time_created") {
            "time_created"
        } else {
            "rowid"
        };
        let sql =
            format!("SELECT data FROM {tbl} WHERE session_id = ? {type_filter} ORDER BY {order_col} DESC");

        if let Ok(mut msg_stmt) = conn.prepare(&sql)
            && let Ok(msg_rows) = msg_stmt.query_map([session_id], |row| row.get::<_, String>(0))
        {
            for data_raw in msg_rows.flatten() {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&data_raw) else {
                    continue;
                };

                if !sql_filtered_assistant
                    && let Some(role) = val
                        .get("role")
                        .or_else(|| val.get("type"))
                        .and_then(|r| r.as_str())
                    && !role.eq_ignore_ascii_case("assistant")
                {
                    continue;
                }

                // Tokens de turno activo y acumulado de sesión desde message.data
                if !found_active_tokens
                    && let Some(toks) = parse_tokens_from_value(&val)
                {
                    session.active_turn_tokens = toks;
                    found_active_tokens = true;
                }

                let mut turn_cost = val.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let mut turn_in = 0;
                let mut turn_out = 0;
                let mut turn_cr = 0;
                let mut turn_cw = 0;

                if let Some(tok_obj) = val.get("tokens").or_else(|| val.get("usage")) {
                    turn_in = tok_obj.get("input").and_then(|v| v.as_i64()).unwrap_or(0);
                    turn_out = tok_obj.get("output").and_then(|v| v.as_i64()).unwrap_or(0)
                        + tok_obj
                            .get("reasoning")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                    turn_cr = tok_obj
                        .get("cache")
                        .and_then(|c| c.get("read"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    turn_cw = tok_obj
                        .get("cache")
                        .and_then(|c| c.get("write"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);

                    msg_tokens_in += turn_in;
                    msg_tokens_out += turn_out;
                }
                msg_turns += 1;

                if turn_cost <= 0.0 && (turn_in > 0 || turn_out > 0 || turn_cr > 0 || turn_cw > 0) {
                    let msg_model = extract_model_name_from_val(&val)
                        .unwrap_or_else(|| session.model.clone());
                    turn_cost = estimate_tokens_cost(&msg_model, turn_in, turn_out, turn_cr, turn_cw);
                }

                if msg_last_cost.is_none() && turn_cost > 0.0 {
                    msg_last_cost = Some(turn_cost);
                }
                msg_cost += turn_cost;

                // Uso de herramientas en contenido estructurado
                if let Some(content_arr) = val.get("content").and_then(|c| c.as_array()) {
                    for item in content_arr {
                        let t = item.get("type").and_then(|t| t.as_str());
                        if t == Some("tool")
                            || t == Some("tool-call")
                            || t == Some("tool_call")
                            || item.get("name").is_some()
                            || item.get("tool").is_some()
                        {
                            let tool_name = item
                                .get("name")
                                .and_then(|n| n.as_str())
                                .or_else(|| item.get("tool").and_then(|t| t.as_str()))
                                .unwrap_or("");
                            record_tool_item(tool_name, item, session, &mut modified_files);
                        }
                    }
                }

                // Uso de herramientas en arrays alternativos (tool_calls / tools)
                if let Some(tools_arr) = val
                    .get("tool_calls")
                    .or_else(|| val.get("tools"))
                    .and_then(|t| t.as_array())
                {
                    for item in tools_arr {
                        let tool_name = item
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .or_else(|| item.get("name").and_then(|n| n.as_str()))
                            .unwrap_or("");
                        record_tool_item(tool_name, item, session, &mut modified_files);
                    }
                }
            }
        }
    }

    // 2. Consultar tabla de partes emparejada (`part` o `session_part`), ordenada de más reciente a más antigua
    if let Some(ptbl) = part_table
        && has_column(conn, ptbl, "data")
    {
        let type_expr = if has_column(conn, ptbl, "type") {
            "type"
        } else {
            "'unknown' AS type"
        };
        let type_filter = if has_column(conn, ptbl, "type") {
            "AND type IN ('tool', 'tool-call', 'tool_call', 'step-finish', 'finish', 'tokens')"
        } else {
            ""
        };
        let order_col = if has_column(conn, ptbl, "time_created") {
            "time_created"
        } else {
            "rowid"
        };
        let psql = format!(
            "SELECT {type_expr}, data FROM {ptbl} WHERE session_id = ? {type_filter} ORDER BY {order_col} DESC"
        );

        if let Ok(mut part_stmt) = conn.prepare(&psql)
            && let Ok(part_rows) = part_stmt.query_map([session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
        {
            for (part_type, data_raw) in part_rows.flatten() {
                let Ok(val) = serde_json::from_str::<serde_json::Value>(&data_raw) else {
                    continue;
                };
                let effective_type = if part_type == "unknown" {
                    val.get("type").and_then(|t| t.as_str()).unwrap_or("")
                } else {
                    part_type.as_str()
                };

                let is_step_tokens = effective_type == "step-finish"
                    || effective_type == "finish"
                    || effective_type == "tokens"
                    || val.get("tokens").is_some()
                    || val.get("usage").is_some();

                if is_step_tokens {
                    if !found_active_tokens
                        && let Some(toks) = parse_tokens_from_value(&val)
                    {
                        session.active_turn_tokens = toks;
                        found_active_tokens = true;
                    }
                    let mut turn_cost = val.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let mut turn_in = 0;
                    let mut turn_out = 0;
                    let mut turn_cr = 0;
                    let mut turn_cw = 0;

                    if let Some(tok_obj) = val.get("tokens").or_else(|| val.get("usage")) {
                        turn_in = tok_obj.get("input").and_then(|v| v.as_i64()).unwrap_or(0);
                        turn_out = tok_obj.get("output").and_then(|v| v.as_i64()).unwrap_or(0)
                            + tok_obj
                                .get("reasoning")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0);
                        turn_cr = tok_obj
                            .get("cache")
                            .and_then(|c| c.get("read"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        turn_cw = tok_obj
                            .get("cache")
                            .and_then(|c| c.get("write"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);

                        part_tokens_in += turn_in;
                        part_tokens_out += turn_out;
                    }
                    part_turns += 1;

                    if turn_cost <= 0.0 && (turn_in > 0 || turn_out > 0 || turn_cr > 0 || turn_cw > 0) {
                        let part_model = extract_model_name_from_val(&val)
                            .unwrap_or_else(|| session.model.clone());
                        turn_cost = estimate_tokens_cost(&part_model, turn_in, turn_out, turn_cr, turn_cw);
                    }

                    if part_last_cost.is_none() && turn_cost > 0.0 {
                        part_last_cost = Some(turn_cost);
                    }
                    part_cost += turn_cost;
                }

                if effective_type == "tool"
                    || effective_type == "tool-call"
                    || effective_type == "tool_call"
                    || val.get("tool").is_some()
                    || val.get("name").is_some()
                {
                    let tool_name = val
                        .get("name")
                        .and_then(|n| n.as_str())
                        .or_else(|| val.get("tool").and_then(|t| t.as_str()))
                        .unwrap_or("");
                    record_tool_item(tool_name, &val, session, &mut modified_files);
                }
            }
        }
    }

    // 3. Hidratar acumulado de sesión y costo si la tabla `session` no tenía columnas dedicadas o cost era 0.0
    let mut cached_ti = None;
    let mut cached_to = None;
    let mut cached_cost = None;
    if session.tokens_input == 0 && session.tokens_output == 0 {
        let (ti, to) = if msg_tokens_in > 0 || msg_tokens_out > 0 {
            (msg_tokens_in, msg_tokens_out)
        } else {
            (part_tokens_in, part_tokens_out)
        };
        if ti > 0 || to > 0 {
            session.tokens_input = ti;
            session.tokens_output = to;
            cached_ti = Some(ti);
            cached_to = Some(to);
        }
    }

    if session.cost == 0.0 {
        let resolved_cost = if msg_cost > 0.0 { msg_cost } else { part_cost };
        if resolved_cost > 0.0 {
            session.cost = resolved_cost;
            cached_cost = Some(resolved_cost);
        } else if session.tokens_input > 0 || session.tokens_output > 0 {
            let fallback_cost =
                estimate_tokens_cost(&session.model, session.tokens_input, session.tokens_output, 0, 0);
            if fallback_cost > 0.0 {
                session.cost = fallback_cost;
                cached_cost = Some(fallback_cost);
            }
        }
    }

    session.turn_count = if msg_turns > 0 {
        msg_turns
    } else if part_turns > 0 {
        part_turns
    } else if session.tokens_input > 0 || session.tokens_output > 0 {
        1
    } else {
        0
    };

    session.last_turn_cost = msg_last_cost.or(part_last_cost).unwrap_or(0.0);
    if session.last_turn_cost == 0.0 && session.turn_count > 0 && session.cost > 0.0 {
        session.last_turn_cost = session.cost / (session.turn_count as f64);
    }

    // 4. Fallback de tokens de turno: si las tablas de mensajes/partes no proveyeron el desglose,
    // utilizar el acumulado persistido directamente en el registro de sesión de OpenCode
    if (!found_active_tokens || session.active_turn_tokens == 0)
        && (session.tokens_input > 0 || session.tokens_output > 0)
    {
        session.active_turn_tokens = session.tokens_input + session.tokens_output;
    }

    session.attributed_changes = AttributedChanges {
        files_count: modified_files.len(),
        additions: 0,
    };

    if let Ok(mut guard) = MESSAGE_STATS_CACHE.lock() {
        if guard.len() >= 64 {
            guard.clear();
        }
        guard.insert(
            session_id.to_string(),
            MessageStatsCacheEntry {
                time_updated: resolved_time_updated,
                active_turn_tokens: session.active_turn_tokens,
                tokens_input: cached_ti,
                tokens_output: cached_to,
                cost: cached_cost,
                last_turn_cost: session.last_turn_cost,
                turn_count: session.turn_count,
                tool_total: session.tool_total,
                tool_reads: session.tool_reads,
                tool_writes: session.tool_writes,
                tool_bash: session.tool_bash,
                tool_engram: session.tool_engram,
                attributed_changes: session.attributed_changes.clone(),
            },
        );
    }
}
