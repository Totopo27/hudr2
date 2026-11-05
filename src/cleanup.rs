use crate::config::resolve_opencode_db_path;
use crate::herdr::HerdrClient;
use crate::models::AccountQuota;
use crate::opencode::{extract_prefix, resolve_pool_account_prefixes, resolve_session_for_pane};
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

static RE_OC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(?:OC|opencode)\s*\|").unwrap());

/// Construye un `Command` para invocar el CLI de OpenCode de forma multiplataforma.
/// En Windows se lanza vía `cmd /c` porque `opencode` suele instalarse como shim
/// `.cmd` (npm), que `CreateProcess` no resuelve directamente.
fn opencode_command() -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/c", "opencode"]);
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("opencode")
    }
}

pub struct ReleaseResult {
    pub closed_count: usize,
    pub released_prefixes: Vec<String>,
    pub notice: String,
}

pub fn get_best_healthy_target_prefix(
    accounts_list: &[AccountQuota],
    active_prefixes: &HashSet<String>,
) -> String {
    let compute_health_score = |acc: &AccountQuota| -> i32 {
        let rem_5h = acc.rem_5h as i32;
        let mut rem_wk = 100i32;
        if let Some(wk_pool) = acc.pools.iter().find(|p| p.label.to_lowercase().contains("wk")) {
            rem_wk = wk_pool.percent as i32;
        }
        if rem_wk < 10 {
            return 0;
        }
        ((rem_5h as f64 * 0.7) + (rem_wk as f64 * 0.3)).round() as i32
    };

    // 1. Si hay alguna cuenta activa que esté sana (> 30% libre y sin error), preferir esa
    for p in active_prefixes {
        if let Some(acc) = accounts_list
            .iter()
            .find(|a| a.prefix.to_lowercase() == *p && !a.is_critical && !a.has_error && a.rem_5h > 30)
        {
            return acc.prefix.clone();
        }
    }

    // 2. Si no, elegir la cuenta con mayor score de salud balanceado
    let mut healthy: Vec<&AccountQuota> = accounts_list
        .iter()
        .filter(|a| !a.is_critical && !a.has_error)
        .collect();

    healthy.sort_by_key(|a| std::cmp::Reverse(compute_health_score(a)));

    healthy
        .first()
        .map(|a| a.prefix.clone())
        .or_else(|| accounts_list.first().map(|a| a.prefix.clone()))
        .unwrap_or_else(|| "default".to_string())
}

pub fn migrate_critical_sessions(
    cwd: &str,
    accounts_list: &[AccountQuota],
    active_prefixes: &HashSet<String>,
) -> (usize, String) {
    let critical_list: Vec<&AccountQuota> = accounts_list
        .iter()
        .filter(|a| (a.is_critical || (100u8.saturating_sub(a.rem_5h) >= 85 && !a.has_error))
            && active_prefixes.contains(&a.prefix.to_lowercase()))
        .collect();

    if critical_list.is_empty() {
        return (0, "No hay sesiones activas usando cuentas críticas.".to_string());
    }

    let target_prefix = get_best_healthy_target_prefix(accounts_list, active_prefixes);
    let to_agent = format!("sdd-orchestrator-{target_prefix}");
    let to_model_id = format!("{target_prefix}/gemini-3.8-flash-high");
    let to_model_json = serde_json::json!({
        "id": to_model_id,
        "providerID": "cliproxy"
    })
    .to_string();

    let db_path = resolve_opencode_db_path();
    if !db_path.exists() {
        return (0, "[error] Base de datos de OpenCode no encontrada.".to_string());
    }

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(err) => return (0, format!("[error] Error abriendo BD: {err}")),
    };
    let _ = conn.busy_timeout(std::time::Duration::from_millis(3000));

    let mut total_migrated = 0;
    let mut migrated_from = Vec::new();

    // Obtener sesiones no archivadas
    let mut stmt = match conn.prepare("SELECT id, agent, model FROM session_v2 WHERE time_archived IS NULL") {
        Ok(s) => s,
        Err(e) => return (0, format!("[error] Error consultando sesiones: {e}")),
    };

    let session_rows: Vec<(String, Option<String>, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .map(|iter| iter.flatten().collect())
        .unwrap_or_default();

    for ca in &critical_list {
        let from_agent = format!("sdd-orchestrator-{}", ca.prefix);
        for (ses_id, agent_opt, model_opt) in &session_rows {
            let mut matches = false;
            if let Some(a) = agent_opt
                && (a == &from_agent || a.to_lowercase().contains(&format!("-{}", ca.prefix.to_lowercase())))
            {
                matches = true;
            }
            if !matches
                && let Some(m) = model_opt
                && m.to_lowercase().contains(&format!("{}/", ca.prefix.to_lowercase()))
            {
                matches = true;
            }

            if matches {
                let updated = conn.execute(
                    "UPDATE session_v2 SET agent = ?, model = ? WHERE id = ?",
                    rusqlite::params![to_agent, to_model_json, ses_id],
                );
                if let Ok(changes) = updated && changes > 0 {
                    total_migrated += changes;
                    if !migrated_from.contains(&ca.prefix) {
                        migrated_from.push(ca.prefix.clone());
                    }

                    // Intentar notificar a la API de OpenCode CLI
                    let _ = opencode_command()
                        .args([
                            "api",
                            "post",
                            &format!("/api/session/{ses_id}/agent"),
                            "-d",
                            &format!("{{\"agent\":\"{to_agent}\"}}"),
                        ])
                        .output();

                    let _ = opencode_command()
                        .args([
                            "api",
                            "post",
                            &format!("/api/session/{ses_id}/model"),
                            "-d",
                            &format!("{{\"model\":{{\"id\":\"{to_model_id}\",\"providerID\":\"cliproxy\"}}}}"),
                        ])
                        .output();
                }
            }
        }
    }

    if total_migrated > 0 {
        let from_str = migrated_from.join(", ");
        let notice = format!("[ok] {total_migrated} sesion(es) migradas en vivo de [{from_str}] a {target_prefix}!");

        // Guardar checkpoint en Engram
        if let Some(cand) = crate::paths::resolve_engram_binary() {
            let engram_project = Path::new(cwd)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_lowercase();
            let title = format!("Checkpoint migración: [{from_str}] -> {target_prefix}");
            let content = format!(
                "**What**: Checkpoint y rotación en frío de {total_migrated} sesión(es) desde [{from_str}] hacia {target_prefix}.\n**Why**: Cuota crítica agotada (>=85% usado) en cuenta(s) saliente(s).\n**Where**: {cwd}"
            );
            let _ = Command::new(&cand)
                .args([
                    "save",
                    &title,
                    &content,
                    "--type",
                    "decision",
                    "--project",
                    &engram_project,
                ])
                .output();
        }

        (total_migrated, notice)
    } else {
        (0, format!("No se encontraron sesiones abiertas usando [{}]", critical_list.iter().map(|c| c.prefix.as_str()).collect::<Vec<_>>().join(", ")))
    }
}

