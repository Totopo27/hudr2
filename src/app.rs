use crate::cleanup::release_inactive_sessions;
use crate::engram::read_engram_telemetry;
use crate::git::read_git_telemetry;
use crate::herdr::HerdrClient;
use crate::models::AccountQuota;
use crate::opencode::read_opencode_session;
use crate::quotas::QuotaCollector;
use crate::state::AppState;
use crate::tree::{read_mcp_servers, read_project_tree};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn refresh_all(app_state: &Arc<RwLock<AppState>>) {
    let (project_dir, mut accounts_copy, is_tree_collapsed, cache_path) = {
        let state = app_state.read().await;
        let cp = state
            .config_path
            .as_deref()
            .map(QuotaCollector::quota_cache_path_for);
        (
            state.snapshot.project_dir.clone(),
            state.snapshot.accounts.clone(),
            state.collapsed_cards[2],
            cp,
        )
    };
    QuotaCollector::discover_accounts(&mut accounts_copy);
    QuotaCollector::hydrate_accounts_from_cache(&mut accounts_copy, cache_path.as_deref());

    let panes = HerdrClient::get_panes();
    let (my_pane_id, my_tab_id) = HerdrClient::get_current_pane_and_tab(&panes);

    let dir_clone = project_dir.clone();
    let panes_clone = panes.clone();
    let my_pid_clone = my_pane_id.clone();
    let my_tid_clone = my_tab_id.clone();
    let opencode_fut = tokio::task::spawn_blocking(move || {
        if let Some((_, pi_session)) = crate::pi::try_read_pi_session(
            &dir_clone,
            &panes_clone,
            &accounts_copy,
            my_pid_clone.as_deref(),
            my_tid_clone.as_deref(),
        ) {
            return pi_session;
        }
        read_opencode_session(
            &dir_clone,
            &panes_clone,
            &accounts_copy,
            my_pid_clone.as_deref(),
            my_tid_clone.as_deref(),
        )
    });

    let dir_clone = project_dir.clone();
    let engram_fut = tokio::task::spawn_blocking(move || read_engram_telemetry(&dir_clone));

    let dir_clone = project_dir.clone();
    let panes_for_git = panes.clone();
    let git_fut = tokio::task::spawn_blocking(move || {
        let mut g = read_git_telemetry(&dir_clone);
        g.cross_session_conflicts =
            crate::git::collect_cross_session_conflicts(&dir_clone, &g.files, &panes_for_git);
        g
    });

    let tree_fut = if is_tree_collapsed {
        None
    } else {
        let dir_clone = project_dir.clone();
        Some(tokio::task::spawn_blocking(move || {
            read_project_tree(&dir_clone)
        }))
    };

    let dir_clone = project_dir.clone();
    let mcps_fut = tokio::task::spawn_blocking(move || read_mcp_servers(&dir_clone));

    let eco_fut = tokio::task::spawn_blocking(crate::ecosystem::read_ecosystem_telemetry);

    let (opencode_res, engram_res, git_res, mcps_res, eco_res) =
        tokio::join!(opencode_fut, engram_fut, git_fut, mcps_fut, eco_fut);

    let tree_res = match tree_fut {
        Some(handle) => handle.await.ok(),
        None => None,
    };

    {
        let mut state = app_state.write().await;
        let prev_snapshot = state.snapshot.clone();

        if let Ok(oc) = opencode_res {
            state.snapshot.agent_session = oc;
        }
        if let Ok(en) = engram_res {
            state.snapshot.engram = en;
        }
        if let Ok(g) = git_res {
            if let Some(ref sel_file) = state.selected_diff_file
                && !g.files.iter().any(|f| &f.path == sel_file)
            {
                state.selected_diff_file = None;
                state.selected_diff_lines.clear();
                state.needs_redraw = true;
            }
            state.snapshot.git = g;
        }
        if let Some(tr) = tree_res {
            state.snapshot.tree = tr;
        }
        if let Ok(m) = mcps_res {
            state.snapshot.mcps = m;
        }
        if let Ok(eco) = eco_res {
            state.snapshot.ecosystem = eco;
        }

        // Descubrir cuentas e hidratar desde caché en disco (0ms) cuando hay config_path activo
        QuotaCollector::discover_accounts(&mut state.snapshot.accounts);
        if let Some(ref cp) = cache_path {
            QuotaCollector::hydrate_accounts_from_cache(&mut state.snapshot.accounts, Some(cp));
        }
        let active_model = if !state.snapshot.agent_session.model.is_empty() {
            state.snapshot.agent_session.model.clone()
        } else {
            state.snapshot.agent_session.model_short.clone()
        };
        for acc in &mut state.snapshot.accounts {
            QuotaCollector::sync_account_health_for_model(acc, &active_model);
        }
        state.snapshot.my_pane_id = my_pane_id;
        state.snapshot.my_tab_id = my_tab_id;

        if state.snapshot != prev_snapshot {
            state.needs_redraw = true;
        }
    }
}

