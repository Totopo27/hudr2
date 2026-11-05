//! Telemetría de OpenCode: lee `opencode.db` (solo lectura) para resolver la sesión
//! activa del espacio, su modelo/tokens/herramientas y las cuentas en uso.
//!
//! Submódulos:
//! - [`model_info`]: normalización de nombres de modelo y límites de contexto.
//! - [`prefix`]: atribución de sesiones a cuentas (prefijos, pools, subagentes SDD).
//! - [`session`]: consultas a `session_v2` y resolución de sesión por pane.

mod model_info;
mod prefix;
mod session;

pub use model_info::{
    clean_model_name, clean_opencode_terminal_title, resolve_model_context_limit,
};
pub use prefix::{
    extract_prefix, get_sdd_subagent_account_prefixes, normalize_model_str,
    resolve_pool_account_prefixes,
};
pub use session::{DbSessionRecord, resolve_session_for_pane};

use crate::config::resolve_opencode_db_path;
use crate::models::{AccountQuota, AttributedChanges, HerdrPane, OpenCodeSession};
use rusqlite::{Connection, OpenFlags};
use session::{extract_tool_path, recent_alive_sessions};
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct MessageStatsCacheEntry {
    session_id: String,
    time_updated: Option<i64>,
    active_turn_tokens: i64,
    tool_total: usize,
    tool_reads: usize,
    tool_writes: usize,
    tool_bash: usize,
    tool_engram: usize,
    tool_other: usize,
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
) -> OpenCodeSession {
    let default_model = "gemini-3.8-flash-high".to_string();
    let default_limit = resolve_model_context_limit(&default_model);
    let mut session = OpenCodeSession {
        provider_name: "opencode".to_string(),
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

    let norm_target = crate::paths::normalize_path(target_dir);
    let known_prefixes = resolve_known_prefixes(accounts_list);

    // 1. Recolectar TODAS las cuentas activas desde los panes vivos de Herdr
    for p in herdr_panes {
        // Solo considerar panes que realmente estén ejecutando OpenCode o pertenezcan al espacio actual
        let is_agent_pane = p.agent_session_id.is_some()
            || p.agent.as_deref() == Some("opencode")
            || p.agent_status.as_deref() == Some("working")
            || p.terminal_title
                .as_deref()
                .map(|t| {
                    let lower = t.to_lowercase();
                    lower.starts_with("oc |")
                        || lower.starts_with("opencode |")
                        || lower == "opencode"
                })
                .unwrap_or(false)
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
    let is_target_pane = |p: &HerdrPane| -> bool {
        p.agent_session_id.is_some()
            || p.agent.as_deref() == Some("opencode")
            || p.cwd.as_deref().map(crate::paths::normalize_path).as_ref() == Some(&norm_target)
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
        p.cwd.as_deref().map(crate::paths::normalize_path).as_ref() == Some(&norm_target)
            && is_target_pane(p)
    });

    let primary_pane = sibling_pane
        .or(focused_pane)
        .or(working_pane)
        .or(current_cwd_pane);

    let mut target_session_id: Option<String> = None;
    let mut target_time_updated: Option<i64> = None;

    if let Some(pp) = primary_pane {
        if let Some(status) = &pp.agent_status
            && status != "unknown"
        {
            session.agent_status = status.clone();
        }
        if let Some(target_s) = resolve_session_for_pane(&conn, pp) {
            target_session_id = Some(target_s.id.clone());
            target_time_updated = target_s.time_updated;
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
            s.apply_to(&mut session);
        }
    }

    // Prefijo de la cuenta local activa
    if let Some(local_pfx) = extract_prefix(&session.agent, &session.model, &known_prefixes) {
        session.local_prefix = Some(local_pfx.clone());
        session.all_active_prefixes.insert(local_pfx);
    }
    for pp in resolve_pool_account_prefixes(&session.model, &known_prefixes) {
        session.all_active_prefixes.insert(pp);
    }

    // Si encontramos sesión, calcular tokens y herramientas
    if let Some(ref session_id) = target_session_id {
        collect_message_stats(&conn, session_id, target_time_updated, &mut session);
    }

    session
}

/// Prefijos de cuenta conocidos en minúsculas, ordenados del más largo al más corto
/// (para que `agy3_1` gane sobre `agy3`). Si no hay cuotas cargadas, los descubre del disco.
fn resolve_known_prefixes(accounts_list: &[AccountQuota]) -> Vec<String> {
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
    known_prefixes
}

/// Recorre los mensajes del asistente de la sesión para calcular los tokens del turno
/// activo, el uso de herramientas por categoría y los archivos modificados.
/// Si time_updated no ha cambiado, reutiliza el resultado cacheado para evitar queries
/// pesadas y deserialización de JSON repetitiva.
fn collect_message_stats(
    conn: &Connection,
    session_id: &str,
    time_updated: Option<i64>,
    session: &mut OpenCodeSession,
) {
    let resolved_time_updated = time_updated.or_else(|| {
        conn.query_row(
            "SELECT time_updated FROM session_v2 WHERE id = ?",
            [session_id],
            |row| row.get(0),
        )
        .ok()
    });

    if let Some(t_up) = resolved_time_updated
        && let Ok(guard) = MESSAGE_STATS_CACHE.lock()
        && let Some(cached) = guard.get(session_id)
        && cached.time_updated == Some(t_up)
    {
        session.active_turn_tokens = cached.active_turn_tokens;
        session.tool_total = cached.tool_total;
        session.tool_reads = cached.tool_reads;
        session.tool_writes = cached.tool_writes;
        session.tool_bash = cached.tool_bash;
        session.tool_engram = cached.tool_engram;
        session.tool_other = cached.tool_other;
        session.attributed_changes = cached.attributed_changes.clone();
        return;
    }

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

    if let Ok(mut guard) = MESSAGE_STATS_CACHE.lock() {
        guard.insert(
            session_id.to_string(),
            MessageStatsCacheEntry {
                session_id: session_id.to_string(),
                time_updated: resolved_time_updated,
                active_turn_tokens: session.active_turn_tokens,
                tool_total: session.tool_total,
                tool_reads: session.tool_reads,
                tool_writes: session.tool_writes,
                tool_bash: session.tool_bash,
                tool_engram: session.tool_engram,
                tool_other: session.tool_other,
                attributed_changes: session.attributed_changes.clone(),
            },
        );
    }
}
