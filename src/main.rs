pub mod app;
pub mod cleanup;
pub mod config;
pub mod engram;
pub mod git;
pub mod herdr;
pub mod models;
pub mod opencode;
pub mod paths;
pub mod quotas;
pub mod state;
pub mod sync;
pub mod tree;
pub mod ui;

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

    // 2. Disparar de inmediato la consulta de cuotas de red en segundo plano (silenciosa al arrancar)
    let initial_fetch_state = app_state.clone();
    tokio::spawn(async move {
        app::force_refresh(&initial_fetch_state, false).await;
    });

    // 3. Background worker periódico:
    // Actualiza datos de disco cada 2.5s y refresca cuotas de red cada 60s
    let bg_state = app_state.clone();
    tokio::spawn(async move {
        let mut loop_counter = 0;
        loop {
            sleep(Duration::from_millis(2500)).await;
            app::refresh_all(&bg_state).await;
            loop_counter += 1;
            // Cada ~60 segundos (24 * 2.5s), actualizar cuotas de red automáticamente (silencioso)
            if loop_counter >= 24 {
                loop_counter = 0;
                let s_clone = bg_state.clone();
                tokio::spawn(async move {
                    app::force_refresh(&s_clone, false).await;
                });
            }
        }
    });

    let mut should_quit = false;

    while !should_quit {
        let actual_rendered_offset;

        // 1. Renderizado y expiración de avisos
        {
            let mut state = app_state.write().await;
            let _ = state.check_flash_notice_expiry();

            let (cols, rows) = size().unwrap_or((48, 36));
            let width = (cols as usize).clamp(36, 50);

            let flash = state.flash_notice.clone();
            let render_out = ui::render_screen(
                &state.snapshot,
                &state.collapsed_cards,
                flash.as_deref(),
                width,
            );
            state.card_bounds = render_out.bounds;

            let max_visible = (rows as usize).max(10);
            let max_scroll = render_out.lines.len().saturating_sub(max_visible);
            if state.scroll_offset > max_scroll {
                state.scroll_offset = max_scroll;
            }
            actual_rendered_offset = state.scroll_offset;

            let visible = &render_out.lines[actual_rendered_offset
                ..render_out
                    .lines
                    .len()
                    .min(actual_rendered_offset + max_visible)];

            let mut buffer = String::from("\x1b[H");
            for (idx, line) in visible.iter().enumerate() {
                buffer.push_str(line);
                buffer.push_str("\x1b[K");
                if idx + 1 < visible.len() {
                    buffer.push('\n');
                }
            }
            buffer.push_str("\x1b[J");

            print!("{}", buffer);
            let _ = stdout().flush();
        }

        // 2. Procesar Eventos de Teclado y Mouse
        if event::poll(Duration::from_millis(30))? {
            let mut ev_opt = Some(event::read()?);
            while let Some(ev) = ev_opt {
                match ev {
                    // Eventos de Mouse (Clics y Rueda de Scroll)
                    Event::Mouse(mouse) => match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            let mut state = app_state.write().await;
                            if let Some(action) =
                                state.handle_mouse_click(mouse.row as usize, actual_rendered_offset)
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
                                        app::force_refresh(&state_ref, true).await;
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