use std::sync::atomic::{AtomicBool, Ordering};

static IS_FETCHING_QUOTAS: AtomicBool = AtomicBool::new(false);

struct FetchGuard;
impl Drop for FetchGuard {
    fn drop(&mut self) {
        IS_FETCHING_QUOTAS.store(false, Ordering::SeqCst);
    }
}

/// Selecciona y ordena las cuentas a consultar según prioridad de estado:
/// - Prioridad 0: cuenta activa del espacio actual (`local_prefix`) -> consulta inmediata (`0s`).
/// - Prioridad 1: cuentas activas en otros paneles (`active_prefixes`).
/// - Prioridad 2: cuentas inactivas (en `force_all` todas salvo cooldown 429; en modo selectivo solo nuevas/vencidas).
pub fn select_accounts_for_refresh(
    accounts: &[AccountQuota],
    local_prefix: Option<&str>,
    active_prefixes: &std::collections::HashSet<String>,
    force_all: bool,
    rate_limited: bool,
) -> Vec<AccountQuota> {
    let local_lower = local_prefix.map(str::to_lowercase);
    let mut tiered: Vec<(u8, AccountQuota)> = accounts
        .iter()
        .filter_map(|acc| {
            let p_low = acc.prefix.to_lowercase();
            let tier = if local_lower.as_deref() == Some(p_low.as_str()) {
                0
            } else if active_prefixes.contains(&p_low) {
                1
            } else {
                2
            };

            let include = if tier <= 1 {
                true
            } else if rate_limited {
                false
            } else if force_all {
                true
            } else {
                QuotaCollector::needs_refresh(acc)
            };

            include.then(|| (tier, acc.clone()))
        })
        .collect();

    tiered.sort_by(|(ta, a), (tb, b)| ta.cmp(tb).then_with(|| a.prefix.cmp(&b.prefix)));
    tiered.into_iter().map(|(_, acc)| acc).collect()
}

/// Cuando una cuenta activa entra en zona amarilla (`is_warning`, `<= 30%` libre) o crítica (`<= 15%`),
/// devuelve las mejores candidatas a migración cuya caché aún no sea fresca (`> max_fresh_age_secs`),
/// deteniéndose en cuanto ya existan `target_ready` candidatas frescas y sanas listas.
pub fn select_yellow_zone_prewarm_candidates(
    accounts: &[AccountQuota],
    active_prefixes: &std::collections::HashSet<String>,
    active_model: &str,
    max_fresh_age_secs: i64,
    target_ready: usize,
) -> Vec<AccountQuota> {
    let any_active_pressured = accounts.iter().any(|a| {
        if !active_prefixes.contains(&a.prefix.to_lowercase()) {
            return false;
        }
        let eff = QuotaCollector::evaluate_account_for_model(a, active_model);
        eff.is_warning || eff.is_critical
    });
    if !any_active_pressured {
        return Vec::new();
    }

    let already_ready = accounts
        .iter()
        .filter(|a| {
            if active_prefixes.contains(&a.prefix.to_lowercase()) || a.has_error {
                return false;
            }
            let eff = QuotaCollector::evaluate_account_for_model(a, active_model);
            !eff.is_critical && eff.rem_5h > 30 && QuotaCollector::is_fresh_within(a, max_fresh_age_secs)
        })
        .count();

    if already_ready >= target_ready {
        return Vec::new();
    }

    let ranked_prefixes =
        crate::cleanup::rank_migration_candidates_for_model(accounts, active_prefixes, active_model);
    ranked_prefixes
        .into_iter()
        .filter_map(|pfx| {
            accounts.iter().find(|a| {
                a.prefix.eq_ignore_ascii_case(&pfx)
                    && !QuotaCollector::is_fresh_within(a, max_fresh_age_secs)
            })
        })
        .cloned()
        .collect()
}

