use hudr2::herdr::{HerdrClient, HerdrWireEvent};
use hudr2::{app, state, ui};

use crossterm::ExecutableCommand;
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::style::Print;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
};
use notify::{RecursiveMode, Watcher};
use std::error::Error;
use std::io::{Write, stdout};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc};

/// Secuencias SGR de ratón "button-only" (1000 + 1006): reporta clics y rueda,
/// pero NUNCA movimiento del cursor (1003), evitando despertar el proceso al mover el mouse.
const MOUSE_ON: &str = "\x1b[?1000h\x1b[?1006h";
const MOUSE_OFF: &str = "\x1b[?1006l\x1b[?1000l";

const HELP_TEXT: &str = "\
hudr2 [--once] [--attach] [--pane <HERDR_PANE_ID>] [--cwd <dir>] [--config <file>]

Sidebar de telemetría reactiva para Herdr/Telar, OpenCode y Pi / Gentle Shell.

Opciones:
  --once             Imprime una captura completa en texto plano y termina de inmediato
  --attach           Abre un split lateral derecho (20%) en Herdr/Telar y lanza hudr2 allí
  --pane <id>        Enlaza el HUD a un pane específico de Herdr (se cierra al cerrarse ese pane)
  --cwd <dir>        Directorio del proyecto a monitorear (por defecto: directorio actual)
  --config <file>    Archivo JSON donde persistir el estado de tarjetas colapsadas
  --theme <name>     Tema visual (auto [adaptativo de Herdr], 5 nativos [linear, cyber, bauhaus, synthwave, editorial] o 13 de Herdr [catppuccin, tokyo-night, dracula, nord, gruvbox, ...])
  --name, --title    Nombre personalizado del sidebar (por defecto: HUDR2)
  -v, --version      Muestra la versión de hudr2
  -h, --help         Muestra esta ayuda";

struct CliArgs {
    once: bool,
    attach: bool,
    pane_id: Option<String>,
    cwd: Option<String>,
    config_path: Option<PathBuf>,
    theme: Option<String>,
    sidebar_name: Option<String>,
    show_help: bool,
    show_version: bool,
}

