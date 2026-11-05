use hudr2::{app, state, ui};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
    MouseEventKind,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
};
use std::error::Error;
use std::io::{Write, stdout};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut out = stdout();
    crossterm::execute!(
        out,
        EnterAlternateScreen,
        EnableMouseCapture,
        crossterm::cursor::Hide
    )?;

    let app_state = std::sync::Arc::new(tokio::sync::RwLock::new(state::AppState::new()));

    // 1. Carga inicial rápida de datos locales
    app::refresh_all(&app_state).await;

    // 2. Disparar de inmediato la consulta de cuotas de red en segundo plano (snapshot inicial completo)
    if hudr2::quotas::QuotaCollector::is_available() {
        let initial_fetch_state = app_state.clone();
        tokio::spawn(async move {
            app::force_refresh(&initial_fetch_state, false, true).await;
        });
    }

    // 3. Background worker periódico adaptativo:
    // - Si el agente está trabajando (working): sleep 1500ms
    // - Si el agente está ocioso (!= working): sleep 4000ms
    // Refresca cuotas de red cada 5 minutos (300s) automáticamente para cuentas activas
    let bg_state = app_state.clone();
    tokio::spawn(async move {
        let mut last_quota_refresh = tokio::time::Instant::now();
        loop {
            let sleep_duration = {
                let state = bg_state.read().await;
                if state.snapshot.agent_session.agent_status == "working" {
                    Duration::from_millis(1500)
                } else {
                    Duration::from_millis(4000)
                }
            };
            sleep(sleep_duration).await;

            app::refresh_all(&bg_state).await;

            if hudr2::quotas::QuotaCollector::is_available()
                && last_quota_refresh.elapsed() >= Duration::from_secs(300)
            {
                last_quota_refresh = tokio::time::Instant::now();
                let s_clone = bg_state.clone();
                tokio::spawn(async move {
                    app::force_refresh(&s_clone, false, false).await;
                });
            }
        }
    });

    let mut should_quit = false;
    let mut last_rendered_offset = 0;

    while !should_quit {
        // 1a. Expirar aviso flash si existe
        if app_state.read().await.flash_notice.is_some() {
            let mut state = app_state.write().await;
            if state.check_flash_notice_expiry() {
                state.needs_redraw = true;
            }
        }

        // 1b. Renderizado condicional bajo demanda (needs_redraw)
        let should_render = app_state.read().await.needs_redraw;
        if should_render {
            let (cols, rows) = size().unwrap_or((48, 36));
            let width = (cols as usize).clamp(36, 50);
            let max_visible = (rows as usize).max(10);

            let render_out = {
                let state = app_state.read().await;
                ui::render_screen(
                    &state.snapshot,
                    &state.collapsed_cards,
                    state.flash_notice.as_deref(),
                    width,
                )
            };
            let total_lines = render_out.lines.len();

            let actual_rendered_offset = {
                let mut state = app_state.write().await;
                state.card_bounds = render_out.bounds;
                let max_scroll = total_lines.saturating_sub(max_visible);
                if state.scroll_offset > max_scroll {
                    state.scroll_offset = max_scroll;
                }
                state.needs_redraw = false;
                state.scroll_offset
            };
            last_rendered_offset = actual_rendered_offset;

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

        // 2. Procesar Eventos de Teclado, Mouse y Resize
        if event::poll(Duration::from_millis(50))? {
            let mut ev_opt = Some(event::read()?);
            while let Some(ev) = ev_opt {
                match ev {
                    Event::Resize(_, _) => {
                        app_state.write().await.needs_redraw = true;
                    }
                    // Eventos de Mouse (Clics y Rueda de Scroll)
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            let mut state = app_state.write().await;
                            state.needs_redraw = true;
                            if let Some(action) =
                                state.handle_mouse_click(mouse.row as usize, last_rendered_offset)
                            {
                                match action {
                                    state::ClickAction::SyncIntegrations => {
                                        let state_ref = app_state.clone();
                                        tokio::spawn(async move {
                                            app::sync_integrations_action(&state_ref).await;
                                        });
                                    }
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
                                    state::ClickAction::ToggledCard(_) => {}
                                }
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            let mut state = app_state.write().await;
                            state.scroll_up();
                        }
                        MouseEventKind::ScrollDown => {
                            let mut state = app_state.write().await;
                            state.scroll_down();
                        }
                        _ => {}
                    },
                    // Eventos de Teclado
                    Event::Key(key) => {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('c')
                        {
                            should_quit = true;
                        } else {
                            let mut state = app_state.write().await;
                            state.needs_redraw = true;
                            match key.code {
                                KeyCode::Char('q') => should_quit = true,
                                KeyCode::Char('c') => state.toggle_all_collapse(),
                                KeyCode::Char('1') => state.toggle_card(0), // Estado
                                KeyCode::Char('2') => state.toggle_card(1), // Contexto
                                KeyCode::Char('3') => state.toggle_card(2), // Proyecto
                                KeyCode::Char('4') => state.toggle_card(3), // Integraciones
                                KeyCode::Char('5') => state.toggle_card(4), // Cuotas de Modelos
                                KeyCode::Char('6') => state.toggle_card(5), // Cambios
                                KeyCode::Char('7') => state.toggle_card(6), // Engram
                                KeyCode::Char('8') => state.toggle_card(7), // Herramientas
                                KeyCode::Char('9') => state.toggle_card(8), // MCP
                                KeyCode::Char('s') => {
                                    let state_ref = app_state.clone();
                                    tokio::spawn(async move {
                                        app::sync_integrations_action(&state_ref).await;
                                    });
                                }
                                KeyCode::Char('r') => {
                                    let state_ref = app_state.clone();
                                    tokio::spawn(async move {
                                        app::force_refresh(&state_ref, true, true).await;
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
                                    tokio::spawn(async move {
                                        app::release_sessions_action(&state_ref).await;
                                    });
                                }
                                KeyCode::Char('x') => {
                                    let state_ref = app_state.clone();
                                    tokio::spawn(async move {
                                        app::migrate_sessions_action(&state_ref).await;
                                    });
                                }
                                KeyCode::Up | KeyCode::Char('k') => state.scroll_up(),
                                KeyCode::Down | KeyCode::Char('j') => state.scroll_down(),
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }

                if event::poll(Duration::from_millis(0))? {
                    ev_opt = Some(event::read()?);
                } else {
                    ev_opt = None;
                }
            }
        }
    }

    crossterm::execute!(
        stdout(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;
    disable_raw_mode()?;

    Ok(())
}