pub async fn force_refresh(app_state: &Arc<RwLock<AppState>>, interactive: bool, force_all: bool) {
    force_refresh_with_notify(app_state, interactive, force_all, None).await;
}

pub async fn force_refresh_with_notify(
    app_state: &Arc<RwLock<AppState>>,
    interactive: bool,
    force_all: bool,
    notify: Option<&(dyn Fn() + Send + Sync)>,
) {
    let wake = || {
        if let Some(cb) = notify {
            cb();
        }
    };

    if interactive {
        let mut state = app_state.write().await;
        let notice = if !QuotaCollector::is_available() {
            "+ Refrescando contexto..."
        } else if force_all {
            "+ Auditando todas las cuotas..."
        } else {
            "+ Refrescando cuota activa y contexto..."
        };
        state.set_flash_notice(Some(notice.to_string()), 4500);
        drop(state);
        wake();
    }

    crate::git::clear_git_cache();
    crate::opencode::clear_message_stats_cache();
    crate::pi::clear_pi_tail_cache();
    crate::engram::clear_engram_cache();
    crate::ecosystem::clear_ecosystem_cache();

    refresh_all(app_state).await;
    wake();

    // Si no hay proveedor de cuotas disponible, evitar llamadas de red innecesarias
    if !QuotaCollector::is_available() {
        if interactive {
            let mut state = app_state.write().await;
            state.set_flash_notice(Some("+ Contexto actualizado".to_string()), 4000);
            drop(state);
            wake();
        }
        return;
    }

    // Evitar peticiones concurrentes superpuestas si ya hay un refresco en vuelo
    if IS_FETCHING_QUOTAS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        if interactive {
            let mut state = app_state.write().await;
            state.set_flash_notice(
                Some("+ Contexto actualizado (cuotas en curso...)".to_string()),
                4000,
            );
            drop(state);
            wake();
        }
        return;
    }
    let _fetch_guard = FetchGuard;

    let (accounts, local_prefix, active_prefixes, cache_path) = {
        let state = app_state.read().await;
        (
            state.snapshot.accounts.clone(),
            state.snapshot.agent_session.local_prefix.clone(),
            state.snapshot.agent_session.all_active_prefixes.clone(),
            state
                .config_path
                .as_deref()
                .map(QuotaCollector::quota_cache_path_for),
        )
    };

    if accounts.is_empty() {
        if interactive {
            let mut state = app_state.write().await;
            state.set_flash_notice(Some("+ Contexto actualizado".to_string()), 4000);
            drop(state);
            wake();
        }
        return;
    }

    let target_accounts = select_accounts_for_refresh(
        &accounts,
        local_prefix.as_deref(),
        &active_prefixes,
        force_all,
        QuotaCollector::is_rate_limit_cooling_down(),
    );

    let total = target_accounts.len();
    let mut calls_made = 0usize;
    let mut halted_by_rate_limit = false;
    let mut checked_prefixes = std::collections::HashSet::new();

    for (idx, mut acc) in target_accounts.into_iter().enumerate() {
        let is_active = local_prefix
            .as_deref()
            .is_some_and(|lp| lp.eq_ignore_ascii_case(&acc.prefix))
            || active_prefixes.contains(&acc.prefix.to_lowercase());

        if !is_active && QuotaCollector::is_rate_limit_cooling_down() {
            halted_by_rate_limit = true;
            break;
        }

        if total > 1 {
            let progress_msg = if force_all && interactive {
                format!("+ Auditando cuotas ({}/{total} \u{00b7} {})...", idx + 1, acc.prefix)
            } else {
                format!("+ Cuotas ({}/{total} \u{00b7} {})...", idx + 1, acc.prefix)
            };
            let mut state = app_state.write().await;
            state.check_flash_notice_expiry();
            if interactive || state.flash_notice.is_none() {
                state.set_flash_notice(Some(progress_msg), 5000);
                drop(state);
                wake();
            }
        }

        if calls_made > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }

        checked_prefixes.insert(acc.prefix.to_lowercase());
        QuotaCollector::fetch_quota_for_account_with_cache(&mut acc, cache_path.as_deref()).await;
        calls_made += 1;

        let mut state = app_state.write().await;
        let active_model = if !state.snapshot.agent_session.model.is_empty() {
            state.snapshot.agent_session.model.clone()
        } else {
            state.snapshot.agent_session.model_short.clone()
        };
        QuotaCollector::sync_account_health_for_model(&mut acc, &active_model);
        if let Some(existing) = state
            .snapshot
            .accounts
            .iter_mut()
            .find(|a| a.prefix == acc.prefix || a.auth_file == acc.auth_file)
        {
            *existing = acc;
        }
        state.snapshot.accounts.sort_by(|a, b| a.prefix.cmp(&b.prefix));
        state.needs_redraw = true;
        drop(state);
        wake();
    }

    // Pre-calentamiento silencioso desde Zona Amarilla (`is_warning` <= 30% libre):
    // valida hasta 3 mejores candidatas a migración para que 'x' sea instantáneo al llegar a crítico.
    if !force_all && !halted_by_rate_limit {
        const PREWARM_FRESH_SECS: i64 = 600;
        const PREWARM_TARGET_READY: usize = 3;
        const PREWARM_MAX_CHECKS: usize = 4;
        let mut prewarm_checks = 0usize;

        while prewarm_checks < PREWARM_MAX_CHECKS && !QuotaCollector::is_rate_limit_cooling_down() {
            let next_cand = {
                let state = app_state.read().await;
                let active_model = if !state.snapshot.agent_session.model.is_empty() {
                    state.snapshot.agent_session.model.clone()
                } else {
                    state.snapshot.agent_session.model_short.clone()
                };
                select_yellow_zone_prewarm_candidates(
                    &state.snapshot.accounts,
                    &state.snapshot.agent_session.all_active_prefixes,
                    &active_model,
                    PREWARM_FRESH_SECS,
                    PREWARM_TARGET_READY,
                )
                .into_iter()
                .find(|c| !checked_prefixes.contains(&c.prefix.to_lowercase()))
            };

            let Some(mut cand) = next_cand else {
                break;
            };

            if calls_made > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            checked_prefixes.insert(cand.prefix.to_lowercase());
            prewarm_checks += 1;
            calls_made += 1;

            QuotaCollector::fetch_quota_for_account_with_cache(&mut cand, cache_path.as_deref())
                .await;

            let mut state = app_state.write().await;
            let active_model = if !state.snapshot.agent_session.model.is_empty() {
                state.snapshot.agent_session.model.clone()
            } else {
                state.snapshot.agent_session.model_short.clone()
            };
            QuotaCollector::sync_account_health_for_model(&mut cand, &active_model);
            if let Some(existing) = state
                .snapshot
                .accounts
                .iter_mut()
                .find(|a| a.prefix == cand.prefix || a.auth_file == cand.auth_file)
            {
                *existing = cand;
            }
            state.snapshot.accounts.sort_by(|a, b| a.prefix.cmp(&b.prefix));
            state.needs_redraw = true;
            drop(state);
            wake();
        }
    }

    if interactive {
        let mut state = app_state.write().await;
        let msg = if halted_by_rate_limit {
            "[!] Rate limit detectado: pausa preventiva en inactivas"
        } else if calls_made > 0 {
            if force_all {
                "+ Auditor\u{00ed}a de cuotas y contexto completada"
            } else {
                "+ Cuotas y contexto actualizados"
            }
        } else {
            "+ Contexto actualizado (cuotas al d\u{00ed}a)"
        };
        state.set_flash_notice(Some(msg.to_string()), 4000);
        drop(state);
        wake();
    }
}