fn parse_cli_args() -> CliArgs {
    let mut args = std::env::args().skip(1);
    let mut once = false;
    let mut attach = false;
    let mut pane_id = None;
    let mut cwd = None;
    let mut config_path = None;
    let mut theme = None;
    let mut sidebar_name = None;
    let mut show_help = false;
    let mut show_version = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--once" => once = true,
            "--attach" => attach = true,
            "-v" | "--version" | "version" => show_version = true,
            "-h" | "--help" => show_help = true,
            "--pane" => {
                if let Some(v) = args.next().filter(|s| !s.trim().is_empty()) {
                    pane_id = Some(v);
                }
            }
            "--cwd" => {
                if let Some(v) = args.next().filter(|s| !s.trim().is_empty()) {
                    cwd = Some(v);
                }
            }
            "--config" => {
                if let Some(v) = args.next().filter(|s| !s.trim().is_empty()) {
                    config_path = Some(PathBuf::from(v));
                }
            }
            "--theme" => {
                if let Some(v) = args.next().filter(|s| !s.trim().is_empty()) {
                    theme = Some(v);
                }
            }
            "--name" | "--title" => {
                if let Some(v) = args.next().filter(|s| !s.trim().is_empty()) {
                    sidebar_name = Some(v);
                }
            }
            _ => {}
        }
    }

    CliArgs {
        once,
        attach,
        pane_id,
        cwd,
        config_path,
        theme,
        sidebar_name,
        show_help,
        show_version,
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let mut out = stdout();
    let _ = out.execute(Print(MOUSE_OFF));
    let _ = out.execute(LeaveAlternateScreen);
    let _ = out.execute(crossterm::cursor::Show);
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

#[derive(Debug)]
enum MainMsg {
    Terminal(Event),
    DataChanged { git_touched: bool },
    Redraw,
    QuotaTick,
    Quit,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = parse_cli_args();
    if cli.show_version {
        println!("hudr2 {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if cli.show_help {
        println!("{HELP_TEXT}");
        return Ok(());
    }

    if cli.attach && !cli.once
        && let Ok(exe_path) = std::env::current_exe()
    {
        let attach_cwd = cli.cwd.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });
        if HerdrClient::attach_sidebar_split(&exe_path, &attach_cwd, cli.config_path.as_deref()) {
            return Ok(());
        }
    }

    let resolved_config = cli
        .config_path
        .or_else(|| Some(hudr2::paths::resolve_hudr2_config_path()));
    let mut initial_state = state::AppState::with_config(
        resolved_config,
        cli.cwd.clone(),
    );

    if let Some(ref name) = cli.sidebar_name {
        initial_state.set_sidebar_name(Some(name.clone()));
    }

    if let Some(ref th) = cli.theme {
        initial_state.theme = Some(th.clone());
        initial_state.snapshot.theme_name = Some(th.clone());
    }

    // Inicializar tema visual: prioridad CLI -> config persistida -> adaptativo Herdr
    let target_theme_str = initial_state.theme.as_deref().unwrap_or("auto");
    let active_theme = match target_theme_str {
        "auto" | "herdr" => hudr2::theme::resolve_adaptive_theme(),
        name => hudr2::theme::find_theme(name).unwrap_or_else(|| {
            hudr2::theme::resolve_adaptive_theme()
        }),
    };
    hudr2::theme::set_active_theme(active_theme);

    let app_state = Arc::new(RwLock::new(initial_state));

    // 1. Carga inicial rápida de datos locales
    app::refresh_all(&app_state).await;

    // Modo --once: imprime todas las tarjetas sin entrar en modo raw ni alternate screen y termina
    if cli.once {
        let state = app_state.read().await;
        let render_out = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            state.flash_notice.as_deref(),
            state.selected_diff_file.as_deref(),
            &state.selected_diff_lines,
            &state.hidden_cards,
            48,
        );
        for line in render_out.lines {
            println!("{line}");
        }
        return Ok(());
    }

    // Configurar hook de pánico y guard RAII para restaurar siempre la terminal
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        prev_hook(info);
    }));
    let _term_guard = TerminalGuard;

    enable_raw_mode()?;
    let mut out = stdout();
    crossterm::execute!(
        out,
        EnterAlternateScreen,
        Print(MOUSE_ON),
        crossterm::cursor::Hide
    )?;

    // 2. Disparar en segundo plano el refresco selectivo de cuotas (activas + nuevas/vencidas; el resto carga en 0ms desde caché)
    let (tx, mut rx) = mpsc::unbounded_channel::<MainMsg>();

    if hudr2::quotas::is_available() {
        let initial_fetch_state = app_state.clone();
        let tx_quota = tx.clone();
        tokio::spawn(async move {
            let wake_tx = tx_quota.clone();
            app::force_refresh_with_notify(
                &initial_fetch_state,
                false,
                false,
                Some(&move || {
                    let _ = wake_tx.send(MainMsg::Redraw);
                }),
            )
            .await;
            let _ = tx_quota.send(MainMsg::DataChanged { git_touched: false });
        });
    }

    // Sondeo de versiones de ecosistema en segundo plano (si está activado y la caché de 3h expiró)
    spawn_upstream_check(app_state.clone(), tx.clone());

    // 3. Hilo bloqueante dedicado para eventos de entrada (teclado, clics/scroll SGR y resize)
    let tx_input = tx.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = event::read() {
            let forward = match &ev {
                Event::Resize(_, _) => true,
                Event::Key(k) => k.kind == KeyEventKind::Press,
                Event::Mouse(m) => matches!(
                    m.kind,
                    MouseEventKind::Down(MouseButton::Left)
                        | MouseEventKind::ScrollUp
                        | MouseEventKind::ScrollDown
                ),
                _ => false,
            };
            if forward && tx_input.send(MainMsg::Terminal(ev)).is_err() {
                break;
            }
        }
    });

    // 4. Suscriptor IPC en vivo a Herdr (events.subscribe)
    let bound_pane = cli.pane_id.clone().or_else(|| {
        std::env::var("HERDR_PANE_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    });
    let tx_herdr = tx.clone();
    let bound_for_sub = bound_pane.clone();
    HerdrClient::spawn_event_subscriber(bound_for_sub.clone(), move |wire_ev| match wire_ev {
        HerdrWireEvent::PaneClosed(closed_id) => {
            if bound_for_sub.as_deref() == Some(closed_id.as_str()) {
                let _ = tx_herdr.send(MainMsg::Quit);
            } else {
                let _ = tx_herdr.send(MainMsg::DataChanged { git_touched: false });
            }
        }
        HerdrWireEvent::PaneUpdated(_)
        | HerdrWireEvent::StatusChanged { .. }
        | HerdrWireEvent::WorkspaceFocused
        | HerdrWireEvent::SubscriptionStarted => {
            let _ = tx_herdr.send(MainMsg::DataChanged { git_touched: false });
        }
        HerdrWireEvent::Disconnected => {}
    });

    // 5. File Watcher nativo (notify) con debounce de 300ms sobre .git, opencode.db y engram.db
    let project_dir_for_watch = app_state.read().await.snapshot.project_dir.clone();
    let tx_watch = tx.clone();
    std::thread::spawn(move || {
        let (raw_tx, raw_rx) = std::sync::mpsc::channel::<notify::Event>();
        let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                let _ = raw_tx.send(ev);
            }
        }) {
            Ok(w) => w,
            Err(_) => return,
        };

        // Vigilar .git (HEAD, index, refs)
        if let Some(git_dir) = hudr2::git::find_git_dir_for_watch(&project_dir_for_watch) {
            let _ = watcher.watch(&git_dir, RecursiveMode::NonRecursive);
            let refs_dir = git_dir.join("refs");
            if refs_dir.exists() {
                let _ = watcher.watch(&refs_dir, RecursiveMode::Recursive);
            }
        }

        // Vigilar directorio de opencode.db y engram.db
        for db_path in [
            hudr2::paths::resolve_opencode_db_path(),
            hudr2::paths::resolve_engram_db_path(),
        ] {
            if let Some(parent) = db_path.parent()
                && parent.exists()
            {
                let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
            }
        }

        // Vigilar sesiones de Pi / Gentle Shell si existen
        if let Some(home) = dirs::home_dir() {
            for agent_sessions in [
                home.join(".gentle-shell").join("agent").join("sessions"),
                home.join(".pi").join("agent").join("sessions"),
            ] {
                if agent_sessions.exists() {
                    let _ = watcher.watch(&agent_sessions, RecursiveMode::Recursive);
                }
            }
        }

        let mut deadline: Option<Instant> = None;
        let mut pending_git = false;

        loop {
            let msg = match deadline {
                Some(d) => raw_rx.recv_timeout(d.saturating_duration_since(Instant::now())),
                None => raw_rx
                    .recv()
                    .map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected),
            };

            match msg {
                Ok(ev) => {
                    if !matches!(
                        ev.kind,
                        notify::EventKind::Create(_)
                            | notify::EventKind::Modify(_)
                            | notify::EventKind::Remove(_)
                    ) {
                        continue;
                    }
                    let is_git = ev.paths.iter().any(|p| {
                        let name = p.file_name().unwrap_or_default();
                        name == "HEAD"
                            || name == "index"
                            || p.components().any(|c| c.as_os_str() == ".git" || c.as_os_str() == "refs")
                    });
                    if is_git {
                        pending_git = true;
                    }
                    deadline = Some(Instant::now() + Duration::from_millis(300));
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    deadline = None;
                    let git_touched = std::mem::replace(&mut pending_git, false);
                    if tx_watch.send(MainMsg::DataChanged { git_touched }).is_err() {
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    // 6. Timer de respaldo suave (para entornos sin socket IPC de Herdr) y refresco de cuotas cada 120s
    let tx_timer = tx.clone();
    let timer_state = app_state.clone();
    tokio::spawn(async move {
        let mut last_quota_refresh = tokio::time::Instant::now();
        let has_ipc = HerdrClient::resolve_ipc_path().is_some();
        loop {
            let sleep_dur = {
                let st = timer_state.read().await;
                if st.snapshot.agent_session.agent_status == "working" {
                    if has_ipc {
                        Duration::from_secs(4)
                    } else {
                        Duration::from_millis(2000)
                    }
                } else if has_ipc {
                    Duration::from_secs(60)
                } else {
                    Duration::from_secs(5)
                }
            };
            tokio::time::sleep(sleep_dur).await;

            if tx_timer
                .send(MainMsg::DataChanged { git_touched: false })
                .is_err()
            {
                break;
            }

            if hudr2::quotas::is_available()
                && last_quota_refresh.elapsed() >= Duration::from_secs(120)
            {
                last_quota_refresh = tokio::time::Instant::now();
                let _ = tx_timer.send(MainMsg::QuotaTick);
            }
        }
    });

    let mut should_quit = false;
    let mut last_rendered_offset = 0;

    while !should_quit {
        // 1a. Expirar aviso flash si existe
        let flash_wait = {
            let st = app_state.read().await;
            st.flash_notice_until
                .map(|until| until.saturating_duration_since(Instant::now()))
        };
        if flash_wait.is_some_and(|d| d.is_zero()) {
            let mut st = app_state.write().await;
            st.check_flash_notice_expiry();
        }

        // 1b. Renderizado condicional bajo demanda (solo cuando needs_redraw == true)
        let should_render = app_state.read().await.needs_redraw;
        if should_render {
            let (cols, rows) = size().unwrap_or((42, 36));
            let width = (cols as usize).clamp(32, 48);
            let max_visible = (rows as usize).max(10);

            let mut render_out = {
                let state = app_state.read().await;
                ui::render_screen_with_diff(
                    &state.snapshot,
                    &state.collapsed_cards,
                    state.flash_notice.as_deref(),
                    state.selected_diff_file.as_deref(),
                    &state.selected_diff_lines,
                    &state.hidden_cards,
                    width,
                )
            };
            let base_total_lines = render_out.lines.len();

            let actual_rendered_offset = {
                let mut state = app_state.write().await;
                let max_scroll = base_total_lines.saturating_sub(max_visible);
                if state.scroll_offset > max_scroll {
                    state.scroll_offset = max_scroll;
                }
                let offset = state.scroll_offset;
                if let Some(ref peek) = state.quick_peek {
                    ui::apply_quick_peek_overlay(&mut render_out, peek, offset, width, max_visible);
                }
                state.card_bounds = render_out.bounds;
                state.needs_redraw = false;
                offset
            };
            last_rendered_offset = actual_rendered_offset;
            let total_lines = render_out.lines.len();

            let visible = &render_out.lines
                [actual_rendered_offset..total_lines.min(actual_rendered_offset + max_visible)];

            let mut buffer =
                String::with_capacity(visible.iter().map(|l| l.len() + 4).sum::<usize>() + 8);
            buffer.push_str("\x1b[H");
            for (idx, line) in visible.iter().enumerate() {
                buffer.push_str(line);
                buffer.push_str("\x1b[K");
                if idx + 1 < visible.len() {
                    buffer.push('\n');
                }
            }
            buffer.push_str("\x1b[J");

            let mut out = stdout().lock();
            let _ = out.write_all(buffer.as_bytes());
            let _ = out.flush();
        }

        // 2. Esperar el próximo evento sin consumir CPU (0% en reposo)
        let flash_wait = {
            let st = app_state.read().await;
            st.flash_notice_until
                .map(|u| u.saturating_duration_since(Instant::now()))
        };
        let next_msg = if let Some(wait_dur) = flash_wait {
            match tokio::time::timeout(wait_dur, rx.recv()).await {
                Ok(Some(m)) => Some(m),
                Ok(None) => break,
                Err(_) => {
                    let mut st = app_state.write().await;
                    st.check_flash_notice_expiry();
                    continue;
                }
            }
        } else {
            match rx.recv().await {
                Some(m) => Some(m),
                None => break,
            }
        };

        let mut refresh_needed = false;
        let mut git_dirty = false;
        let mut quota_tick = false;

        let mut current_msg = next_msg;
        while let Some(msg) = current_msg {
            match msg {
                MainMsg::Quit => {
                    should_quit = true;
                    break;
                }
                MainMsg::DataChanged { git_touched } => {
                    refresh_needed = true;
                    if git_touched {
                        git_dirty = true;
                    }
                }
                MainMsg::Redraw => {
                    app_state.write().await.needs_redraw = true;
                }
                MainMsg::QuotaTick => {
                    quota_tick = true;
                    let is_auto = {
                        let s = app_state.read().await;
                        s.is_auto_theme()
                    };
                    if is_auto {
                        let resolved = hudr2::theme::resolve_adaptive_theme();
                        if resolved.id != hudr2::theme::active_theme().id {
                            hudr2::theme::set_active_theme(resolved);
                            app_state.write().await.needs_redraw = true;
                        }
                    }
                }
                MainMsg::Terminal(ev) => match ev {
                    Event::Resize(_, _) => {
                        app_state.write().await.needs_redraw = true;
                    }
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            let mut state = app_state.write().await;
                            if let Some(action) =
                                state.handle_mouse_click(mouse.row as usize, last_rendered_offset)
                            {
                                match action {
                                    state::ClickAction::OpenFolder(path_to_open) => {
                                        let folder_name = std::path::Path::new(&path_to_open)
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or(&path_to_open);
                                        state.set_flash_notice(
                                            Some(format!("+ Abierto en explorador: {folder_name}")),
                                            4000,
                                        );
                                        tokio::task::spawn_blocking(move || {
                                            app::open_folder_in_explorer(&path_to_open);
                                        });
                                    }
                                    state::ClickAction::ToggledCard(idx) => {
                                        if idx == 2
                                            && !state.collapsed_cards[2]
                                            && state.snapshot.tree.root_path.is_empty()
                                        {
                                            refresh_needed = true;
                                        }
                                    }
                                    state::ClickAction::ToggleCheckUpdates(now_enabled) => {
                                        if now_enabled {
                                            spawn_upstream_check(app_state.clone(), tx.clone());
                                        }
                                    }
                                    state::ClickAction::ToggleGitFileDiff(_)
                                    | state::ClickAction::OpenQuickPeek(_)
                                    | state::ClickAction::CloseQuickPeek
                                    | state::ClickAction::ToggleCardVisibility(_)
                                    | state::ClickAction::CycleTheme(_)
                                    | state::ClickAction::StartEditSidebarName => {}
                                }
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            app_state.write().await.scroll_up();
                        }
                        MouseEventKind::ScrollDown => {
                            app_state.write().await.scroll_down();
                        }
                        _ => {}
                    },
                    Event::Key(key) => {
                        let norm_code = match key.code {
                            KeyCode::Char(ch) => KeyCode::Char(ch.to_ascii_lowercase()),
                            other => other,
                        };
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && norm_code == KeyCode::Char('c')
                        {
                            should_quit = true;
                        } else {
                            let mut state = app_state.write().await;
                            if state.is_editing_sidebar_name {
                                match key.code {
                                    KeyCode::Enter => {
                                        state.commit_sidebar_name();
                                    }
                                    KeyCode::Esc => {
                                        state.cancel_edit_sidebar_name();
                                    }
                                    KeyCode::Backspace => {
                                        let mut buf = state.name_input_buffer.clone();
                                        buf.pop();
                                        state.update_name_input_buffer(buf);
                                    }
                                    KeyCode::Char(c)
                                        if state.name_input_buffer.chars().count() < 24
                                            && !c.is_control() =>
                                    {
                                        let mut buf = state.name_input_buffer.clone();
                                        buf.push(c);
                                        state.update_name_input_buffer(buf);
                                    }
                                    _ => {}
                                }
                            } else {
                                match norm_code {
                                    KeyCode::Char('q') => should_quit = true,
                                    KeyCode::Char('c') => {
                                        state.toggle_all_collapse();
                                        if !state.collapsed_cards[2]
                                            && state.snapshot.tree.root_path.is_empty()
                                        {
                                            refresh_needed = true;
                                        }
                                    }
                                    KeyCode::Char('1') => state.toggle_card(0), // Estado
                                    KeyCode::Char('2') => state.toggle_card(1), // Contexto
                                    KeyCode::Char('3') => {
                                        state.toggle_card(2); // Proyecto
                                        if !state.collapsed_cards[2]
                                            && state.snapshot.tree.root_path.is_empty()
                                        {
                                            refresh_needed = true;
                                        }
                                    }
                                    KeyCode::Char('4') => state.toggle_card(3), // Integraciones
                                    KeyCode::Char('5') => state.toggle_card(4), // Cuotas de Modelos
                                    KeyCode::Char('6') => state.toggle_card(5), // Cambios
                                    KeyCode::Char('7') => state.toggle_card(6), // Engram
                                    KeyCode::Char('8') => state.toggle_card(7), // Herramientas
                                    KeyCode::Char('9') => state.toggle_card(8), // MCP
                                    KeyCode::Char('e') => state.toggle_card(9), // Stack · Ecosistema
                                    KeyCode::Char('0') | KeyCode::Char('t') => state.toggle_card(10), // Personalizar
                                    KeyCode::Char('n') => {
                                        state.start_edit_sidebar_name();
                                    }
                                    KeyCode::Char('u') => {
                                        let now_enabled = state.toggle_check_updates();
                                        let msg = if now_enabled {
                                            "+ Updates: Chequeo online activado (3h)".to_string()
                                        } else {
                                            "+ Updates: Chequeo online apagado".to_string()
                                        };
                                        state.set_flash_notice(Some(msg), 2500);
                                        if now_enabled {
                                            spawn_upstream_check(app_state.clone(), tx.clone());
                                        }
                                    }
                                KeyCode::Char('m') => {
                                    state.cycle_theme();
                                }
                                KeyCode::Char('s') => {
                                    let state_ref = app_state.clone();
                                    let tx_done = tx.clone();
                                    tokio::spawn(async move {
                                        let wake_tx = tx_done.clone();
                                        let notify_cb = move || {
                                            let _ = wake_tx.send(MainMsg::Redraw);
                                        };
                                        app::sync_integrations_action_with_notify(
                                            &state_ref,
                                            Some(&notify_cb),
                                        )
                                        .await;
                                        app::force_refresh_with_notify(
                                            &state_ref,
                                            false,
                                            false,
                                            Some(&notify_cb),
                                        )
                                        .await;
                                        let _ = tx_done
                                            .send(MainMsg::DataChanged { git_touched: false });
                                    });
                                }
                                KeyCode::Char('r') => {
                                    let state_ref = app_state.clone();
                                    let tx_done = tx.clone();
                                    tokio::spawn(async move {
                                        let wake_tx = tx_done.clone();
                                        app::force_refresh_with_notify(
                                            &state_ref,
                                            true,
                                            false,
                                            Some(&move || {
                                                let _ = wake_tx.send(MainMsg::Redraw);
                                            }),
                                        )
                                        .await;
                                        let _ = tx_done
                                            .send(MainMsg::DataChanged { git_touched: false });
                                    });
                                }
                                KeyCode::Char('a') => {
                                    let state_ref = app_state.clone();
                                    let tx_done = tx.clone();
                                    tokio::spawn(async move {
                                        let wake_tx = tx_done.clone();
                                        app::force_refresh_with_notify(
                                            &state_ref,
                                            true,
                                            true,
                                            Some(&move || {
                                                let _ = wake_tx.send(MainMsg::Redraw);
                                            }),
                                        )
                                        .await;
                                        let _ = tx_done
                                            .send(MainMsg::DataChanged { git_touched: false });
                                    });
                                }
                                KeyCode::Char('o') => {
                                    let proj_dir = state.snapshot.project_dir.clone();
                                    let folder_name = std::path::Path::new(&proj_dir)
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or(&proj_dir)
                                        .to_string();
                                    state.set_flash_notice(
                                        Some(format!("+ Abierto en explorador: {folder_name}")),
                                        4000,
                                    );
                                    tokio::task::spawn_blocking(move || {
                                        app::open_folder_in_explorer(&proj_dir);
                                    });
                                }
                                KeyCode::Char('d') => {
                                    let state_ref = app_state.clone();
                                    let tx_done = tx.clone();
                                    tokio::spawn(async move {
                                        app::release_sessions_action(&state_ref).await;
                                        let _ = tx_done
                                            .send(MainMsg::DataChanged { git_touched: false });
                                    });
                                }
                                KeyCode::Char('x') => {
                                    let state_ref = app_state.clone();
                                    let tx_done = tx.clone();
                                    tokio::spawn(async move {
                                        app::migrate_sessions_action(&state_ref).await;
                                        let _ = tx_done
                                            .send(MainMsg::DataChanged { git_touched: false });
                                    });
                                }
                                KeyCode::Esc => state.close_quick_peek(),
                                KeyCode::Char(' ') | KeyCode::Char('p') => {
                                    state.toggle_quick_peek()
                                }
                                KeyCode::Up | KeyCode::Char('k') => state.scroll_up(),
                                KeyCode::Down | KeyCode::Char('j') => state.scroll_down(),
                                _ => {}
                            }
                            }
                        }
                    }
                    _ => {}
                },
            }

            current_msg = rx.try_recv().ok();
        }

        if should_quit {
            break;
        }

        if git_dirty {
            hudr2::git::clear_git_cache();
        }
        if refresh_needed {
            app::refresh_all(&app_state).await;
        }
        if quota_tick {
            let s_clone = app_state.clone();
            let tx_done = tx.clone();
            tokio::spawn(async move {
                let wake_tx = tx_done.clone();
                app::force_refresh_with_notify(
                    &s_clone,
                    false,
                    false,
                    Some(&move || {
                        let _ = wake_tx.send(MainMsg::Redraw);
                    }),
                )
                .await;
                let _ = tx_done.send(MainMsg::DataChanged { git_touched: false });
            });
        }
    }

    restore_terminal();
    Ok(())
}

fn spawn_upstream_check(
    app_state: Arc<RwLock<state::AppState>>,
    wake_tx: mpsc::UnboundedSender<MainMsg>,
) {
    tokio::spawn(async move {
        let (check_updates, cfg_path) = {
            let st = app_state.read().await;
            (st.check_updates, st.config_path.clone())
        };
        if check_updates && hudr2::ecosystem::is_upstream_cache_expired(cfg_path.as_deref()) {
            let cache =
                hudr2::ecosystem::fetch_upstream_versions_with_cache(cfg_path.as_deref()).await;
            let mut st = app_state.write().await;
            hudr2::ecosystem::apply_upstream_cache_to_ecosystem(&mut st.snapshot.ecosystem, &cache);
            st.needs_redraw = true;
            drop(st);
            let _ = wake_tx.send(MainMsg::Redraw);
        }
    });
}
