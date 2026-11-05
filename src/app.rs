use crate::cleanup::release_inactive_sessions;
use crate::engram::read_engram_telemetry;
use crate::git::read_git_telemetry;
use crate::herdr::HerdrClient;
use crate::opencode::read_opencode_session;
use crate::quotas::QuotaCollector;
use crate::state::AppState;
use crate::tree::{read_mcp_servers, read_project_tree};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn refresh_all(app_state: &Arc<RwLock<AppState>>) {
    let (project_dir, accounts_copy) = {
        let state = app_state.read().await;
        (
            state.snapshot.project_dir.clone(),
            state.snapshot.accounts.clone(),
        )
    };

    let panes = HerdrClient::get_panes();
    let (my_pane_id, my_tab_id) = HerdrClient::get_current_pane_and_tab(&panes);

    let dir_clone = project_dir.clone();
    let panes_clone = panes.clone();
    let my_pid_clone = my_pane_id.clone();
    let my_tid_clone = my_tab_id.clone();
    let opencode_fut = tokio::task::spawn_blocking(move || {
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

    let dir_clone = project_dir.clone();
    let tree_fut = tokio::task::spawn_blocking(move || read_project_tree(&dir_clone));

    let dir_clone = project_dir.clone();
    let mcps_fut = tokio::task::spawn_blocking(move || read_mcp_servers(&dir_clone));

    let (opencode_res, engram_res, git_res, tree_res, mcps_res) =
        tokio::join!(opencode_fut, engram_fut, git_fut, tree_fut, mcps_fut);

    {
        let mut state = app_state.write().await;
        if let Ok(oc) = opencode_res {
            state.snapshot.opencode = oc;
        }
        if let Ok(en) = engram_res {
            state.snapshot.engram = en;
        }
        if let Ok(g) = git_res {
            state.snapshot.git = g;
        }
        if let Ok(tr) = tree_res {
            state.snapshot.tree = tr;
        }
        if let Ok(m) = mcps_res {
            state.snapshot.mcps = m;
        }

        // Descubrir cuentas preservando los valores de cuota ya obtenidos
        QuotaCollector::discover_accounts(&mut state.snapshot.accounts);
        state.snapshot.my_pane_id = my_pane_id;
        state.snapshot.my_tab_id = my_tab_id;
    }
}

pub async fn force_refresh(app_state: &Arc<RwLock<AppState>>, interactive: bool) {
    if interactive {
        let mut state = app_state.write().await;
        state.set_flash_notice(Some("+ Refrescando cuotas y contexto...".to_string()), 4000);
    }

    refresh_all(app_state).await;

    let accounts = {
        let state = app_state.read().await;
        state.snapshot.accounts.clone()
    };

    // Actualizar cuotas concurrentemente
    let mut handles = Vec::new();
    for mut acc in accounts {
        handles.push(tokio::spawn(async move {
            QuotaCollector::fetch_quota_for_account(&mut acc).await;
            acc
        }));
    }

    let mut updated_accounts = Vec::new();
    for h in handles {
        if let Ok(acc) = h.await {
            updated_accounts.push(acc);
        }
    }

    if !updated_accounts.is_empty() {
        updated_accounts.sort_by(|a, b| a.prefix.cmp(&b.prefix));
        let mut state = app_state.write().await;
        state.snapshot.accounts = updated_accounts;
        if interactive {
            state.set_flash_notice(Some("+ Cuotas y contexto actualizados".to_string()), 4000);
        }
    } else if interactive {
        let mut state = app_state.write().await;
        state.set_flash_notice(Some("+ Contexto actualizado".to_string()), 4000);
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

    force_refresh(app_state, false).await;

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
            state.snapshot.opencode.all_active_prefixes.clone(),
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