pub fn open_folder_in_explorer(target_path: &str) {
    let resolved_path = if target_path.is_empty() || target_path == "." {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string())
    } else {
        target_path.to_string()
    };

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut wide_path: Vec<u16> = std::ffi::OsStr::new(&resolved_path).encode_wide().collect();
        wide_path.push(0);

        let mut wide_op: Vec<u16> = std::ffi::OsStr::new("open").encode_wide().collect();
        wide_op.push(0);

        #[link(name = "shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                lpOperation: *const u16,
                lpFile: *const u16,
                lpParameters: *const u16,
                lpDirectory: *const u16,
                nShowCmd: i32,
            ) -> isize;
        }

        unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                wide_op.as_ptr(),
                wide_path.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1, // SW_SHOWNORMAL
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(&resolved_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(&resolved_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

pub async fn sync_integrations_action_with_notify(
    app_state: &Arc<RwLock<AppState>>,
    notify: Option<&(dyn Fn() + Send + Sync)>,
) {
    let wake = || {
        if let Some(cb) = notify {
            cb();
        }
    };

    {
        let mut state = app_state.write().await;
        state.set_flash_notice(Some(">> Sincronizando integraciones...".to_string()), 6000);
    }
    wake();

    // 1. Sincronización nativa en Rust (~20ms) y aviso inmediato sin bloqueo por cuotas
    let sync_res = crate::sync::sync_integrations_native().await;
    let notice = match sync_res {
        Ok(res) => format!("+ {}", res.message),
        Err(err) => format!("[!] Sync: {err}"),
    };

    refresh_all(app_state).await;

    {
        let mut state = app_state.write().await;
        state.set_flash_notice(Some(notice), 5000);
    }
    wake();
}

pub async fn release_sessions_action(app_state: &Arc<RwLock<AppState>>) {
    let (project_dir, accounts, my_pane_id, my_tab_id) = {
        let state = app_state.read().await;
        (
            state.snapshot.project_dir.clone(),
            state.snapshot.accounts.clone(),
            state.snapshot.my_pane_id.clone(),
            state.snapshot.my_tab_id.clone(),
        )
    };

    let result = tokio::task::spawn_blocking(move || {
        release_inactive_sessions(
            &project_dir,
            &accounts,
            my_pane_id.as_deref(),
            my_tab_id.as_deref(),
        )
    })
    .await
    .unwrap_or_else(|err| crate::cleanup::ReleaseResult {
        closed_count: 0,
        released_prefixes: Vec::new(),
        notice: format!("[error] Falló tarea de liberación: {err}"),
    });

    {
        let mut state = app_state.write().await;
        state.set_flash_notice(Some(result.notice), 4000);
    }

    refresh_all(app_state).await;
}

pub async fn migrate_sessions_action(app_state: &Arc<RwLock<AppState>>) {
    let (project_dir, mut accounts, active_prefixes, active_model, cache_path) = {
        let state = app_state.read().await;
        let model = if !state.snapshot.agent_session.model.is_empty() {
            state.snapshot.agent_session.model.clone()
        } else {
            state.snapshot.agent_session.model_short.clone()
        };
        (
            state.snapshot.project_dir.clone(),
            state.snapshot.accounts.clone(),
            state.snapshot.agent_session.all_active_prefixes.clone(),
            model,
            state
                .config_path
                .as_deref()
                .map(QuotaCollector::quota_cache_path_for),
        )
    };

    // Respaldo puntual: si la candidata #1 no fue pre-verificada en zona amarilla (ej. caída brusca),
    // validar solo esa candidata (máx. 2 intentos) en vez de escanear todas las cuentas.
    for _ in 0..2 {
        let target_pfx = crate::cleanup::get_best_healthy_target_prefix_for_model(
            &accounts,
            &active_prefixes,
            &active_model,
        );
        let Some(idx) = accounts.iter().position(|a| {
            a.prefix.eq_ignore_ascii_case(&target_pfx)
                && !active_prefixes.contains(&a.prefix.to_lowercase())
        }) else {
            break;
        };
        if QuotaCollector::is_fresh_within(&accounts[idx], 600)
            || !std::path::Path::new(&accounts[idx].auth_file).exists()
        {
            break;
        }
        QuotaCollector::fetch_quota_for_account_with_cache(
            &mut accounts[idx],
            cache_path.as_deref(),
        )
        .await;
        QuotaCollector::sync_account_health_for_model(&mut accounts[idx], &active_model);
        let refreshed = accounts[idx].clone();
        {
            let mut state = app_state.write().await;
            if let Some(existing) = state
                .snapshot
                .accounts
                .iter_mut()
                .find(|a| a.prefix == refreshed.prefix || a.auth_file == refreshed.auth_file)
            {
                *existing = refreshed.clone();
            }
            state.needs_redraw = true;
        }
        if !refreshed.has_error && !refreshed.is_critical {
            break;
        }
    }

    let (_, notice) = tokio::task::spawn_blocking(move || {
        crate::cleanup::migrate_critical_sessions_for_model(
            &project_dir,
            &accounts,
            &active_prefixes,
            &active_model,
        )
    })
    .await
    .unwrap_or_else(|err| (0, format!("[error] Falló migración: {err}")));

    {
        let mut state = app_state.write().await;
        state.set_flash_notice(Some(notice), 5000);
    }

    refresh_all(app_state).await;
}
