use crate::herdr::HerdrClient;
use crate::models::AccountQuota;
use crate::opencode::{
    clean_model_name, extract_prefix, is_opencode_terminal_title, normalize_model_str,
    resolve_pool_account_prefixes, resolve_session_for_pane,
};
use crate::paths::resolve_opencode_db_path;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SUBPROCESS_TIMEOUT: Duration = Duration::from_millis(1500);

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

fn configure_silent_command(cmd: &mut Command) {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

fn wait_children_bounded(children: &mut [Child], timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let mut any_running = false;
        for child in children.iter_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                any_running = true;
            }
        }
        if !any_running {
            return;
        }
        if Instant::now() >= deadline {
            for child in children.iter_mut() {
                if matches!(child.try_wait(), Ok(None)) {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn run_command_bounded(mut cmd: Command, timeout: Duration) {
    configure_silent_command(&mut cmd);
    if let Ok(child) = cmd.spawn() {
        wait_children_bounded(&mut [child], timeout);
    }
}

fn notify_opencode_session_migration(ses_id: &str, to_agent: &str, to_model_id: &str) {
    let mut agent_cmd = opencode_command();
    agent_cmd.args([
        "api",
        "post",
        &format!("/api/session/{ses_id}/agent"),
        "-d",
        &format!("{{\"agent\":\"{to_agent}\"}}"),
    ]);
    configure_silent_command(&mut agent_cmd);

    let mut model_cmd = opencode_command();
    model_cmd.args([
        "api",
        "post",
        &format!("/api/session/{ses_id}/model"),
        "-d",
        &format!("{{\"model\":{{\"id\":\"{to_model_id}\",\"providerID\":\"cliproxy\"}}}}"),
    ]);
    configure_silent_command(&mut model_cmd);

    let mut children = Vec::with_capacity(2);
    if let Ok(c) = agent_cmd.spawn() {
        children.push(c);
    }
    if let Ok(c) = model_cmd.spawn() {
        children.push(c);
    }
    if !children.is_empty() {
        wait_children_bounded(&mut children, SUBPROCESS_TIMEOUT);
    }
}

fn collect_sorted_known_prefixes(accounts_list: &[AccountQuota]) -> Vec<String> {
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

fn rewrite_migrated_agent(
    current_agent: Option<&str>,
    from_prefix: &str,
    target_prefix: &str,
) -> String {
    let agent = current_agent.map(str::trim).unwrap_or("");
    if agent.is_empty() {
        return format!("sdd-orchestrator-{target_prefix}");
    }
    if agent.eq_ignore_ascii_case(from_prefix) {
        return target_prefix.to_string();
    }
    for sep in ['-', '_'] {
        let suffix = format!("{sep}{from_prefix}");
        if agent.len() > suffix.len() {
            let split_idx = agent.len() - suffix.len();
            if agent
                .get(split_idx..)
                .is_some_and(|tail| tail.eq_ignore_ascii_case(&suffix))
            {
                return format!("{}{sep}{target_prefix}", &agent[..split_idx]);
            }
        }
    }
    agent.to_string()
}

fn extract_base_model_id(
    raw_model: &str,
    from_prefix: &str,
    allow_any_prefix: bool,
) -> Option<String> {
    let trimmed = raw_model.trim();
    if trimmed.is_empty() {
        return None;
    }
    let cleaned = clean_model_name(trimmed);
    let normalized = normalize_model_str(cleaned.trim());
    if normalized.is_empty() {
        return None;
    }
    if let Some((first_seg, rest)) = normalized.split_once('/') {
        let rest = rest.trim();
        if rest.is_empty() {
            return None;
        }
        if first_seg.eq_ignore_ascii_case(from_prefix)
            || first_seg.to_ascii_lowercase().starts_with("pool-")
            || allow_any_prefix
        {
            return Some(rest.to_string());
        }
        None
    } else {
        Some(normalized)
    }
}

fn rewrite_migrated_model(
    current_model: Option<&str>,
    from_prefix: &str,
    target_prefix: &str,
    fallback_active_model: &str,
) -> (String, String) {
    let provider_id = current_model
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|v| {
            v.get("providerID")
                .and_then(|p| p.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "cliproxy".to_string());

    let base_model = current_model
        .and_then(|m| extract_base_model_id(m, from_prefix, false))
        .or_else(|| extract_base_model_id(fallback_active_model, from_prefix, true))
        .unwrap_or_else(|| "gemini-3.8-flash-high".to_string());

    let to_model_id = format!("{target_prefix}/{base_model}");
    let to_model_json = serde_json::json!({
        "id": to_model_id,
        "providerID": provider_id
    })
    .to_string();

    (to_model_id, to_model_json)
}

fn match_critical_prefix_for_session<'a>(
    agent_opt: Option<&str>,
    model_opt: Option<&str>,
    known_prefixes: &[String],
    critical_accounts: &[&'a AccountQuota],
) -> Option<&'a str> {
    let agent_str = agent_opt.unwrap_or("");
    let model_str = model_opt.unwrap_or("");
    if let Some(resolved) = extract_prefix(agent_str, model_str, known_prefixes)
        && let Some(ca) = critical_accounts
            .iter()
            .find(|a| a.prefix.eq_ignore_ascii_case(&resolved))
    {
        return Some(ca.prefix.as_str());
    }
    for pp in resolve_pool_account_prefixes(model_str, known_prefixes) {
        if let Some(ca) = critical_accounts
            .iter()
            .find(|a| a.prefix.eq_ignore_ascii_case(&pp))
        {
            return Some(ca.prefix.as_str());
        }
    }
    None
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
    get_best_healthy_target_prefix_for_model(accounts_list, active_prefixes, "")
}

pub fn get_best_healthy_target_prefix_for_model(
    accounts_list: &[AccountQuota],
    active_prefixes: &HashSet<String>,
    active_model: &str,
) -> String {
    let compute_health_score = |acc: &AccountQuota| -> i32 {
        let eff = crate::quotas::QuotaManager::evaluate_account_for_model(acc, active_model);
        let rem_5h = eff.rem_5h as i32;
        let rem_wk = eff.rem_wk as i32;
        if rem_wk < 10 {
            return 0;
        }
        ((rem_5h as f64 * 0.7) + (rem_wk as f64 * 0.3)).round() as i32
    };

    // 1. Si hay alguna cuenta activa que esté sana (> 30% libre y sin error), preferir esa
    for p in active_prefixes {
        if let Some(acc) = accounts_list.iter().find(|a| {
            let eff = crate::quotas::QuotaManager::evaluate_account_for_model(a, active_model);
            a.prefix.to_lowercase() == *p && !eff.is_critical && !a.has_error && eff.rem_5h > 30
        }) {
            return acc.prefix.clone();
        }
    }

    // 2. Si no, elegir la cuenta con mayor score de salud balanceado para el modelo activo
    let mut healthy: Vec<&AccountQuota> = accounts_list
        .iter()
        .filter(|a| {
            let eff = crate::quotas::QuotaManager::evaluate_account_for_model(a, active_model);
            !eff.is_critical && !a.has_error
        })
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
    migrate_critical_sessions_for_model(cwd, accounts_list, active_prefixes, "")
}

pub fn migrate_critical_sessions_for_model(
    cwd: &str,
    accounts_list: &[AccountQuota],
    active_prefixes: &HashSet<String>,
    active_model: &str,
) -> (usize, String) {
    let critical_list: Vec<&AccountQuota> = accounts_list
        .iter()
        .filter(|a| {
            let eff = crate::quotas::QuotaManager::evaluate_account_for_model(a, active_model);
            (eff.is_critical || (100u8.saturating_sub(eff.rem_5h) >= 85 && !a.has_error))
                && active_prefixes.contains(&a.prefix.to_lowercase())
        })
        .collect();

    if critical_list.is_empty() {
        return (
            0,
            "No hay sesiones activas usando cuentas críticas.".to_string(),
        );
    }

    let target_prefix =
        get_best_healthy_target_prefix_for_model(accounts_list, active_prefixes, active_model);
    let migratable_critical: Vec<&AccountQuota> = critical_list
        .iter()
        .copied()
        .filter(|a| !a.prefix.eq_ignore_ascii_case(&target_prefix))
        .collect();

    if migratable_critical.is_empty() {
        return (
            0,
            "No hay cuentas alternativas sanas disponibles para migrar.".to_string(),
        );
    }

    let db_path = resolve_opencode_db_path();
    if !db_path.exists() {
        return (
            0,
            "[error] Base de datos de OpenCode no encontrada.".to_string(),
        );
    }

    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(err) => return (0, format!("[error] Error abriendo BD: {err}")),
    };
    let _ = conn.busy_timeout(Duration::from_millis(3000));

    let known_prefixes = collect_sorted_known_prefixes(accounts_list);
    let mut total_migrated = 0;
    let mut migrated_from = Vec::new();

    // Obtener sesiones no archivadas
    let mut stmt =
        match conn.prepare("SELECT id, agent, model FROM session_v2 WHERE time_archived IS NULL") {
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

    for (ses_id, agent_opt, model_opt) in &session_rows {
        let Some(from_prefix) = match_critical_prefix_for_session(
            agent_opt.as_deref(),
            model_opt.as_deref(),
            &known_prefixes,
            &migratable_critical,
        ) else {
            continue;
        };

        let to_agent = rewrite_migrated_agent(agent_opt.as_deref(), from_prefix, &target_prefix);
        let (to_model_id, to_model_json) = rewrite_migrated_model(
            model_opt.as_deref(),
            from_prefix,
            &target_prefix,
            active_model,
        );

        let updated = conn.execute(
            "UPDATE session_v2 SET agent = ?, model = ? WHERE id = ?",
            rusqlite::params![to_agent, to_model_json, ses_id],
        );
        if let Ok(changes) = updated
            && changes > 0
        {
            total_migrated += changes;
            let from_owned = from_prefix.to_string();
            if !migrated_from.contains(&from_owned) {
                migrated_from.push(from_owned);
            }

            notify_opencode_session_migration(ses_id, &to_agent, &to_model_id);
        }
    }

    if total_migrated > 0 {
        let from_str = migrated_from.join(", ");
        let notice = format!(
            "[ok] {total_migrated} sesion(es) migradas en vivo de [{from_str}] a {target_prefix}!"
        );

        // Guardar checkpoint en Engram (acotado con timeout y sin ventana en Windows)
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
            let mut cmd = Command::new(&cand);
            cmd.args([
                "save",
                &title,
                &content,
                "--type",
                "decision",
                "--project",
                &engram_project,
            ]);
            run_command_bounded(cmd, SUBPROCESS_TIMEOUT);
        }

        (total_migrated, notice)
    } else {
        (
            0,
            format!(
                "No se encontraron sesiones abiertas usando [{}]",
                migratable_critical
                    .iter()
                    .map(|c| c.prefix.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
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

    let known_prefixes = collect_sorted_known_prefixes(accounts_list);

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
                        .is_some_and(is_opencode_terminal_title)
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
                .is_some_and(is_opencode_terminal_title);

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

        // Trazabilidad en Engram de la limpieza (acotada con timeout y sin ventana en Windows)
        if let Some(cand) = crate::paths::resolve_engram_binary() {
            let title =
                format!("Limpieza de sesiones huérfanas: {engram_project} [{released_list_str}]");
            let content = format!(
                "**What**: Cierre y archivado limpio de {closed_count} sesión(es) huérfana(s) en desuso.\n**Cuentas liberadas**: {released_list_str}\n**Why**: Deselección manual vía tecla 'd' para liberar pines en HUD y evitar saturación concurrente.\n**Where**: {cwd}"
            );
            let mut cmd = Command::new(&cand);
            cmd.args([
                "save",
                &title,
                &content,
                "--type",
                "decision",
                "--project",
                &engram_project,
            ]);
            run_command_bounded(cmd, SUBPROCESS_TIMEOUT);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_migrated_agent_preserves_role_and_non_prefixed_agents() {
        assert_eq!(
            rewrite_migrated_agent(Some("sdd-orchestrator-agy1"), "agy1", "agy2"),
            "sdd-orchestrator-agy2"
        );
        assert_eq!(
            rewrite_migrated_agent(Some("sdd-apply-AGY1"), "agy1", "agy2"),
            "sdd-apply-agy2"
        );
        assert_eq!(
            rewrite_migrated_agent(Some("worker_agy_1"), "agy_1", "agy2"),
            "worker_agy2"
        );
        assert_eq!(
            rewrite_migrated_agent(Some("build"), "agy1", "agy2"),
            "build"
        );
        assert_eq!(
            rewrite_migrated_agent(None, "agy1", "agy2"),
            "sdd-orchestrator-agy2"
        );
    }

    #[test]
    fn rewrite_migrated_model_preserves_base_model_and_provider() {
        let (id1, json1) = rewrite_migrated_model(
            Some(r#"{"id":"agy1/claude-sonnet-4-6","providerID":"cliproxy"}"#),
            "agy1",
            "agy2",
            "",
        );
        assert_eq!(id1, "agy2/claude-sonnet-4-6");
        assert!(json1.contains(r#""id":"agy2/claude-sonnet-4-6""#));
        assert!(json1.contains(r#""providerID":"cliproxy""#));

        let (id2, _) = rewrite_migrated_model(
            Some("cliproxy/AGY1/gpt-oss-120b-medium"),
            "agy1",
            "agy3",
            "",
        );
        assert_eq!(id2, "agy3/gpt-oss-120b-medium");

        let (id3, _) = rewrite_migrated_model(None, "agy1", "agy2", "agy1/deepseek-r1");
        assert_eq!(id3, "agy2/deepseek-r1");

        let (id4, _) = rewrite_migrated_model(None, "agy1", "agy2", "");
        assert_eq!(id4, "agy2/gemini-3.8-flash-high");
    }

    #[test]
    fn match_critical_prefix_respects_prefix_boundaries() {
        let ca = AccountQuota {
            prefix: "agy".to_string(),
            is_critical: true,
            ..Default::default()
        };
        let critical = vec![&ca];
        let known = vec!["agy2".to_string(), "agy_1".to_string(), "agy".to_string()];

        // Healthy sibling accounts sharing substring "agy" must never be clobbered
        assert_eq!(
            match_critical_prefix_for_session(
                Some("sdd-orchestrator-agy2"),
                Some(r#"{"id":"agy2/claude-sonnet-4-6"}"#),
                &known,
                &critical,
            ),
            None
        );
        assert_eq!(
            match_critical_prefix_for_session(
                Some("sdd-orchestrator-agy_1"),
                Some(r#"{"id":"agy_1/claude-sonnet-4-6"}"#),
                &known,
                &critical,
            ),
            None
        );

        // Exact critical account matches cleanly
        assert_eq!(
            match_critical_prefix_for_session(
                Some("sdd-apply-agy"),
                Some(r#"{"id":"agy/claude-sonnet-4-6"}"#),
                &known,
                &critical,
            ),
            Some("agy")
        );
    }
}
