#[cfg(test)]
mod tests {
    use hudr2::models::HerdrPane;
    use hudr2::opencode::{extract_prefix, resolve_pool_account_prefixes};
    use hudr2::state::{AppState, ClickAction};
    use hudr2::tree::read_mcp_servers;
    use hudr2::ui;

    #[test]
    fn test_app_state_collapse_toggle() {
        let mut state = AppState::new();
        assert!(!state.collapsed_cards[0]);
        state.toggle_card(0);
        assert!(state.collapsed_cards[0]);
        state.toggle_card(0);
        assert!(!state.collapsed_cards[0]);
    }

    #[test]
    fn test_app_state_toggle_all() {
        let mut state = AppState::new();
        state.toggle_all_collapse();
        assert!(state.collapsed_cards.iter().all(|&c| c));
        state.toggle_all_collapse();
        assert!(state.collapsed_cards.iter().all(|&c| !c));
    }

    #[test]
    fn test_herdr_pane_model() {
        let pane = HerdrPane {
            pane_id: "w1:p1".to_string(),
            tab_id: Some("w1:t1".to_string()),
            workspace_id: Some("w1".to_string()),
            cwd: Some("D:/test".to_string()),
            terminal_title: Some("OC | Titulo".to_string()),
            terminal_title_stripped: Some("OC | Titulo".to_string()),
            focused: true,
            agent_status: Some("working".to_string()),
            agent: Some("opencode".to_string()),
            agent_session_id: Some("ses_123".to_string()),
        };
        assert_eq!(pane.pane_id, "w1:p1");
        assert!(pane.focused);
        assert_eq!(pane.agent_session_id.as_deref(), Some("ses_123"));
    }

    #[test]
    fn test_mouse_click_open_pc_vs_collapse() {
        let mut state = AppState::new();
        // Estado (0): expandido, Contexto (1): expandido, Proyecto (2): expandido
        state.collapsed_cards[2] = false;

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);
        state.card_bounds = render_out.bounds;

        // La línea 0 es el banner
        assert!(render_out.lines[0].contains("HUDR2"));

        // Ubicar coordenadas antes de la llamada mutable
        let proj_start_line = state
            .card_bounds
            .iter()
            .find(|b| b.id == "proyecto")
            .unwrap()
            .start_line;
        let open_start_line = state
            .card_bounds
            .iter()
            .find(|b| b.id == "open_folder")
            .unwrap()
            .start_line;

        assert_eq!(open_start_line, proj_start_line + 1);

        // 1. Clic en el encabezado de Proyecto (start_line) debe colapsar Proyecto (idx 2)
        let click_header = state.handle_mouse_click(proj_start_line, 0);
        match click_header {
            Some(ClickAction::ToggledCard(2)) => {}
            other => panic!("Esperaba ToggledCard(2), obtuve {:?}", other.is_some()),
        }