pub fn release_inactive_sessions(
    cwd: &str,
    accounts_list: &[AccountQuota],
    my_pane_id: Option<&str>,
    my_tab_id: Option<&str>,
) -> ReleaseResult {
    let current_session_id = std::env::var("OPENCODE_SESSION_ID").ok();
    let mut closed_count = 0;
    let mut released_prefixes = HashSet::new();

    let engram_project = Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_lowercase();

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

    let panes = HerdrClient::get_panes();

    let db_path = resolve_opencode_db_path();
    if !db_path.exists() {
        return ReleaseResult {
            closed_count: 0,
            released_prefixes: Vec::new(),
            notice: "[error] Base de datos de OpenCode no encontrada.".to_string(),
        };
    }

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(err) => {
            return ReleaseResult {
                closed_count: 0,
                released_prefixes: Vec::new(),
                notice: format!("[error] No se pudo abrir la BD de OpenCode: {err}"),
            };
        }
    };

    let _ = conn.busy_timeout(std::time::Duration::from_millis(3000));

    // Descubrir qué pane / tab es el que tiene el foco activo del usuario
    let focused_pane = panes
        .iter()
        .find(|p| p.focused || my_pane_id.map(|mp| mp == p.pane_id).unwrap_or(false));
    let active_tab_id = focused_pane.and_then(|p| p.tab_id.as_deref()).or(my_tab_id);

    // Obtener la sesión del pane actualmente en foco o en su tab activo
    let mut keep_session_id: Option<String> =
        focused_pane.and_then(|fp| resolve_session_for_pane(&conn, fp).map(|s| s.id));
    if keep_session_id.is_none() && active_tab_id.is_some() {
        let tab_agent_pane = panes.iter().find(|p| {
            p.tab_id.as_deref() == active_tab_id
                && (p.agent.as_deref() == Some("opencode")
                    || p.terminal_title
                        .as_deref()
                        .map(|t| t.starts_with("OC"))
                        .unwrap_or(false)
                    || p.agent_session_id.is_some())
        });
        keep_session_id =
            tab_agent_pane.and_then(|tap| resolve_session_for_pane(&conn, tap).map(|s| s.id));
    }

    if keep_session_id.is_none() && current_session_id.is_some() {
        keep_session_id = current_session_id;
    }

    let now = chrono::Utc::now().timestamp_millis();
    let mut closed_panes: HashSet<String> = HashSet::new();
    let mut closed_workspaces: HashSet<String> = HashSet::new();

    for p in &panes {
        let s = resolve_session_for_pane(&conn, p);
        let ses_id = s
            .as_ref()
            .map(|x| x.id.clone())
            .or_else(|| p.agent_session_id.clone());

        // Proteger el pane/sesión activo actual
        if let Some(ref sid) = ses_id
            && keep_session_id.as_ref() == Some(sid)
        {
            continue;
        }

        if my_pane_id.map(|mp| mp == p.pane_id).unwrap_or(false) {
            continue;
        }

        if active_tab_id.is_some() && p.tab_id.as_deref() == active_tab_id && focused_pane.is_some()
        {
            continue;
        }

        // Proteger las sesiones que están 'working'
        if p.agent_status.as_deref() == Some("working") {
            continue;
        }

        let is_opencode_pane = p.agent.as_deref() == Some("opencode")
            || p.agent_session_id.is_some()
            || p.terminal_title
                .as_deref()
                .map(|t| {
                    let lower = t.to_lowercase();
                    lower.starts_with("oc |") || lower.starts_with("opencode |") || lower == "opencode" || RE_OC.is_match(t)
                })
                .unwrap_or(false);

        if let Some(ref session_rec) = s {
            let _ = conn.execute(
                "UPDATE session_v2 SET time_archived = ? WHERE id = ?",
                rusqlite::params![now, session_rec.id],
            );
            closed_count += 1;

            if let Some(pfx) = extract_prefix(
                session_rec.agent.as_deref().unwrap_or(""),
                session_rec.model.as_deref().unwrap_or(""),
                &known_prefixes,
            ) {
                released_prefixes.insert(pfx);
            }

            let model_str = session_rec.model.as_deref().unwrap_or("");
            let pool_prefixes = resolve_pool_account_prefixes(model_str, &known_prefixes);
            for pp in pool_prefixes {
                released_prefixes.insert(pp);
            }
        }

        // Cerrar en Herdr si es un pane de agente inactivo
        if is_opencode_pane && my_pane_id.map(|mp| mp != p.pane_id).unwrap_or(true) {
            let ok = HerdrClient::close_pane(&p.pane_id);
            if ok {
                closed_panes.insert(p.pane_id.clone());
            }

            // Si todos los panes del workspace están siendo cerrados, cerrar el workspace
            if let Some(ref ws_id) = p.workspace_id {
                let ws_panes: Vec<&crate::models::HerdrPane> = panes
                    .iter()
                    .filter(|x| x.workspace_id.as_ref() == Some(ws_id))
                    .collect();
                let all_other_closed = ws_panes
                    .iter()
                    .all(|x| x.pane_id == p.pane_id || closed_panes.contains(&x.pane_id));
                if all_other_closed
                    && !closed_workspaces.contains(ws_id)
                    && HerdrClient::close_workspace(ws_id)
                {
                    closed_workspaces.insert(ws_id.clone());
                }
            }
        }
    }

    // Archivar también cualquier sesión huérfana en DB que haya quedado en desuso
    if let Ok(mut orphan_stmt) = conn.prepare(
        "SELECT id, agent, model FROM session_v2 WHERE time_archived IS NULL OR (time_updated IS NOT NULL AND time_archived IS NOT NULL AND time_updated > time_archived)",
    ) {
        let rows = orphan_stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        });
        if let Ok(iter) = rows {
            for (orphan_id, agent_opt, model_opt) in iter.flatten() {
                if keep_session_id.as_deref() == Some(&orphan_id) {
                    continue;
                }
                let is_live_working = panes.iter().any(|p| {
                    p.agent_session_id.as_deref() == Some(&orphan_id)
                        && p.agent_status.as_deref() == Some("working")
                });
                if is_live_working {
                    continue;
                }

                if let Ok(affected) = conn.execute(
                    "UPDATE session_v2 SET time_archived = ? WHERE id = ? AND time_archived IS NULL",
                    rusqlite::params![now, orphan_id],
                ) && affected > 0 {
                    closed_count += 1;
                    if let Some(pfx) = extract_prefix(
                        agent_opt.as_deref().unwrap_or(""),
                        model_opt.as_deref().unwrap_or(""),
                        &known_prefixes,
                    ) {
                        released_prefixes.insert(pfx);
                    }
                    let model_str = model_opt.as_deref().unwrap_or("");
                    let pool_prefixes = resolve_pool_account_prefixes(model_str, &known_prefixes);
                    for pp in pool_prefixes {
                        released_prefixes.insert(pp);
                    }
                }
            }
        }
    }

    let mut released_vec: Vec<String> = released_prefixes.into_iter().collect();
    released_vec.sort();

    let notice = if closed_count > 0 {
        let released_list_str = if !released_vec.is_empty() {
            released_vec.join(", ")
        } else {
            "cuentas secundarias".to_string()
        };

        // Trazabilidad en Engram de la limpieza
        if let Some(cand) = crate::paths::resolve_engram_binary() {
            let title =
                format!("Limpieza de sesiones huérfanas: {engram_project} [{released_list_str}]");
            let content = format!(
                "**What**: Cierre y archivado limpio de {closed_count} sesión(es) huérfana(s) en desuso.\n**Cuentas liberadas**: {released_list_str}\n**Why**: Deselección manual vía tecla 'd' para liberar pines en HUD y evitar saturación concurrente.\n**Where**: {cwd}"
            );
            let _ = Command::new(&cand)
                .args([
                    "save",
                    &title,
                    &content,
                    "--type",
                    "decision",
                    "--project",
                    &engram_project,
                ])
                .output();
        }

        format!("[!] {closed_count} sesion(es) archivadas. Liberadas: [{released_list_str}]")
    } else {
        "No había sesiones secundarias para liberar.".to_string()
    };

    ReleaseResult {
        closed_count,
        released_prefixes: released_vec,
        notice,
    }
}
