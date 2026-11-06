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
    let (project_dir, accounts_copy, is_tree_collapsed) = {
        let state = app_state.read().await;
        (
            state.snapshot.project_dir.clone(),
            state.snapshot.accounts.clone(),
            state.collapsed_cards[2],
        )
    };

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
    let git_fut = tokio::task::spawn_blocking(move || read_git_telemetry(&dir_clone));

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

        // Descubrir cuentas preservando los valores de cuota ya obtenidos
        QuotaCollector::discover_accounts(&mut state.snapshot.accounts);
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

pub async fn force_refresh(app_state: &Arc<RwLock<AppState>>, interactive: bool, force_all: bool) {
    if interactive {
        let mut state = app_state.write().await;
        let notice = if QuotaCollector::is_available() {
            "+ Refrescando cuotas y contexto..."
        } else {
            "+ Refrescando contexto..."
        };
        state.set_flash_notice(Some(notice.to_string()), 4000);
    }

    crate::git::clear_git_cache();
    crate::opencode::clear_message_stats_cache();
    crate::pi::clear_pi_tail_cache();
    crate::engram::clear_engram_cache();
    crate::ecosystem::clear_ecosystem_cache();

    refresh_all(app_state).await;

    // Si no hay proveedor de cuotas disponible, evitar llamadas de red innecesarias
    if !QuotaCollector::is_available() {
        if interactive {
            let mut state = app_state.write().await;
            state.set_flash_notice(Some("+ Contexto actualizado".to_string()), 4000);
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
        }
        return;
    }
    let _fetch_guard = FetchGuard;

    let (accounts, active_prefixes) = {
        let state = app_state.read().await;
        (
            state.snapshot.accounts.clone(),
            state.snapshot.agent_session.all_active_prefixes.clone(),
        )
    };

    if accounts.is_empty() {
        if interactive {
            let mut state = app_state.write().await;
            state.set_flash_notice(Some("+ Contexto actualizado".to_string()), 4000);
        }
        return;
    }

    // Estrategia de Refresco Inteligente (Snapshot Inicial / Manual vs Polling Periódico):
    // - Si force_all es true (arranque inicial o tecla 'r'): se consultan todas las cuentas para pintar la foto completa.
    // - Si force_all es false (intervalo periódico de 2m): SOLO se consultan las cuentas activas en uso en alguna sesión
    //   o aquellas recién descubiertas que aún tengan los valores placeholder iniciales.
    let target_accounts: Vec<AccountQuota> = if force_all {
        accounts
    } else {
        accounts
            .into_iter()
            .filter(|a| {
                let is_unfetched = a.pools.is_empty()
                    || (a.reset_5h == "4h 59m" && a.reset_wk == "4d 23h" && !a.has_error);
                active_prefixes.contains(&a.prefix.to_lowercase()) || is_unfetched
            })
            .collect()
    };

    if target_accounts.is_empty() {
        if interactive {
            let mut state = app_state.write().await;
            state.set_flash_notice(Some("+ Contexto actualizado".to_string()), 4000);
        }
        return;
    }

    // Staggering de 3 segundos entre consultas de cuentas con actualización progresiva en la UI
    let mut any_updated = false;
    for (idx, mut acc) in target_accounts.into_iter().enumerate() {
        if idx > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
        QuotaCollector::fetch_quota_for_account(&mut acc).await;
        any_updated = true;

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
    }

    if interactive {
        let mut state = app_state.write().await;
        let msg = if any_updated {
            "+ Cuotas y contexto actualizados"
        } else {
            "+ Contexto actualizado"
        };
        state.set_flash_notice(Some(msg.to_string()), 4000);
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

pub async fn sync_integrations_action(app_state: &Arc<RwLock<AppState>>) {
    {
        let mut state = app_state.write().await;
        state.set_flash_notice(Some(">> Sincronizando integraciones...".to_string()), 6000);
    }

    // 1. Sincronización nativa en Rust (cero dependencias externas)
    let sync_res = crate::sync::sync_integrations_native().await;
    let notice = match sync_res {
        Ok(res) => format!("+ {}", res.message),
        Err(err) => format!("[!] Sync: {err}"),
    };

    force_refresh(app_state, false, true).await;

    let mut state = app_state.write().await;
    state.set_flash_notice(Some(notice), 4000);
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
    let (project_dir, accounts, active_prefixes) = {
        let state = app_state.read().await;
        (
            state.snapshot.project_dir.clone(),
            state.snapshot.accounts.clone(),
            state.snapshot.agent_session.all_active_prefixes.clone(),
        )
    };

    let (_, notice) = tokio::task::spawn_blocking(move || {
        crate::cleanup::migrate_critical_sessions(&project_dir, &accounts, &active_prefixes)
    })
    .await
    .unwrap_or_else(|err| (0, format!("[error] Falló migración: {err}")));

    {
        let mut state = app_state.write().await;
        state.set_flash_notice(Some(notice), 5000);
    }

    refresh_all(app_state).await;
}