        // Reabrir proyecto para probar clic en [abrir en PC ↗]
        state.collapsed_cards[2] = false;
        let render_out2 = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);
        state.card_bounds = render_out2.bounds;

        // 2. Clic en [abrir en PC ↗] (start_line + 1) debe disparar OpenFolder y NO colapsar Proyecto
        let click_open = state.handle_mouse_click(open_start_line, 0);
        match click_open {
            Some(ClickAction::OpenFolder(_)) => {}
            other => panic!("Esperaba OpenFolder, obtuve {:?}", other.is_some()),
        }
        // Proyecto debe seguir expandido
        assert!(!state.collapsed_cards[2]);
    }

    #[test]
    fn test_card_boundaries_no_off_by_one() {
        let mut state = AppState::new();
        // Expandir Estado, Contexto y Proyecto
        state.collapsed_cards[0] = false;
        state.collapsed_cards[1] = false;
        state.collapsed_cards[2] = false;

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);
        state.card_bounds = render_out.bounds;

        let estado_start = state
            .card_bounds
            .iter()
            .find(|b| b.id == "estado")
            .unwrap()
            .start_line;
        let estado_end = state
            .card_bounds
            .iter()
            .find(|b| b.id == "estado")
            .unwrap()
            .end_line;
        let ctx_start = state
            .card_bounds
            .iter()
            .find(|b| b.id == "contexto")
            .unwrap()
            .start_line;
        let ctx_end = state
            .card_bounds
            .iter()
            .find(|b| b.id == "contexto")
            .unwrap()
            .end_line;
        let proj_start = state
            .card_bounds
            .iter()
            .find(|b| b.id == "proyecto")
            .unwrap()
            .start_line;

        // Estado arranca exactamente en línea 1 (línea 0 es el banner)
        assert_eq!(estado_start, 1);
        // Contexto arranca en estado.end_line + 1
        assert_eq!(ctx_start, estado_end + 1);
        // Proyecto arranca en contexto.end_line + 1
        assert_eq!(proj_start, ctx_end + 1);

        // Clic en encabezado de Estado (línea 1) debe alternar Estado (idx 0)
        let res = state.handle_mouse_click(estado_start, 0);
        assert!(matches!(res, Some(ClickAction::ToggledCard(0))));

        // Clic en encabezado de Contexto debe alternar Contexto (idx 1)
        let res2 = state.handle_mouse_click(ctx_start, 0);
        assert!(matches!(res2, Some(ClickAction::ToggledCard(1))));
    }

    #[test]
    fn test_mcp_deduplication() {
        let dir = tempfile::tempdir().unwrap();
        let opencode_json = dir.path().join("opencode.json");
        std::fs::write(
            &opencode_json,
            r#"{
                "mcp": {
                    "engram": { "type": "local", "enabled": true },
                    "servers": {
                        "engram": { "type": "local", "enabled": true },
                        "context7": { "type": "remote", "enabled": true }
                    }
                }
            }"#,
        )
        .unwrap();

        let mcps = read_mcp_servers(dir.path().to_str().unwrap());
        let engram_count = mcps.iter().filter(|m| m.name == "engram").count();
        assert_eq!(engram_count, 1, "Engram no debe estar duplicado");
    }

    #[test]
    fn test_flash_notice_placement_in_integraciones_not_in_header() {
        let state = AppState::new();
        let flash_text = "+ Cuotas actualizadas";
        let render_out = ui::render_screen(
            &state.snapshot,
            &state.collapsed_cards,
            Some(flash_text),
            44,
        );

        // La línea 0 (header) NUNCA debe contener el flash notice
        assert!(!render_out.lines[0].contains("Cuotas actualizadas"));
        assert!(render_out.lines[0].contains("HUDR2"));

        // La línea 1 debe ser el inicio de Estado
        assert!(render_out.lines[1].contains("Estado"));

        // El flash notice debe estar dentro de la tarjeta de Integraciones
        let _int_bounds = state.card_bounds.iter().find(|b| b.id == "integraciones");
        // Verificar que alguna línea contenga el flash notice
        let found_notice = render_out
            .lines
            .iter()
            .any(|l| l.contains("Cuotas actualizadas"));
        assert!(
            found_notice,
            "El flash notice debe estar presente en la pantalla"
        );

        // Y debe estar después de proyecto
        let notice_idx = render_out
            .lines
            .iter()
            .position(|l| l.contains("Cuotas actualizadas"))
            .unwrap();
        let proj_idx = render_out
            .lines
            .iter()
            .position(|l| l.contains("Proyecto"))
            .unwrap();
        assert!(
            notice_idx > proj_idx,
            "El flash notice debe estar en integraciones (después de proyecto)"
        );
    }

    #[test]
    fn test_flash_notice_expiry() {
        let mut state = AppState::new();
        state.set_flash_notice(Some("Test aviso".to_string()), 10);
        assert_eq!(state.flash_notice.as_deref(), Some("Test aviso"));

        // Esperar que expire
        std::thread::sleep(std::time::Duration::from_millis(15));
        let expired = state.check_flash_notice_expiry();
        assert!(expired);
        assert!(state.flash_notice.is_none());
    }

    #[test]
    fn test_prefix_and_pool_extraction() {
        let known = vec![
            "agy2_1".to_string(),
            "agy2_2".to_string(),
            "agy3_1".to_string(),
            "agy3_2".to_string(),
            "agy_1".to_string(),
            "agy".to_string(),
        ];

        // 1. Extraer de agente sdd-orchestrator-agy2_1
        let pfx = extract_prefix("sdd-orchestrator-agy2_1", "gemini-3.8-flash-high", &known);
        assert_eq!(pfx.as_deref(), Some("agy2_1"));

        // 2. Extraer de modelo con pool
        let pool_prefixes = resolve_pool_account_prefixes("cliproxy/Pool-02-gemini", &known);
        assert!(
            pool_prefixes.contains(&"agy2_1".to_string())
                || pool_prefixes.contains(&"agy2_2".to_string())
        );

        // 3. Extraer de modelo directo
        let pfx_mod = extract_prefix(
            "sdd-orchestrator",
            "cliproxy/agy3_1/gemini-3.8-flash-high",
            &known,
        );
        assert_eq!(pfx_mod.as_deref(), Some("agy3_1"));
    }

    #[test]
    fn test_contexto_and_proyecto_click_separation() {
        let mut state = AppState::new();
        // Contexto (1) expandido, Proyecto (2) colapsado por defecto
        state.collapsed_cards[1] = false;
        state.collapsed_cards[2] = true;

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);
        state.card_bounds = render_out.bounds;

        let ctx_start = state
            .card_bounds
            .iter()
            .find(|b| b.id == "contexto")
            .unwrap()
            .start_line;
        let ctx_end = state
            .card_bounds
            .iter()
            .find(|b| b.id == "contexto")
            .unwrap()
            .end_line;
        let proj_start = state
            .card_bounds
            .iter()
            .find(|b| b.id == "proyecto")
            .unwrap()
            .start_line;

        // Proyecto inicia exactamente después del final de Contexto
        assert_eq!(proj_start, ctx_end + 1);

        // A. Clic en el borde inferior de Contexto (ctx_end) NO debe colapsar Contexto
        let click_ctx_bottom = state.handle_mouse_click(ctx_end, 0);
        assert!(
            click_ctx_bottom.is_none(),
            "El borde inferior de tarjeta abierta no debe colapsarla"
        );
        assert!(!state.collapsed_cards[1], "Contexto debe seguir expandido");

        // B. Clic en el encabezado de Proyecto (proj_start) debe expandir Proyecto
        let click_proj = state.handle_mouse_click(proj_start, 0);
        assert!(matches!(click_proj, Some(ClickAction::ToggledCard(2))));
        assert!(
            !state.collapsed_cards[2],
            "Proyecto ahora debe estar expandido"
        );
        assert!(
            !state.collapsed_cards[1],
            "Contexto debe seguir expandido sin colisión"
        );

        // Re-render con Proyecto expandido
        let render_out_expanded =
            ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);
        state.card_bounds = render_out_expanded.bounds;

        let open_line = state
            .card_bounds
            .iter()
            .find(|b| b.id == "open_folder")
            .unwrap()
            .start_line;
        let proj_start_expanded = state
            .card_bounds
            .iter()
            .find(|b| b.id == "proyecto")
            .unwrap()
            .start_line;
        assert_eq!(open_line, proj_start_expanded + 1);

        // C. Clic en [abrir en PC ↗] no colapsa Proyecto ni Contexto
        let click_open = state.handle_mouse_click(open_line, 0);
        assert!(matches!(click_open, Some(ClickAction::OpenFolder(_))));
        assert!(
            !state.collapsed_cards[2],
            "Proyecto no debe colapsar al abrir en PC"
        );
        assert!(
            !state.collapsed_cards[1],
            "Contexto no debe alterarse al abrir en PC"
        );

        // D. Clic en encabezado de Proyecto expandido colapsa Proyecto
        let click_proj_collapse = state.handle_mouse_click(proj_start_expanded, 0);
        assert!(matches!(
            click_proj_collapse,
            Some(ClickAction::ToggledCard(2))
        ));
        assert!(
            state.collapsed_cards[2],
            "Proyecto debe volver a colapsarse"
        );
        assert!(
            !state.collapsed_cards[1],
            "Contexto sigue expandido intacto"
        );

        // E. Clic en encabezado de Contexto colapsa Contexto
        let click_ctx_collapse = state.handle_mouse_click(ctx_start, 0);
        assert!(matches!(
            click_ctx_collapse,
            Some(ClickAction::ToggledCard(1))
        ));
        assert!(state.collapsed_cards[1], "Contexto debe colapsarse");
    }

    #[test]
    fn test_open_pc_button_not_truncated_in_narrow_width() {
        let mut state = AppState::new();
        state.collapsed_cards[2] = false; // Expandir proyecto
        state.snapshot.tree.root_path =
            "/path/to/some/very/long/project_directory_for_testing".to_string();

        // Probar en anchos estrechos típicos de un sidebar (36, 38, 40, 44, 48)
        for w in [36, 38, 40, 44, 48] {
            let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, w);
            let open_line = render_out
                .lines
                .iter()
                .find(|l| l.contains("abrir en PC"))
                .unwrap();

            // La frase "[abrir en PC ->]" NUNCA debe aparecer truncada como "[abrir en P"
            assert!(
                open_line.contains("[abrir en PC ->]"),
                "El botón debe estar completo en ancho {}: {}",
                w,
                open_line
            );
            assert!(
                !open_line.contains("[abrir en P "),
                "No debe estar cortado en ancho {}: {}",
                w,
                open_line
            );
        }
    }

    #[test]
    fn test_open_folder_function_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        hudr2::app::open_folder_in_explorer(dir.path().to_str().unwrap());
    }

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_paths_env_overrides() {
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let fake_opencode_db = tmp.path().join("opencode.db");
        std::fs::write(&fake_opencode_db, b"sqlite").unwrap();

        let fake_engram_db = tmp.path().join("engram.db");
        std::fs::write(&fake_engram_db, b"sqlite").unwrap();

        let fake_auths_dir = tmp.path().join("auths");
        std::fs::create_dir_all(&fake_auths_dir).unwrap();

        // Probar OPENCODE_DB_PATH
        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", fake_opencode_db.to_str().unwrap());
        }
        assert_eq!(hudr2::paths::resolve_opencode_db_path(), fake_opencode_db);
        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
        }

        // Probar ENGRAM_DB_PATH
        unsafe {
            std::env::set_var("ENGRAM_DB_PATH", fake_engram_db.to_str().unwrap());
        }
        assert_eq!(hudr2::paths::resolve_engram_db_path(), fake_engram_db);
        unsafe {
            std::env::remove_var("ENGRAM_DB_PATH");
        }

        // Probar CLIPROXY_AUTHS_DIR
        unsafe {
            std::env::set_var("CLIPROXY_AUTHS_DIR", fake_auths_dir.to_str().unwrap());
        }
        assert_eq!(hudr2::paths::resolve_auths_dir(), fake_auths_dir);
        unsafe {
            std::env::remove_var("CLIPROXY_AUTHS_DIR");
        }
    }

    #[test]
    fn test_paths_opencode_config_candidates_includes_project_and_user() {
        let tmp = tempfile::tempdir().unwrap();
        let target_str = tmp.path().to_str().unwrap();
        let candidates = hudr2::paths::opencode_config_candidates(target_str);

        assert!(!candidates.is_empty());
        assert!(candidates.iter().any(|c| c.ends_with("opencode.json")));
        assert!(candidates.iter().any(|c| c.starts_with(tmp.path())));
    }

    #[test]
    fn test_resolve_model_context_limit() {
        use hudr2::opencode::resolve_model_context_limit;

        // Gemini / Flash / Pro / Sonnet-4 / Opus-4 / DeepSeek -> 1_000_000
        assert_eq!(
            resolve_model_context_limit("gemini-3.8-flash-high"),
            1_000_000
        );
        assert_eq!(
            resolve_model_context_limit("cliproxy/gemini-2.5-pro"),
            1_000_000
        );
        assert_eq!(resolve_model_context_limit("google/flash-2.0"), 1_000_000);
        assert_eq!(resolve_model_context_limit("claude-sonnet-4"), 1_000_000);
        assert_eq!(resolve_model_context_limit("sonnet-4-20250501"), 1_000_000);
        assert_eq!(resolve_model_context_limit("claude-opus-4"), 1_000_000);
        assert_eq!(resolve_model_context_limit("opus-4"), 1_000_000);
        assert_eq!(resolve_model_context_limit("deepseek-coder-v2"), 1_000_000);
        assert_eq!(resolve_model_context_limit("deepseek-r1"), 1_000_000);

        // Claude-3-5 / Claude-3.5 / Haiku -> 200_000
        assert_eq!(
            resolve_model_context_limit("claude-3-5-sonnet-20241022"),
            200_000
        );
        assert_eq!(resolve_model_context_limit("claude-3.5-sonnet"), 200_000);
        assert_eq!(
            resolve_model_context_limit("claude-3-5-haiku-20241022"),
            200_000
        );
        assert_eq!(resolve_model_context_limit("claude-3.5-haiku"), 200_000);
        assert_eq!(
            resolve_model_context_limit("claude-3-haiku-20240307"),
            200_000
        );
        assert_eq!(resolve_model_context_limit("anthropic/haiku"), 200_000);

        // Qwen -> 128_000
        assert_eq!(resolve_model_context_limit("qwen-2.5-coder-32b"), 128_000);
        assert_eq!(resolve_model_context_limit("qwen-turbo"), 128_000);
        assert_eq!(resolve_model_context_limit("QWEN-2.5"), 128_000);

        // Default -> 1_000_000
        assert_eq!(resolve_model_context_limit("gpt-4o"), 1_000_000);
        assert_eq!(resolve_model_context_limit("custom-model"), 1_000_000);
        assert_eq!(resolve_model_context_limit(""), 1_000_000);
    }

    #[test]
    fn test_context_card_rendering_no_hardcoded_fallbacks() {
        let mut state = AppState::new();
        state.collapsed_cards[1] = false; // Contexto expandido
        state.snapshot.agent_session.active_turn_tokens = 0;
        state.snapshot.agent_session.tokens_input = 0;
        state.snapshot.agent_session.tokens_output = 0;
        state.snapshot.agent_session.context_limit = 1_000_000;

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);
        let ctx_bounds = render_out
            .bounds
            .iter()
            .find(|b| b.id == "contexto")
            .unwrap();
        let ctx_lines: Vec<&str> = render_out.lines[ctx_bounds.start_line..=ctx_bounds.end_line]
            .iter()
            .map(|s| s.as_str())
            .collect();

        // No debe contener los valores hardcodeados antiguos
        for line in &ctx_lines {
            let clean = ui::strip_ansi(line);
            assert!(!clean.contains("28.1k"), "No debe usar fallback 28_100");
            assert!(!clean.contains("64.6k"), "No debe usar fallback 64_600");
            assert!(!clean.contains("228"), "No debe usar fallback 228");
        }

        // Debe renderizar 0.0k
        let has_zero = ctx_lines.iter().any(|l| ui::strip_ansi(l).contains("0.0k"));
        assert!(has_zero, "Debe mostrar 0.0k para tokens cuando son 0");
    }

    #[test]
    fn test_context_card_dynamic_thresholds_and_notice() {
        let mut state = AppState::new();
        state.collapsed_cards[1] = false;
        state.snapshot.agent_session.context_limit = 100_000;

        // 1. < 50% -> Optimo, MINT, sin notice row
        state.snapshot.agent_session.active_turn_tokens = 40_000; // 40%
        let render_opt = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);
        let ctx_bounds = render_opt
            .bounds
            .iter()
            .find(|b| b.id == "contexto")
            .unwrap();
        let ctx_text: String =
            render_opt.lines[ctx_bounds.start_line..=ctx_bounds.end_line].join("\n");
        let ctx_clean = ui::strip_ansi(&ctx_text);

        assert!(
            ctx_clean.contains("● Optimo 40.0%"),
            "Debe mostrar Optimo 40.0%"
        );
        assert!(
            !ctx_clean.contains("[aviso]"),
            "Optimo < 50% no debe mostrar aviso"
        );
        assert!(
            ctx_text.contains(hudr2::config::MINT),
            "Optimo debe usar color MINT"
        );

        // 2. >= 50% y < 80% -> Maduro, AMBER, con notice row
        state.snapshot.agent_session.active_turn_tokens = 60_000; // 60%
        let render_mad = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 60);
        let ctx_bounds = render_mad
            .bounds
            .iter()
            .find(|b| b.id == "contexto")
            .unwrap();
        let ctx_text_mad: String =
            render_mad.lines[ctx_bounds.start_line..=ctx_bounds.end_line].join("\n");
        let ctx_clean_mad = ui::strip_ansi(&ctx_text_mad);

        assert!(
            ctx_clean_mad.contains("● Maduro 60.0% [/snew]"),
            "Debe mostrar Maduro 60.0% [/snew]"
        );
        assert!(
            ctx_clean_mad.contains("[aviso] Sesion madura: usa /snew para handover limpio"),
            "Maduro debe mostrar aviso"
        );
        assert!(
            ctx_text_mad.contains(hudr2::config::AMBER),
            "Maduro debe usar color AMBER"
        );

        // 3. >= 80% -> Saturado, CORAL, con notice row
        state.snapshot.agent_session.active_turn_tokens = 85_000; // 85%
        let render_sat = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 60);
        let ctx_bounds = render_sat
            .bounds
            .iter()
            .find(|b| b.id == "contexto")
            .unwrap();
        let ctx_text_sat: String =
            render_sat.lines[ctx_bounds.start_line..=ctx_bounds.end_line].join("\n");
        let ctx_clean_sat = ui::strip_ansi(&ctx_text_sat);

        assert!(
            ctx_clean_sat.contains("● Saturado 85.0% [/snew]"),
            "Debe mostrar Saturado 85.0% [/snew]"
        );
        assert!(
            ctx_clean_sat.contains("[aviso] Sesion madura: usa /snew para handover limpio"),
            "Saturado debe mostrar aviso"
        );
        assert!(
            ctx_text_sat.contains(hudr2::config::CORAL),
            "Saturado debe usar color CORAL"
        );
    }

    #[test]
    fn test_opencode_db_telemetry_scanning() {
        let _guard = ENV_LOCK.lock().unwrap();
        use hudr2::models::HerdrPane;
        use hudr2::opencode::{read_opencode_session, resolve_session_for_pane};
        use rusqlite::Connection;

        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("opencode.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch(
            "CREATE TABLE session_v2 (
                id TEXT PRIMARY KEY,
                agent TEXT,
                model TEXT,
                directory TEXT,
                title TEXT,
                time_updated INTEGER,
                time_archived INTEGER,
                tokens_input INTEGER,
                tokens_output INTEGER,
                cost REAL
            );
            CREATE TABLE session_message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                type TEXT,
                time_created INTEGER,
                data TEXT
            );",
        )
        .unwrap();

        // 1. Sesión activa con tokens y costo en session_v2
        conn.execute(
            "INSERT INTO session_v2 (id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost)
             VALUES ('ses_active', 'sdd-orchestrator', 'gemini-3.8-flash-high', 'D:/myproj', 'test session', 2000, NULL, 50000, 1500, 0.042)",
            [],
        ).unwrap();

        // 2. Sesión archivada en el mismo directorio (debe ignorarse por time_archived IS NOT NULL)
        conn.execute(
            "INSERT INTO session_v2 (id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost)
             VALUES ('ses_archived', 'sdd-orchestrator', 'claude-3.5-sonnet', 'D:/myproj', 'old session', 3000, 3100, 1000, 200, 0.01)",
            [],
        ).unwrap();

        // Mensaje del asistente con tokens que incluyen cache.read y razonamiento
        let msg_data = r#"{
            "tokens": {
                "input": 2000,
                "output": 300,
                "reasoning": 50,
                "cache": {
                    "read": 40000,
                    "write": 100
                }
            },
            "content": [
                {
                    "type": "tool",
                    "name": "read",
                    "state": { "status": "completed", "input": { "path": "src/lib.rs" } }
                },
                {
                    "type": "tool",
                    "name": "edit",
                    "state": { "status": "completed", "input": { "path": "src/main.rs" } }
                },
                {
                    "type": "tool",
                    "name": "shell",
                    "state": { "status": "completed", "input": { "command": "cargo check" } }
                },
                {
                    "type": "tool",
                    "name": "mem_save",
                    "state": { "status": "completed" }
                }
            ]
        }"#;

        conn.execute(
            "INSERT INTO session_message (id, session_id, type, time_created, data)
             VALUES ('msg_1', 'ses_active', 'assistant', 2000, ?)",
            [msg_data],
        )
        .unwrap();

        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", db_path.to_str().unwrap());
        }

        let pane = HerdrPane {
            pane_id: "p1".to_string(),
            tab_id: Some("t1".to_string()),
            workspace_id: None,
            cwd: Some("D:/myproj".to_string()),
            terminal_title: Some("test session".to_string()),
            terminal_title_stripped: Some("test session".to_string()),
            focused: true,
            agent_status: Some("working".to_string()),
            agent: Some("opencode".to_string()),
            agent_session_id: None,
        };

        // Debe resolver ses_active y NO ses_archived
        let resolved = resolve_session_for_pane(&conn, &pane);
        assert!(resolved.is_some());
        assert_eq!(resolved.as_ref().unwrap().id, "ses_active");
        assert_eq!(resolved.as_ref().unwrap().tokens_input, Some(50000));
        assert_eq!(resolved.as_ref().unwrap().tokens_output, Some(1500));
        assert_eq!(resolved.as_ref().unwrap().cost, Some(0.042));

        let panes = vec![pane];
        let session = read_opencode_session("D:/myproj", &panes, &[], None, None);

        // Validar tokens desde session_v2
        assert_eq!(session.tokens_input, 50000);
        assert_eq!(session.tokens_output, 1500);
        assert_eq!(session.cost, 0.042);

        // Validar active turn tokens: 2000 + 300 + 50 + 40000 + 100 = 42450
        assert_eq!(session.active_turn_tokens, 42450);

        // Validar tool counts
        assert_eq!(session.tool_total, 4);
        assert_eq!(session.tool_reads, 1);
        assert_eq!(session.tool_writes, 1);
        assert_eq!(session.tool_bash, 1);
        assert_eq!(session.tool_engram, 1);

        // Validar attributed changes files_count (src/main.rs de edit)
        assert_eq!(session.attributed_changes.files_count, 1);

        // Validar caché por time_updated: si borramos los mensajes pero time_updated no cambió,
        // collect_message_stats debe devolver las estadísticas cacheadas intactas.
        conn.execute("DELETE FROM session_message", []).unwrap();
        let session2 = read_opencode_session("D:/myproj", &panes, &[], None, None);
        assert_eq!(
            session2.active_turn_tokens, 42450,
            "Caché preservó active_turn_tokens"
        );
        assert_eq!(session2.tool_total, 4, "Caché preservó tool_total");

        // Al limpiar caché y no haber mensajes, se recalculan desde SQLite vacío
        hudr2::opencode::clear_message_stats_cache();
        let session3 = read_opencode_session("D:/myproj", &panes, &[], None, None);
        assert_eq!(
            session3.tool_total, 0,
            "Al limpiar caché y no haber mensajes, total es 0"
        );

        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
        }
    }

    #[tokio::test]
    async fn test_force_refresh_updates_both_context_and_quotas() {
        use hudr2::app::force_refresh;
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let app_state = Arc::new(RwLock::new(AppState::new()));

        // Ejecutar force_refresh interactivo (simulando tecla 'r')
        force_refresh(&app_state, true, true).await;

        let state = app_state.read().await;
        // El flash notice debe reflejar que actualizó cuotas y contexto
        assert!(state.flash_notice.is_some());
        let notice = state.flash_notice.as_ref().unwrap();
        assert!(notice.contains("contexto") || notice.contains("Contexto"));
    }

    #[test]
    fn test_pane_priority_focused_and_working_over_current_cwd() {
        let _guard = ENV_LOCK.lock().unwrap();
        use hudr2::models::HerdrPane;
        use hudr2::opencode::read_opencode_session;
        use rusqlite::Connection;

        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("opencode.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch(
            "CREATE TABLE session_v2 (
                id TEXT PRIMARY KEY,
                agent TEXT,
                model TEXT,
                directory TEXT,
                title TEXT,
                time_updated INTEGER,
                time_archived INTEGER,
                tokens_input INTEGER,
                tokens_output INTEGER,
                cost REAL
            );
            CREATE TABLE session_message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                type TEXT,
                time_created INTEGER,
                data TEXT
            );",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO session_v2 (id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost)
             VALUES ('ses_cwd', 'agent_cwd', 'gemini-3.8-flash-high', 'D:/workdir', 'title cwd', 1000, NULL, 100, 10, 0.01)",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO session_v2 (id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost)
             VALUES ('ses_focused', 'agent_focused', 'gemini-3.8-flash-high', 'D:/workdir', 'title focused', 2000, NULL, 200, 20, 0.02)",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO session_v2 (id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost)
             VALUES ('ses_working', 'agent_working', 'gemini-3.8-flash-high', 'D:/workdir', 'title working', 3000, NULL, 300, 30, 0.03)",
            [],
        ).unwrap();

        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", db_path.to_str().unwrap());
        }

        let pane_cwd = HerdrPane {
            pane_id: "p_cwd".to_string(),
            tab_id: Some("t_other".to_string()),
            workspace_id: None,
            cwd: Some("D:/workdir".to_string()),
            terminal_title: Some("title cwd".to_string()),
            terminal_title_stripped: Some("title cwd".to_string()),
            focused: false,
            agent_status: Some("idle".to_string()),
            agent: Some("opencode".to_string()),
            agent_session_id: Some("ses_cwd".to_string()),
        };

        let pane_focused = HerdrPane {
            pane_id: "p_focused".to_string(),
            tab_id: Some("t_other".to_string()),
            workspace_id: None,
            cwd: Some("D:/workdir".to_string()),
            terminal_title: Some("title focused".to_string()),
            terminal_title_stripped: Some("title focused".to_string()),
            focused: true,
            agent_status: Some("idle".to_string()),
            agent: Some("opencode".to_string()),
            agent_session_id: Some("ses_focused".to_string()),
        };

        let pane_working = HerdrPane {
            pane_id: "p_working".to_string(),
            tab_id: Some("t_other".to_string()),
            workspace_id: None,
            cwd: Some("D:/workdir".to_string()),
            terminal_title: Some("title working".to_string()),
            terminal_title_stripped: Some("title working".to_string()),
            focused: false,
            agent_status: Some("working".to_string()),
            agent: Some("opencode".to_string()),
            agent_session_id: Some("ses_working".to_string()),
        };

        // 1. focused_pane debe tener prioridad sobre generic current_cwd_pane
        let panes1 = vec![pane_cwd.clone(), pane_focused.clone()];
        let s1 = read_opencode_session("D:/workdir", &panes1, &[], None, None);
        assert_eq!(
            s1.agent, "agent_focused",
            "focused_pane debe ganar sobre generic current_cwd_pane"
        );

        // 2. working_pane debe tener prioridad sobre generic current_cwd_pane
        let panes2 = vec![pane_cwd.clone(), pane_working.clone()];
        let s2 = read_opencode_session("D:/workdir", &panes2, &[], None, None);
        assert_eq!(
            s2.agent, "agent_working",
            "working_pane debe ganar sobre generic current_cwd_pane"
        );

        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
        }
    }

    #[test]
    fn test_cuota_de_modelos_scoped_to_workspace_local_prefix() {
        let mut state = AppState::new();
        state.collapsed_cards[4] = false;

        state.snapshot.accounts = vec![
            hudr2::models::AccountQuota {
                prefix: "agy1".to_string(),
                email: "agy1@example.com".to_string(),
                auth_file: "auth1.json".to_string(),
                rem_5h: 90,
                rem_wk: 80,
                reset_5h: "2h".to_string(),
                reset_wk: "3d".to_string(),
                is_warning: false,
                is_critical: false,
                has_error: false,
                error_msg: String::new(),
                pools: vec![hudr2::models::QuotaPool {
                    label: "Claude-3.5".to_string(),
                    percent: 90,
                    pace: "normal".to_string(),
                    reset: "2h".to_string(),
                }],
            },
            hudr2::models::AccountQuota {
                prefix: "agy2".to_string(),
                email: "agy2@example.com".to_string(),
                auth_file: "auth2.json".to_string(),
                rem_5h: 50,
                rem_wk: 60,
                reset_5h: "4h".to_string(),
                reset_wk: "5d".to_string(),
                is_warning: false,
                is_critical: false,
                has_error: false,
                error_msg: String::new(),
                pools: vec![hudr2::models::QuotaPool {
                    label: "GPT-4o".to_string(),
                    percent: 50,
                    pace: "fast".to_string(),
                    reset: "4h".to_string(),
                }],
            },
        ];

        // Simulamos que globalmente en Herdr existen agy1 y agy2 activos,
        // pero en este espacio específico el local_prefix es agy1
        state.snapshot.agent_session.local_prefix = Some("agy1".to_string());
        state
            .snapshot
            .agent_session
            .all_active_prefixes
            .insert("agy1".to_string());
        state
            .snapshot
            .agent_session
            .all_active_prefixes
            .insert("agy2".to_string());

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let pools_bound = render_out.bounds.iter().find(|b| b.id == "pools").unwrap();
        let pools_text: String =
            render_out.lines[pools_bound.start_line..=pools_bound.end_line].join("\n");
        let pools_clean = ui::strip_ansi(&pools_text);

        // Debe ser Cuota de Modelos específico de agy1
        assert!(pools_clean.contains("Cuota de Modelos · agy1 *"));
        assert!(
            !pools_clean.contains("Multi-cuenta"),
            "No debe mostrar Multi-cuenta aunque all_active_prefixes tenga varias"
        );
        assert!(
            pools_clean.contains("Claude-3.5"),
            "Debe mostrar el pool de agy1"
        );
        assert!(
            !pools_clean.contains("GPT-4o"),
            "No debe mostrar el pool de agy2"
        );
    }

    #[test]
    fn test_gemini_is_sovereign_for_quota_health_and_critical_alerts() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::QuotaCollector;

        let mut acc = AccountQuota {
            prefix: "agy2_1".to_string(),
            email: "agy2_1@example.com".to_string(),
            auth_file: "dummy.json".to_string(),
            rem_5h: 100,
            reset_5h: "4h 59m".to_string(),
            rem_wk: 100,
            reset_wk: "4d 23h".to_string(),
            is_critical: false,
            is_warning: false,
            has_error: false,
            error_msg: String::new(),
            pools: vec![],
        };

        // Simulamos un payload donde Claude está al 0% restante (consumo 100%, crítico si contara),
        // pero Gemini está saludable (88% restante en 5h y 95% semanal).
        let payload = serde_json::json!({
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "window": "5h",
                            "remainingFraction": 0.88,
                            "resetTime": "2026-11-05T18:00:00Z"
                        },
                        {
                            "window": "weekly",
                            "remainingFraction": 0.95,
                            "resetTime": "2026-11-10T12:00:00Z"
                        }
                    ]
                },
                {
                    "displayName": "Claude Models",
                    "buckets": [
                        {
                            "window": "5h",
                            "remainingFraction": 0.0,
                            "resetTime": "2026-11-05T18:00:00Z"
                        },
                        {
                            "window": "weekly",
                            "remainingFraction": 0.05,
                            "resetTime": "2026-11-12T12:00:00Z"
                        }
                    ]
                }
            ]
        });

        QuotaCollector::apply_quota_data(&mut acc, &payload);

        // Verificamos que Gemini manda sobre la salud:
        assert_eq!(acc.rem_5h, 88);
        assert_eq!(acc.rem_wk, 95);
        assert!(
            !acc.is_critical,
            "No debe ser crítico si Gemini tiene cuota suficiente, aunque Claude esté en 0%"
        );
        assert!(
            !acc.is_warning,
            "No debe estar en warning si Gemini está por encima del 70% restante"
        );

        // Sin embargo, los pools de Claude deben registrarse correctamente para inspección en la card de desglose
        let claude_5h = acc.pools.iter().find(|p| p.label == "Claude 5h").unwrap();
        assert_eq!(claude_5h.percent, 0);

        // Ahora comprobamos que si Gemini realmente se agota (ej. 10% restante en 5h => 90% usado >= 85),
        // sí se marca como crítico
        let critical_payload = serde_json::json!({
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "window": "5h",
                            "remainingFraction": 0.10,
                            "resetTime": "2026-11-05T18:00:00Z"
                        },
                        {
                            "window": "weekly",
                            "remainingFraction": 0.95,
                            "resetTime": "2026-11-10T12:00:00Z"
                        }
                    ]
                }
            ]
        });

        QuotaCollector::apply_quota_data(&mut acc, &critical_payload);
        assert_eq!(acc.rem_5h, 10);
        assert!(
            acc.is_critical,
            "Debe ser crítico cuando Gemini baja del 15% restante"
        );
    }

    #[test]
    fn test_ui_integraciones_card_reflects_gemini_sovereignty_without_false_criticals() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::QuotaCollector;
        use hudr2::state::AppState;

        let mut state = AppState::new();

        // Creamos una cuenta agy2_1 exactamente como en la captura del usuario:
        // Gemini al 98% restante, Claude al 9% restante (gastado)
        let mut acc = AccountQuota {
            prefix: "agy2_1".to_string(),
            email: "agy2_1@gmail.com".to_string(),
            auth_file: "auth.json".to_string(),
            rem_5h: 100,
            reset_5h: "4h 4m".to_string(),
            rem_wk: 100,
            reset_wk: "2d 12h".to_string(),
            is_critical: false,
            is_warning: false,
            has_error: false,
            error_msg: String::new(),
            pools: vec![],
        };

        let quota_payload = serde_json::json!({
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "window": "5h",
                            "remainingFraction": 0.98,
                            "resetTime": "2026-11-05T18:00:00Z"
                        },
                        {
                            "window": "weekly",
                            "remainingFraction": 0.88,
                            "resetTime": "2026-11-10T12:00:00Z"
                        }
                    ]
                },
                {
                    "displayName": "Claude Models",
                    "buckets": [
                        {
                            "window": "5h",
                            "remainingFraction": 0.09,
                            "resetTime": "2026-11-05T18:00:00Z"
                        },
                        {
                            "window": "weekly",
                            "remainingFraction": 0.50,
                            "resetTime": "2026-11-12T12:00:00Z"
                        }
                    ]
                }
            ]
        });

        QuotaCollector::apply_quota_data(&mut acc, &quota_payload);

        state.snapshot.accounts.push(acc);
        state.snapshot.agent_session.local_prefix = Some("agy2_1".to_string());
        state
            .snapshot
            .agent_session
            .all_active_prefixes
            .insert("agy2_1".to_string());

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen_text = ui::strip_ansi(&render_out.lines.join("\n"));

        // 1. La tarjeta NO debe marcar 'crític' en el encabezado
        assert!(
            screen_text.contains("Integraciones · 1 cuentas"),
            "El título debe mostrar 1 cuentas saludable, NO '! 1 crític'. Texto real:\n{}",
            screen_text
        );

        // 2. La línea de la cuenta NO debe decir [AGOTÁNDOSE]
        assert!(
            !screen_text.contains("[AGOTÁNDOSE]"),
            "No debe mostrar [AGOTÁNDOSE] porque Gemini está al 98%"
        );

        // 3. No debe haber banner de pánico ni acción de migración forzada para agy2_1
        assert!(
            !screen_text.contains("! CRÍTICO: agy2_1 (activa)"),
            "No debe alertar de migración crítica para agy2_1"
        );
        assert!(
            !screen_text.contains("x: migrar"),
            "No debe haber atajo de migración crítica en el footer"
        );

        // 4. En Cuota de Modelos: Claude 5h (0%) debe usar CORAL (rojo),
        // Gemini (98%) debe usar MINT (verde), y Claude Wk (50%) AMBER (amarillo)
        let pools_bound = render_out.bounds.iter().find(|b| b.id == "pools").unwrap();
        let pools_raw: String =
            render_out.lines[pools_bound.start_line..=pools_bound.end_line].join("\n");

        // Verificamos que contenga secuencias ANSI con CORAL (\x1b[38;2;248;113;113m) para Claude 5h
        assert!(
            pools_raw.contains(hudr2::config::CORAL),
            "Cuota de Modelos debe incluir color CORAL (rojo) para cuotas agotadas <=15%"
        );
        assert!(
            pools_raw.contains(hudr2::config::AMBER),
            "Cuota de Modelos debe incluir color AMBER (amarillo) para cuotas <=50%"
        );
        assert!(
            pools_raw.contains(hudr2::config::MINT),
            "Cuota de Modelos debe incluir color MINT (verde) para cuotas saludables"
        );
    }

    #[test]
    fn test_needs_redraw_lifecycle() {
        let mut state = AppState::new();
        assert!(
            state.needs_redraw,
            "AppState nuevo debe nacer con needs_redraw = true"
        );

        state.needs_redraw = false;
        state.set_flash_notice(Some("Aviso".to_string()), 5000);
        assert!(
            state.needs_redraw,
            "set_flash_notice debe marcar needs_redraw = true"
        );

        state.needs_redraw = false;
        state.toggle_card(1);
        assert!(
            state.needs_redraw,
            "toggle_card debe marcar needs_redraw = true"
        );

        state.needs_redraw = false;
        state.toggle_all_collapse();
        assert!(
            state.needs_redraw,
            "toggle_all_collapse debe marcar needs_redraw = true"
        );

        state.needs_redraw = false;
        state.scroll_down();
        assert!(
            state.needs_redraw,
            "scroll_down debe marcar needs_redraw = true"
        );

        state.needs_redraw = false;
        state.scroll_up();
        assert!(
            state.needs_redraw,
            "scroll_up debe marcar needs_redraw = true"
        );

        state.needs_redraw = false;
        state.set_snapshot(hudr2::models::TelemetrySnapshot::default());
        assert!(
            state.needs_redraw,
            "set_snapshot debe marcar needs_redraw = true"
        );

        // Expiración de aviso flash
        state.set_flash_notice(Some("Flash rápido".to_string()), 10);
        state.needs_redraw = false;
        std::thread::sleep(std::time::Duration::from_millis(15));
        let expired = state.check_flash_notice_expiry();
        assert!(expired);
        assert!(
            state.needs_redraw,
            "check_flash_notice_expiry al expirar debe marcar needs_redraw = true"
        );
    }

    #[test]
    fn test_git_telemetry_caching() {
        hudr2::git::clear_git_cache();
        let cur = std::env::current_dir().unwrap();
        let cur_str = cur.to_string_lossy().to_string();

        let t1 = hudr2::git::read_git_telemetry(&cur_str);
        assert!(t1.has_git, "El directorio de hudr2 es un repositorio git");

        let t2 = hudr2::git::read_git_telemetry(&cur_str);
        assert_eq!(t1.branch, t2.branch);
        assert_eq!(t1.modified, t2.modified);
        assert_eq!(t1.staged, t2.staged);

        hudr2::git::clear_git_cache();
    }

    #[test]
    fn test_engram_telemetry_caching() {
        let _guard = ENV_LOCK.lock().unwrap();
        hudr2::engram::clear_engram_cache();

        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("engram.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        conn.execute_batch(
            "CREATE TABLE observations (
                id INTEGER PRIMARY KEY,
                project TEXT,
                deleted_at TEXT
            );
            CREATE TABLE sync_enrolled_projects (
                project TEXT PRIMARY KEY
            );
            INSERT INTO observations (id, project, deleted_at) VALUES (1, 'mytestproj', NULL);
            INSERT INTO observations (id, project, deleted_at) VALUES (2, 'mytestproj', NULL);",
        )
        .unwrap();
        drop(conn);

        unsafe {
            std::env::set_var("ENGRAM_DB_PATH", db_path.to_str().unwrap());
        }

        let proj_path = tmp.path().join("mytestproj");
        std::fs::create_dir_all(&proj_path).unwrap();
        let proj_str = proj_path.to_str().unwrap();

        let t1 = hudr2::engram::read_engram_telemetry(proj_str);
        assert!(t1.online);
        assert_eq!(t1.obs_count, 2);

        // Volver a leer: debe devolver la telemetría en caché
        let t2 = hudr2::engram::read_engram_telemetry(proj_str);
        assert_eq!(t2.obs_count, 2);

        hudr2::engram::clear_engram_cache();
        unsafe {
            std::env::remove_var("ENGRAM_DB_PATH");
        }
    }

    #[tokio::test]
    async fn test_refresh_all_skips_tree_when_collapsed() {
        let app_state = std::sync::Arc::new(tokio::sync::RwLock::new(AppState::new()));
        let sentinel_tree = hudr2::models::ProjectTreeData {
            root_path: "sentinel_path".to_string(),
            total_dirs: 42,
            total_files: 99,
            tree: Vec::new(),
        };
        {
            let mut state = app_state.write().await;
            state.collapsed_cards[2] = true;
            state.snapshot.tree = sentinel_tree.clone();
        }

        hudr2::app::refresh_all(&app_state).await;

        {
            let state = app_state.read().await;
            assert_eq!(
                state.snapshot.tree, sentinel_tree,
                "Cuando la tarjeta de proyecto (idx 2) está colapsada, read_project_tree debe omitirse y mantener el árbol previo"
            );
            assert!(
                state.needs_redraw,
                "refresh_all debe marcar needs_redraw = true"
            );
        }
    }

    #[tokio::test]
    async fn test_read_project_tree_prioritizes_directories_and_orders_alphabetically() {
        use hudr2::tree::read_project_tree;
        let tree_data = read_project_tree(".");
        assert!(!tree_data.tree.is_empty(), "El árbol no debe estar vacío");

        // Verificar que las carpetas aparezcan antes que los archivos en la raíz
        let mut seen_file = false;
        for node in &tree_data.tree {
            if node.is_dir {
                assert!(
                    !seen_file,
                    "Se encontró la carpeta '{}' después de un archivo; las carpetas deben ir primero",
                    node.name
                );
            } else {
                seen_file = true;
            }
        }
    }

    #[test]
    fn test_normalize_model_str_strips_proxy_prefixes() {
        use hudr2::opencode::normalize_model_str;

        assert_eq!(
            normalize_model_str("cliproxy/agy3_1/gemini-3.8-flash-high"),
            "agy3_1/gemini-3.8-flash-high"
        );
        assert_eq!(
            normalize_model_str("opencode/pool-02-claude"),
            "pool-02-claude"
        );
        assert_eq!(
            normalize_model_str("cliproxy/opencode/gemini-2.5-pro"),
            "gemini-2.5-pro"
        );
        assert_eq!(
            normalize_model_str("direct-model-name"),
            "direct-model-name"
        );
    }

    #[test]
    fn test_quota_manager_and_collector_availability() {
        use hudr2::quotas::{QuotaCollector, QuotaManager};

        // QuotaCollector es un alias de QuotaManager
        assert_eq!(QuotaCollector::is_available(), QuotaManager::is_available());

        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("CLIPROXY_AUTHS_DIR", tmp.path().to_str().unwrap());
        }
        // En una carpeta vacía, is_available() debe ser false
        assert!(!QuotaManager::is_available());

        // Con credencial JSON válida, is_available() debe ser true
        let auth_file = tmp.path().join("account.json");
        std::fs::write(
            &auth_file,
            r#"{"access_token": "token123", "email": "test@example.com"}"#,
        )
        .unwrap();
        assert!(QuotaManager::is_available());

        unsafe {
            std::env::remove_var("CLIPROXY_AUTHS_DIR");
        }
    }

    #[test]
    fn test_ui_empty_accounts_renders_inactive_rows_gracefully() {
        let mut state = AppState::new();
        state.snapshot.accounts.clear();
        state.collapsed_cards[3] = false; // Integraciones expandida
        state.collapsed_cards[4] = false; // Cuota de Modelos expandida

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let screen_text = render_out.lines.join("\n");

        // Debe contener la fila inactiva en ambas tarjetas
        let inactive_matches: Vec<_> = render_out
            .lines
            .iter()
            .filter(|l| l.contains("Sin proveedor de cuotas activo") && l.contains("[inactivo]"))
            .collect();
        assert_eq!(
            inactive_matches.len(),
            2,
            "Tanto Integraciones como Cuota de Modelos deben mostrar la fila inactiva cuando accounts está vacío. Pantalla:\n{}",
            screen_text
        );
    }

    #[test]
    fn test_agent_session_generalization_renders_cleanly() {
        use hudr2::models::{AgentSession, OpenCodeSession};

        // 1. Verificar equivalencia de tipo (alias retrocompatible)
        let session = AgentSession {
            provider_name: "pi".to_string(),
            agent: "pi-coder".to_string(),
            model: "anthropic/claude-3-5-sonnet".to_string(),
            model_short: "claude-3.5-sonnet".to_string(),
            context_limit: 200_000,
            active_turn_tokens: 15_000,
            tokens_input: 45_000,
            tokens_output: 5_000,
            cost: 0.042,
            tool_reads: 8,
            tool_writes: 3,
            tool_bash: 2,
            tool_engram: 0,
            tool_other: 0,
            tool_total: 13,
            agent_status: "working".to_string(),
            ..Default::default()
        };

        let mut state = AppState::new();
        state.snapshot.agent_session = session;

        // Renderizar pantalla completa
        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let screen_text = ui::strip_ansi(&render_out.lines.join("\n"));

        // El estado debe reflejar que el agente está trabajando y su modelo
        assert!(screen_text.contains(">> WORKING"));
        assert!(screen_text.contains("claude-3.5-sonnet"));
        assert!(screen_text.contains("Contexto"));
        assert!(screen_text.contains("Herramientas"));

        // Validar que OpenCodeSession sigue siendo el mismo tipo
        let oc_session: OpenCodeSession = state.snapshot.agent_session.clone();
        assert_eq!(oc_session.provider_name, "pi");
        assert_eq!(oc_session.tool_total, 13);
    }

    #[test]
    fn test_quota_error_handling_and_orphan_purge() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::{QuotaManager, calculate_quota_pace};

        let mut acc = AccountQuota::default();

        // 1. 401 -> token expirado
        let err_401 = serde_json::json!({
            "error": { "code": 401, "status": "UNAUTHENTICATED", "message": "Token expired" }
        });
        QuotaManager::apply_quota_error(&mut acc, 401, &err_401);
        assert!(acc.has_error);
        assert_eq!(acc.error_msg, "token expirado");

        // 2. 403 TOS_VIOLATION -> tos suspendida
        let err_tos = serde_json::json!({
            "error": {
                "code": 403,
                "status": "PERMISSION_DENIED",
                "message": "This service has been disabled in this account for violation of Terms of Service.",
                "details": [{ "reason": "TOS_VIOLATION" }]
            }
        });
        QuotaManager::apply_quota_error(&mut acc, 403, &err_tos);
        assert!(acc.has_error);
        assert_eq!(acc.error_msg, "tos suspendida");

        // 3. 429 -> rate limited
        let err_429 = serde_json::json!({
            "error": { "code": 429, "status": "RESOURCE_EXHAUSTED" }
        });
        QuotaManager::apply_quota_error(&mut acc, 429, &err_429);
        assert!(acc.has_error);
        assert_eq!(acc.error_msg, "rate limited");

        // 4. Purga de cuentas huérfanas cuando un archivo .json se elimina
        let _guard = ENV_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("CLIPROXY_AUTHS_DIR", tmp.path().to_str().unwrap());
        }
        let f1 = tmp.path().join("antigravity-acc1.json");
        let f2 = tmp.path().join("antigravity-acc2.json");
        std::fs::write(&f1, r#"{"access_token":"t1","email":"a1@test.com"}"#).unwrap();
        std::fs::write(&f2, r#"{"access_token":"t2","email":"a2@test.com"}"#).unwrap();

        let mut list = Vec::new();
        QuotaManager::discover_accounts(&mut list);
        assert_eq!(list.len(), 2);

        // Eliminar f2 y redescubrir: debe purgar acc2
        std::fs::remove_file(&f2).unwrap();
        QuotaManager::discover_accounts(&mut list);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].prefix, "acc1");

        unsafe {
            std::env::remove_var("CLIPROXY_AUTHS_DIR");
        }

        // 5. Cálculo de pace sin fecha devuelve ~0%
        assert_eq!(calculate_quota_pace(90, false, None), "~0%");
    }

    #[test]
    fn test_ecosystem_card_rendering_and_click() {
        use hudr2::models::{EcosystemTelemetry, ToolVersionInfo};

        let mut state = AppState::new();
        state.snapshot.ecosystem = EcosystemTelemetry {
            tools: vec![
                ToolVersionInfo {
                    name: "pi".to_string(),
                    version: Some("v1.5.0".to_string()),
                    is_installed: true,
                },
                ToolVersionInfo {
                    name: "hudr2".to_string(),
                    version: Some("v0.1.0".to_string()),
                    is_installed: true,
                },
                ToolVersionInfo {
                    name: "gentle-ai".to_string(),
                    version: None,
                    is_installed: false,
                },
            ],
        };

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);
        state.card_bounds = render_out.bounds;

        let eco_bound = state
            .card_bounds
            .iter()
            .find(|b| b.id == "ecosystem")
            .expect("ecosystem card bound must exist");

        // El texto de la pantalla debe contener el título y herramientas
        let all_text = render_out.lines.join("\n");
        assert!(all_text.contains("Stack"));
        assert!(all_text.contains("Ecosistema"));
        assert!(all_text.contains("pi"));
        assert!(all_text.contains("v1.5.0"));
        assert!(all_text.contains("gentle-ai"));

        // Clic en la tarjeta de ecosistema alterna el colapso (idx 9)
        assert!(!state.collapsed_cards[9]);
        let res = state.handle_mouse_click(eco_bound.start_line, 0);
        assert!(matches!(res, Some(ClickAction::ToggledCard(9))));
        assert!(state.collapsed_cards[9]);
    }

    #[test]
    fn test_pi_jsonl_incremental_tail_and_context_cache() {
        use hudr2::pi::{clear_pi_tail_cache, read_pi_session_file};
        use std::io::Write;

        clear_pi_tail_cache();
        let tmp = tempfile::tempdir().unwrap();
        let agent_dir = tmp.path().join(".gentle-shell").join("agent");
        let ses_dir = agent_dir.join("sessions").join("test-proj");
        std::fs::create_dir_all(&ses_dir).unwrap();

        // 1. Configurar settings.json, cpamc-models-cache.json y models-store.json
        std::fs::write(
            agent_dir.join("settings.json"),
            r#"{"defaultProvider":"cpamc","defaultModel":"gpt-6.1-sol","defaultThinkingLevel":"high"}"#,
        )
        .unwrap();
        std::fs::write(
            agent_dir.join("cpamc-models-cache.json"),
            r#"[{"id":"gpt-6.1-sol","contextWindow":272000}]"#,
        )
        .unwrap();
        std::fs::write(
            agent_dir.join("models-store.json"),
            r#"{"cpamc":{"models":[{"id":"gpt-6.1-sol","contextWindow":370000}]}}"#,
        )
        .unwrap();

        let jsonl_path = ses_dir.join("2026_s1.jsonl");
        std::fs::write(&jsonl_path, "").unwrap();

        // Sesión vacía recién creada: debe hacer fallback a settings.json y preferir cpamc-models-cache.json (272k)
        let s0 = read_pi_session_file(&jsonl_path, &[], "working").unwrap();
        assert_eq!(s0.provider_name, "gentle-shell");
        assert_eq!(s0.model, "cpamc/gpt-6.1-sol");
        assert_eq!(s0.model_short, "gpt-6.1-sol");
        assert_eq!(s0.thinking_level.as_deref(), Some("high"));
        assert_eq!(s0.context_limit, 272_000);
        assert_eq!(s0.agent_status, "working");

        // 2. Append incremental de mensajes de asistente y toolResult de todo
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&jsonl_path)
            .unwrap();
        writeln!(
            f,
            r#"{{"type":"message","message":{{"role":"assistant","model":"gpt-6.1-sol","provider":"cpamc","usage":{{"input":1200,"output":300,"cacheRead":500,"totalTokens":2000,"cost":{{"total":0.015}}}},"content":[{{"type":"toolCall","name":"read"}},{{"type":"toolCall","name":"edit","arguments":{{"path":"src/main.rs"}}}},{{"type":"toolCall","name":"bash"}},{{"type":"toolCall","name":"mem_save"}}]}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"message","message":{{"role":"toolResult","toolName":"todo","details":{{"gentleTodo":{{"tasks":[{{"title":"Optimizar HUD","status":"in_progress"}},{{"title":"Tests","status":"done"}}]}}}}}}}}"#
        )
        .unwrap();
        f.flush().unwrap();

        let s1 = read_pi_session_file(&jsonl_path, &[], "idle").unwrap();
        assert_eq!(s1.tokens_input, 1200);
        assert_eq!(s1.tokens_output, 300);
        assert_eq!(s1.active_turn_tokens, 2000);
        assert!((s1.cost - 0.015).abs() < 1e-6);
        assert_eq!(s1.tool_total, 4);
        assert_eq!(s1.tool_reads, 1);
        assert_eq!(s1.tool_writes, 1);
        assert_eq!(s1.tool_bash, 1);
        assert_eq!(s1.tool_engram, 1);
        assert_eq!(s1.attributed_changes.files_count, 1);
        assert_eq!(s1.todos.len(), 2);
        assert_eq!(s1.todos[0].title, "Optimizar HUD");
        assert_eq!(s1.todos[0].status, "in_progress");

        // 3. Segundo turno append-only: solo lee los nuevos bytes y acumula tokens
        writeln!(
            f,
            r#"{{"type":"message","message":{{"role":"assistant","usage":{{"input":800,"output":200,"totalTokens":3100,"cost":{{"total":0.010}}}}}}}}"#
        )
        .unwrap();
        f.flush().unwrap();

        let s2 = read_pi_session_file(&jsonl_path, &[], "idle").unwrap();
        assert_eq!(s2.tokens_input, 2000);
        assert_eq!(s2.tokens_output, 500);
        assert_eq!(s2.active_turn_tokens, 3100);
        assert!((s2.cost - 0.025).abs() < 1e-6);

        clear_pi_tail_cache();
    }

    #[test]
    fn test_git_sensitive_file_protection_and_inline_diff_click() {
        use hudr2::git::{is_sensitive_file, read_file_diff_preview};
        use hudr2::models::GitFileChange;

        for secret in [
            ".env",
            ".env.local",
            "config/secrets.json",
            "keys/id_rsa",
            "cert.pem",
            "private.key",
            "oauth_token.txt",
            "auth-store.json",
        ] {
            assert!(
                is_sensitive_file(secret),
                "{secret} debe detectarse como sensible"
            );
            let preview = read_file_diff_preview(".", secret);
            assert_eq!(preview, vec!["[archivo sensible protegido - diff oculto]"]);
        }

        for safe in ["src/main.rs", "README.md", "Cargo.toml"] {
            assert!(
                !is_sensitive_file(safe),
                "{safe} no debe marcarse como sensible"
            );
        }

        // Probar clic en archivo dentro de la tarjeta Cambios (Git)
        let mut state = AppState::new();
        state.collapsed_cards[5] = false; // Cambios abierto
        state.snapshot.git.is_clean = false;
        state.snapshot.git.modified = 1;
        state.snapshot.git.files = vec![GitFileChange {
            path: ".env.secret".to_string(),
            status_code: "M".to_string(),
            added: 2,
            deleted: 1,
        }];

        let render_out = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            state.selected_diff_file.as_deref(),
            &state.selected_diff_lines,
            48,
        );
        state.card_bounds = render_out.bounds;

        let file_bound = state
            .card_bounds
            .iter()
            .find(|b| b.action_type == Some("git_file_diff"))
            .expect("debe existir bound clicable para el archivo en Git");

        let click_res = state.handle_mouse_click(file_bound.start_line, 0);
        assert_eq!(
            click_res,
            Some(ClickAction::ToggleGitFileDiff(".env.secret".to_string()))
        );
        assert_eq!(state.selected_diff_file.as_deref(), Some(".env.secret"));
        assert_eq!(
            state.selected_diff_lines,
            vec!["[archivo sensible protegido - diff oculto]"]
        );

        // Renderizar con el diff activo y comprobar que aparece el aviso protegido
        let render_out2 = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            state.selected_diff_file.as_deref(),
            &state.selected_diff_lines,
            48,
        );
        let screen_clean = ui::strip_ansi(&render_out2.lines.join("\n"));
        assert!(screen_clean.contains("[archivo sensible protegido"));
    }

    #[test]
    fn test_collapsed_cards_atomic_persistence() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("hudr2.json");

        let mut st = AppState::with_config(Some(cfg_path.clone()), None);
        assert!(!st.collapsed_cards[0]);
        st.toggle_card(0); // Colapsar "estado"
        st.toggle_card(5); // Colapsar "git"
        assert!(st.collapsed_cards[0]);
        assert!(st.collapsed_cards[5]);
        assert!(cfg_path.exists(), "El archivo de configuración debe haberse guardado");

        // Recargar en un nuevo AppState y comprobar que recuerda las tarjetas colapsadas
        let st2 = AppState::with_config(Some(cfg_path), None);
        assert!(st2.collapsed_cards[0]);
        assert!(st2.collapsed_cards[2]); // proyecto venía colapsado por defecto
        assert!(st2.collapsed_cards[5]);
        assert!(!st2.collapsed_cards[1]);
    }

    #[test]
    fn test_herdr_wire_event_parsing() {
        use hudr2::herdr::{HerdrClient, HerdrWireEvent};
        use serde_json::json;

        // Ambos formatos (guion bajo y punto) para agent_status_changed deben funcionar
        for ev_name in ["pane_agent_status_changed", "pane.agent_status_changed"] {
            let payload = json!({
                "event": ev_name,
                "data": { "pane_id": "w1:p2", "agent_status": "working" }
            });
            assert_eq!(
                HerdrClient::parse_wire_event(&payload),
                Some(HerdrWireEvent::StatusChanged {
                    pane_id: "w1:p2".to_string(),
                    agent_status: "working".to_string()
                })
            );
        }

        let closed = json!({
            "event": "pane_closed",
            "data": { "pane_id": "w1:p2" }
        });
        assert_eq!(
            HerdrClient::parse_wire_event(&closed),
            Some(HerdrWireEvent::PaneClosed("w1:p2".to_string()))
        );

        let started = json!({
            "result": { "type": "subscription_started" }
        });
        assert_eq!(
            HerdrClient::parse_wire_event(&started),
            Some(HerdrWireEvent::SubscriptionStarted)
        );
    }

    #[test]
    fn test_engram_handoff_summary_extraction_and_render() {
        let _guard = ENV_LOCK.lock().unwrap();
        hudr2::engram::clear_engram_cache();

        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("engram.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        conn.execute_batch(
            "CREATE TABLE observations (
                id INTEGER PRIMARY KEY,
                project TEXT,
                type TEXT,
                content TEXT,
                created_at TEXT,
                deleted_at TEXT
            );
            CREATE TABLE sync_enrolled_projects (
                project TEXT PRIMARY KEY
            );
            INSERT INTO observations (id, project, type, content, created_at, deleted_at)
            VALUES (77, 'handoffproj', 'session_summary', '## Goal\nOptimizar hudr2 con eventos\n- Listo', '2026-11-06 12:00:00', NULL);",
        )
        .unwrap();
        drop(conn);

        unsafe {
            std::env::set_var("ENGRAM_DB_PATH", db_path.to_str().unwrap());
        }

        let proj_path = tmp.path().join("handoffproj");
        std::fs::create_dir_all(&proj_path).unwrap();
        let tele = hudr2::engram::read_engram_telemetry(proj_path.to_str().unwrap());
        assert!(tele.online);
        assert_eq!(tele.obs_count, 1);
        let handoff = tele.last_handoff.as_ref().expect("debe extraer last_handoff");
        assert_eq!(handoff.id, 77);
        assert!(handoff.lines.iter().any(|l| l.contains("Optimizar hudr2")));

        let mut state = AppState::new();
        state.snapshot.engram = tele;
        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let text = ui::strip_ansi(&render_out.lines.join("\n"));
        assert!(text.contains("Handoff #77"));
        assert!(text.contains("Optimizar hudr2"));

        hudr2::engram::clear_engram_cache();
        unsafe {
            std::env::remove_var("ENGRAM_DB_PATH");
        }
    }

    #[tokio::test]
    async fn test_refresh_all_skips_redraw_when_snapshot_unchanged() {
        let app_state = std::sync::Arc::new(tokio::sync::RwLock::new(AppState::new()));
        hudr2::app::refresh_all(&app_state).await;

        {
            let mut state = app_state.write().await;
            state.needs_redraw = false;
        }

        // Segunda llamada consecutiva sin cambios en el estado subyacente:
        // needs_redraw debe mantenerse en false (cero redibujados redundantes)
        hudr2::app::refresh_all(&app_state).await;
        {
            let state = app_state.read().await;
            assert!(
                !state.needs_redraw,
                "refresh_all no debe forzar needs_redraw = true cuando el snapshot no cambió"
            );
        }
    }

    #[test]
    fn test_dynamic_model_family_quota_health_claude_gpt_deepseek_gemini() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::QuotaCollector;

        let mut acc = AccountQuota {
            prefix: "agy".to_string(),
            email: "agy@example.com".to_string(),
            auth_file: "auth.json".to_string(),
            rem_5h: 100,
            reset_5h: "4h 59m".to_string(),
            rem_wk: 100,
            reset_wk: "4d 23h".to_string(),
            is_critical: false,
            is_warning: false,
            has_error: false,
            error_msg: String::new(),
            pools: vec![],
        };

        let payload = serde_json::json!({
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        { "window": "5h", "remainingFraction": 0.92, "resetTime": "2026-11-06T18:00:00Z" },
                        { "window": "weekly", "remainingFraction": 0.90, "resetTime": "2026-11-12T12:00:00Z" }
                    ]
                },
                {
                    "displayName": "Claude Models",
                    "buckets": [
                        { "window": "5h", "remainingFraction": 0.08, "resetTime": "2026-11-06T18:00:00Z" },
                        { "window": "weekly", "remainingFraction": 0.45, "resetTime": "2026-11-12T12:00:00Z" }
                    ]
                },
                {
                    "displayName": "DeepSeek Models",
                    "buckets": [
                        { "window": "5h", "remainingFraction": 0.25, "resetTime": "2026-11-06T18:00:00Z" },
                        { "window": "weekly", "remainingFraction": 0.80, "resetTime": "2026-11-12T12:00:00Z" }
                    ]
                }
            ]
        });

        QuotaCollector::apply_quota_data(&mut acc, &payload);

        // 1. Con modelo Gemini: saludable (92% 5h, 90% Wk)
        let eff_gem = QuotaCollector::evaluate_account_for_model(&acc, "gemini-3.8-flash-high");
        assert_eq!(eff_gem.family, "Gemini");
        assert_eq!(eff_gem.rem_5h, 92);
        assert!(!eff_gem.is_critical);
        assert!(!eff_gem.is_warning);

        // 2. Con modelo Claude: crítico (8% 5h <= 15%)
        let eff_cla = QuotaCollector::evaluate_account_for_model(&acc, "claude-sonnet-4-5");
        assert_eq!(eff_cla.family, "Claude");
        assert_eq!(eff_cla.rem_5h, 8);
        assert_eq!(eff_cla.rem_wk, 45);
        assert!(eff_cla.is_critical, "Claude al 8% debe marcarse crítico cuando la sesión usa Claude");

        // 3. Con modelo GPT (compartiendo bucket con Claude cuando no hay bucket GPT separado): crítico (8%)
        let eff_gpt = QuotaCollector::evaluate_account_for_model(&acc, "gpt-4o");
        assert_eq!(eff_gpt.family, "GPT");
        assert_eq!(eff_gpt.rem_5h, 8);
        assert!(eff_gpt.is_critical);

        // 4. Con modelo DeepSeek: en warning (25% restante => 75% usado, entre 70% y 85%)
        let eff_ds = QuotaCollector::evaluate_account_for_model(&acc, "deepseek-r1");
        assert_eq!(eff_ds.family, "DeepSeek");
        assert_eq!(eff_ds.rem_5h, 25);
        assert!(!eff_ds.is_critical);
        assert!(eff_ds.is_warning);

        // 5. Renderizado UI en Integraciones cuando la sesión activa usa Claude:
        let mut state = AppState::new();
        state.snapshot.accounts.push(acc);
        state.snapshot.agent_session.model = "claude-sonnet-4-5".to_string();
        state.snapshot.agent_session.local_prefix = Some("agy".to_string());
        state.snapshot.agent_session.all_active_prefixes.insert("agy".to_string());

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen_clean = ui::strip_ansi(&render_out.lines.join("\n"));
        assert!(
            screen_clean.contains("! 1 crític"),
            "Integraciones debe mostrar '! 1 crític' cuando el modelo activo en sesión es Claude y su pool está al 8%"
        );
        assert!(screen_clean.contains("[AGOTÁNDOSE]"));
        assert!(screen_clean.contains("DeepSeek Wk"));
        assert!(screen_clean.contains("DeepSeek 5h"));
    }
}
