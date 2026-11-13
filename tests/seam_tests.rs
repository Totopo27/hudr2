#[cfg(test)]
mod tests {
    use hudr2::models::{HerdrPane, TelemetrySnapshot};
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
            ..Default::default()
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
        assert!(render_out.lines[0].contains(":: HUDR2 ::"));
        assert!(!render_out.lines[0].contains("GENTLEMAN"));

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
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    fn test_mcp_config_larger_than_one_mib_is_ignored() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let opencode_json = dir.path().join("opencode.json");
        let mut config = String::from(r#"{"mcp":{"oversized":{"type":"local","enabled":true}}}"#);
        config.push_str(&" ".repeat(1024 * 1024));
        std::fs::write(opencode_json, config).unwrap();
        let pi_dir = dir.path().join("pi-agent");
        std::fs::create_dir(&pi_dir).unwrap();
        std::fs::write(
            pi_dir.join("mcp.json"),
            r#"{"mcpServers":{"fallback":{"type":"remote","enabled":true}}}"#,
        )
        .unwrap();
        unsafe {
            std::env::set_var("PI_AGENT_DIR", &pi_dir);
        }

        let mcps = read_mcp_servers(dir.path().to_str().unwrap());
        unsafe {
            std::env::remove_var("PI_AGENT_DIR");
        }

        assert!(
            mcps.iter().all(|m| m.name != "oversized"),
            "Las configuraciones MCP mayores de 1 MiB deben ignorarse"
        );
        assert!(
            mcps.iter().any(|m| m.name == "fallback"),
            "Ignorar un archivo sobredimensionado no debe impedir procesar los demás candidatos"
        );
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
        assert!(render_out.lines[0].contains(":: HUDR2 ::"));
        assert!(!render_out.lines[0].contains("GENTLEMAN"));

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
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            ..Default::default()
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

    #[test]
    fn test_force_refresh_updates_both_context_and_quotas() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::app::force_refresh;
        use std::sync::Arc;
        use tokio::sync::RwLock;

        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("CLIPROXY_AUTHS_DIR", tmp.path());
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let app_state = Arc::new(RwLock::new(AppState::new()));

            // Ejecutar force_refresh interactivo sin golpear credenciales reales del host
            force_refresh(&app_state, true, true).await;

            let state = app_state.read().await;
            // El flash notice debe reflejar que actualizó cuotas y contexto
            assert!(state.flash_notice.is_some());
            let notice = state.flash_notice.as_ref().unwrap();
            assert!(notice.contains("contexto") || notice.contains("Contexto"));
        });

        unsafe {
            std::env::remove_var("CLIPROXY_AUTHS_DIR");
        }
    }

    #[test]
    fn test_pane_priority_focused_and_working_over_current_cwd() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
        use hudr2::quotas::apply_quota_data;

        let mut acc = AccountQuota {
            prefix: "agy2_1".to_string(),
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

        apply_quota_data(&mut acc, &payload);

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

        apply_quota_data(&mut acc, &critical_payload);
        assert_eq!(acc.rem_5h, 10);
        assert!(
            acc.is_critical,
            "Debe ser crítico cuando Gemini baja del 15% restante"
        );
    }

    #[test]
    fn test_ui_integraciones_card_reflects_gemini_sovereignty_without_false_criticals() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::apply_quota_data;
        use hudr2::state::AppState;

        let mut state = AppState::new();

        // Creamos una cuenta agy2_1 exactamente como en la captura del usuario:
        // Gemini al 98% restante, Claude al 9% restante (gastado)
        let mut acc = AccountQuota {
            prefix: "agy2_1".to_string(),
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

        apply_quota_data(&mut acc, &quota_payload);

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
        state.toggle_all_collapse();
        assert!(
            state.needs_redraw,
            "toggle_all_collapse debe marcar needs_redraw = true"
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
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

    #[test]
    fn test_refresh_all_skips_tree_when_collapsed() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
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
        });
    }

    #[test]
    fn test_refresh_all_preserves_in_flight_tool_update_progress() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let app_state = std::sync::Arc::new(tokio::sync::RwLock::new(AppState::new()));
            let progress = hudr2::models::ToolUpdateProgress {
                percent: 42,
                stage: "descargando".to_string(),
                spinner_idx: 3,
                elapsed_secs: 7,
            };
            {
                let mut state = app_state.write().await;
                state.collapsed_cards[2] = true;
                state.snapshot.ecosystem = hudr2::models::EcosystemTelemetry {
                    tools: vec![hudr2::models::ToolVersionInfo {
                        name: "pi".to_string(),
                        version: Some("v0.87.0".to_string()),
                        is_installed: true,
                        is_updating: true,
                        update_progress: Some(progress.clone()),
                        ..Default::default()
                    }],
                };
            }

            // A watcher/Herdr event triggers a full refresh while the update is running.
            hudr2::app::refresh_all(&app_state).await;

            let state = app_state.read().await;
            let pi = state
                .snapshot
                .ecosystem
                .tools
                .iter()
                .find(|t| t.name == "pi")
                .expect("refreshed ecosystem must still list pi");
            assert!(
                pi.is_updating,
                "refresh_all must not clear an in-flight update"
            );
            assert_eq!(pi.update_progress, Some(progress));

            let render = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 56);
            let text = render.lines.join("\n");
            assert!(
                text.contains("42%]") && text.contains("[actualizando]"),
                "the progress monitor must stay visible after a refresh"
            );
            for tool in state
                .snapshot
                .ecosystem
                .tools
                .iter()
                .filter(|t| t.name != "pi")
            {
                assert!(!tool.is_updating && tool.update_progress.is_none());
            }
        });
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
    fn test_read_project_tree_keeps_globally_best_entries_at_capacity() {
        use hudr2::tree::read_project_tree;

        let tmp = tempfile::tempdir().unwrap();
        for name in ["z-dir", "a-dir", "m-dir"] {
            std::fs::create_dir(tmp.path().join(name)).unwrap();
        }
        for index in (0..2005).rev() {
            std::fs::write(tmp.path().join(format!("file-{index:04}.txt")), b"").unwrap();
        }

        let tree_data = read_project_tree(tmp.path().to_str().unwrap());
        let names: Vec<_> = tree_data
            .tree
            .iter()
            .map(|node| node.name.as_str())
            .collect();

        assert_eq!(names.len(), 2000);
        assert_eq!(&names[..3], ["a-dir", "m-dir", "z-dir"]);
        assert_eq!(names[3], "file-0000.txt");
        assert_eq!(names[1999], "file-1996.txt");
        assert!(!names.contains(&"file-1997.txt"));
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
        use hudr2::quotas::is_available;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("CLIPROXY_AUTHS_DIR", tmp.path().to_str().unwrap());
        }
        // En una carpeta vacía, is_available() debe ser false
        assert!(!is_available());

        // Con credencial JSON válida, is_available() debe ser true
        let auth_file = tmp.path().join("account.json");
        std::fs::write(
            &auth_file,
            r#"{"access_token": "token123", "email": "test@example.com"}"#,
        )
        .unwrap();
        assert!(is_available());

        unsafe {
            std::env::remove_var("CLIPROXY_AUTHS_DIR");
        }
    }

    #[test]
    fn test_ui_empty_accounts_renders_inactive_rows_gracefully() {
        let mut state = AppState::new();
        state.snapshot.accounts.clear();
        state.collapsed_cards[3] = false; // Integraciones expandida
        state.collapsed_cards[5] = false; // Cuota de Modelos expandida

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
        use hudr2::models::AgentSession;

        // 1. Verificar equivalencia de tipo y effective_model()
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

        // Validar effective_model
        assert_eq!(
            state.snapshot.agent_session.effective_model(),
            "anthropic/claude-3-5-sonnet"
        );
        assert_eq!(state.snapshot.agent_session.provider_name, "pi");
        assert_eq!(state.snapshot.agent_session.tool_total, 13);
    }

    #[test]
    fn test_quota_error_handling_and_orphan_purge() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::{apply_quota_error, calculate_quota_pace, discover_accounts};

        let mut acc = AccountQuota::default();

        // 1. 401 -> expirado (acortado para no desbordar en terminales compactas)
        let err_401 = serde_json::json!({
            "error": { "code": 401, "status": "UNAUTHENTICATED", "message": "Token expired" }
        });
        apply_quota_error(&mut acc, 401, &err_401);
        assert!(acc.has_error);
        assert_eq!(acc.error_msg, "expirado");

        // 2. 403 TOS_VIOLATION -> tos suspendida
        let err_tos = serde_json::json!({
            "error": {
                "code": 403,
                "status": "PERMISSION_DENIED",
                "message": "This service has been disabled in this account for violation of Terms of Service.",
                "details": [{ "reason": "TOS_VIOLATION" }]
            }
        });
        apply_quota_error(&mut acc, 403, &err_tos);
        assert!(acc.has_error);
        assert_eq!(acc.error_msg, "tos suspendida");

        // 3. 429 -> rate limited
        let err_429 = serde_json::json!({
            "error": { "code": 429, "status": "RESOURCE_EXHAUSTED" }
        });
        apply_quota_error(&mut acc, 429, &err_429);
        assert!(acc.has_error);
        assert_eq!(acc.error_msg, "rate limited");

        // 4. Purga de cuentas huérfanas cuando un archivo .json se elimina
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("CLIPROXY_AUTHS_DIR", tmp.path().to_str().unwrap());
        }
        let f1 = tmp.path().join("antigravity-acc1.json");
        let f2 = tmp.path().join("antigravity-acc2.json");
        std::fs::write(&f1, r#"{"access_token":"t1","email":"a1@test.com"}"#).unwrap();
        std::fs::write(&f2, r#"{"access_token":"t2","email":"a2@test.com"}"#).unwrap();

        let mut list = Vec::new();
        discover_accounts(&mut list);
        assert_eq!(list.len(), 2);

        // Eliminar f2 y redescubrir: debe purgar acc2
        std::fs::remove_file(&f2).unwrap();
        discover_accounts(&mut list);
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
                    ..Default::default()
                },
                ToolVersionInfo {
                    name: "hudr2".to_string(),
                    version: Some("v0.1.0".to_string()),
                    is_installed: true,
                    ..Default::default()
                },
                ToolVersionInfo {
                    name: "gentle-ai".to_string(),
                    version: None,
                    is_installed: false,
                    ..Default::default()
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

        // Clic en la tarjeta de ecosistema alterna el colapso (idx 11)
        assert!(!state.collapsed_cards[11]);
        let res = state.handle_mouse_click(eco_bound.start_line, 0);
        assert!(matches!(res, Some(ClickAction::ToggledCard(11))));
        assert!(state.collapsed_cards[11]);
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
            &state.hidden_cards,
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
            &state.hidden_cards,
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
        assert!(
            cfg_path.exists(),
            "El archivo de configuración debe haberse guardado"
        );

        // Recargar en un nuevo AppState y comprobar que recuerda las tarjetas colapsadas
        let st2 = AppState::with_config(Some(cfg_path), None);
        assert!(st2.collapsed_cards[0]);
        assert!(!st2.collapsed_cards[2]); // arranca expandido por defecto
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
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        let handoff = tele
            .last_handoff
            .as_ref()
            .expect("debe extraer last_handoff");
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

    #[test]
    fn test_refresh_all_skips_redraw_when_snapshot_unchanged() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp_dir = tempfile::tempdir().unwrap();
        let fake_db = temp_dir.path().join("fake_opencode.db");
        std::fs::File::create(&fake_db).unwrap();
        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", fake_db.to_str().unwrap());
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let app_state = std::sync::Arc::new(tokio::sync::RwLock::new(AppState::new()));
            {
                let mut state = app_state.write().await;
                state.snapshot.project_dir = temp_dir.path().to_str().unwrap().to_string();
            }
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
        });

        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
        }
    }

    #[test]
    fn test_dynamic_model_family_quota_health_claude_gpt_deepseek_gemini() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::{apply_quota_data, evaluate_account_for_model};

        let mut acc = AccountQuota {
            prefix: "proxy_acc".to_string(),
            auth_file: "proxy_acc.json".to_string(),
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

        apply_quota_data(&mut acc, &payload);

        // 1. Con modelo Claude: crítico (8% 5h <= 15%)
        let eff_cla = evaluate_account_for_model(&acc, "claude-sonnet-4-5");
        assert_eq!(eff_cla.family, "Claude");
        assert_eq!(eff_cla.rem_5h, 8);
        assert_eq!(eff_cla.rem_wk, 45);
        assert!(
            eff_cla.is_critical,
            "Claude al 8% debe marcarse crítico cuando la sesión usa Claude"
        );

        // 2. Con modelo GPT (compartiendo bucket con Claude cuando no hay bucket GPT separado): crítico (8%)
        let eff_gpt = evaluate_account_for_model(&acc, "gpt-4o");
        assert_eq!(eff_gpt.family, "GPT");
        assert_eq!(eff_gpt.rem_5h, 8);
        assert!(eff_gpt.is_critical);

        // 3. Con modelo DeepSeek: en warning (25% restante => 75% usado, entre 70% y 85%)
        let eff_ds = evaluate_account_for_model(&acc, "deepseek-r1");
        assert_eq!(eff_ds.family, "DeepSeek");
        assert_eq!(eff_ds.rem_5h, 25);
        assert!(!eff_ds.is_critical);
        assert!(eff_ds.is_warning);

        // 4. Renderizado UI en Integraciones cuando la sesión activa usa Claude:
        let mut state = AppState::new();
        state.snapshot.accounts.push(acc);
        state.snapshot.agent_session.model = "claude-sonnet-4-5".to_string();
        state.snapshot.agent_session.local_prefix = Some("proxy_acc".to_string());
        state
            .snapshot
            .agent_session
            .all_active_prefixes
            .insert("proxy_acc".to_string());

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

    #[test]
    fn test_gemini_argon_separate_pools_and_active_model_sovereignty() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::{apply_quota_data, evaluate_account_for_model};
        use hudr2::state::AppState;

        let mut acc = AccountQuota {
            prefix: "agy3".to_string(),
            auth_file: "auth_agy3.json".to_string(),
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

        // Payload idéntico al de Google CLIProxyAPI con Gemini Models, Claude/GPT y Gemini 4 Argon
        let payload = serde_json::json!({
            "description": "Within each group, models share a weekly limit and a 5-hour limit.",
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "description": "Models within this group: Gemini Flash, Gemini Pro",
                    "buckets": [
                        { "bucketId": "gemini-weekly", "window": "weekly", "remainingFraction": 0.4858, "resetTime": "2026-11-14T00:17:08Z" },
                        { "bucketId": "gemini-5h", "window": "5h", "remainingFraction": 0.9899, "resetTime": "2026-11-09T18:26:03Z" }
                    ]
                },
                {
                    "displayName": "Claude and GPT models",
                    "description": "Models within this group: Claude Opus, Claude Sonnet, GPT-OSS",
                    "buckets": [
                        { "bucketId": "3p-weekly", "window": "weekly", "remainingFraction": 0.0003, "resetTime": "2026-11-12T13:37:58Z" },
                        { "bucketId": "3p-5h", "window": "5h", "remainingFraction": 1.0, "resetTime": "2026-11-09T18:29:20Z" }
                    ]
                },
                {
                    "displayName": "Gemini 4 Argon",
                    "description": "For paid Pro plans, Argon quota is separate.",
                    "buckets": [
                        { "bucketId": "gemini-argon-wtu-weekly", "window": "weekly", "remainingFraction": 0.0150, "resetTime": "2026-11-14T00:34:17Z" }
                    ]
                }
            ]
        });

        apply_quota_data(&mut acc, &payload);

        // 1. Verificar que Gemini base NO fue sobreescrito por Argon
        assert_eq!(
            acc.rem_5h, 99,
            "Gemini 5h debe reflejar Gemini Models (99%)"
        );
        assert_eq!(
            acc.rem_wk, 49,
            "Gemini Wk debe reflejar Gemini Models (49%), no el 2% de Argon"
        );
        assert!(
            !acc.is_critical,
            "La cuenta base Gemini no debe ser crítica con 49% restante"
        );

        // 2. Verificar que los pools incluyen Gemini, Claude y Argon por separado
        assert!(
            acc.pools
                .iter()
                .any(|p| p.label == "Gemini Wk" && p.percent == 49)
        );
        assert!(
            acc.pools
                .iter()
                .any(|p| p.label == "Gemini 5h" && p.percent == 99)
        );
        assert!(
            acc.pools
                .iter()
                .any(|p| p.label == "Claude Wk" && p.percent == 0)
        );
        assert!(
            acc.pools
                .iter()
                .any(|p| p.label == "Claude 5h" && p.percent == 100)
        );
        assert!(
            acc.pools
                .iter()
                .any(|p| p.label == "Argon Wk" && p.percent == 2)
        );
        // Argon no tiene bucket 5h en Pro plan, no debe inventarse
        assert!(!acc.pools.iter().any(|p| p.label == "Argon 5h"));

        // 3. Con modelo activo Gemini 3.8 Flash (High): la sesión es saludable, NO crítica
        let eff_gem = evaluate_account_for_model(&acc, "gemini-3.8-flash-high");
        assert_eq!(eff_gem.family, "Gemini");
        assert_eq!(eff_gem.rem_5h, 99);
        assert_eq!(eff_gem.rem_wk, 49);
        assert!(
            !eff_gem.is_critical,
            "Gemini Flash High no debe marcarse crítico por cuota de Argon"
        );
        assert!(!eff_gem.is_warning);

        // 4. Con cuentas Antigravity (agy3): la cuota principal soberana en Integraciones es GEMINI MODELS.
        // Incluso si la sesión activa usa Claude Opus o Gemini Argon, Integraciones evalúa la salud
        // global de la cuenta basándose en GEMINI MODELS (99% 5h, 49% Wk), sin alarmas falsas de [AGOTÁNDOSE].
        let eff_claude = evaluate_account_for_model(&acc, "claude-opus-5-5");
        assert_eq!(eff_claude.family, "Gemini");
        assert_eq!(eff_claude.rem_wk, 49);
        assert!(
            !eff_claude.is_critical,
            "Claude Opus como orquestador no debe marcar crítico agy3 en Integraciones"
        );

        let eff_argon = evaluate_account_for_model(&acc, "gemini-4-argon");
        assert_eq!(eff_argon.family, "Gemini");
        assert_eq!(eff_argon.rem_wk, 49);
        assert!(
            !eff_argon.is_critical,
            "Argon secundario no debe marcar crítico agy3 en Integraciones"
        );

        // 5. Renderizado en UI:
        // A) Cuando la sesión activa usa Claude Opus:
        let mut state = AppState::new();
        state.snapshot.accounts.push(acc.clone());
        state.snapshot.agent_session.model = "claude-opus-5-5".to_string();
        state.snapshot.agent_session.local_prefix = Some("agy3".to_string());
        state
            .snapshot
            .agent_session
            .all_active_prefixes
            .insert("agy3".to_string());

        let render_claude = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen_claude = ui::strip_ansi(&render_claude.lines.join("\n"));
        assert!(
            !screen_claude.contains("[AGOTÁNDOSE]"),
            "Integraciones no debe mostrar [AGOTÁNDOSE] para agy3 cuando se orquesta con Claude y Gemini Wk tiene 49%"
        );
        assert!(
            !screen_claude.contains("! 1 crític"),
            "Integraciones no debe alertar '! 1 crític' para agy3 cuando se orquesta con Claude"
        );
        // Los pools de Claude y Argon siguen visibles en la card de desglose de cuenta (Cuota de Modelos):
        assert!(screen_claude.contains("Gemini Wk"));
        assert!(screen_claude.contains("Claude Wk"));
        assert!(screen_claude.contains("Argon Wk"));

        // B) Cuando la sesión activa usa Gemini 3.8 Flash:
        state.snapshot.agent_session.model = "gemini-3.8-flash-high".to_string();
        let render_gemini = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen_gem = ui::strip_ansi(&render_gemini.lines.join("\n"));
        assert!(
            !screen_gem.contains("[AGOTÁNDOSE]"),
            "No debe mostrar [AGOTÁNDOSE] cuando se usa Gemini Flash y su cuota es 49%"
        );
        assert!(
            !screen_gem.contains("! 1 crític"),
            "Integraciones no debe alertar '! 1 crític' cuando el modelo activo es Gemini Flash"
        );
    }

    #[test]
    fn test_read_git_head_short_zero_subprocess_and_packed_refs() {
        use hudr2::git::{find_git_root_and_dir, read_git_head_short};

        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        let heads_dir = git_dir.join("refs").join("heads");
        std::fs::create_dir_all(&heads_dir).unwrap();

        // 1. Referencia suelta en .git/refs/heads/main
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            heads_dir.join("main"),
            "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678\n",
        )
        .unwrap();
        assert_eq!(read_git_head_short(&git_dir).as_deref(), Some("a1b2c3d"));
        let (_, dir_gd) = find_git_root_and_dir(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(read_git_head_short(&dir_gd).as_deref(), Some("a1b2c3d"));

        // 2. Fallback a packed-refs cuando la rama no tiene archivo suelto
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/feature/telar\n").unwrap();
        std::fs::write(
            git_dir.join("packed-refs"),
            "# pack-refs with: peeled fully-peeled sorted\n9f8e7d6c5b4a39281726354433221100aabbccdd refs/heads/feature/telar\n",
        )
        .unwrap();
        assert_eq!(read_git_head_short(&git_dir).as_deref(), Some("9f8e7d6"));

        // 3. Detached HEAD directo (40 hex chars)
        std::fs::write(
            git_dir.join("HEAD"),
            "deadbeefcafebabe1234567890abcdef12345678\n",
        )
        .unwrap();
        assert_eq!(read_git_head_short(&git_dir).as_deref(), Some("deadbee"));

        // 4. Worktree con archivo .git apuntando a gitdir + commondir
        let wt_git_dir = git_dir.join("worktrees").join("wt1");
        std::fs::create_dir_all(&wt_git_dir).unwrap();
        std::fs::write(wt_git_dir.join("HEAD"), "ref: refs/heads/feature/telar\n").unwrap();
        std::fs::write(wt_git_dir.join("commondir"), "../..\n").unwrap();

        let wt_root = tmp.path().join("wt1_workspace");
        std::fs::create_dir_all(&wt_root).unwrap();
        std::fs::write(
            wt_root.join(".git"),
            format!("gitdir: {}\n", wt_git_dir.display()),
        )
        .unwrap();

        let (_, wt_gd) = find_git_root_and_dir(wt_root.to_str().unwrap()).unwrap();
        assert_eq!(read_git_head_short(&wt_gd).as_deref(), Some("9f8e7d6"));
    }

    #[test]
    fn test_cross_session_conflicts_radar_and_ui_alert() {
        use hudr2::git::detect_cross_session_conflicts;

        // 1. Detección entre dos sesiones con rutas normalizadas
        let s1 = vec!["./src\\ui.rs".to_string(), "src/main.rs".to_string()];
        let s2 = vec!["src/ui.rs".to_string(), "Cargo.toml".to_string()];
        let s3 = vec!["README.md".to_string()];

        let conflicts = detect_cross_session_conflicts(&[s1.clone(), s2]);
        assert_eq!(conflicts, vec!["src/ui.rs".to_string()]);

        let no_conflicts = detect_cross_session_conflicts(&[s1, s3]);
        assert!(no_conflicts.is_empty());

        // 2. Renderizado en la tarjeta Cambios (Git) respetando anchos estrechos y estándar
        let mut state = AppState::new();
        state.collapsed_cards[5] = false;
        state.snapshot.git.branch = "main".to_string();
        state.snapshot.git.cross_session_conflicts =
            vec!["src/ui.rs".to_string(), "src/git.rs".to_string()];

        for w in [36usize, 48usize] {
            let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, w);
            for line in &render_out.lines {
                assert!(
                    ui::string_width(line) <= w,
                    "Línea excede ancho {w}: {}",
                    ui::strip_ansi(line)
                );
            }
            let clean = ui::strip_ansi(&render_out.lines.join("\n"));
            assert!(clean.contains("! 2 colisión") || clean.contains("! CONFLICTO:"));
            assert!(clean.contains("src/ui.rs"));
        }
    }

    #[test]
    fn test_herdr_metadata_tokens_parsing_and_pi_live_telemetry() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::herdr::HerdrClient;
        use hudr2::models::AccountQuota;
        use hudr2::pi::try_read_pi_session;
        use serde_json::json;

        let pane_json = json!({
            "pane_id": "w1:p9",
            "tab_id": "w1:t1",
            "workspace_id": "w1",
            "cwd": "D:/telar_proj",
            "focused": true,
            "agent_status": "working",
            "agent": "pi",
            "tokens": {
                "profile": "cliproxy/agy2_1/claude-sonnet-4-5",
                "ctx_tok": "64000",
                "ctx_win": "200000",
                "tok_in": "52000",
                "tok_out": "12000",
                "cost": "0.185",
                "git_files": "4",
                "git_lines": "120",
                "todo_2": "prog:Integrar metadata_tokens",
                "todo_1": "done:Analizar TelarDev"
            }
        });

        let pane = HerdrClient::parse_pane_json(&pane_json).expect("debe parsear pane con tokens");
        assert_eq!(
            pane.tokens.get("ctx_tok").map(String::as_str),
            Some("64000")
        );

        let accounts = vec![AccountQuota {
            prefix: "agy2_1".to_string(),
            ..Default::default()
        }];

        let (_, ses) = try_read_pi_session(
            "D:/telar_proj",
            &[pane],
            &accounts,
            Some("w1:hud"),
            Some("w1:t1"),
        )
        .expect("debe construir sesión viva desde metadata_tokens sin requerir .jsonl en disco");

        assert_eq!(ses.model_short, "claude-sonnet-4-5");
        assert_eq!(ses.local_prefix.as_deref(), Some("agy2_1"));
        assert_eq!(ses.active_turn_tokens, 64_000);
        assert_eq!(ses.context_limit, 200_000);
        assert_eq!(ses.tokens_input, 52_000);
        assert_eq!(ses.tokens_output, 12_000);
        assert!((ses.cost - 0.185).abs() < 1e-6);
        assert_eq!(ses.attributed_changes.files_count, 4);
        assert_eq!(ses.attributed_changes.additions, 120);
        assert_eq!(ses.todos.len(), 2);
        assert_eq!(ses.todos[0].title, "Analizar TelarDev");
        assert_eq!(ses.todos[0].status, "done");
        assert_eq!(ses.todos[1].title, "Integrar metadata_tokens");
        assert_eq!(ses.todos[1].status, "in_progress");
    }

    #[test]
    fn test_quick_peek_modal_lifecycle_scroll_and_sensitive_guard() {
        let mut state = AppState::new();

        // 1. Archivo sensible siempre queda bloqueado en Quick Peek
        state.open_quick_peek(".env.production");
        let peek_sec = state.quick_peek.as_ref().expect("quick_peek debe abrirse");
        assert_eq!(
            peek_sec.lines,
            vec!["[archivo sensible protegido - diff oculto]"]
        );
        state.close_quick_peek();
        assert!(state.quick_peek.is_none());

        // 2. Quick Peek con múltiples líneas, scroll y cierre por clic en el modal
        state.quick_peek = Some(hudr2::state::QuickPeekState {
            title: "src/git.rs".to_string(),
            lines: (1..=20).map(|i| format!("+línea de prueba {i}")).collect(),
            scroll: 0,
        });

        state.scroll_down();
        assert_eq!(state.quick_peek.as_ref().unwrap().scroll, 2);
        state.scroll_up();
        assert_eq!(state.quick_peek.as_ref().unwrap().scroll, 0);

        let mut render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        ui::apply_quick_peek_overlay(
            &mut render_out,
            state.quick_peek.as_ref().unwrap(),
            0,
            48,
            24,
        );
        state.card_bounds = render_out.bounds.clone();

        let clean = ui::strip_ansi(&render_out.lines.join("\n"));
        assert!(clean.contains("Quick Peek"));
        assert!(clean.contains("src/git.rs"));
        assert!(clean.contains("+línea de prueba 1"));

        let close_bound = state
            .card_bounds
            .iter()
            .find(|b| b.action_type == Some("close_quick_peek"))
            .expect("debe existir bound para cerrar Quick Peek");
        let click_res = state.handle_mouse_click(close_bound.start_line, 0);
        assert_eq!(click_res, Some(ClickAction::CloseQuickPeek));
        assert!(state.quick_peek.is_none());
    }

    #[test]
    fn test_tarjetas_card_renders_and_toggles_visibility() {
        let mut state = AppState::new();
        // Expandir card de Personalizar (idx 12)
        state.collapsed_cards[12] = false;

        let render_out = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        let clean = ui::strip_ansi(&render_out.lines.join("\n"));

        // Comprobar que aparecen los items con sus casillas universales [x] sin símbolos rotos [?]
        assert!(
            clean.contains("Personalizar"),
            "debe mostrar el título Personalizar"
        );
        assert!(clean.contains("[x] Estado"));
        assert!(clean.contains("[x] Contexto"));
        assert!(clean.contains("[x] Proyecto"));
        assert!(
            !clean.contains("fija"),
            "la fila fija redundante fue removida"
        );
        assert!(
            !clean.contains("[x] Personalizar"),
            "no debe listar Personalizar dentro de Personalizar"
        );
        assert!(clean.contains("Nombre"), "debe incluir sección de Nombre");
        assert!(clean.contains("HUDR2"), "debe indicar nombre por defecto");
        assert!(
            clean.contains("[editar >]"),
            "debe mostrar botón interactivo de editar nombre"
        );
        assert!(
            clean.contains("Tema"),
            "debe incluir sección interactiva de Tema"
        );
        assert!(clean.contains("[cambiar >]"));

        // Ocultar card "proyecto"
        state.toggle_card_visibility("proyecto");
        assert!(state.is_card_hidden("proyecto"));

        let render_out_hidden = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        let clean_hidden = ui::strip_ansi(&render_out_hidden.lines.join("\n"));

        // La tarjeta Proyecto NO debe renderizarse en absoluto
        assert!(!clean_hidden.contains("~ Proyecto ·"));
        // Pero en la card Personalizar debe figurar como [ ] Proyecto oculta
        assert!(clean_hidden.contains("[ ] Proyecto"));
        assert!(clean_hidden.contains("oculta"));

        // Reactivar "proyecto"
        state.toggle_card_visibility("proyecto");
        assert!(!state.is_card_hidden("proyecto"));

        let render_out_restored = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        let clean_restored = ui::strip_ansi(&render_out_restored.lines.join("\n"));
        assert!(clean_restored.contains("[x] Proyecto"));
        assert!(clean_restored.contains("~ Proyecto ·"));
    }

    #[test]
    fn test_tarjetas_cannot_hide_itself() {
        let mut state = AppState::new();
        state.toggle_card_visibility("personalizar");
        assert!(
            !state.is_card_hidden("personalizar"),
            "Personalizar no puede ocultarse a sí misma"
        );
        // Compatibilidad hacia atrás con id legado
        state.toggle_card_visibility("tarjetas");
        assert!(
            !state.is_card_hidden("tarjetas"),
            "id legado tarjetas no puede ocultarse"
        );
    }

    #[test]
    fn test_tarjetas_mouse_click_toggles_visibility() {
        let mut state = AppState::new();
        state.collapsed_cards[12] = false; // Expandir

        let render_out = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        state.card_bounds = render_out.bounds;

        // Buscar el bound de toggle para "engram"
        let engram_toggle = state
            .card_bounds
            .iter()
            .find(|b| {
                b.action_type == Some("toggle_card_visibility")
                    && b.action_path.as_deref() == Some("engram")
            })
            .expect("debe existir bound para alternar visibilidad de engram");

        let click_res = state.handle_mouse_click(engram_toggle.start_line, 0);
        assert_eq!(
            click_res,
            Some(ClickAction::ToggleCardVisibility("engram".to_string()))
        );
        assert!(
            state.is_card_hidden("engram"),
            "engram debe quedar oculto tras el clic"
        );
    }

    #[test]
    fn test_tarjetas_config_persistence() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("hudr2.json");

        let mut st = AppState::with_config(Some(cfg_path.clone()), None);
        st.toggle_card_visibility("ecosystem");
        st.toggle_card_visibility("engram");
        if st.collapsed_cards[12] {
            st.toggle_card(12);
        }
        st.set_sidebar_name(Some("Mi Proyecto HUD".to_string()));
        st.cycle_theme(); // avanza a "linear"

        assert!(
            cfg_path.exists(),
            "hudr2.json debe existir tras persistencia atómica"
        );

        // Cargar en un nuevo AppState
        let st2 = AppState::with_config(Some(cfg_path), None);
        assert!(st2.is_card_hidden("ecosystem"));
        assert!(st2.is_card_hidden("engram"));
        assert!(!st2.is_card_hidden("estado"));
        assert!(!st2.collapsed_cards[12]); // recuerda el estado colapsado/expandido
        assert_eq!(st2.sidebar_name.as_deref(), Some("Mi Proyecto HUD"));
        assert_eq!(st2.theme.as_deref(), Some("linear"));
        assert_eq!(
            st2.snapshot.sidebar_name.as_deref(),
            Some("Mi Proyecto HUD")
        );
        assert_eq!(st2.snapshot.theme_name.as_deref(), Some("linear"));
    }

    #[test]
    fn test_sidebar_name_custom_style() {
        let mut snap = TelemetrySnapshot::default();
        let collapsed = [true; 13];

        // 1. Fallback por defecto: HUDR2
        let out_default = ui::render_screen(&snap, &collapsed, None, 48);
        let header_def = ui::strip_ansi(&out_default.lines[0]);
        assert!(header_def.contains(":: HUDR2 ::"));

        // 2. Nombre personalizado del sidebar
        snap.sidebar_name = Some("Gentle Shell".to_string());
        let out_custom = ui::render_screen(&snap, &collapsed, None, 48);
        let header_custom = ui::strip_ansi(&out_custom.lines[0]);
        assert!(header_custom.contains(":: Gentle Shell ::"));

        // 3. Personalizar card muestra el nombre y el botón interactivo de edición
        let mut collapsed_open = [true; 13];
        collapsed_open[12] = false;
        let out_card = ui::render_screen(&snap, &collapsed_open, None, 48);
        let clean_card = ui::strip_ansi(&out_card.lines.join("\n"));
        assert!(clean_card.contains("Nombre  Gentle Shell"));
        assert!(clean_card.contains("[editar >]"));

        // 4. Protección contra desbordamiento en anchos reducidos (ancho 20)
        snap.sidebar_name = Some("Nombre Extremadamente Largo Que Desborda El Panel".to_string());
        let out_narrow = ui::render_screen(&snap, &collapsed, None, 20);
        let narrow_header = &out_narrow.lines[0];
        assert!(
            ui::string_width(narrow_header) <= 20,
            "el header nunca debe exceder el ancho"
        );
    }

    #[test]
    fn test_personalizar_name_inline_editing_lifecycle() {
        let mut state = AppState::new();
        state.collapsed_cards[12] = false; // Expandir Personalizar

        // 1. Estado inicial
        assert!(!state.is_editing_sidebar_name);
        assert_eq!(state.name_input_buffer, "");
        assert_eq!(state.sidebar_name, None);

        // 2. Iniciar edición (tecla 'n' o clic en [editar >])
        state.start_edit_sidebar_name();
        assert!(state.is_editing_sidebar_name);
        assert_eq!(state.name_input_buffer, "HUDR2");
        assert!(state.snapshot.is_editing_name);

        // 3. Verificar renderizado en modo edición
        let render_edit = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        let clean_edit = ui::strip_ansi(&render_edit.lines.join("\n"));
        assert!(clean_edit.contains("Nombre  > HUDR2_"));
        assert!(clean_edit.contains("[Enter|Esc]"));
        assert!(clean_edit.contains("Enter: guardar"));

        // 4. Modificar buffer y guardar
        state.update_name_input_buffer("Gentle Shell 3".to_string());
        state.commit_sidebar_name();

        assert!(!state.is_editing_sidebar_name);
        assert_eq!(state.sidebar_name.as_deref(), Some("Gentle Shell 3"));
        assert_eq!(
            state.snapshot.sidebar_name.as_deref(),
            Some("Gentle Shell 3")
        );
        assert!(
            state
                .flash_notice
                .as_ref()
                .unwrap()
                .contains("Gentle Shell 3")
        );

        // 5. Verificar que el header superior refleja el nuevo nombre
        let render_saved = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        let header_line = ui::strip_ansi(&render_saved.lines[0]);
        assert!(header_line.contains(":: Gentle Shell 3 ::"));

        // 6. Iniciar edición de nuevo y cancelar con Esc
        state.start_edit_sidebar_name();
        assert_eq!(state.name_input_buffer, "Gentle Shell 3");
        state.update_name_input_buffer("Algo No Guardado".to_string());
        state.cancel_edit_sidebar_name();

        assert!(!state.is_editing_sidebar_name);
        assert_eq!(state.sidebar_name.as_deref(), Some("Gentle Shell 3"));

        // 7. Restablecer al nombre por defecto escribiendo vacío o HUDR2
        state.start_edit_sidebar_name();
        state.update_name_input_buffer("".to_string());
        state.commit_sidebar_name();

        assert_eq!(state.sidebar_name, None);
        assert!(state.flash_notice.as_ref().unwrap().contains("defecto"));
    }

    #[test]
    fn test_personalizar_mouse_interaction_and_double_click() {
        let mut state = AppState::new();
        state.collapsed_cards[12] = false;

        let render_out = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        state.card_bounds = render_out.bounds;

        // 1. Clic simple en la fila Nombre ([editar >]) activa la edición
        let edit_row = state
            .card_bounds
            .iter()
            .find(|b| b.action_type == Some("edit_sidebar_name"))
            .expect("debe existir bound interactivo para editar nombre")
            .start_line;

        let click_action = state.handle_mouse_click(edit_row, 0);
        assert_eq!(click_action, Some(ClickAction::StartEditSidebarName));
        assert!(state.is_editing_sidebar_name);

        // Cancelar para probar doble clic
        state.cancel_edit_sidebar_name();
        assert!(!state.is_editing_sidebar_name);

        // 2. Doble clic en la tarjeta Personalizar activa la edición
        let card_header = state
            .card_bounds
            .iter()
            .find(|b| b.id == "personalizar" && b.action_type.is_none())
            .expect("debe existir bound de la card personalizar")
            .start_line;

        // Primer clic (colapsa la card)
        let _ = state.handle_mouse_click(card_header, 0);
        // Segundo clic rápido en la misma fila (dentro de 500ms) -> detectado como doble clic
        let dbl_click = state.handle_mouse_click(card_header, 0);
        assert_eq!(dbl_click, Some(ClickAction::StartEditSidebarName));
        assert!(state.is_editing_sidebar_name);
    }

    #[test]
    fn test_theme_cycle_interaction_and_persistence() {
        let mut state = AppState::new();
        state.collapsed_cards[12] = false; // Expandir Tarjetas

        let render_out = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        state.card_bounds = render_out.bounds;

        // Buscar la línea de bound para ciclar tema
        let theme_start_line = state
            .card_bounds
            .iter()
            .find(|b| b.action_type == Some("cycle_theme"))
            .expect("debe existir bound interactivo para ciclar tema")
            .start_line;

        // Clic en la fila de tema
        let click_res = state.handle_mouse_click(theme_start_line, 0);
        assert_eq!(
            click_res,
            Some(ClickAction::CycleTheme("linear".to_string())),
            "primer ciclo debe avanzar a linear"
        );
        assert_eq!(state.theme.as_deref(), Some("linear"));
        assert_eq!(hudr2::theme::active_theme().id, "linear");
        assert!(
            state
                .flash_notice
                .as_ref()
                .unwrap()
                .contains("Linear Studio")
        );

        // Siguiente ciclo avanza a "cyber"
        let click_res2 = state.handle_mouse_click(theme_start_line, 0);
        assert_eq!(
            click_res2,
            Some(ClickAction::CycleTheme("cyber".to_string()))
        );
        assert_eq!(state.theme.as_deref(), Some("cyber"));
        assert_eq!(hudr2::theme::active_theme().id, "cyber");
    }

    #[test]
    fn test_responsive_compact_layout_and_shortened_labels() {
        let mut state = AppState::new();
        // Expandir tarjetas clave para comprobar el layout compacto
        state.collapsed_cards[3] = false; // Integraciones
        state.collapsed_cards[5] = false; // Cuota de Modelos
        state.collapsed_cards[8] = false; // Engram
        state.collapsed_cards[12] = false; // Personalizar

        // Configurar cuentas con error de expirado
        let mut acc = hudr2::models::AccountQuota {
            prefix: "agy".to_string(),
            has_error: true,
            error_msg: "expirado".to_string(),
            ..Default::default()
        };
        acc.pools.push(hudr2::models::QuotaPool {
            label: "Gemini Wk".to_string(),
            percent: 100,
            reset: "4d 23h".to_string(),
            pace: "~0%".to_string(),
        });
        state.snapshot.accounts = vec![acc];
        state.snapshot.engram.online = true;
        state.snapshot.engram.obs_count = 12;

        for w in [32usize, 34usize, 36usize, 38usize, 42usize, 48usize] {
            let render_out = ui::render_screen_with_diff(
                &state.snapshot,
                &state.collapsed_cards,
                None,
                None,
                &[],
                &state.hidden_cards,
                w,
            );
            for line in &render_out.lines {
                let lw = ui::string_width(line);
                assert!(
                    lw <= w,
                    "En ancho {w}, línea excede con ancho {lw}: {}",
                    ui::strip_ansi(line)
                );
            }
            let clean = ui::strip_ansi(&render_out.lines.join("\n"));
            if w >= 34 {
                assert!(
                    clean.contains("[expirado]"),
                    "Debe mostrar [expirado] conciso en ancho {w}"
                );
            } else {
                assert!(
                    clean.contains("[exp]"),
                    "Debe mostrar [exp] ultra-compacto en ancho ultra-estrecho {w}"
                );
            }
            assert!(
                !clean.contains("[token expirado]"),
                "No debe usar la frase larga [token expirado] que desborda el panel"
            );
            assert!(
                clean.contains("Personalizar"),
                "Debe mostrar card Personalizar en ancho {w}"
            );
        }
    }

    #[test]
    fn test_header_banner_renders_only_hudr2_without_gentleman() {
        let state = AppState::new();
        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 44);

        assert!(render_out.lines[0].contains(":: HUDR2 ::"));
        assert!(!render_out.lines[0].contains("GENTLEMAN"));
    }

    #[test]
    fn test_opencode_modern_schema_session_part_and_cpamc_attribution() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // 1. Crear esquema moderno OpenCode 1.19+ (session, message, part)
        conn.execute(
            "CREATE TABLE session (
                id TEXT PRIMARY KEY,
                project_id TEXT,
                directory TEXT,
                title TEXT,
                model TEXT,
                cost REAL DEFAULT 0,
                tokens_input INTEGER DEFAULT 0,
                tokens_output INTEGER DEFAULT 0,
                tokens_reasoning INTEGER DEFAULT 0,
                tokens_cache_read INTEGER DEFAULT 0,
                tokens_cache_write INTEGER DEFAULT 0,
                time_created INTEGER,
                time_updated INTEGER,
                time_archived INTEGER
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                type TEXT,
                data TEXT,
                time_created INTEGER,
                time_updated INTEGER
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE part (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                message_id TEXT,
                type TEXT,
                data TEXT,
                time_created INTEGER,
                time_updated INTEGER
            )",
            [],
        )
        .unwrap();

        let target_dir = "d:/proyectosai/ai_video_studio_app";
        let now = 1715000000;

        conn.execute(
            "INSERT INTO session (id, directory, title, model, cost, tokens_input, tokens_output, time_created, time_updated, time_archived)
             VALUES ('ses_modern_test', ?, 'Refactor Task', 'gemini-3.8-flash-high (cuenta4)', 0.045, 12500, 1450, ?, ?, NULL)",
            rusqlite::params![target_dir, now - 100, now],
        )
        .unwrap();

        // Agregar una parte de herramienta
        conn.execute(
            "INSERT INTO part (id, session_id, message_id, type, data, time_created, time_updated)
             VALUES ('part_1', 'ses_modern_test', 'msg_1', 'tool', '{\"name\":\"read_file\",\"state\":{\"input\":{\"path\":\"src/main.rs\"}}}', ?, ?)",
            rusqlite::params![now - 50, now - 50],
        )
        .unwrap();

        // Agregar una parte de edición
        conn.execute(
            "INSERT INTO part (id, session_id, message_id, type, data, time_created, time_updated)
             VALUES ('part_2', 'ses_modern_test', 'msg_1', 'tool', '{\"name\":\"edit_file\",\"state\":{\"input\":{\"file_path\":\"src/lib.rs\"}}}', ?, ?)",
            rusqlite::params![now - 40, now - 40],
        )
        .unwrap();

        // Agregar una parte de finalización con tokens
        conn.execute(
            "INSERT INTO part (id, session_id, message_id, type, data, time_created, time_updated)
             VALUES ('part_3', 'ses_modern_test', 'msg_1', 'step-finish', '{\"tokens\":{\"input\":12500,\"output\":1450,\"reasoning\":200,\"cache\":{\"read\":1000,\"write\":0}}}', ?, ?)",
            rusqlite::params![now - 10, now - 10],
        )
        .unwrap();

        drop(conn);

        // Configurar variable de entorno para que hudr2 use esta base de datos
        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", db_path.to_str().unwrap());
        }
        hudr2::opencode::clear_message_stats_cache();

        let accounts = vec![
            hudr2::models::AccountQuota {
                prefix: "cuenta1".to_string(),
                ..Default::default()
            },
            hudr2::models::AccountQuota {
                prefix: "cuenta4".to_string(),
                ..Default::default()
            },
        ];

        let panes = vec![HerdrPane {
            pane_id: "w19:p1".to_string(),
            cwd: Some(target_dir.to_string()),
            agent_status: Some("working".to_string()),
            ..Default::default()
        }];

        let session =
            hudr2::opencode::read_opencode_session(target_dir, &panes, &accounts, None, None);

        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
        }
        hudr2::opencode::clear_message_stats_cache();

        assert_eq!(session.local_prefix.as_deref(), Some("cuenta4"));
        assert!(session.all_active_prefixes.contains("cuenta4"));
        assert_eq!(session.model, "gemini-3.8-flash-high");
        assert_eq!(session.tokens_input, 12500);
        assert_eq!(session.tokens_output, 1450);
        assert_eq!(session.active_turn_tokens, 12500 + 1450 + 200 + 1000);
        assert_eq!(session.tool_reads, 1);
        assert_eq!(session.tool_writes, 1);
        assert_eq!(session.attributed_changes.files_count, 1);
    }

    #[test]
    fn test_opencode_dual_schema_precedence_session_over_stale_session_v2() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        conn.execute(
            "CREATE TABLE session_v2 (
                id TEXT PRIMARY KEY,
                directory TEXT,
                title TEXT,
                model TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                time_archived INTEGER
            )",
            [],
        )
        .unwrap();

        conn.execute(
            "CREATE TABLE session (
                id TEXT PRIMARY KEY,
                directory TEXT,
                title TEXT,
                model TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                time_archived INTEGER
            )",
            [],
        )
        .unwrap();

        let target_dir = "d:/social-hub-pro";
        let stale_time = 10000;
        let modern_time = 50000;

        conn.execute(
            "INSERT INTO session_v2 (id, directory, title, model, time_created, time_updated, time_archived)
             VALUES ('stale_ses', ?, 'Old Task', 'cuenta1/gemini-2.5-pro', ?, ?, NULL)",
            rusqlite::params![target_dir, stale_time, stale_time],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO session (id, directory, title, model, time_created, time_updated, time_archived)
             VALUES ('active_ses', ?, 'Modern Task', 'gemini-3.8-flash-high (cuenta4)', ?, ?, NULL)",
            rusqlite::params![target_dir, modern_time, modern_time],
        )
        .unwrap();

        assert_eq!(
            hudr2::opencode::resolve_session_table_name(&conn),
            "session"
        );

        drop(conn);

        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", db_path.to_str().unwrap());
        }
        hudr2::opencode::clear_message_stats_cache();

        let accounts = vec![
            hudr2::models::AccountQuota {
                prefix: "cuenta1".to_string(),
                ..Default::default()
            },
            hudr2::models::AccountQuota {
                prefix: "cuenta4".to_string(),
                ..Default::default()
            },
        ];

        let panes = vec![HerdrPane {
            pane_id: "w1:p1".to_string(),
            cwd: Some(target_dir.to_string()),
            ..Default::default()
        }];

        let session =
            hudr2::opencode::read_opencode_session(target_dir, &panes, &accounts, None, None);

        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
        }
        hudr2::opencode::clear_message_stats_cache();

        assert_eq!(session.local_prefix.as_deref(), Some("cuenta4"));
        assert!(session.all_active_prefixes.contains("cuenta4"));
        assert_eq!(session.model, "gemini-3.8-flash-high");
    }

    #[test]
    fn test_cpamc_and_parentheses_prefix_attribution_with_accounts() {
        let known = vec![
            "cuenta1".to_string(),
            "cuenta4".to_string(),
            "cuenta9".to_string(),
        ];

        assert_eq!(
            extract_prefix("", "gemini-3.8-flash-high (cuenta4)", &known).as_deref(),
            Some("cuenta4")
        );
        assert_eq!(
            extract_prefix("", "claude-3-5-sonnet [cuenta9]", &known).as_deref(),
            Some("cuenta9")
        );
        assert_eq!(
            extract_prefix("", "cpamc/cuenta4/gemini-3.8-flash-high", &known).as_deref(),
            Some("cuenta4")
        );
        assert_eq!(
            extract_prefix("", "cpamc:cuenta1:gemini-2.5-pro", &known).as_deref(),
            Some("cuenta1")
        );
        assert_eq!(
            hudr2::opencode::clean_model_name(
                "{\"id\":\"cuenta4/gemini-3.8-flash-high\",\"providerID\":\"cpamc\"}"
            ),
            "cuenta4/gemini-3.8-flash-high"
        );
        assert_eq!(
            hudr2::opencode::clean_model_name("gemini-3.8-flash-high (cuenta4)"),
            "gemini-3.8-flash-high"
        );
    }

    #[test]
    fn test_opencode_upgraded_db_coexisting_legacy_tables_and_message_data_hydration() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("opencode.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // 1. Tablas legacy coexistentes con datos viejos de cuenta1 (caso real tras upgrade de OpenCode)
        conn.execute_batch(
            "CREATE TABLE session_v2 (
                id TEXT PRIMARY KEY,
                agent TEXT,
                model TEXT,
                directory TEXT,
                title TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                time_archived INTEGER
            );
            CREATE TABLE session_message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                type TEXT,
                data TEXT,
                time_created INTEGER
            );
            CREATE TABLE session_part (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                message_id TEXT,
                type TEXT,
                data TEXT,
                time_created INTEGER
            );
            -- 2. Esquema Drizzle real de OpenCode 1.19+ (SIN columnas agent/model/tokens en session, y SIN type en message/part)
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                directory TEXT,
                title TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                time_archived INTEGER
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                data TEXT
            );
            CREATE TABLE part (
                id TEXT PRIMARY KEY,
                message_id TEXT,
                session_id TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                data TEXT
            );",
        )
        .unwrap();

        let target_dir = "d:/social-hub-pro";
        conn.execute(
            "INSERT INTO session_v2 VALUES ('ses_old', 'sdd-orchestrator-cuenta1', 'cuenta1/gemini-2.5-pro', ?, 'Vieja', 1000, 2000, NULL)",
            [target_dir],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO session VALUES ('ses_live_cpamc', ?, 'Social Hub', 50000, 60000, NULL)",
            [target_dir],
        )
        .unwrap();

        // Mensaje de usuario + mensaje de asistente con providerID=cpamc y modelID=cuenta4/...
        conn.execute(
            "INSERT INTO message VALUES ('msg_u1', 'ses_live_cpamc', 50100, 50100, ?)",
            [r#"{"id":"msg_u1","role":"user","agent":"Gentle-Orchestrator","model":{"providerID":"cpamc","modelID":"cuenta4/gemini-3.8-flash-high"}}"#],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message VALUES ('msg_a1', 'ses_live_cpamc', 50200, 50200, ?)",
            [r#"{"id":"msg_a1","role":"assistant","mode":"Gentle-Orchestrator","providerID":"cpamc","modelID":"cuenta4/gemini-3.8-flash-high","cost":0.021,"tokens":{"input":12000,"output":1500,"reasoning":500,"cache":{"read":146100,"write":0}}}"#],
        )
        .unwrap();

        // Partes: un turno viejo (4k) y uno reciente (160.1k) + edit con filePath camelCase (sin columna SQL `type`)
        conn.execute(
            "INSERT INTO part VALUES ('prt_old', 'msg_a0', 'ses_live_cpamc', 50050, 50050, ?)",
            [r#"{"type":"step-finish","tokens":{"input":3000,"output":1000}}"#],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part VALUES ('prt_edit', 'msg_a1', 'ses_live_cpamc', 50150, 50150, ?)",
            [r#"{"type":"tool","tool":"edit","state":{"input":{"filePath":"src/social.ts"}}}"#],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part VALUES ('prt_new', 'msg_a1', 'ses_live_cpamc', 50250, 50250, ?)",
            [r#"{"type":"step-finish","tokens":{"input":12000,"output":1500,"reasoning":500,"cache":{"read":146100,"write":0}}}"#],
        )
        .unwrap();
        drop(conn);

        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", db_path.to_str().unwrap());
        }
        hudr2::opencode::clear_message_stats_cache();

        let accounts = vec![
            hudr2::models::AccountQuota {
                prefix: "cuenta1".to_string(),
                rem_5h: 100,
                rem_wk: 100,
                reset_5h: "4h 59m".to_string(),
                reset_wk: "6d 23h".to_string(),
                ..Default::default()
            },
            hudr2::models::AccountQuota {
                prefix: "cuenta2".to_string(),
                rem_5h: 98,
                rem_wk: 99,
                reset_5h: "4h 50m".to_string(),
                reset_wk: "6d 20h".to_string(),
                ..Default::default()
            },
            hudr2::models::AccountQuota {
                prefix: "cuenta4".to_string(),
                rem_5h: 10,
                rem_wk: 80,
                is_critical: true,
                reset_5h: "2h 10m".to_string(),
                reset_wk: "5d 10h".to_string(),
                ..Default::default()
            },
        ];

        let panes = vec![HerdrPane {
            pane_id: "w19:p1".to_string(),
            cwd: Some(target_dir.to_string()),
            agent_status: Some("working".to_string()),
            ..Default::default()
        }];

        let session =
            hudr2::opencode::read_opencode_session(target_dir, &panes, &accounts, None, None);

        assert_eq!(session.local_prefix.as_deref(), Some("cuenta4"));
        assert!(session.all_active_prefixes.contains("cuenta4"));
        assert!(!session.all_active_prefixes.contains("cuenta1"));
        assert_eq!(session.agent, "Gentle-Orchestrator");
        assert_eq!(session.model_short, "gemini-3.8-flash-high");
        assert_eq!(session.active_turn_tokens, 160_100);
        assert_eq!(session.tokens_input, 12_000);
        assert_eq!(session.tokens_output, 2_000);
        assert!((session.cost - 0.021).abs() < 1e-6);
        assert_eq!(session.tool_writes, 1);
        assert_eq!(session.attributed_changes.files_count, 1);

        // Probar migración crítica en esquema `session` sin columna SQL `model` preservando `cpamc`
        let (migrated, notice) = hudr2::cleanup::migrate_critical_sessions_for_model(
            target_dir,
            &accounts,
            &session.all_active_prefixes,
            &session.model,
        );
        assert_eq!(migrated, 1, "Debe migrar ses_live_cpamc: {notice}");
        assert!(notice.contains("cuenta1"));

        hudr2::opencode::clear_message_stats_cache();
        let post_mig =
            hudr2::opencode::read_opencode_session(target_dir, &panes, &accounts, None, None);
        assert_eq!(post_mig.local_prefix.as_deref(), Some("cuenta1"));

        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
        }
        hudr2::opencode::clear_message_stats_cache();
    }

    #[test]
    fn test_quota_disk_cache_50_accounts_scaling_reset_expiry_and_429_circuit_breaker() {
        use hudr2::app::select_accounts_for_refresh;
        use hudr2::models::AccountQuota;
        use hudr2::quotas::{
            UNFETCHED_RESET, clear_quota_runtime_cache, hydrate_accounts_from_cache,
            is_rate_limit_cooling_down, quota_cache_path_for, record_cached_quota_at,
            trigger_rate_limit_cooldown,
        };
        use serde_json::json;
        use std::collections::HashSet;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_quota_runtime_cache();

        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("hudr2.json");
        let cache_path = quota_cache_path_for(&cfg_path);
        let now = chrono::Utc::now().timestamp();
        let future_iso = (chrono::Utc::now() + chrono::Duration::hours(4)).to_rfc3339();
        let expired_iso = (chrono::Utc::now() - chrono::Duration::minutes(2)).to_rfc3339();

        // 1. Poblar 49 cuentas en caché en disco y dejar cuenta50 nueva sin consultar ("...")
        for i in 1..=49 {
            let pfx = format!("cuenta{i:02}");
            let auth_path = tmp.path().join(format!("{pfx}.json"));
            std::fs::write(&auth_path, r#"{"access_token":"tok"}"#).unwrap();

            let mut acc = AccountQuota {
                prefix: pfx,
                auth_file: auth_path.to_string_lossy().to_string(),
                reset_5h: UNFETCHED_RESET.to_string(),
                reset_wk: UNFETCHED_RESET.to_string(),
                ..Default::default()
            };

            let (fetched_at, reset_iso, frac) = if i == 12 {
                // Ventana vencida: se consultó hace 1h y su resetTime ya pasó hace 2m
                (now - 3600, expired_iso.as_str(), 0.20)
            } else {
                (now - 300, future_iso.as_str(), 0.96)
            };

            record_cached_quota_at(
                &mut acc,
                &json!({
                    "groups": [{
                        "displayName": "Gemini",
                        "buckets": [
                            {"window": "5h", "remainingFraction": frac, "resetTime": reset_iso},
                            {"window": "weekly", "remainingFraction": 0.98, "resetTime": future_iso}
                        ]
                    }]
                }),
                fetched_at,
                Some(&cache_path),
            );
        }
        assert!(
            cache_path.exists(),
            "quotas_cache.json debe persistirse en disco"
        );

        // 2. Simular arranque en frío de otra instancia/panel en Herdr con las 50 cuentas
        clear_quota_runtime_cache();
        let mut cold_accounts: Vec<AccountQuota> = (1..=50)
            .map(|i| {
                let pfx = format!("cuenta{i:02}");
                let auth_path = tmp.path().join(format!("{pfx}.json"));
                if i == 50 {
                    std::fs::write(&auth_path, r#"{"access_token":"tok50"}"#).unwrap();
                }
                AccountQuota {
                    prefix: pfx,
                    auth_file: auth_path.to_string_lossy().to_string(),
                    rem_5h: 100,
                    rem_wk: 100,
                    reset_5h: UNFETCHED_RESET.to_string(),
                    reset_wk: UNFETCHED_RESET.to_string(),
                    ..Default::default()
                }
            })
            .collect();

        hydrate_accounts_from_cache(&mut cold_accounts, Some(&cache_path));
        assert_eq!(
            cold_accounts[3].rem_5h, 96,
            "cuenta04 debe hidratar 96% en 0ms"
        );
        assert_ne!(cold_accounts[3].reset_5h, UNFETCHED_RESET);
        assert_eq!(
            cold_accounts[49].reset_5h, UNFETCHED_RESET,
            "cuenta50 sigue nueva"
        );

        // 3. Comando 'r' (selectivo): de 50 cuentas solo debe consultar cuenta04 (activa), cuenta12 (vencida) y cuenta50 (nueva)
        let mut active = HashSet::new();
        active.insert("cuenta04".to_string());
        let selected_r =
            select_accounts_for_refresh(&cold_accounts, Some("cuenta04"), &active, false, false);
        let prefixes_r: Vec<&str> = selected_r.iter().map(|a| a.prefix.as_str()).collect();
        assert_eq!(
            prefixes_r,
            vec!["cuenta04", "cuenta12", "cuenta50"],
            "En modo 'r' con 50 cuentas solo deben consultarse activa + vencida + nueva, con activa en idx 0"
        );

        // 4. Comando 'a' (auditar todas): selecciona las 50 pero poniendo cuenta04 en idx 0
        let selected_a =
            select_accounts_for_refresh(&cold_accounts, Some("cuenta04"), &active, true, false);
        assert_eq!(selected_a.len(), 50);
        assert_eq!(selected_a[0].prefix, "cuenta04");

        // 5. Freno ante HTTP 429: corta de inmediato las 49 inactivas
        trigger_rate_limit_cooldown(60);
        assert!(is_rate_limit_cooling_down());
        let selected_429 = select_accounts_for_refresh(
            &cold_accounts,
            Some("cuenta04"),
            &active,
            true,
            is_rate_limit_cooling_down(),
        );
        assert_eq!(selected_429.len(), 1);
        assert_eq!(selected_429[0].prefix, "cuenta04");

        clear_quota_runtime_cache();
    }

    #[test]
    fn test_yellow_zone_migration_candidate_prewarming_and_unfetched_exclusion() {
        use hudr2::app::select_yellow_zone_prewarm_candidates;
        use hudr2::cleanup::get_best_healthy_target_prefix_for_model;
        use hudr2::models::AccountQuota;
        use hudr2::quotas::{
            UNFETCHED_RESET, clear_quota_runtime_cache, record_cached_quota_at,
            sync_account_health_for_model,
        };
        use serde_json::json;
        use std::collections::HashSet;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_quota_runtime_cache();

        let now = chrono::Utc::now().timestamp();
        let future_iso = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        let make_quota_json = |frac_5h: f64, frac_wk: f64| {
            json!({
                "groups": [{
                    "displayName": "Gemini",
                    "buckets": [
                        {"window": "5h", "remainingFraction": frac_5h, "resetTime": future_iso},
                        {"window": "weekly", "remainingFraction": frac_wk, "resetTime": future_iso}
                    ]
                }]
            })
        };

        // cuenta1: activa en ZONA AMARILLA (25% libre -> 75% usado -> is_warning = true, no crítica aún)
        let mut c1 = AccountQuota {
            prefix: "cuenta1".to_string(),
            ..Default::default()
        };
        record_cached_quota_at(&mut c1, &make_quota_json(0.25, 0.90), now - 30, None);
        sync_account_health_for_model(&mut c1, "gemini-3.8-flash-high");
        assert!(c1.is_warning && !c1.is_critical);

        // cuenta2: pendiente sin consultar ("...") con 100% provisional — NO debe ganarle a cuentas verificadas
        let c2 = AccountQuota {
            prefix: "cuenta2".to_string(),
            rem_5h: 100,
            rem_wk: 100,
            reset_5h: UNFETCHED_RESET.to_string(),
            reset_wk: UNFETCHED_RESET.to_string(),
            ..Default::default()
        };

        // cuenta3: verificada hace 25 min (> 10 min -> requiere pre-validación), 96% libre
        let mut c3 = AccountQuota {
            prefix: "cuenta3".to_string(),
            ..Default::default()
        };
        record_cached_quota_at(&mut c3, &make_quota_json(0.96, 0.98), now - 1500, None);

        // cuenta4: verificada hace 2 min (< 10 min -> YA está fresca y lista), 92% libre
        let mut c4 = AccountQuota {
            prefix: "cuenta4".to_string(),
            ..Default::default()
        };
        record_cached_quota_at(&mut c4, &make_quota_json(0.92, 0.95), now - 120, None);

        // cuenta5: verificada hace 30 min (> 10 min -> requiere pre-validación), 88% libre
        let mut c5 = AccountQuota {
            prefix: "cuenta5".to_string(),
            ..Default::default()
        };
        record_cached_quota_at(&mut c5, &make_quota_json(0.88, 0.90), now - 1800, None);

        let mut accounts = vec![c1, c2, c3, c4, c5];
        let mut active = HashSet::new();
        active.insert("cuenta1".to_string());

        // 1. La sugerencia de migración debe preferir cuenta3 (verificada 96%) sobre cuenta2 ("..." 100% falso)
        assert_eq!(
            get_best_healthy_target_prefix_for_model(&accounts, &active, "gemini-3.8-flash-high"),
            "cuenta3"
        );

        // 2. En zona amarilla, cuenta4 ya es fresca (<10m); deben encolarse cuenta3 y cuenta5 antes que cuenta2
        let prewarm = select_yellow_zone_prewarm_candidates(
            &accounts,
            &active,
            "gemini-3.8-flash-high",
            600,
            3,
        );
        let prewarm_pfxs: Vec<&str> = prewarm.iter().map(|a| a.prefix.as_str()).collect();
        assert!(
            !prewarm_pfxs.contains(&"cuenta4"),
            "cuenta4 ya estaba fresca (<10m), no debe re-consultarse"
        );
        assert_eq!(prewarm_pfxs[0], "cuenta3");
        assert_eq!(prewarm_pfxs[1], "cuenta5");

        // 3. Una vez pre-validadas cuenta3 y cuenta5 en segundo plano, ya hay 3 candidatas frescas -> 0 consultas extra
        record_cached_quota_at(&mut accounts[2], &make_quota_json(0.95, 0.98), now, None);
        record_cached_quota_at(&mut accounts[4], &make_quota_json(0.87, 0.90), now, None);
        let prewarm_after = select_yellow_zone_prewarm_candidates(
            &accounts,
            &active,
            "gemini-3.8-flash-high",
            600,
            3,
        );
        assert!(
            prewarm_after.is_empty(),
            "Con 3 candidatas frescas confirmadas en zona amarilla, no debe hacer más llamadas"
        );

        clear_quota_runtime_cache();
    }

    #[tokio::test]
    async fn test_cpamc_sync_integrations_and_gentle_orchestrator_audit() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req_str = String::from_utf8_lossy(&buf[..n]);
            assert!(
                req_str
                    .to_ascii_lowercase()
                    .contains("authorization: bearer cpamc-key-test"),
                "Debe enviar el apiKey del bloque provider.cpamc: {req_str}"
            );
            let body = r#"{"data":[{"id":"cuenta4/gemini-3.8-flash-high"},{"id":"cuenta4/claude-sonnet-4-6"}]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        });

        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("opencode.json");
        let initial_cfg = serde_json::json!({
            "provider": {
                "cpamc": {
                    "options": {
                        "apiKey": "cpamc-key-test",
                        "baseURL": format!("http://{addr}/v1")
                    }
                }
            },
            "agent": {
                "gentle-orchestrator-cuenta4": { "model": "" },
                "sdd-orchestrator-cuenta4": { "model": "" }
            }
        });
        std::fs::write(
            &cfg_path,
            serde_json::to_string_pretty(&initial_cfg).unwrap(),
        )
        .unwrap();

        let res = hudr2::sync::sync_integrations_for_path(&cfg_path)
            .await
            .expect("sync con provider.cpamc debe completarse sin error");
        server.await.unwrap();

        assert_eq!(res.total_models, 2);
        assert_eq!(res.added_models, 2);
        assert_eq!(res.audited_orchestrators, 2);

        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&cfg_path).unwrap()).unwrap();
        assert!(saved["provider"]["cpamc"]["models"]["cuenta4/gemini-3.8-flash-high"].is_object());
        assert_eq!(
            saved["agent"]["gentle-orchestrator-cuenta4"]["model"].as_str(),
            Some("cpamc/cuenta4/gemini-3.8-flash-high")
        );
        assert_eq!(
            saved["agent"]["sdd-orchestrator-cuenta4"]["model"].as_str(),
            Some("cpamc/cuenta4/gemini-3.8-flash-high")
        );
    }

    #[test]
    fn test_wal_mtime_precedence_and_footer_a_r_shortcuts() {
        let tmp = tempfile::tempdir().unwrap();
        let db_file = tmp.path().join("opencode.db");
        let wal_file = tmp.path().join("opencode.db-wal");
        std::fs::write(&db_file, b"sqlite").unwrap();
        let m1 = hudr2::paths::db_effective_mtime(&db_file).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(25));
        std::fs::write(&wal_file, b"wal-newer").unwrap();
        let m2 = hudr2::paths::db_effective_mtime(&db_file).unwrap();
        assert!(
            m2 >= m1,
            "db_effective_mtime debe considerar opencode.db-wal"
        );

        let state = AppState::new();
        for w in [36usize, 44, 48] {
            let out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, w);
            for line in &out.lines {
                assert!(
                    ui::string_width(line) <= w,
                    "Ninguna línea puede exceder {w} columnas: {}",
                    ui::strip_ansi(line)
                );
            }
            let plain = ui::strip_ansi(&out.lines.join("\n"));
            assert!(
                !plain.contains("[sincronizar]"),
                "Integraciones no debe renderizar el botón >> [sincronizar]"
            );
            assert!(
                out.bounds.iter().all(|b| b.id != "sync_integrations"),
                "No debe quedar bound clicable huérfano sync_integrations"
            );
            if w >= 44 {
                assert!(
                    plain.contains("s: sync")
                        && plain.contains("r: act")
                        && plain.contains("a: auditar")
                );
            }
        }
    }

    #[test]
    fn test_utf8_folder_truncation_active_telemetry_proyecto_fallback_and_tools_visibility() {
        use hudr2::herdr::HerdrClient;
        use hudr2::models::SessionTodo;

        // 1. F-02: extracción de pane_id en formatos JSON de Herdr/Telar
        let split_json = serde_json::json!({
            "result": { "pane": { "pane_id": "w1:p9" } }
        });
        assert_eq!(
            HerdrClient::extract_split_pane_id(&split_json).as_deref(),
            Some("w1:p9")
        );

        // 2. F-01 + A1 + F-03: carpeta multibyte >18 bytes en límite UTF-8, thinking_level, todos, tool_bash y fallback project_dir
        let mut state = AppState::new();
        state.snapshot.project_dir =
            "D:/proyectos/documentaci\u{00f3}n_t\u{00e9}cnica_avanzada".to_string();
        state.snapshot.agent_session.model_short = "gemini-3.8-flash-h".to_string();
        state.snapshot.agent_session.thinking_level = Some("high".to_string());
        state.snapshot.agent_session.tool_bash = 7;
        state.snapshot.agent_session.todos = vec![
            SessionTodo {
                title: "Auditar UTF-8".to_string(),
                status: "done".to_string(),
            },
            SessionTodo {
                title: "Conectar UI de telemetr\u{00ed}a".to_string(),
                status: "in_progress".to_string(),
            },
            SessionTodo {
                title: "Subir commit".to_string(),
                status: "pending".to_string(),
            },
        ];
        // Expandir Proyecto (idx 2) mientras tree.root_path sigue vacío
        state.collapsed_cards[2] = false;

        let out = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        let plain = ui::strip_ansi(&out.lines.join("\n"));
        assert!(
            plain.contains("[high]"),
            "Estado debe renderizar thinking_level: {plain}"
        );
        assert!(
            plain.contains("Tareas")
                && plain.contains("✓1")
                && plain.contains("◉1")
                && plain.contains("○1"),
            "Estado debe renderizar resumen de todos: {plain}"
        );
        assert!(
            plain.contains("$ 7 cmds"),
            "Herramientas debe renderizar tool_bash cuando > 0: {plain}"
        );

        let open_bound = out
            .bounds
            .iter()
            .find(|b| b.id == "open_folder")
            .expect("Proyecto expandido debe exponer open_folder");
        assert_eq!(
            open_bound.action_path.as_deref(),
            Some(state.snapshot.project_dir.as_str()),
            "open_folder debe usar project_dir cuando tree.root_path está vacío"
        );

        // 3. F-05: visibilidad independiente de "tools" vs "mcp" y orden Herramientas -> MCP -> Stack
        let tools_pos = plain.find("Herramientas").unwrap();
        let mcp_pos = plain.find("MCP ·").unwrap();
        let stack_pos = plain.find("Stack · Ecosistema").unwrap();
        assert!(
            tools_pos < mcp_pos && mcp_pos < stack_pos,
            "El orden visual debe coincidir con las teclas 8 (Herramientas) -> 9 (MCP) -> e (Stack)"
        );

        state.toggle_card_visibility("tools");
        let out_tools_hidden = ui::render_screen_with_diff(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
        );
        let plain_hidden = ui::strip_ansi(&out_tools_hidden.lines.join("\n"));
        assert!(
            !plain_hidden.contains("Herramientas ·"),
            "Ocultar 'tools' debe ocultar Herramientas"
        );
        assert!(
            plain_hidden.contains("MCP ·"),
            "Ocultar 'tools' NO debe ocultar MCP"
        );
    }

    #[test]
    fn test_quota_error_cache_preservation_429_403_ttls_and_invalid_reset() {
        use hudr2::models::AccountQuota;
        use hudr2::quotas::{
            UNFETCHED_RESET, clear_quota_runtime_cache, format_relative_reset,
            hydrate_accounts_from_cache, is_fresh_within, needs_refresh, quota_cache_path_for,
            record_cached_quota_at, record_cached_quota_error_at,
        };
        use serde_json::json;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_quota_runtime_cache();

        assert_eq!(
            format_relative_reset("not-a-valid-rfc3339"),
            "?",
            "Un resetTime corrupto jamás debe fabricar '4h 30m'"
        );

        let tmp = tempfile::tempdir().unwrap();
        let cache_path = quota_cache_path_for(&tmp.path().join("hudr2.json"));
        let auth1 = tmp.path().join("cuenta01.json");
        let auth2 = tmp.path().join("cuenta02.json");
        std::fs::write(&auth1, r#"{"access_token":"tok1"}"#).unwrap();
        std::fs::write(&auth2, r#"{"access_token":"tok2"}"#).unwrap();

        let now = chrono::Utc::now().timestamp();
        let future_iso = (chrono::Utc::now() + chrono::Duration::hours(4)).to_rfc3339();

        // 1. cuenta01 tiene datos previos (94%), luego recibe HTTP 429: DataChanged no debe pisar el error
        let mut acc1 = AccountQuota {
            prefix: "cuenta01".to_string(),
            auth_file: auth1.to_string_lossy().to_string(),
            reset_5h: UNFETCHED_RESET.to_string(),
            reset_wk: UNFETCHED_RESET.to_string(),
            ..Default::default()
        };
        record_cached_quota_at(
            &mut acc1,
            &json!({
                "groups": [{
                    "displayName": "Gemini",
                    "buckets": [
                        {"window": "5h", "remainingFraction": 0.94, "resetTime": future_iso},
                        {"window": "weekly", "remainingFraction": 0.98, "resetTime": future_iso}
                    ]
                }]
            }),
            now - 600,
            Some(&cache_path),
        );
        record_cached_quota_error_at(
            &mut acc1,
            429,
            &json!({"error": {"status": "RESOURCE_EXHAUSTED"}}),
            now - 10,
            Some(&cache_path),
        );

        let mut hydrated = vec![AccountQuota {
            prefix: "cuenta01".to_string(),
            auth_file: auth1.to_string_lossy().to_string(),
            reset_5h: UNFETCHED_RESET.to_string(),
            reset_wk: UNFETCHED_RESET.to_string(),
            ..Default::default()
        }];
        hydrate_accounts_from_cache(&mut hydrated, Some(&cache_path));
        assert!(
            hydrated[0].has_error && hydrated[0].error_msg == "rate limited",
            "hydrate_accounts_from_cache no debe borrar un error 429 vigente con datos viejos"
        );
        assert!(
            !needs_refresh(&hydrated[0]),
            "429 reciente (<120s) no debe reintentarse en cada pulso"
        );
        assert!(!is_fresh_within(&hydrated[0], 600));

        // Al superar los 120s de TTL transitorio, 'r' debe volver a consultar cuenta01
        record_cached_quota_error_at(&mut acc1, 429, &json!({}), now - 125, Some(&cache_path));
        assert!(
            needs_refresh(&acc1),
            "429 con >=120s de antigüedad debe habilitar reintento selectivo"
        );

        // 2. cuenta02 recibe HTTP 403 (verificación requerida): sin cambiar mtime en disco, reintenta tras 300s
        let mut acc2 = AccountQuota {
            prefix: "cuenta02".to_string(),
            auth_file: auth2.to_string_lossy().to_string(),
            reset_5h: UNFETCHED_RESET.to_string(),
            reset_wk: UNFETCHED_RESET.to_string(),
            ..Default::default()
        };
        let err_403 = json!({
            "error": {
                "message": "Verify your account to continue",
                "details": [{"reason": "VALIDATION_REQUIRED"}]
            }
        });
        record_cached_quota_error_at(&mut acc2, 403, &err_403, now - 30, Some(&cache_path));
        assert_eq!(acc2.error_msg, "verif. requerida");
        assert!(
            !needs_refresh(&acc2),
            "403 reciente (<300s) sin cambio de mtime espera enfriamiento"
        );

        record_cached_quota_error_at(&mut acc2, 403, &err_403, now - 305, Some(&cache_path));
        assert!(
            needs_refresh(&acc2),
            "403 con >=300s debe permitir reintento en modo 'r' aunque el token no cambie de mtime"
        );

        // 3. Fallo de transporte/red (status 0) marca 'sin red'
        record_cached_quota_error_at(&mut acc2, 0, &json!(null), now - 5, None);
        assert!(acc2.has_error);
        assert_eq!(acc2.error_msg, "sin red");

        clear_quota_runtime_cache();
    }

    #[test]
    fn test_engram_wal_mtime_invalidation_resurrected_orphan_release_and_atomic_write() {
        use hudr2::models::AccountQuota;
        use rusqlite::Connection;
        use std::collections::HashSet;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();

        // 1. Escritura atómica (.{pid}.tmp + rename) sin residuos temporales
        let nested_cfg = tmp.path().join("cfg_sub").join("opencode.json");
        hudr2::paths::atomic_write_bytes(&nested_cfg, br#"{"ok":true}"#).unwrap();
        assert_eq!(
            std::fs::read_to_string(&nested_cfg).unwrap(),
            r#"{"ok":true}"#
        );
        let leftovers: Vec<_> = std::fs::read_dir(nested_cfg.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "No deben quedar archivos .tmp tras atomic_write_bytes"
        );

        // 2. Engram en modo WAL + preservación de mayúsculas/minúsculas del proyecto
        let engram_db = tmp.path().join("engram.db");
        {
            let conn = Connection::open(&engram_db).unwrap();
            let _: String = conn
                .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
                .unwrap();
            conn.execute_batch(
                "CREATE TABLE observations (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project TEXT,
                    type TEXT,
                    content TEXT,
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    deleted_at TEXT
                );
                CREATE TABLE sync_enrolled_projects (project TEXT PRIMARY KEY);
                INSERT INTO observations (project, type, content) VALUES ('MiProyectoMixto', 'discovery', 'obs 1');",
            )
            .unwrap();
        }

        let proj_dir = tmp.path().join("MiProyectoMixto");
        std::fs::create_dir_all(&proj_dir).unwrap();
        let proj_dir_str = proj_dir.to_string_lossy().to_string();

        unsafe {
            std::env::set_var("ENGRAM_DB_PATH", &engram_db);
        }
        hudr2::engram::clear_engram_cache();

        let t1 = hudr2::engram::read_engram_telemetry(&proj_dir_str);
        assert!(t1.online);
        assert_eq!(t1.obs_count, 1);

        // Escribir en conexión WAL abierta sin hacer checkpoint sobre engram.db
        std::thread::sleep(std::time::Duration::from_millis(25));
        let wal_conn = Connection::open(&engram_db).unwrap();
        let _ = wal_conn.pragma_update(None, "wal_autocheckpoint", 0);
        wal_conn
            .execute(
                "INSERT INTO observations (project, type, content) VALUES ('MiProyectoMixto', 'decision', 'obs 2')",
                [],
            )
            .unwrap();

        let t2 = hudr2::engram::read_engram_telemetry(&proj_dir_str);
        assert_eq!(
            t2.obs_count, 2,
            "ENGRAM_CACHE debe invalidarse al cambiar engram.db-wal sin requerir clear_engram_cache"
        );
        drop(wal_conn);

        // 3. Cleanup: archivado de huérfanas resucitadas (time_updated > time_archived) y migración sin falsos positivos (.max(1))
        let oc_db = tmp.path().join("opencode.db");
        {
            let conn = Connection::open(&oc_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE session (
                    id TEXT PRIMARY KEY,
                    directory TEXT,
                    title TEXT,
                    time_updated INTEGER,
                    time_archived INTEGER
                );
                CREATE TABLE message (
                    id TEXT PRIMARY KEY,
                    session_id TEXT,
                    time_created INTEGER,
                    data TEXT
                );
                INSERT INTO message (id, session_id, time_created, data)
                    VALUES ('m1', 'ses_resurrected', 5000, '{\"agent\":\"sdd-orchestrator-cuenta4\",\"providerID\":\"cpamc\",\"modelID\":\"cuenta4/gemini-3.8-flash-high\"}');",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO session (id, directory, title, time_updated, time_archived)
                    VALUES ('ses_resurrected', ?1, 'Resucitada', 5000, 1000)",
                [&proj_dir_str],
            )
            .unwrap();
        }

        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", &oc_db);
            std::env::set_var("ENGRAM_BIN", "off");
        }

        let accounts = vec![
            AccountQuota {
                prefix: "cuenta1".to_string(),
                rem_5h: 95,
                rem_wk: 95,
                reset_5h: "4h".to_string(),
                reset_wk: "6d".to_string(),
                ..Default::default()
            },
            AccountQuota {
                prefix: "cuenta4".to_string(),
                rem_5h: 5,
                rem_wk: 80,
                is_critical: true,
                reset_5h: "1h".to_string(),
                reset_wk: "5d".to_string(),
                ..Default::default()
            },
        ];

        let rel = hudr2::cleanup::release_inactive_sessions(
            &proj_dir_str,
            &accounts,
            Some("nonexistent_pane"),
            Some("nonexistent_tab"),
        );
        assert_eq!(
            rel.closed_count, 1,
            "release_inactive_sessions debe archivar huérfanas resucitadas (time_updated > time_archived): {}",
            rel.notice
        );
        assert_eq!(rel.released_prefixes, vec!["cuenta4".to_string()]);

        // Verificar en SQLite que time_archived fue actualizado por encima de time_updated (5000)
        {
            let conn = Connection::open(&oc_db).unwrap();
            let archived_ts: i64 = conn
                .query_row(
                    "SELECT time_archived FROM session WHERE id = 'ses_resurrected'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(archived_ts > 5000);
        }

        let mut active = HashSet::new();
        active.insert("cuenta4".to_string());
        let (migrated_zero, _) = hudr2::cleanup::migrate_critical_sessions_for_model(
            &proj_dir_str,
            &accounts,
            &active,
            "cpamc/cuenta4/gemini-3.8-flash-high",
        );
        assert_eq!(
            migrated_zero, 0,
            "Una sesión ya archivada no debe contarse como migrada"
        );

        unsafe {
            std::env::remove_var("ENGRAM_DB_PATH");
            std::env::remove_var("OPENCODE_DB_PATH");
            std::env::remove_var("ENGRAM_BIN");
        }
        hudr2::engram::clear_engram_cache();
    }

    #[test]
    fn test_zero_cpu_bounded_subprocesses_sensitive_hardening_and_cache_caps() {
        use hudr2::git::{is_sensitive_file, read_file_diff_preview};
        use hudr2::herdr::HerdrClient;
        use hudr2::paths::resolve_engram_binary;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // 1. Endurecimiento de is_sensitive_file y prevención de path traversal (..)
        assert!(is_sensitive_file("~/.ssh/id_ecdsa"));
        assert!(is_sensitive_file(".ssh/id_rsa"));
        assert!(is_sensitive_file("configs/.aws/credentials"));
        assert!(is_sensitive_file(".kube/config"));
        assert!(is_sensitive_file(".gnupg/trustdb.gpg"));
        assert!(is_sensitive_file("app.keystore"));
        assert!(is_sensitive_file("release.jks"));
        assert!(!is_sensitive_file("src/main.rs"));

        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("repo");
        std::fs::create_dir_all(repo_dir.join(".git")).unwrap();
        let target_str = repo_dir.to_string_lossy().to_string();

        let traversal = read_file_diff_preview(&target_str, "../escaped_secret.txt");
        assert_eq!(traversal, vec!["[ruta inválida fuera del repositorio]"]);
        let abs_target = if cfg!(windows) {
            "C:/test.txt"
        } else {
            "/test.txt"
        };
        let absolute_escape = read_file_diff_preview(&target_str, abs_target);
        assert_eq!(
            absolute_escape,
            vec!["[ruta inválida fuera del repositorio]"]
        );

        // 2. Pure-Rust resolve_engram_binary vía PATH sin subprocesos
        let fake_bin_dir = tmp.path().join("fake_bin");
        std::fs::create_dir_all(&fake_bin_dir).unwrap();
        let fake_engram = if cfg!(target_os = "windows") {
            fake_bin_dir.join("engram.exe")
        } else {
            fake_bin_dir.join("engram")
        };
        std::fs::write(&fake_engram, b"mock-binary").unwrap();

        let old_path = std::env::var_os("PATH");
        unsafe {
            std::env::remove_var("ENGRAM_BIN");
            std::env::set_var("PATH", &fake_bin_dir);
        }
        let resolved = resolve_engram_binary();
        assert_eq!(resolved.as_ref(), Some(&fake_engram));

        // 3. HerdrClient: caché de binario disponible y timeout acotado
        hudr2::herdr::clear_herdr_binary_cache();
        // Cuando no existe herdr/telardev en PATH, get_panes debe retornar sin colgarse
        unsafe {
            std::env::set_var("PATH", &fake_bin_dir);
            std::env::set_var(
                "HERDR_SOCKET_PATH",
                tmp.path().join("missing-herdr-endpoint"),
            );
        }
        let started = std::time::Instant::now();
        let panes = HerdrClient::get_panes();
        assert!(panes.is_empty());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "get_panes must return within its bounded timeout"
        );
        unsafe {
            std::env::remove_var("HERDR_SOCKET_PATH");
            if let Some(prev) = old_path {
                std::env::set_var("PATH", prev);
            } else {
                std::env::remove_var("PATH");
            }
        }
    }

    #[test]
    fn test_adaptive_theming_and_herdr_config_sync() {
        use hudr2::theme::{
            HERDR_CATPPUCCIN, HERDR_DRACULA, LINEAR_STUDIO, active_theme, adopt_herdr_theme,
            find_theme, parse_herdr_theme_name, set_active_theme,
        };

        // 1. Simulación de config.toml de Herdr con diferentes temas adoptados directamente
        let toml_dracula = "[theme]\nname = \"dracula\"\n";
        assert_eq!(
            parse_herdr_theme_name(toml_dracula).as_deref(),
            Some("dracula")
        );
        assert_eq!(adopt_herdr_theme("dracula").id, "dracula");

        let toml_nord = "[theme]\nname = \"nord\"\n";
        assert_eq!(parse_herdr_theme_name(toml_nord).as_deref(), Some("nord"));
        assert_eq!(adopt_herdr_theme("nord").id, "nord");

        let toml_gruvbox_light = "[theme]\nname = 'gruvbox-light'\n";
        assert_eq!(
            parse_herdr_theme_name(toml_gruvbox_light).as_deref(),
            Some("gruvbox-light")
        );
        assert_eq!(adopt_herdr_theme("gruvbox-light").id, "gruvbox-light");

        let toml_catppuccin = "[theme]\nname = \"catppuccin\"\n";
        assert_eq!(
            parse_herdr_theme_name(toml_catppuccin).as_deref(),
            Some("catppuccin")
        );
        assert_eq!(adopt_herdr_theme("catppuccin").id, "catppuccin");

        // 2. Comprobar que find_theme resuelve los 5 nativos de HUDR2 y los 14 de Herdr
        // Nativos HUDR2:
        assert_eq!(find_theme("linear").unwrap().id, "linear");
        assert_eq!(find_theme("cyber").unwrap().id, "cyber");
        assert_eq!(find_theme("bauhaus").unwrap().id, "bauhaus");
        assert_eq!(find_theme("synthwave").unwrap().id, "synthwave");
        assert_eq!(find_theme("editorial").unwrap().id, "editorial");

        // Adoptados de Herdr:
        assert_eq!(find_theme("catppuccin").unwrap().id, "catppuccin");
        assert_eq!(
            find_theme("catppuccin-latte").unwrap().id,
            "catppuccin-latte"
        );
        assert_eq!(find_theme("terminal").unwrap().id, "terminal");
        assert_eq!(find_theme("tokyo-night").unwrap().id, "tokyo-night");
        assert_eq!(find_theme("tokyo-night-day").unwrap().id, "tokyo-night-day");
        assert_eq!(find_theme("dracula").unwrap().id, "dracula");
        assert_eq!(find_theme("nord").unwrap().id, "nord");
        assert_eq!(find_theme("gruvbox").unwrap().id, "gruvbox");
        assert_eq!(find_theme("gruvbox-light").unwrap().id, "gruvbox-light");
        assert_eq!(find_theme("one-dark").unwrap().id, "one-dark");
        assert_eq!(find_theme("one-light").unwrap().id, "one-light");
        assert_eq!(find_theme("solarized").unwrap().id, "solarized");
        assert_eq!(find_theme("solarized-light").unwrap().id, "solarized-light");
        assert_eq!(find_theme("gentleman").unwrap().id, "gentleman");

        // 3. Renderizado con tema adoptado de Herdr (Dracula)
        let state = hudr2::state::AppState::new();
        let out_dracula = ui::render_screen_with_diff_and_theme(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
            &HERDR_DRACULA,
        );
        let dracula_raw = out_dracula.lines.join("\n");
        assert!(dracula_raw.contains(HERDR_DRACULA.border));
        assert!(dracula_raw.contains(HERDR_DRACULA.title_color));

        let out_catppuccin = ui::render_screen_with_diff_and_theme(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
            &HERDR_CATPPUCCIN,
        );
        let catppuccin_raw = out_catppuccin.lines.join("\n");
        assert!(catppuccin_raw.contains(HERDR_CATPPUCCIN.border));
        assert!(catppuccin_raw.contains(HERDR_CATPPUCCIN.title_color));

        // 4. Conmutación dinámica del tema activo
        set_active_theme(&HERDR_DRACULA);
        assert_eq!(active_theme().id, "dracula");
        set_active_theme(&HERDR_CATPPUCCIN);
        assert_eq!(active_theme().id, "catppuccin");
        set_active_theme(&LINEAR_STUDIO);
        assert_eq!(active_theme().id, "linear");
    }

    #[tokio::test]
    async fn test_windows_ipc_timeout_and_silent_quota_notice_clear() {
        use hudr2::herdr::HerdrClient;
        use std::path::Path;

        let dummy_pipe = if cfg!(windows) {
            Path::new(r"\\.\pipe\hudr2_nonexistent_ipc_test")
        } else {
            Path::new("/tmp/hudr2_nonexistent_ipc.sock")
        };
        let res = HerdrClient::request_ipc_at(dummy_pipe, "pane.list", serde_json::json!({}));
        assert!(res.is_none());

        let app_state =
            std::sync::Arc::new(tokio::sync::RwLock::new(hudr2::state::AppState::new()));
        {
            let mut st = app_state.write().await;
            st.set_flash_notice(Some("+ Cuotas (2/9)".to_string()), 5000);
        }
        hudr2::app::force_refresh_with_notify(&app_state, false, false, None).await;
        {
            let st = app_state.read().await;
            assert_ne!(
                st.flash_notice.as_deref(),
                Some("+ Cuotas (2/9)"),
                "El aviso residual de cuotas debe limpiarse al finalizar el barrido silencioso"
            );
        }
    }

    #[test]
    fn test_ecosystem_hudr2_version_system_no_gs3_hud() {
        use hudr2::ecosystem::collect_ecosystem_telemetry;

        let tele = collect_ecosystem_telemetry();
        // Jamás debe existir gs3-hud en las herramientas del Stack
        assert!(
            !tele.tools.iter().any(|t| t.name == "gs3-hud"),
            "gs3-hud no debe buscarse ni listarse en el Stack"
        );

        // Debe existir hudr2 con versión compilada/global
        let hud = tele
            .tools
            .iter()
            .find(|t| t.name == "hudr2")
            .expect("hudr2 debe existir en la lista de herramientas de Stack");
        assert!(hud.is_installed);
        let ver = hud.version.as_deref().unwrap_or("");
        assert!(
            ver.starts_with('v'),
            "La versión de hudr2 debe comenzar con 'v', obtenido: {}",
            ver
        );
        let expected_min_ver = format!("v{}", env!("CARGO_PKG_VERSION"));
        assert_eq!(
            ver, expected_min_ver,
            "La versión de hudr2 debe coincidir con CARGO_PKG_VERSION o la global detectada"
        );
    }

    #[test]
    fn test_hudr2_cli_version_flag_and_help() {
        let exe = env!("CARGO_BIN_EXE_hudr2");

        // 1. hudr2 --version
        let out_long = std::process::Command::new(exe)
            .arg("--version")
            .output()
            .expect("falló ejecución de hudr2 --version");
        assert!(out_long.status.success());
        let stdout_long = String::from_utf8_lossy(&out_long.stdout);
        let expected_ver = format!("hudr2 {}", env!("CARGO_PKG_VERSION"));
        assert!(
            stdout_long.contains(&expected_ver),
            "hudr2 --version debe imprimir '{}', obtenido: '{}'",
            expected_ver,
            stdout_long
        );

        // 2. hudr2 -v
        let out_short = std::process::Command::new(exe)
            .arg("-v")
            .output()
            .expect("falló ejecución de hudr2 -v");
        assert!(out_short.status.success());
        let stdout_short = String::from_utf8_lossy(&out_short.stdout);
        assert!(
            stdout_short.contains(&expected_ver),
            "hudr2 -v debe imprimir '{}', obtenido: '{}'",
            expected_ver,
            stdout_short
        );

        // 3. hudr2 --help incluye --version
        let out_help = std::process::Command::new(exe)
            .arg("--help")
            .output()
            .expect("falló ejecución de hudr2 --help");
        assert!(out_help.status.success());
        let stdout_help = String::from_utf8_lossy(&out_help.stdout);
        assert!(
            stdout_help.contains("-v, --version"),
            "hudr2 --help debe listar -v, --version"
        );
        assert!(
            stdout_help.contains("14 de Herdr"),
            "hudr2 --help debe reflejar los 14 temas adoptados de Herdr"
        );
    }

    #[test]
    fn test_ecosystem_upstream_version_indicator_and_badge() {
        use hudr2::models::{EcosystemTelemetry, ToolVersionInfo};

        let mut state = AppState::new();
        state.snapshot.ecosystem = EcosystemTelemetry {
            tools: vec![
                ToolVersionInfo {
                    name: "engram".to_string(),
                    version: Some("v3.6.2".to_string()),
                    is_installed: true,
                    upstream_version: Some("v3.7.0".to_string()),
                    update_available: true,
                    ..Default::default()
                },
                ToolVersionInfo {
                    name: "herdr".to_string(),
                    version: Some("v0.9.5".to_string()),
                    is_installed: true,
                    upstream_version: Some("v0.9.5".to_string()),
                    update_available: false,
                    ..Default::default()
                },
            ],
        };

        // 1. Con check_updates activado: muestra badge ↑1 en el header y ↑v3.7.0 en la fila
        let render_on = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let text_on = render_on.lines.join("\n");
        assert!(text_on.contains("Stack"));
        assert!(
            text_on.contains("\u{2191}1"),
            "Header debe contener aviso ↑1"
        );
        assert!(
            text_on.contains("\u{2191}v3.7.0"),
            "Fila de engram debe mostrar ↑v3.7.0"
        );
        assert!(
            !text_on.contains("\u{2191}v0.9.5"),
            "Fila de herdr no debe mostrar flecha"
        );

        // 2. Con check_updates desactivado: se ocultan los avisos de actualización
        state.toggle_check_updates();
        assert!(!state.check_updates);
        let render_off = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let text_off = render_off.lines.join("\n");
        assert!(
            !text_off.contains("\u{2191}1"),
            "Header no debe mostrar ↑1 cuando check_updates está apagado"
        );
        assert!(
            !text_off.contains("\u{2191}v3.7.0"),
            "Fila no debe mostrar ↑v3.7.0 cuando check_updates está apagado"
        );
    }

    #[test]
    fn test_check_updates_config_persistence_and_mouse_toggle() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("hudr2.json");

        let mut st = AppState::with_config(Some(cfg_path.clone()), None);
        assert!(st.check_updates); // default true

        // Asegurar que la tarjeta personalizar (idx 12) esté expandida y renderizar
        if st.collapsed_cards[12] {
            st.toggle_card(12);
        }
        let render_out = ui::render_screen(&st.snapshot, &st.collapsed_cards, None, 48);
        st.card_bounds = render_out.bounds;

        let bound = st
            .card_bounds
            .iter()
            .find(|b| b.id == "toggle_check_updates")
            .expect("bound para toggle_check_updates debe existir");

        // Simular clic en la fila de toggle_check_updates
        let action = st.handle_mouse_click(bound.start_line, 0);
        assert!(matches!(
            action,
            Some(ClickAction::ToggleCheckUpdates(false))
        ));
        assert!(!st.check_updates);

        // Recargar en un nuevo AppState desde disco para verificar persistencia atómica
        let st2 = AppState::with_config(Some(cfg_path.clone()), None);
        assert!(
            !st2.check_updates,
            "El estado apagado de check_updates debe persistir en hudr2.json"
        );

        // Volver a activar con clic
        let mut st3 = AppState::with_config(Some(cfg_path.clone()), None);
        if st3.collapsed_cards[12] {
            st3.toggle_card(12);
        }
        let render_out2 = ui::render_screen(&st3.snapshot, &st3.collapsed_cards, None, 48);
        st3.card_bounds = render_out2.bounds;
        let bound2 = st3
            .card_bounds
            .iter()
            .find(|b| b.id == "toggle_check_updates")
            .expect("bound para toggle_check_updates debe existir");

        let action2 = st3.handle_mouse_click(bound2.start_line, 0);
        assert!(matches!(
            action2,
            Some(ClickAction::ToggleCheckUpdates(true))
        ));
        assert!(st3.check_updates);

        let st4 = AppState::with_config(Some(cfg_path), None);
        assert!(
            st4.check_updates,
            "El estado reactivado debe persistir en disco"
        );
    }

    #[test]
    fn test_sound_mode_config_persistence_and_mouse_cycle() {
        use hudr2::sound::SoundMode;

        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("hudr2.json");

        let mut st = AppState::with_config(Some(cfg_path.clone()), None);
        assert_eq!(st.sound_mode, SoundMode::Bell); // default Bell

        // Asegurar tarjeta personalizar (idx 12) expandida y renderizar
        if st.collapsed_cards[12] {
            st.toggle_card(12);
        }
        let render_out = ui::render_screen(&st.snapshot, &st.collapsed_cards, None, 48);
        st.card_bounds = render_out.bounds;

        let click_line = st
            .card_bounds
            .iter()
            .find(|b| b.id == "cycle_sound_mode")
            .map(|b| b.start_line)
            .expect("bound para cycle_sound_mode debe existir en Personalizar");

        // Simular clic en la fila de cycle_sound_mode: Bell -> Native
        let action = st.handle_mouse_click(click_line, 0);
        assert!(matches!(
            action,
            Some(ClickAction::CycleSoundMode(SoundMode::Native))
        ));
        assert_eq!(st.sound_mode, SoundMode::Native);

        // Recargar en un nuevo AppState desde disco para verificar persistencia atómica
        let st2 = AppState::with_config(Some(cfg_path.clone()), None);
        assert_eq!(
            st2.sound_mode,
            SoundMode::Native,
            "El modo Native debe persistir en hudr2.json"
        );

        // Simular segundo clic: Native -> Off
        let action2 = st.handle_mouse_click(click_line, 0);
        assert!(matches!(
            action2,
            Some(ClickAction::CycleSoundMode(SoundMode::Off))
        ));
        assert_eq!(st.sound_mode, SoundMode::Off);

        let st3 = AppState::with_config(Some(cfg_path.clone()), None);
        assert_eq!(
            st3.sound_mode,
            SoundMode::Off,
            "El modo Off debe persistir en hudr2.json"
        );

        // Simular tercer clic: Off -> Bell
        let action3 = st.handle_mouse_click(click_line, 0);
        assert!(matches!(
            action3,
            Some(ClickAction::CycleSoundMode(SoundMode::Bell))
        ));
        assert_eq!(st.sound_mode, SoundMode::Bell);

        let st4 = AppState::with_config(Some(cfg_path), None);
        assert_eq!(
            st4.sound_mode,
            SoundMode::Bell,
            "El modo Bell debe persistir en hudr2.json"
        );
    }

    #[test]
    fn test_orchestrator_recently_finished_visual_transition() {
        use hudr2::models::{OrchestratorProfile, OrchestratorStatus};

        let mut st = AppState::new();
        let orch_name = "test-orch".to_string();

        // 1. Orquestador trabajando
        st.snapshot.orchestrators = vec![OrchestratorProfile {
            name: orch_name.clone(),
            display_name: orch_name.clone(),
            model: "claude-3-7-sonnet".to_string(),
            model_short: "claude-3-7".to_string(),
            prefix: Some("claude".to_string()),
            thinking: None,
            is_default: true,
            status: OrchestratorStatus::Working,
            is_critical: false,
            is_warning: false,
            rem_pct: 90,
            subagents: Vec::new(),
            recently_finished: false,
        }];
        st.update_orchestrator_alerts();

        // Verificar que el estado inicial en Working fue registrado
        assert_eq!(
            st.orch_trackers.get(&orch_name).unwrap().last_status,
            OrchestratorStatus::Working
        );

        // 2. Transición a Idle
        st.snapshot.orchestrators[0].status = OrchestratorStatus::Idle;
        st.update_orchestrator_alerts();

        // Verificar que recently_finished se encendió
        assert!(st.snapshot.orchestrators[0].recently_finished);

        // Renderizar y comprobar que el glifo '✓ finalizado' está presente en el render
        let out = ui::render_screen(&st.snapshot, &st.collapsed_cards, None, 48);
        let screen_text = out.lines.join("\n");
        assert!(
            screen_text.contains("finalizado"),
            "La pantalla debe contener '✓ finalizado' durante la ventana de confirmación visual"
        );
    }

    #[test]
    fn test_herdr_build_attach_cmd_line_call_operator_on_windows() {
        use hudr2::herdr::HerdrClient;
        use std::path::Path;

        let exe = Path::new("C:\\Users\\$demo\\O'Brien\\hudr2.exe");
        let cmd = HerdrClient::build_attach_cmd_line(
            exe,
            "D:\\projects\\$(Write-Output hacked)\\demo",
            Some("w1:p'1"),
            Some(Path::new("D:\\cfg's.json")),
        );

        #[cfg(target_os = "windows")]
        {
            assert!(cmd.starts_with("& 'C:\\Users\\$demo\\O''Brien\\hudr2.exe'"));
            assert!(cmd.contains("--cwd 'D:\\projects\\$(Write-Output hacked)\\demo'"));
            assert!(cmd.contains("--pane 'w1:p''1'"));
            assert!(cmd.contains("--config 'D:\\cfg''s.json'"));
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert!(cmd.starts_with("\"C:\\Users\\$demo\\O'Brien\\hudr2.exe\""));
            assert!(cmd.contains("--cwd \"D:\\projects\\$(Write-Output hacked)\\demo\""));
            assert!(cmd.contains("--pane \"w1:p'1\""));
            assert!(cmd.contains("--config \"D:\\cfg's.json\""));
        }
    }

    #[test]
    fn test_herdr_split_pane_id_extraction_and_empty_output_handling() {
        use hudr2::herdr::HerdrClient;
        use serde_json::Value;

        // 1. Extracción de split_pane_id desde varios formatos válidos de Herdr
        let val1 = serde_json::json!({ "result": { "pane": { "pane_id": "w11:p2" } } });
        assert_eq!(
            HerdrClient::extract_split_pane_id(&val1).as_deref(),
            Some("w11:p2")
        );

        let val2 = serde_json::json!({ "pane": { "pane_id": "w11:p3" } });
        assert_eq!(
            HerdrClient::extract_split_pane_id(&val2).as_deref(),
            Some("w11:p3")
        );

        let val3 = serde_json::json!({ "pane_id": "w11:p4" });
        assert_eq!(
            HerdrClient::extract_split_pane_id(&val3).as_deref(),
            Some("w11:p4")
        );

        // 2. Value::Null no extrae id
        assert_eq!(HerdrClient::extract_split_pane_id(&Value::Null), None);
    }

    #[test]
    fn test_ecosystem_individual_update_buttons_and_click() {
        use hudr2::models::{EcosystemTelemetry, ToolVersionInfo};

        let mut state = AppState::new();
        state.snapshot.ecosystem = EcosystemTelemetry {
            tools: vec![
                ToolVersionInfo {
                    name: "pi".to_string(),
                    version: Some("v0.87.0".to_string()),
                    is_installed: true,
                    upstream_version: Some("v1.5.0".to_string()),
                    update_available: true,
                    ..Default::default()
                },
                ToolVersionInfo {
                    name: "herdr".to_string(),
                    version: Some("v0.9.5".to_string()),
                    is_installed: true,
                    upstream_version: Some("v0.9.5".to_string()),
                    update_available: false,
                    ..Default::default()
                },
            ],
        };

        // 1. Renderizado a 48 columnas: muestra el botón [actualizar] para la herramienta obsoleta
        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        state.card_bounds = render_out.bounds;

        let rendered_text = render_out.lines.join("\n");
        assert!(rendered_text.contains("[actualizar]"));

        // Verificar que existe el bound para update_tool en "pi"
        let pi_bound = state
            .card_bounds
            .iter()
            .find(|b| b.id == "update_tool" && b.action_path.as_deref() == Some("pi"))
            .expect("bound para update_tool(pi) debe existir");

        // Simular clic en la fila de pi
        let action = state.handle_mouse_click(pi_bound.start_line, 0);
        assert_eq!(
            action,
            Some(ClickAction::UpdateTool("pi".to_string())),
            "Clic en la fila de pi debe emitir UpdateTool(\"pi\")"
        );

        // herdr no debe tener bound de actualización
        assert!(
            !state
                .card_bounds
                .iter()
                .any(|b| b.id == "update_tool" && b.action_path.as_deref() == Some("herdr")),
            "herdr no debe tener bound de actualización porque está al día"
        );

        // 2. Estado is_updating con monitor de porcentaje en tiempo real
        state.snapshot.ecosystem.tools[0].is_updating = true;
        state.snapshot.ecosystem.tools[0].update_progress =
            Some(hudr2::models::ToolUpdateProgress {
                percent: 45,
                stage: "descargando".to_string(),
                spinner_idx: 0,
                elapsed_secs: 2,
            });
        let render_updating = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        state.card_bounds = render_updating.bounds;

        let updating_text = render_updating.lines.join("\n");
        assert!(
            updating_text.contains("[⠋ 45%]"),
            "Debe mostrar el monitor con porcentaje a la izquierda del botón"
        );
        assert!(
            updating_text.contains("[\u{2191}]"),
            "En ancho 48 muestra botón compacto [↑] junto al monitor"
        );
        assert!(
            !state
                .card_bounds
                .iter()
                .any(|b| b.id == "update_tool" && b.action_path.as_deref() == Some("pi")),
            "pi no debe tener bound de actualización mientras se está actualizando"
        );

        // 3. Ancho ancho (56 columnas): monitor [⠋ 45%] + [actualizando]
        let render_updating_56 =
            ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 56);
        let text_56 = render_updating_56.lines.join("\n");
        assert!(text_56.contains("[⠋ 45%]"));
        assert!(text_56.contains("[actualizando]"));

        // 4. Ancho adaptativo estrecho (42 columnas): monitor [⠋ 45%] + botón compacto [↑]
        let render_updating_42 =
            ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 42);
        let text_42 = render_updating_42.lines.join("\n");
        assert!(text_42.contains("[⠋ 45%]"));
        assert!(text_42.contains("[\u{2191}]"));

        // 4b. Estado completado (100%): monitor verde [✓ 100%] + [completado]
        state.snapshot.ecosystem.tools[0].update_progress =
            Some(hudr2::models::ToolUpdateProgress {
                percent: 100,
                stage: "completado".to_string(),
                spinner_idx: 0,
                elapsed_secs: 4,
            });
        let render_done_56 = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 56);
        let text_done_56 = render_done_56.lines.join("\n");
        assert!(
            text_done_56.contains("[✓ 100%]"),
            "Al 100% debe mostrar [✓ 100%]"
        );
        assert!(
            text_done_56.contains("[completado]"),
            "En ancho 56 debe mostrar [completado]"
        );

        let render_done_42 = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 42);
        let text_done_42 = render_done_42.lines.join("\n");
        assert!(text_done_42.contains("[✓ 100%]"));
        assert!(text_done_42.contains("[✓]"));

        // 5. Ancho estrecho estándar (36 columnas): botón adaptativo compacto [↑]
        state.snapshot.ecosystem.tools[0].is_updating = false;
        state.snapshot.ecosystem.tools[0].update_progress = None;
        let render_narrow = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 36);
        let narrow_text = render_narrow.lines.join("\n");
        assert!(narrow_text.contains("[↑]"));

        // Ninguna línea debe superar el ancho especificado
        for line in &render_narrow.lines {
            let visible_w = ui::string_width(line);
            assert!(
                visible_w <= 36,
                "Línea excede ancho 36 (ancho real {}): {}",
                visible_w,
                line
            );
        }
    }

    #[test]
    fn test_hudr2_assisted_update_notice_and_button() {
        use hudr2::ecosystem::{assisted_update_notice, update_cmd_for_tool};
        use hudr2::models::{EcosystemTelemetry, ToolVersionInfo};

        // 1. Invariantes del ecosistema para hudr2:
        // No tiene comando automático in-process (evita saturación y bloqueo os error 5 en Windows)
        assert!(update_cmd_for_tool("hudr2").is_none());
        let notice = assisted_update_notice("hudr2").expect("aviso asistido de hudr2 debe existir");
        assert!(notice.contains("cargo install --git"));

        // 2. Renderizado UI:
        let mut state = AppState::new();
        state.snapshot.ecosystem = EcosystemTelemetry {
            tools: vec![ToolVersionInfo {
                name: "hudr2".to_string(),
                version: Some("v0.1.0".to_string()),
                is_installed: true,
                upstream_version: Some("v0.2.0".to_string()),
                update_available: true,
                ..Default::default()
            }],
        };

        // Ancho estándar (48 columnas): muestra [asistido]
        let render_48 = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let text_48 = render_48.lines.join("\n");
        assert!(text_48.contains("[asistido]"));
        assert!(!text_48.contains("[actualizar]"));

        // Ancho estrecho (36 columnas): muestra [↑]
        let render_36 = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 36);
        let text_36 = render_36.lines.join("\n");
        assert!(text_36.contains("[↑]"));

        // 3. Verificación de bound de clic y acción UpdateTool("hudr2")
        state.card_bounds = render_48.bounds;
        let hudr2_bound = state
            .card_bounds
            .iter()
            .find(|b| b.id == "update_tool" && b.action_path.as_deref() == Some("hudr2"))
            .expect("bound para update_tool(hudr2) debe existir");

        let action = state.handle_mouse_click(hudr2_bound.start_line, 0);
        assert_eq!(
            action,
            Some(ClickAction::UpdateTool("hudr2".to_string())),
            "Clic en la fila de hudr2 debe emitir UpdateTool(\"hudr2\")"
        );
    }

    #[test]
    fn test_claude_api_key_accounts_discovery_and_ui_rendering() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {

        use hudr2::quotas::{
            discover_accounts_for, evaluate_account_for_model, fetch_quota_for_account_with_cache,
            is_api_key, is_fresh_within, needs_refresh, parse_api_key_accounts_from_opencode_json,
            parse_api_key_accounts_from_yaml,
        };

        // 1. Verificar parseo de config.yaml con cuentas de Claude
        let yaml_sample = r#"
api-keys:
    claude:
        - name: claude-1
          prefix: "tavoclaude"
          models:
            - name: "claude-sonnet-5-5"
            - name: "claude-opus-5-5"
          keys:
            - api-key: "sk-ant-sample-tavo"
        - name: claude-3
          prefix: "gianniclaude"
          keys:
            - api-key: "sk-ant-sample-gianni"
        - name: claude-4
          prefix: "clasclaude"
          keys:
            - api-key: "sk-ant-sample-clas"
"#;
        let accounts_yaml = parse_api_key_accounts_from_yaml(yaml_sample);
        assert_eq!(accounts_yaml.len(), 3);
        assert_eq!(
            accounts_yaml[0],
            ("tavoclaude".to_string(), "Claude".to_string())
        );
        assert_eq!(
            accounts_yaml[1],
            ("gianniclaude".to_string(), "Claude".to_string())
        );
        assert_eq!(
            accounts_yaml[2],
            ("clasclaude".to_string(), "Claude".to_string())
        );

        // 2. Verificar parseo de opencode.json models
        let opencode_json_sample = r#"{
            "providers": {
                "cliproxy": {
                    "models": {
                        "tavoclaude/claude-opus-5-5": {
                            "name": "Claude Opus 5.5 (high) [tavoclaude]"
                        },
                        "gianniclaude/claude-opus-5-5": {
                            "name": "Claude Opus 5.5 (high) [gianniclaude]"
                        }
                    }
                }
            }
        }"#;
        let accounts_opencode = parse_api_key_accounts_from_opencode_json(opencode_json_sample);
        assert_eq!(accounts_opencode.len(), 2);
        assert!(accounts_opencode.iter().any(|(p, _)| p == "tavoclaude"));
        assert!(accounts_opencode.iter().any(|(p, _)| p == "gianniclaude"));

        // 3. Crear entorno temporal con config.yaml y probar discover_accounts_for
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("config.yaml");
        let hudr2_cfg_path = tmp.path().join("hudr2.json");
        std::fs::write(&cfg_path, yaml_sample).unwrap();
        std::fs::write(&hudr2_cfg_path, "{}").unwrap();
        unsafe {
            std::env::set_var("CLIPROXY_CONFIG_PATH", cfg_path.to_str().unwrap());
            std::env::set_var("CLIPROXY_AUTHS_DIR", tmp.path().to_str().unwrap());
            std::env::set_var("HUDR2_CONFIG_PATH", hudr2_cfg_path.to_str().unwrap());
        }

        let mut discovered = Vec::new();
        discover_accounts_for(tmp.path().to_str().unwrap(), &mut discovered);

        unsafe {
            std::env::remove_var("CLIPROXY_CONFIG_PATH");
            std::env::remove_var("CLIPROXY_AUTHS_DIR");
        }

        assert_eq!(discovered.len(), 3);
        assert!(discovered.iter().any(|a| a.prefix == "tavoclaude"));
        assert!(discovered.iter().any(|a| a.prefix == "gianniclaude"));
        assert!(discovered.iter().any(|a| a.prefix == "clasclaude"));

        let tavo = discovered
            .iter()
            .find(|a| a.prefix == "tavoclaude")
            .unwrap();
        assert!(is_api_key(tavo));
        assert_eq!(tavo.reset_5h, "api-key");
        assert_eq!(tavo.reset_wk, "api-key");
        assert!(!needs_refresh(tavo));
        assert!(is_fresh_within(tavo, 600));

        // 4. Probar fetch_quota_for_account_with_cache (no debe fallar ni llamar HTTP)
        let mut tavo_clone = tavo.clone();
        let status = fetch_quota_for_account_with_cache(&mut tavo_clone, None).await;
        assert_eq!(status, 200);
        assert!(!tavo_clone.has_error);
        assert_eq!(tavo_clone.rem_5h, 100);

        // 5. Probar evaluación dinámica y semáforo de saldo para cuenta API Key
        let eff_claude = evaluate_account_for_model(tavo, "claude-opus-5-5");
        assert_eq!(eff_claude.family, "Claude");
        assert_eq!(eff_claude.rem_5h, 100);
        assert_eq!(eff_claude.reset_5h, "api-key");
        assert!(!eff_claude.is_critical);
        assert!(!eff_claude.is_warning);

        // Probar umbrales de advertencia (<= 20%) y crítico (<= 8%)
        let mut tavo_warning = tavo.clone();
        tavo_warning.rem_5h = 20;
        let eff_warn = evaluate_account_for_model(&tavo_warning, "claude-opus-5-5");
        assert!(!eff_warn.is_critical);
        assert!(eff_warn.is_warning);

        let mut tavo_critical = tavo.clone();
        tavo_critical.rem_5h = 8;
        let eff_crit = evaluate_account_for_model(&tavo_critical, "claude-opus-5-5");
        assert!(eff_crit.is_critical);
        assert!(!eff_crit.is_warning);

        // 6. Probar renderizado en UI sin saldo configurado (verdad de campo: $0.00 sesión y [definir saldo])
        let mut state = AppState::new();
        state.snapshot.accounts = discovered.clone();
        state.snapshot.agent_session.local_prefix = Some("clasclaude".to_string());
        state.snapshot.agent_session.model = "claude-opus-5-5".to_string();
        state.collapsed_cards[3] = false; // Integraciones expandida
        state.collapsed_cards[5] = false; // Cuota de Modelos expandida
        state.collapsed_cards[6] = false; // Saldos API expandida

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let screen = render_out.lines.join("\n");
        let clean_screen = ui::strip_ansi(&screen);

        assert!(clean_screen.contains("Integraciones · 3 cuentas"));
        assert!(clean_screen.contains("[api-key]"));
        assert!(clean_screen.contains("clasclaude"));
        assert!(!clean_screen.contains("> clasclaude"));
        assert!(clean_screen.contains("$0.00 sesión"));
        assert!(!clean_screen.contains("Wk:100%"));
        assert!(clean_screen.contains("Cuota de Modelos · clasclaude *"));
        assert!(clean_screen.contains("Claude API"));
        assert!(clean_screen.contains("Saldos API · 3 perfiles"));
        assert!(clean_screen.contains("[definir saldo]"));
        assert!(clean_screen.contains("Anthropic Console"));
        assert!(clean_screen.contains("[visitar ->]"));
        assert!(!clean_screen.contains("Claude Wk"));
        assert!(!clean_screen.contains("Claude 5h"));

        unsafe {
            std::env::remove_var("HUDR2_CONFIG_PATH");
        }

        // Verificar límites de clics interactivos para saldo y facturación oficial
        assert!(
            render_out
                .bounds
                .iter()
                .any(|b| b.action_type == Some("edit_balance")
                    && b.action_path.as_deref() == Some("clasclaude"))
        );
        assert!(
            render_out
                .bounds
                .iter()
                .any(|b| b.action_type == Some("open_billing_url")
                    && b.action_path.as_deref() == Some("https://platform.claude.com/spending"))
        );
        assert!(render_out.bounds.iter().any(|b| b.id == "saldos"));

        // 7. Probar mapeo modular de URLs y etiquetas de facturación para diferentes proveedores
        use hudr2::quotas::cliproxy::{provider_billing_label, provider_billing_url};
        assert_eq!(
            provider_billing_url("claude"),
            "https://platform.claude.com/spending"
        );
        assert_eq!(provider_billing_label("claude"), "Anthropic Console");
        assert_eq!(
            provider_billing_url("anthropic"),
            "https://platform.claude.com/spending"
        );
        assert_eq!(provider_billing_label("anthropic"), "Anthropic Console");
        assert_eq!(
            provider_billing_url("deepseek"),
            "https://platform.deepseek.com/top_up"
        );
        assert_eq!(provider_billing_label("deepseek"), "DeepSeek Platform");
        assert_eq!(
            provider_billing_url("google"),
            "https://aistudio.google.com/"
        );
        assert_eq!(provider_billing_label("google"), "Google AI Studio");
        assert_eq!(
            provider_billing_url("gemini"),
            "https://aistudio.google.com/"
        );
        assert_eq!(provider_billing_label("gemini"), "Google AI Studio");
        assert_eq!(provider_billing_url("opencode"), "https://opencode.ai/zen");
        assert_eq!(provider_billing_label("opencode"), "OpenCode Zen");
        assert_eq!(
            provider_billing_url("kimi"),
            "https://platform.moonshot.cn/console/pay"
        );
        assert_eq!(provider_billing_label("kimi"), "Moonshot Platform");
        assert_eq!(
            provider_billing_url("codex"),
            "https://chatgpt.com/#settings/Subscription"
        );
        assert_eq!(provider_billing_label("codex"), "ChatGPT Subscription");
        assert_eq!(
            provider_billing_url("openai"),
            "https://platform.openai.com/settings/organization/billing/overview"
        );
        assert_eq!(provider_billing_label("openai"), "OpenAI Platform");
        assert_eq!(
            provider_billing_url("openrouter"),
            "https://openrouter.ai/credits"
        );
        assert_eq!(provider_billing_label("openrouter"), "OpenRouter Credits");
        assert_eq!(
            provider_billing_url("groq"),
            "https://console.groq.com/settings/billing"
        );
        assert_eq!(provider_billing_label("groq"), "Groq Console");

        // 8. Probar ciclo de vida de edición de saldo interactiva
        state.start_edit_balance("clasclaude");
        assert!(state.is_editing_balance);
        state.update_balance_input_buffer("50.00".to_string());
        let edit_render = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let edit_screen = ui::strip_ansi(&edit_render.lines.join("\n"));
        assert!(edit_screen.contains("clasclaude Saldo $: 50.00"));
        assert!(edit_screen.contains("[Enter|Esc]"));

        state.cancel_edit_balance();
        assert!(!state.is_editing_balance);
        });
    }

    #[test]
    fn test_turn_and_task_cost_metrics_in_contexto_and_cuota_de_modelos() {
        let mut state = AppState::new();
        state.collapsed_cards[1] = false; // Contexto expandido
        state.collapsed_cards[5] = false; // Cuota de Modelos expandida
        state.snapshot.agent_session.turn_count = 10;
        state.snapshot.agent_session.cost = 0.25;
        state.snapshot.agent_session.last_turn_cost = 0.035;
        state.snapshot.agent_session.context_limit = 200_000;
        state.snapshot.agent_session.active_turn_tokens = 30_000;

        assert_eq!(state.snapshot.agent_session.avg_turn_cost(), 0.025);

        // Renderizado a 48 columnas
        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));

        // Verificaciones en Contexto
        assert!(screen.contains("10 turnos · prom: $0.025/req"));
        assert!(screen.contains("último: $0.035"));

        // Caso singular: 1 turno
        state.snapshot.agent_session.turn_count = 1;
        state.snapshot.agent_session.cost = 0.040;
        state.snapshot.agent_session.last_turn_cost = 0.040;
        let render_one = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let screen_one = ui::strip_ansi(&render_one.lines.join("\n"));
        assert!(screen_one.contains("1 turno · prom: $0.040/req"));
        assert!(screen_one.contains("último: $0.040"));
    }

    #[test]
    fn test_claude_api_key_balance_baseline_tracking() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cfg_path = temp_dir.path().join("hudr2.json");

        // Guardamos un budget de 37.27 para "tavoclaude" con un costo base de 4.10 en la sesión "ses_test_1"
        hudr2::state::save_account_budget(
            &cfg_path,
            "tavoclaude",
            Some(37.27),
            4.10,
            Some("ses_test_1".to_string()),
        )
        .unwrap();

        unsafe {
            std::env::set_var("HUDR2_CONFIG_PATH", &cfg_path);
        }

        // Caso 1: En la misma sesión, el costo total acumulado es 4.19 (hubo una consulta de 0.09)
        // El costo desde que se definió es 4.19 - 4.10 = 0.09
        // El saldo remanente debe ser 37.27 - 0.09 = 37.18
        let (budget, rem_balance, rem_pct, cost_since_set) =
            hudr2::quotas::calculate_api_key_balance("tavoclaude", 4.19, Some("ses_test_1"));

        assert_eq!(budget, Some(37.27));
        assert!((rem_balance.unwrap() - 37.18).abs() < 1e-4);
        assert_eq!(rem_pct, 99); // 37.18 / 37.27 = 99.7% -> 99%
        assert!((cost_since_set - 0.09).abs() < 1e-4);

        // Caso 2: Sesión diferente
        let (_, rem_diff_session, _, cost_diff_session) =
            hudr2::quotas::calculate_api_key_balance("tavoclaude", 0.09, Some("ses_diff_2"));
        assert!((rem_diff_session.unwrap() - 37.18).abs() < 1e-4);
        assert!((cost_diff_session - 0.09).abs() < 1e-4);

        unsafe {
            std::env::remove_var("HUDR2_CONFIG_PATH");
        }
    }

    #[tokio::test]
    async fn test_codex_subscription_account_discovery_local_validation_and_ui_rendering() {
        use hudr2::opencode::extract_prefix;
        use hudr2::quotas::{
            UNFETCHED_RESET, apply_codex_quota_data, discover_accounts_for,
            hydrate_accounts_from_cache, is_api_key, is_codex_account,
        };

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let auths_dir = tmp.path().join("auths");
        std::fs::create_dir_all(&auths_dir).unwrap();

        // 1. Archivo con nombre largo generado por CLIProxy/CPAMC sin prefijo explícito
        let codex_json_content = r#"{
            "access_token": "mock_token_codex",
            "account_id": "2566d8ef-3667-4faf-8188-6b811237393d",
            "disabled": false,
            "email": "developer@example.com",
            "expired": "2030-11-21T01:05:38-03:00",
            "last_refresh": "2026-11-11T01:05:38-03:00",
            "plan_type": "plus",
            "type": "codex"
        }"#;
        let long_named_file = auths_dir.join("codex-6b6f7a00-developer@example.com-plus.json");
        std::fs::write(&long_named_file, codex_json_content).unwrap();

        // 2. Archivo con prefijo explícito (ej. tavocodex)
        let tavocodex_json_content = r#"{
            "access_token": "mock_token_tavo",
            "disabled": false,
            "email": "developer@example.com",
            "expired": "2030-11-21T01:05:38-03:00",
            "plan_type": "plus",
            "prefix": "tavocodex",
            "type": "codex"
        }"#;
        let tavo_file = auths_dir.join("tavocodex.json");
        std::fs::write(&tavo_file, tavocodex_json_content).unwrap();

        unsafe {
            std::env::set_var("CLIPROXY_AUTHS_DIR", auths_dir.to_str().unwrap());
        }

        let mut accounts = Vec::new();
        discover_accounts_for(tmp.path().to_str().unwrap(), &mut accounts);

        unsafe {
            std::env::remove_var("CLIPROXY_AUTHS_DIR");
        }
        drop(_guard);

        assert_eq!(accounts.len(), 2);
        let codex_acc = accounts
            .iter()
            .find(|a| a.prefix == "codex")
            .expect("debe normalizar prefijo a 'codex'")
            .clone();
        let tavo_acc = accounts
            .iter()
            .find(|a| a.prefix == "tavocodex")
            .expect("debe respetar prefijo explícito")
            .clone();

        assert!(is_codex_account(&codex_acc));
        assert!(!is_api_key(&codex_acc));
        assert_eq!(codex_acc.reset_5h, UNFETCHED_RESET);
        assert_eq!(codex_acc.reset_wk, UNFETCHED_RESET);
        assert_eq!(codex_acc.rem_5h, 0);
        assert_eq!(codex_acc.rem_wk, 0);
        assert!(!codex_acc.has_error);
        assert_eq!(codex_acc.pools.len(), 1);
        assert_eq!(codex_acc.pools[0].label, "Codex Plus");

        assert!(is_codex_account(&tavo_acc));
        assert!(!is_api_key(&tavo_acc));
        assert_eq!(tavo_acc.reset_5h, UNFETCHED_RESET);
        let unknown_health =
            hudr2::quotas::evaluate_account_for_model(&tavo_acc, "gpt-5.6-sol");
        assert!(!unknown_health.is_critical);
        assert!(!unknown_health.is_warning);

        // 3. Aplicar la respuesta real de Codex: una ventana semanal puede venir en primary_window.
        let mut codex_clone = codex_acc.clone();
        let usage = serde_json::json!({
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 91.0,
                    "limit_window_seconds": 604800,
                    "reset_at": 1800000000
                },
                "secondary_window": null
            }
        });
        assert!(apply_codex_quota_data(&mut codex_clone, &usage));
        assert!(!codex_clone.has_error);
        assert_eq!(codex_clone.rem_5h, 9);
        assert_eq!(codex_clone.rem_wk, 9);
        assert_eq!(codex_clone.pools.len(), 1);
        assert_eq!(codex_clone.pools[0].label, "Codex Plus Wk");
        assert_eq!(codex_clone.pools[0].percent, 9);

        let health = hudr2::quotas::evaluate_account_for_model(&codex_clone, "gpt-5.6-sol");
        assert!(health.is_critical, "9% semanal restante debe ser crítico");
        assert_eq!(health.rem_wk, 9);

        // 4. Probar que una entrada de error en caché no clobbee la cuenta válida
        hydrate_accounts_from_cache(&mut accounts, None);
        let codex_hydrated = accounts.iter().find(|a| a.prefix == "codex").unwrap();
        assert!(!codex_hydrated.has_error);

        // 5. Probar extract_prefix con agente y modelo de Codex
        let known_prefixes = vec![
            "agy".to_string(),
            "codex".to_string(),
            "tavocodex".to_string(),
        ];
        let pfx_agent = extract_prefix(
            "Gentle-Orchestrator-Codex",
            "cliproxy/gpt-5.6-sol",
            &known_prefixes,
        );
        assert_eq!(pfx_agent, Some("codex".to_string()));

        let pfx_model = extract_prefix(
            "orchestrator",
            "GPT-5.6 Sol (high) [codex]",
            &known_prefixes,
        );
        assert_eq!(pfx_model, Some("codex".to_string()));

        let pfx_tavo = extract_prefix(
            "Gentle-Orchestrator-Codex",
            "cliproxy/gpt-5.6-sol",
            &["tavocodex".to_string()],
        );
        assert_eq!(pfx_tavo, Some("tavocodex".to_string()));

        // 6. Probar renderizado en UI de Integraciones y Cuota de Modelos
        let mut state = AppState::new();
        state.collapsed_cards[3] = false; // Integraciones expandida
        state.collapsed_cards[5] = false; // Cuota de Modelos expandida
        state.snapshot.accounts = vec![codex_clone.clone()];
        state.snapshot.agent_session.local_prefix = Some("codex".to_string());
        state.snapshot.agent_session.cost = 0.45;
        state.snapshot.agent_session.model = "gpt-5.6-sol".to_string();

        let render = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48);
        let screen = ui::strip_ansi(&render.lines.join("\n"));

        assert!(screen.contains("codex"));
        assert!(screen.contains("9%"));
        assert!(!screen.contains("[expirado]"));
        assert!(!screen.contains("? codex"));
        assert!(screen.contains("Cuota de Modelos · codex *"));
        assert!(screen.contains("Codex Plus Wk"));
        assert!(screen.contains("cuota real"));
        // Codex es cuenta de suscripción mensual/semanal, NO debe renderizar la card Saldos API
        assert!(!screen.contains("Saldos API ·"));

        // 7. Sin respuesta remota válida, una flota configurada debe mostrar desconocido.
        let mut unknown_state = AppState::new();
        unknown_state.collapsed_cards[3] = false;
        unknown_state.collapsed_cards[4] = false;
        unknown_state.collapsed_cards[5] = false;
        unknown_state.snapshot.accounts = vec![codex_acc];
        unknown_state.snapshot.orchestrators = vec![hudr2::models::OrchestratorProfile {
            name: "fleet-codex".to_string(),
            display_name: "tavocodex".to_string(),
            model: "codex/gpt-5.6-sol".to_string(),
            model_short: "gpt-5.6-sol".to_string(),
            prefix: Some("codex".to_string()),
            status: hudr2::models::OrchestratorStatus::Working,
            ..Default::default()
        }];
        let unknown_render = ui::render_screen(
            &unknown_state.snapshot,
            &unknown_state.collapsed_cards,
            None,
            80,
        );
        let pools = unknown_render
            .bounds
            .iter()
            .find(|bound| bound.id == "pools")
            .unwrap();
        let unknown_screen =
            ui::strip_ansi(&unknown_render.lines[pools.start_line..=pools.end_line].join("\n"));
        assert!(unknown_screen.contains("?[sin consultar]"));
        assert!(!unknown_screen.contains("100%"));

        let integrations = unknown_render
            .bounds
            .iter()
            .find(|bound| bound.id == "integraciones")
            .unwrap();
        let integrations_screen = ui::strip_ansi(
            &unknown_render.lines[integrations.start_line..=integrations.end_line].join("\n"),
        );
        assert!(integrations_screen.contains("[sin consultar]"));
        assert!(!integrations_screen.contains("0%"));
        assert!(!integrations_screen.contains("AGOTÁNDOSE"));
    }

    #[test]
    fn test_integraciones_card_pure_inventory_monitor_no_active_session_tracking() {
        use hudr2::models::{AccountQuota, QuotaPool};
        use hudr2::state::AppState;

        let mut state = AppState::new();
        state.collapsed_cards[3] = false; // Integraciones expandida

        // 1. Tres integraciones: Antigravity, Codex y Claude API Key
        let agy = AccountQuota {
            prefix: "agy1".to_string(),
            auth_file: "auth_agy1.json".to_string(),
            rem_5h: 90,
            reset_5h: "4h 30m".to_string(),
            rem_wk: 85,
            reset_wk: "3d 10h".to_string(),
            is_critical: false,
            is_warning: false,
            has_error: false,
            error_msg: String::new(),
            pools: vec![],
        };

        let codex = AccountQuota {
            prefix: "tavocodex".to_string(),
            auth_file: "tavocodex.json".to_string(),
            rem_5h: 100,
            reset_5h: "plus".to_string(),
            rem_wk: 100,
            reset_wk: "plus".to_string(),
            is_critical: false,
            is_warning: false,
            has_error: false,
            error_msg: String::new(),
            pools: vec![
                QuotaPool {
                    label: "Codex Plus".to_string(),
                    percent: 100,
                    reset: "plus".to_string(),
                    pace: "$0.000".to_string(),
                },
                QuotaPool {
                    label: "Codex Saldo".to_string(),
                    percent: 100,
                    reset: "plus".to_string(),
                    pace: "$50.00".to_string(),
                },
            ],
        };

        let claude = AccountQuota {
            prefix: "tavoclaude".to_string(),
            auth_file: "config.yaml#tavoclaude".to_string(),
            rem_5h: 100,
            reset_5h: "api-key".to_string(),
            rem_wk: 100,
            reset_wk: "api-key".to_string(),
            is_critical: false,
            is_warning: false,
            has_error: false,
            error_msg: String::new(),
            pools: vec![
                QuotaPool {
                    label: "Claude API".to_string(),
                    percent: 100,
                    reset: "api-key".to_string(),
                    pace: "$0.000".to_string(),
                },
                QuotaPool {
                    label: "Claude Saldo".to_string(),
                    percent: 100,
                    reset: "api-key".to_string(),
                    pace: "$50.00".to_string(),
                },
            ],
        };

        state.snapshot.accounts = vec![agy, codex, claude];
        state.snapshot.agent_session.local_prefix = Some("tavoclaude".to_string());
        state
            .snapshot
            .agent_session
            .all_active_prefixes
            .insert("tavoclaude".to_string());
        state
            .snapshot
            .agent_session
            .all_active_prefixes
            .insert("agy1".to_string());

        let render = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen = ui::strip_ansi(&render.lines.join("\n"));

        // 2. Encabezado debe mostrar las 3 integraciones disponibles
        assert!(screen.contains("Integraciones · 3 cuentas"));

        // 3. Ver que están las tres cuentas listadas como inventario
        assert!(screen.contains("agy1"));
        assert!(screen.contains("tavocodex"));
        assert!(screen.contains("tavoclaude"));

        // 4. Integraciones NO debe tener '>' indicando cuenta activa local
        assert!(!screen.contains("> tavoclaude"));
        assert!(!screen.contains("> agy1"));

        // 5. Integraciones NO debe tener alertas de migración activa "(activa)"
        assert!(!screen.contains("(activa)"));
        assert!(!screen.contains("[presioná 'x' -> migrar"));
    }

    #[test]
    fn test_release_inactive_sessions_strict_cwd_containment_and_no_foreign_workspace_close() {
        use hudr2::models::AccountQuota;
        use rusqlite::Connection;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let temp_dir = tempfile::tempdir().unwrap();
        let oc_db = temp_dir.path().join("opencode.db");
        {
            let conn = Connection::open(&oc_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE session (
                    id TEXT PRIMARY KEY,
                    directory TEXT,
                    title TEXT,
                    time_updated INTEGER,
                    time_archived INTEGER
                );
                CREATE TABLE message (
                    id TEXT PRIMARY KEY,
                    session_id TEXT,
                    time_created INTEGER,
                    data TEXT
                );
                INSERT INTO session (id, directory, title, time_updated, time_archived)
                    VALUES ('ses_curr', 'D:/temp/curr_project', 'Current Ses', 5000, NULL);
                INSERT INTO session (id, directory, title, time_updated, time_archived)
                    VALUES ('ses_foreign', 'D:/temp/other_project', 'Foreign Ses', 5000, NULL);
                INSERT INTO message (id, session_id, time_created, data)
                    VALUES ('m_curr', 'ses_curr', 5000, '{\"agent\":\"orchestrator-cuenta1\",\"providerID\":\"cpamc\",\"modelID\":\"cuenta1/gemini-3.8-flash-high\"}');
                INSERT INTO message (id, session_id, time_created, data)
                    VALUES ('m_foreign', 'ses_foreign', 5000, '{\"agent\":\"orchestrator-cuenta2\",\"providerID\":\"cpamc\",\"modelID\":\"cuenta2/gemini-3.8-flash-high\"}');",
            )
            .unwrap();
        }

        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", &oc_db);
            // ENGRAM_BIN establecido a un binario simulado para activar filtrado real de directorio
            std::env::set_var("ENGRAM_BIN", "mock_engram_binary");
        }

        let accounts = vec![
            AccountQuota {
                prefix: "cuenta1".to_string(),
                rem_5h: 90,
                rem_wk: 90,
                ..Default::default()
            },
            AccountQuota {
                prefix: "cuenta2".to_string(),
                rem_5h: 90,
                rem_wk: 90,
                ..Default::default()
            },
        ];

        // Ejecutar release en D:/temp/curr_project
        let rel = hudr2::cleanup::release_inactive_sessions(
            "D:/temp/curr_project",
            &accounts,
            Some("none_pane"),
            Some("none_tab"),
        );

        assert_eq!(
            rel.closed_count, 1,
            "Solo debe archivar la sesión del proyecto actual"
        );
        assert_eq!(rel.released_prefixes, vec!["cuenta1".to_string()]);

        // Verificar en SQLite que ses_foreign NO fue archivada (time_archived IS NULL)
        {
            let conn = Connection::open(&oc_db).unwrap();
            let foreign_archived: Option<i64> = conn
                .query_row(
                    "SELECT time_archived FROM session WHERE id = 'ses_foreign'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                foreign_archived.is_none(),
                "La sesión de otro proyecto/workspace jamás debe ser archivada ni cerrada"
            );

            let curr_archived: Option<i64> = conn
                .query_row(
                    "SELECT time_archived FROM session WHERE id = 'ses_curr'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(
                curr_archived.is_some(),
                "La sesión del proyecto actual en desuso sí debe ser archivada"
            );
        }

        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
            std::env::remove_var("ENGRAM_BIN");
        }
    }

    #[test]
    fn test_read_capped_rejects_oversized_files_instead_of_truncating() {
        let tmp = tempfile::tempdir().unwrap();
        let small = tmp.path().join("small.json");
        let big = tmp.path().join("big.json");
        std::fs::write(&small, b"{\"a\":1}").unwrap();
        std::fs::write(&big, vec![b' '; 65]).unwrap();

        assert_eq!(hudr2::paths::read_capped(&small, 64).unwrap(), b"{\"a\":1}");
        let err = hudr2::paths::read_capped(&big, 64).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            hudr2::paths::read_capped(tmp.path().join("missing"), 64)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            hudr2::paths::read_config_string(&small).unwrap(),
            "{\"a\":1}"
        );
    }

    #[test]
    fn test_pi_tail_skips_oversized_line_and_recovers_on_next_line() {
        use hudr2::pi::{PI_MAX_LINE_BYTES, PiTail};

        let mut tail = PiTail::new();
        let junk = vec![b'x'; PI_MAX_LINE_BYTES / 2 + 1];
        tail.ingest(&junk, false);
        tail.ingest(&junk, false);
        assert!(
            tail.pending.is_empty(),
            "An oversized partial line must not stay buffered"
        );

        let model_line =
            b"{\"type\":\"model_change\",\"provider\":\"cpamc\",\"modelId\":\"gpt-6.1-sol\"}\n";
        let mut rest = junk.clone();
        rest.extend_from_slice(b"tail-of-junk\n");
        rest.extend_from_slice(model_line);
        tail.ingest(&rest, false);

        assert!(tail.pending.is_empty());
        assert_eq!(tail.session.model_short, "gpt-6.1-sol");
        assert_eq!(tail.offset, (junk.len() * 2 + rest.len()) as u64);
    }

    #[test]
    fn test_terminal_output_keeps_sgr_and_strips_untrusted_control_sequences() {
        let mut snap = TelemetrySnapshot::default();
        // Hostile branch name: OSC title change, screen clear, terminal reset, CR, BS, C1 CSI.
        snap.git.branch = "feat\x1b]0;pwned\x07\x1b[2J\x1bc\r\x08\u{9b}31m-x".to_string();
        let collapsed = [false; 13];
        let render = ui::render_screen(&snap, &collapsed, None, 48);
        let frame: String = render
            .lines
            .iter()
            .map(|l| ui::sanitize_terminal_line(l).into_owned())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            frame.contains("feat31m-x"),
            "Visible text must survive sanitization"
        );
        assert!(
            frame.contains("\x1b[38;2;"),
            "SGR styling must be preserved"
        );
        assert!(!frame.contains("\x1b]") && !frame.contains("\x1b[2J") && !frame.contains("\x1bc"));
        assert!(!frame.contains('\r') && !frame.contains('\x08') && !frame.contains('\u{9b}'));

        // Every remaining escape must be an SGR sequence (ESC [ digits/;/: m).
        let mut rest = frame.as_str();
        while let Some(pos) = rest.find('\x1b') {
            let seq = &rest[pos..];
            let end = seq.find('m').expect("escape must terminate with 'm'");
            assert!(
                seq[1..2] == *"["
                    && seq[2..end]
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == ';' || c == ':'),
                "Non-SGR escape leaked: {:?}",
                &seq[..end.min(16)]
            );
            rest = &seq[end + 1..];
        }

        // Plain text has no allocation and no change.
        assert!(matches!(
            ui::sanitize_terminal_line("plain"),
            std::borrow::Cow::Borrowed("plain")
        ));
    }

    #[test]
    fn test_release_inactive_sessions_never_archives_unowned_or_foreign_orphans() {
        use hudr2::models::AccountQuota;
        use rusqlite::Connection;

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let temp_dir = tempfile::tempdir().unwrap();
        let oc_db = temp_dir.path().join("opencode.db");
        {
            let conn = Connection::open(&oc_db).unwrap();
            conn.execute_batch(
                "CREATE TABLE session (
                    id TEXT PRIMARY KEY,
                    directory TEXT,
                    title TEXT,
                    time_updated INTEGER,
                    time_archived INTEGER
                );
                CREATE TABLE message (
                    id TEXT PRIMARY KEY,
                    session_id TEXT,
                    time_created INTEGER,
                    data TEXT
                );
                INSERT INTO session (id, directory, title, time_updated, time_archived) VALUES
                    ('ses_own', 'D:/temp/my_proj', 'Own', 5000, NULL),
                    ('ses_own_variant', ' d:\\TEMP\\My_Proj\\ ', 'Own, other spelling', 5000, NULL),
                    ('ses_null_dir', NULL, 'No dir', 5000, NULL),
                    ('ses_empty_dir', '   ', 'Empty dir', 5000, NULL),
                    ('ses_mock_dir', 'D:/proj', 'Old mock dir', 5000, NULL),
                    ('ses_other', 'D:/temp/other_proj', 'Other', 5000, NULL);",
            )
            .unwrap();
        }

        unsafe {
            std::env::set_var("OPENCODE_DB_PATH", &oc_db);
            std::env::set_var("ENGRAM_BIN", "off");
        }

        let accounts = vec![AccountQuota {
            prefix: "cuenta1".to_string(),
            rem_5h: 90,
            rem_wk: 90,
            ..Default::default()
        }];

        // A cwd ending in "proj" used to trigger a test-only bypass in production code.
        let rel = hudr2::cleanup::release_inactive_sessions(
            "D:/temp/my_proj",
            &accounts,
            Some("none_pane"),
            Some("none_tab"),
        );

        let archived: Vec<String> = {
            let conn = Connection::open(&oc_db).unwrap();
            let mut stmt = conn
                .prepare("SELECT id FROM session WHERE time_archived IS NOT NULL ORDER BY id")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .flatten()
                .collect()
        };

        unsafe {
            std::env::remove_var("OPENCODE_DB_PATH");
            std::env::remove_var("ENGRAM_BIN");
        }

        assert_eq!(
            archived,
            vec!["ses_own".to_string(), "ses_own_variant".to_string()],
            "Only sessions owned by the current workspace may be archived, whatever the path spelling"
        );
        assert_eq!(rel.closed_count, 2);
    }

    #[test]
    fn test_orquestadores_card_discovery_sovereign_quota_and_status() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::models::{AccountQuota, AgentSession, OrchestratorStatus};
        use hudr2::orchestrators::discover_orchestrators;
        use hudr2::state::{AppState, ClickAction};

        let tmp = tempfile::tempdir().unwrap();
        let target_dir = tmp.path().to_str().unwrap();

        // 1. Crear estructura de configuración para Gentle AI profiles.json
        let gentle_dir = tmp.path().join(".gentle-ai");
        std::fs::create_dir_all(&gentle_dir).unwrap();
        let profiles_json = serde_json::json!({
            "default": "fleet-codex",
            "profiles": {
                "fleet-codex": {
                    "orchestrator": {
                        "model": "gpt-5.6-sol",
                        "thinking": "high"
                    },
                    "agents": {
                        "researcher": {
                            "model": "google/gemini-2.5-pro"
                        }
                    }
                },
                "fleet-tavo": {
                    "orchestrator": {
                        "model": "tavoclaude/claude-opus-5-5"
                    },
                    "agents": {
                        "coder": {
                            "model": "google/gemini-2.5-flash"
                        }
                    }
                }
            }
        });
        let profiles_file = gentle_dir.join("profiles.json");
        std::fs::write(
            &profiles_file,
            serde_json::to_string_pretty(&profiles_json).unwrap(),
        )
        .unwrap();

        // 2. Crear opencode.json con un orquestador adicional
        let opencode_json = serde_json::json!({
            "agent": {
                "gentle-orchestrator": {
                    "model": "cliproxy/clasclaude/claude-opus-5-5",
                    "permission": {
                        "task": true
                    }
                }
            }
        });
        let opencode_file = tmp.path().join("opencode.json");
        std::fs::write(
            &opencode_file,
            serde_json::to_string_pretty(&opencode_json).unwrap(),
        )
        .unwrap();

        unsafe {
            std::env::set_var("GENTLE_AI_PROFILES_PATH", profiles_file.to_str().unwrap());
            std::env::set_var("OPENCODE_CONFIG_PATH", opencode_file.to_str().unwrap());
        }

        // 3. Cuentas: tavocodex saludable (100%), tavoclaude CRÍTICA (0%), y clasclaude (80%)
        let codex_acc = AccountQuota {
            prefix: "tavocodex".to_string(),
            auth_file: "tavocodex.json".to_string(),
            rem_5h: 100,
            reset_5h: "plus".to_string(),
            rem_wk: 100,
            reset_wk: "plus".to_string(),
            ..Default::default()
        };
        let claude_acc = AccountQuota {
            prefix: "tavoclaude".to_string(),
            auth_file: "config.yaml#tavoclaude".to_string(),
            rem_5h: 0,
            reset_5h: "1h 10m".to_string(),
            rem_wk: 0,
            reset_wk: "1d 2h".to_string(),
            is_critical: true,
            ..Default::default()
        };
        let clas_acc = AccountQuota {
            prefix: "clasclaude".to_string(),
            auth_file: "config.yaml#clasclaude".to_string(),
            rem_5h: 80,
            reset_5h: "3h 00m".to_string(),
            rem_wk: 80,
            reset_wk: "4d 10h".to_string(),
            ..Default::default()
        };

        let accounts = vec![codex_acc, claude_acc, clas_acc];

        // Sesión activa ejecutando fleet-tavo en estado working
        let active_session = AgentSession {
            agent: "fleet-tavo".to_string(),
            agent_status: "working".to_string(),
            local_prefix: Some("tavoclaude".to_string()),
            model_short: "claude-opus-5-5".to_string(),
            model: "tavoclaude/claude-opus-5-5".to_string(),
            ..Default::default()
        };

        let panes = vec![];

        let discovered = discover_orchestrators(target_dir, &accounts, &active_session, &panes);
        assert!(discovered.len() >= 3);

        // fleet-tavo está Working y con cuota crítica (0%) pues su modelo de orquestador usa tavoclaude
        let tavo = discovered
            .iter()
            .find(|o| o.name == "fleet-tavo")
            .expect("fleet-tavo debe existir");
        assert_eq!(tavo.status, OrchestratorStatus::Working);
        assert!(
            tavo.is_critical,
            "La cuota del orquestador debe reflejar la del modelo principal tavoclaude (0%)"
        );
        assert_eq!(tavo.rem_pct, 0);

        // fleet-codex está Offline (no hay sesión ni pane activo para él) y saludable (100%)
        let codex = discovered
            .iter()
            .find(|o| o.name == "fleet-codex")
            .expect("fleet-codex debe existir");
        assert_eq!(codex.status, OrchestratorStatus::Offline);
        assert!(!codex.is_critical);
        assert_eq!(codex.thinking.as_deref(), Some("high"));
        assert!(codex.is_default);

        // gentle-orchestrator está Offline y saludable (80%)
        let gentle = discovered
            .iter()
            .find(|o| o.name == "gentle-orchestrator")
            .expect("gentle-orchestrator debe existir");
        assert_eq!(gentle.status, OrchestratorStatus::Offline);
        assert_eq!(gentle.rem_pct, 80);

        // 4. Renderizado en AppState con Orquestadores expandido (card index 4)
        let mut state = AppState::new();
        state.collapsed_cards[4] = false; // Expandir Orquestadores
        state.snapshot.orchestrators = discovered;

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));

        assert!(screen.contains("Orquestadores"));
        assert!(screen.contains("tavoclaude"));
        assert!(screen.contains("tavocodex"));
        assert!(screen.contains("clasclaude"));
        assert!(screen.contains("apagado"));
        assert!(screen.contains("[CUOTA CR\u{00cd}TICA]"));
        assert!(screen.contains("! CR\u{00cd}TICO: fleet-tavo [x -> migrar]"));

        // En ancho amplio (80 cols), debe mostrar el banner extendido con la sugerencia de migración
        let render_wide = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 80);
        let screen_wide = ui::strip_ansi(&render_wide.lines.join("\n"));
        assert!(screen_wide.contains(
            "! CR\u{00cd}TICO: fleet-tavo (activo) [presion\u{00e1} 'x' -> migrar a fleet-codex]"
        ));

        // 5. Interacción del mouse: clic en el header de Orquestadores (card 4)
        state.card_bounds = render_out.bounds;
        let orch_bound = state
            .card_bounds
            .iter()
            .find(|b| b.id == "orquestadores")
            .expect("bound de la card orquestadores debe existir");

        let click = state.handle_mouse_click(orch_bound.start_line, 0);
        assert_eq!(click, Some(ClickAction::ToggledCard(4)));
        assert!(state.collapsed_cards[4]);

        unsafe {
            std::env::remove_var("GENTLE_AI_PROFILES_PATH");
            std::env::remove_var("OPENCODE_CONFIG_PATH");
        }
    }

    #[test]
    fn test_cuota_de_modelos_orchestrator_fleet_breakdown() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::models::{
            AccountQuota, OrchestratorProfile, OrchestratorStatus, SubAgentProfile,
        };
        use hudr2::state::{AppState, ClickAction};

        let mut state = AppState::new();
        state.collapsed_cards[5] = false; // Expandir Cuota de Modelos

        // 1. Configurar un perfil de orquestador activo con subagentes
        let orch = OrchestratorProfile {
            name: "fleet-tavo".to_string(),
            display_name: "tavoclaude".to_string(),
            model: "tavoclaude/claude-opus-5-5".to_string(),
            model_short: "claude-opus-5-5".to_string(),
            prefix: Some("tavoclaude".to_string()),
            status: OrchestratorStatus::Working,
            subagents: vec![
                SubAgentProfile {
                    role: "gentle-ai-worker".to_string(),
                    model: "agy2/gemini-3.8-flash-high".to_string(),
                    model_short: "gemini-3.8-flash-high".to_string(),
                    prefix: Some("agy2".to_string()),
                    thinking: Some("high".to_string()),
                },
                SubAgentProfile {
                    role: "jd-fix-agent".to_string(),
                    model: "agy2/gemini-3.8-flash-high".to_string(),
                    model_short: "gemini-3.8-flash-high".to_string(),
                    prefix: Some("agy2".to_string()),
                    thinking: Some("high".to_string()),
                },
                SubAgentProfile {
                    role: "gentle-ai-explore".to_string(),
                    model: "agy/gemini-3.8-flash-medium".to_string(),
                    model_short: "gemini-3.8-flash-medium".to_string(),
                    prefix: Some("agy".to_string()),
                    thinking: Some("medium".to_string()),
                },
                SubAgentProfile {
                    role: "review-risk".to_string(),
                    model: "agy3/gemini-3.8-flash-high".to_string(),
                    model_short: "gemini-3.8-flash-high".to_string(),
                    prefix: Some("agy3".to_string()),
                    thinking: Some("high".to_string()),
                },
            ],
            ..Default::default()
        };

        state.snapshot.orchestrators = vec![orch];

        // 2. Configurar cuentas de Integraciones
        let claude_acc = AccountQuota {
            prefix: "tavoclaude".to_string(),
            auth_file: "config.yaml#tavoclaude".to_string(),
            rem_5h: 100,
            reset_5h: "api-key".to_string(),
            rem_wk: 100,
            reset_wk: "api-key".to_string(),
            ..Default::default()
        };
        let agy2_acc = AccountQuota {
            prefix: "agy2".to_string(),
            auth_file: "auth_agy2.json".to_string(),
            rem_5h: 95,
            reset_5h: "4h 20m".to_string(),
            rem_wk: 90,
            reset_wk: "3d 12h".to_string(),
            ..Default::default()
        };
        let agy_acc = AccountQuota {
            prefix: "agy".to_string(),
            auth_file: "auth_agy.json".to_string(),
            rem_5h: 80,
            reset_5h: "2h 10m".to_string(),
            rem_wk: 85,
            reset_wk: "2d 08h".to_string(),
            ..Default::default()
        };
        let agy3_acc = AccountQuota {
            prefix: "agy3".to_string(),
            auth_file: "auth_agy3.json".to_string(),
            rem_5h: 100,
            reset_5h: "4h 50m".to_string(),
            rem_wk: 98,
            reset_wk: "5d 00h".to_string(),
            ..Default::default()
        };

        state.snapshot.accounts = vec![claude_acc, agy2_acc, agy_acc, agy3_acc];

        // 3. Estado de sesión activa:
        // Orquestador está en working
        state.snapshot.agent_session.agent = "fleet-tavo".to_string();
        state.snapshot.agent_session.agent_status = "working".to_string();
        state.snapshot.agent_session.local_prefix = Some("tavoclaude".to_string());
        state.snapshot.agent_session.model_short = "claude-opus-5-5".to_string();

        // Explore (agy) ya trabajó en esta sesión y ahora está pasivo
        state
            .snapshot
            .agent_session
            .all_active_prefixes
            .insert("agy".to_string());

        // 4. Renderizado y verificación
        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));
        let raw = render_out.lines.join("\n");

        // Header contiene el nombre del orquestador activo
        assert!(screen.contains("Cuota de Modelos · tavoclaude *"));

        // Orquestador activo tiene '>'
        assert!(screen.contains("> orch"));
        assert!(screen.contains("tavoclaude"));

        // Subagente pasivo (explore/agy) tiene '●' en color púrpura/MAGENTA
        assert!(raw.contains(hudr2::config::MAGENTA));
        assert!(screen.contains("● explore"));
        assert!(screen.contains("agy"));
        assert!(screen.contains("80%"));

        // Subagentes inactivos individuales: worker (agy2), fix (agy2), rev-4r (agy3) tienen '○' en gris
        assert!(screen.contains("○ worker"));
        assert!(screen.contains("○ fix"));
        assert!(screen.contains("agy2"));
        assert!(screen.contains("95%"));

        assert!(screen.contains("○ rev-4r"));
        assert!(screen.contains("agy3"));
        assert!(screen.contains("100%"));

        // 5. Clic para colapsar Cuota de Modelos (card index 5)
        state.card_bounds = render_out.bounds;
        let pools_bound = state
            .card_bounds
            .iter()
            .find(|b| b.id == "pools")
            .expect("bound de la card pools debe existir");

        let click = state.handle_mouse_click(pools_bound.start_line, 0);
        assert_eq!(click, Some(ClickAction::ToggledCard(5)));
        assert!(state.collapsed_cards[5]);
    }

    #[test]
    fn test_orchestrators_subagent_disambiguation_and_profile_isolation() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::models::{
            AccountQuota, AgentSession, OrchestratorProfile, OrchestratorStatus, SubAgentProfile,
        };
        use hudr2::state::AppState;

        let mut state = AppState::new();
        state.collapsed_cards[4] = false; // Orquestadores
        state.collapsed_cards[5] = false; // Cuota de Modelos

        // Configuración de 3 orquestadores:
        // 1. fleet-tavo: worker (agy2) y fix (agy2) comparten modelo y prefijo
        let orch_tavo = OrchestratorProfile {
            name: "fleet-tavo".to_string(),
            model: "tavoclaude/claude-opus-5-5".to_string(),
            model_short: "claude-opus-5-5".to_string(),
            prefix: Some("tavoclaude".to_string()),
            status: OrchestratorStatus::Working,
            subagents: vec![
                SubAgentProfile {
                    role: "worker".to_string(),
                    model: "agy2/gemini-3.8-flash-high".to_string(),
                    model_short: "gemini-3.8-flash-high".to_string(),
                    prefix: Some("agy2".to_string()),
                    thinking: Some("high".to_string()),
                },
                SubAgentProfile {
                    role: "fix".to_string(),
                    model: "agy2/gemini-3.8-flash-high".to_string(),
                    model_short: "gemini-3.8-flash-high".to_string(),
                    prefix: Some("agy2".to_string()),
                    thinking: Some("high".to_string()),
                },
            ],
            ..Default::default()
        };

        // 2. fleet-codex: worker asignado también a agy2
        let orch_codex = OrchestratorProfile {
            name: "fleet-codex".to_string(),
            model: "tavocodex/gpt-5.6-sol".to_string(),
            model_short: "gpt-5.6-sol".to_string(),
            prefix: Some("tavocodex".to_string()),
            status: OrchestratorStatus::Offline,
            subagents: vec![SubAgentProfile {
                role: "worker".to_string(),
                model: "agy2/gemini-3.8-flash-high".to_string(),
                model_short: "gemini-3.8-flash-high".to_string(),
                prefix: Some("agy2".to_string()),
                thinking: Some("high".to_string()),
            }],
            ..Default::default()
        };

        // 3. fleet-gianni: agy2 asignado a 'verify' en vez de 'worker', y worker a claude
        let orch_gianni = OrchestratorProfile {
            name: "fleet-gianni".to_string(),
            model: "gianniclaude/claude-opus-5-5".to_string(),
            model_short: "claude-opus-5-5".to_string(),
            prefix: Some("gianniclaude".to_string()),
            status: OrchestratorStatus::Offline,
            subagents: vec![
                SubAgentProfile {
                    role: "verify".to_string(),
                    model: "agy2/gemini-3.8-flash-high".to_string(),
                    model_short: "gemini-3.8-flash-high".to_string(),
                    prefix: Some("agy2".to_string()),
                    thinking: Some("medium".to_string()),
                },
                SubAgentProfile {
                    role: "worker".to_string(),
                    model: "gianniclaude/claude-sonnet-5-5".to_string(),
                    model_short: "claude-sonnet-5-5".to_string(),
                    prefix: Some("gianniclaude".to_string()),
                    thinking: Some("high".to_string()),
                },
            ],
            ..Default::default()
        };

        state.snapshot.orchestrators = vec![orch_tavo, orch_codex, orch_gianni];

        let acc_agy2 = AccountQuota {
            prefix: "agy2".to_string(),
            auth_file: "auth_agy2.json".to_string(),
            rem_5h: 88,
            reset_5h: "3h 40m".to_string(),
            rem_wk: 92,
            reset_wk: "4d 10h".to_string(),
            ..Default::default()
        };
        let acc_claude = AccountQuota {
            prefix: "tavoclaude".to_string(),
            auth_file: "config.yaml#tavoclaude".to_string(),
            rem_5h: 100,
            reset_5h: "api-key".to_string(),
            rem_wk: 100,
            reset_wk: "api-key".to_string(),
            ..Default::default()
        };
        state.snapshot.accounts = vec![acc_agy2, acc_claude];

        // CASO 1: En fleet-tavo está trabajando 'worker' (agy2)
        state.snapshot.agent_session = AgentSession {
            agent: "worker".to_string(),
            agent_status: "working".to_string(),
            local_prefix: Some("agy2".to_string()),
            model_short: "gemini-3.8-flash-high".to_string(),
            model: "agy2/gemini-3.8-flash-high".to_string(),
            ..Default::default()
        };

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));

        // Cuota de Modelos está contextualizada en fleet-tavo (Working)
        assert!(screen.contains("Cuota de Modelos · fleet-tavo *"));

        // 'worker' debe estar activo con '>'
        assert!(screen.contains("> worker"));

        // 'fix' (a pesar de compartir agy2 y el mismo modelo) DEBE permanecer inactivo con '○'
        assert!(screen.contains("○ fix"));
        assert!(!screen.contains("> fix"));

        // CASO 2: En fleet-gianni está activo 'verify' (agy2)
        state.snapshot.orchestrators[0].status = OrchestratorStatus::Offline;
        state.snapshot.orchestrators[2].status = OrchestratorStatus::Working;
        state.snapshot.agent_session.agent = "verify".to_string();

        let render_gianni = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen_gianni = ui::strip_ansi(&render_gianni.lines.join("\n"));

        // Cuota de Modelos ahora está en fleet-gianni
        assert!(screen_gianni.contains("Cuota de Modelos · fleet-gianni *"));

        // 'verify' debe tener '>'
        assert!(screen_gianni.contains("> verify"));

        // 'worker' de fleet-gianni (que usa claude) no debe activarse
        assert!(screen_gianni.contains("○ worker"));
        assert!(!screen_gianni.contains("> worker"));
    }

    #[test]
    fn test_orchestrators_profile_subagent_deduplication_and_long_role_truncation() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::models::{AccountQuota, AgentSession, OrchestratorStatus};
        use hudr2::orchestrators::discover_orchestrators;
        use hudr2::state::AppState;

        let tmp = tempfile::tempdir().unwrap();
        let gentle_dir = tmp.path().join(".gentle-ai");
        std::fs::create_dir_all(&gentle_dir).unwrap();

        // profiles.json con subagente duplicado deliberadamente en "agents" y en raíz del perfil
        let profiles_json = serde_json::json!({
            "default": "fleet-dup",
            "profiles": {
                "fleet-dup": {
                    "orchestrator": {
                        "model": "tavoclaude/claude-opus-5-5"
                    },
                    "agents": {
                        "worker": {
                            "model": "agy2/gemini-3.8-flash-high"
                        },
                        "very-long-specialized-coordinator-agent": {
                            "model": "agy3/gemini-3.8-flash-medium"
                        }
                    },
                    "worker": {
                        "model": "agy2/gemini-3.8-flash-high"
                    }
                }
            }
        });

        let profiles_file = gentle_dir.join("profiles.json");
        std::fs::write(
            &profiles_file,
            serde_json::to_string(&profiles_json).unwrap(),
        )
        .unwrap();

        unsafe {
            std::env::set_var("GENTLE_AI_PROFILES_PATH", profiles_file.to_str().unwrap());
        }

        let acc_agy2 = AccountQuota {
            prefix: "agy2".to_string(),
            auth_file: "auth_agy2.json".to_string(),
            rem_5h: 90,
            reset_5h: "4h 00m".to_string(),
            rem_wk: 95,
            reset_wk: "5d 00h".to_string(),
            ..Default::default()
        };
        let accounts = vec![acc_agy2];
        let session = AgentSession::default();
        let panes = vec![];

        let discovered =
            discover_orchestrators(tmp.path().to_str().unwrap(), &accounts, &session, &panes);
        let dup_orch = discovered
            .iter()
            .find(|o| o.name == "fleet-dup")
            .expect("fleet-dup debe existir");

        // Verificación 1: Deduplicación defensiva
        // 'worker' solo debe existir 1 vez en subagents a pesar de estar repetido en el JSON
        let worker_count = dup_orch
            .subagents
            .iter()
            .filter(|s| s.role == "worker")
            .count();
        assert_eq!(
            worker_count, 1,
            "worker no debe aparecer duplicado en la flota de subagentes"
        );

        // Verificación 2: Truncado inteligente y seguro de roles largos en terminal angosta (38 columnas)
        let mut state = AppState::new();
        state.collapsed_cards[5] = false;
        let mut active_orch = dup_orch.clone();
        active_orch.status = OrchestratorStatus::Working;
        state.snapshot.orchestrators = vec![active_orch];
        state.snapshot.accounts = accounts;

        let render_narrow = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 38);
        let pools_bound = render_narrow
            .bounds
            .iter()
            .find(|b| b.id == "pools")
            .expect("bound de la card pools debe existir");

        for line in &render_narrow.lines[pools_bound.start_line..=pools_bound.end_line] {
            let stripped = ui::strip_ansi(line);
            let w = ui::string_width(&stripped);
            assert_eq!(
                w, 38,
                "Cada fila de la card pools debe medir exactamente 38 caracteres sin desbordar"
            );
        }

        unsafe {
            std::env::remove_var("GENTLE_AI_PROFILES_PATH");
        }
    }

    #[test]
    fn test_gentle_orchestrator_agents_use_short_names_and_merge_with_fleet_profiles() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::models::{AccountQuota, AgentSession};
        use hudr2::orchestrators::discover_orchestrators;

        let tmp = tempfile::tempdir().unwrap();

        let profiles_json = serde_json::json!({
            "profiles": {
                "fleet-zengem": {
                    "orchestrator": { "model": "cliproxy/agy/gemini-3.8-flash" }
                }
            }
        });
        let profiles_file = tmp.path().join("profiles.json");
        std::fs::write(
            &profiles_file,
            serde_json::to_string(&profiles_json).unwrap(),
        )
        .unwrap();

        let opencode_json = serde_json::json!({
            "agent": {
                "gentle-orchestrator-zengem": {
                    "mode": "primary",
                    "model": "cliproxy/agy/gemini-3.8-flash"
                },
                "gentle-orchestrator-agy2": {
                    "mode": "primary",
                    "model": "cliproxy/agy2/gemini-3.8-flash"
                },
                "gentle-orchestrator-codex": {
                    "mode": "primary",
                    "model": "cliproxy/tavocodex/gpt-5.6-sol"
                }
            }
        });
        let opencode_file = tmp.path().join("opencode.json");
        std::fs::write(
            &opencode_file,
            serde_json::to_string(&opencode_json).unwrap(),
        )
        .unwrap();

        unsafe {
            std::env::set_var("OPENCODE_CONFIG_PATH", opencode_file.to_str().unwrap());
            std::env::set_var("GENTLE_AI_PROFILES_PATH", profiles_file.to_str().unwrap());
        }

        let accounts = vec![AccountQuota {
            prefix: "agy2".to_string(),
            auth_file: "agy2.json".to_string(),
            ..Default::default()
        }];
        let discovered = discover_orchestrators(
            tmp.path().to_str().unwrap(),
            &accounts,
            &AgentSession::default(),
            &[],
        );

        unsafe {
            std::env::remove_var("OPENCODE_CONFIG_PATH");
            std::env::remove_var("GENTLE_AI_PROFILES_PATH");
        }

        let mut names: Vec<&str> = discovered.iter().map(|o| o.display_name()).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["agy2", "tavocodex", "zengem"],
            "gentle-orchestrator-<x> must display as <x> and merge with fleet-<x>"
        );
        let agy2 = discovered
            .iter()
            .find(|o| o.display_name() == "agy2")
            .unwrap();
        assert_eq!(agy2.name, "gentle-orchestrator-agy2");
        assert_eq!(agy2.prefix.as_deref(), Some("agy2"));
    }

    #[test]
    fn test_configured_sdd_orchestrators_are_discovered_from_opencode() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::models::{AccountQuota, AgentSession};
        use hudr2::orchestrators::discover_orchestrators;
        use hudr2::state::AppState;

        let tmp = tempfile::tempdir().unwrap();

        // Configurar opencode.json con SDD orchestrator y subagentes en "agents" (plural)
        let opencode_json = serde_json::json!({
            "agents": {
                "sdd-orchestrator-agy3": {
                    "mode": "primary",
                    "model": "cliproxy/agy3/gemini-3.8-flash-thinking",
                    "thinking": "high"
                },
                "sdd-explore-agy3": {
                    "mode": "subagent",
                    "model": "cliproxy/agy3/gemini-3.8-flash"
                },
                "sdd-apply-agy3": {
                    "mode": "subagent",
                    "model": "cliproxy/agy3/gemini-3.8-flash"
                },
                "sdd-verify-agy3": {
                    "mode": "subagent",
                    "model": "cliproxy/agy3/gemini-3.8-flash"
                }
            }
        });
        let opencode_file = tmp.path().join("opencode.json");
        std::fs::write(
            &opencode_file,
            serde_json::to_string_pretty(&opencode_json).unwrap(),
        )
        .unwrap();

        let empty_profiles = tmp.path().join("empty_profiles.json");
        std::fs::write(&empty_profiles, "{}").unwrap();

        unsafe {
            std::env::set_var("OPENCODE_CONFIG_PATH", opencode_file.to_str().unwrap());
            std::env::set_var("GENTLE_AI_PROFILES_PATH", empty_profiles.to_str().unwrap());
        }

        let agy3_acc = AccountQuota {
            prefix: "agy3".to_string(),
            auth_file: "agy3.json".to_string(),
            rem_5h: 92,
            reset_5h: "4h 10m".to_string(),
            rem_wk: 88,
            reset_wk: "3d 05h".to_string(),
            ..Default::default()
        };
        let accounts = vec![agy3_acc];

        // Una sesión SDD activa debe enlazar con la configuración real de OpenCode.
        let session = AgentSession {
            agent: "Sdd-Orchestrator-Agy3".to_string(),
            agent_status: "working".to_string(),
            local_prefix: Some("agy3".to_string()),
            model_short: "gemini-3.8-flash".to_string(),
            model: "cliproxy/agy3/gemini-3.8-flash".to_string(),
            ..Default::default()
        };

        let panes = vec![];
        let discovered =
            discover_orchestrators(tmp.path().to_str().unwrap(), &accounts, &session, &panes);

        assert_eq!(
            discovered.len(),
            1,
            "solo el orquestador SDD primario debe entrar al inventario"
        );
        assert_eq!(discovered[0].name, "sdd-orchestrator-agy3");
        assert_eq!(
            discovered[0].status,
            hudr2::models::OrchestratorStatus::Working
        );
        assert_eq!(discovered[0].subagents.len(), 3);

        let mut state = AppState::new();
        state.collapsed_cards[4] = false; // Orquestadores
        state.collapsed_cards[5] = false; // Cuota de Modelos
        state.snapshot.orchestrators = discovered;
        state.snapshot.accounts = accounts;
        state.snapshot.agent_session = session;

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));

        assert!(screen.contains("sdd-agy3"));

        unsafe {
            std::env::remove_var("OPENCODE_CONFIG_PATH");
            std::env::remove_var("GENTLE_AI_PROFILES_PATH");
        }
    }

    #[test]
    fn test_cuota_de_modelos_multiple_simultaneous_active_subagents() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::models::{
            AccountQuota, AgentSession, HerdrPane, OrchestratorProfile, OrchestratorStatus,
            SubAgentProfile,
        };
        use hudr2::state::AppState;

        let mut state = AppState::new();
        state.collapsed_cards[5] = false;

        let orch = OrchestratorProfile {
            name: "fleet-codex".to_string(),
            display_name: "tavocodex".to_string(),
            model: "tavocodex/gpt-5.6-sol".to_string(),
            model_short: "5.6-sol".to_string(),
            prefix: Some("tavocodex".to_string()),
            status: OrchestratorStatus::Working,
            subagents: vec![
                SubAgentProfile {
                    role: "gentle-ai-worker".to_string(),
                    model: "tavocodex/gpt-5.6-terra".to_string(),
                    model_short: "5.6-terra".to_string(),
                    prefix: Some("tavocodex".to_string()),
                    thinking: Some("high".to_string()),
                },
                SubAgentProfile {
                    role: "gentle-ai-explore".to_string(),
                    model: "tavocodex/gpt-5.6-luna".to_string(),
                    model_short: "5.6-luna".to_string(),
                    prefix: Some("tavocodex".to_string()),
                    thinking: Some("medium".to_string()),
                },
                SubAgentProfile {
                    role: "review-risk".to_string(),
                    model: "gianniclaude/claude-sonnet-5-5".to_string(),
                    model_short: "claude-sonnet-5-5".to_string(),
                    prefix: Some("gianniclaude".to_string()),
                    thinking: Some("high".to_string()),
                },
            ],
            ..Default::default()
        };

        state.snapshot.orchestrators = vec![orch];
        let acc = AccountQuota {
            prefix: "tavocodex".to_string(),
            auth_file: "codex.json".to_string(),
            rem_5h: 100,
            reset_5h: "plus".to_string(),
            rem_wk: 100,
            reset_wk: "plus".to_string(),
            ..Default::default()
        };
        state.snapshot.accounts = vec![acc];

        // Sesión activa ejecutando worker en working
        state.snapshot.agent_session = AgentSession {
            agent: "gentle-ai-worker".to_string(),
            agent_status: "working".to_string(),
            local_prefix: Some("tavocodex".to_string()),
            model_short: "5.6-terra".to_string(),
            model: "tavocodex/gpt-5.6-terra".to_string(),
            ..Default::default()
        };

        // Y simultáneamente un pane subordinado en Herdr ejecutando explore en working
        let mut pane_tokens = std::collections::HashMap::new();
        pane_tokens.insert("agent".to_string(), "gentle-ai-explore".to_string());
        pane_tokens.insert("agent_status".to_string(), "working".to_string());
        pane_tokens.insert("model".to_string(), "5.6-luna".to_string());

        let explore_pane = HerdrPane {
            pane_id: "pane_explore_1".to_string(),
            agent: Some("gentle-ai-explore".to_string()),
            agent_status: Some("working".to_string()),
            tokens: pane_tokens,
            ..Default::default()
        };
        state.snapshot.panes = vec![explore_pane];

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));

        // AMBOS subagentes activos deben mostrar '>' simultáneamente
        assert!(
            screen.contains("> worker"),
            "Worker debe estar activo con '>'"
        );
        assert!(
            screen.contains("> explore"),
            "Explore debe estar activo simultáneamente con '>'"
        );
        // El subagente inactivo debe permanecer en '○'
        assert!(
            screen.contains("○ rev-4r"),
            "rev-4r debe permanecer inactivo en '○'"
        );
    }

    #[test]
    fn test_cuota_de_modelos_selects_orchestrator_matching_active_session_prefix() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use hudr2::models::{AccountQuota, AgentSession, OrchestratorProfile, OrchestratorStatus};
        use hudr2::state::AppState;

        let mut state = AppState::new();
        state.collapsed_cards[5] = false; // Expandir Cuota de Modelos

        // Dos orquestadores SDD en estado Idle: sdd-agy (primero) y sdd-agy2 (segundo)
        let orch1 = OrchestratorProfile {
            name: "sdd-orchestrator-agy".to_string(),
            display_name: "sdd-agy".to_string(),
            model: "cliproxy/agy/gemini-3.8-flash-high".to_string(),
            model_short: "gemini-3.8-flash".to_string(),
            prefix: Some("agy".to_string()),
            status: OrchestratorStatus::Idle,
            ..Default::default()
        };
        let orch2 = OrchestratorProfile {
            name: "sdd-orchestrator-agy2".to_string(),
            display_name: "sdd-agy2".to_string(),
            model: "cliproxy/agy2/gemini-3.8-flash-high".to_string(),
            model_short: "gemini-3.8-flash".to_string(),
            prefix: Some("agy2".to_string()),
            status: OrchestratorStatus::Idle,
            ..Default::default()
        };

        state.snapshot.orchestrators = vec![orch1, orch2];
        state.snapshot.accounts = vec![
            AccountQuota {
                prefix: "agy".to_string(),
                auth_file: "agy.json".to_string(),
                rem_5h: 90,
                rem_wk: 80,
                ..Default::default()
            },
            AccountQuota {
                prefix: "agy2".to_string(),
                auth_file: "agy2.json".to_string(),
                rem_5h: 95,
                rem_wk: 85,
                ..Default::default()
            },
        ];

        // Sesión activa del espacio actual configurada con agy2 (ej. espacio UNED)
        state.snapshot.agent_session = AgentSession {
            agent: "sdd-orchestrator-agy2".to_string(),
            agent_status: "idle".to_string(),
            local_prefix: Some("agy2".to_string()),
            model_short: "gemini-3.8-flash".to_string(),
            model: "cliproxy/agy2/gemini-3.8-flash".to_string(),
            ..Default::default()
        };

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 50);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));

        // Card Cuota de Modelos DEBE titularse sdd-agy2 por afinidad de sesión, NO sdd-agy
        assert!(
            screen.contains("Cuota de Modelos · sdd-agy2 *"),
            "Debe seleccionar sdd-agy2 para Cuota de Modelos según el prefijo agy2 de la sesión activa"
        );
        assert!(
            !screen.contains("Cuota de Modelos · sdd-agy *"),
            "No debe seleccionar erróneamente sdd-agy"
        );
    }

    #[test]
    fn test_saldos_api_card_and_dynamic_quota_projection_with_tasks() {
        use hudr2::models::{
            AccountQuota, AgentSession, OrchestratorProfile, OrchestratorStatus, QuotaPool,
        };

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("hudr2.json");

        unsafe {
            std::env::set_var("HUDR2_CONFIG_PATH", &cfg_path);
        }

        // 1. Guardar saldo prepago de $20.00 para tavoclaude
        let _ = hudr2::state::save_account_budget(
            &cfg_path,
            "tavoclaude",
            Some(20.0),
            0.0,
            Some("sess-01".to_string()),
        );

        let mut state = AppState::with_config(Some(cfg_path), None);
        state.collapsed_cards[4] = false; // Orquestadores
        state.collapsed_cards[5] = false; // Cuota de Modelos
        state.collapsed_cards[6] = false; // Saldos API

        let acc_tavo = AccountQuota {
            prefix: "tavoclaude".to_string(),
            auth_file: "config.yaml#tavoclaude".to_string(),
            reset_5h: "api-key".to_string(),
            reset_wk: "api-key".to_string(),
            rem_5h: 100,
            rem_wk: 100,
            pools: vec![
                QuotaPool {
                    label: "Claude API".to_string(),
                    percent: 100,
                    reset: "api-key".to_string(),
                    pace: "$0.000".to_string(),
                },
                QuotaPool {
                    label: "Claude Saldo".to_string(),
                    percent: 100,
                    reset: "api-key".to_string(),
                    pace: "$20.00".to_string(),
                },
            ],
            ..Default::default()
        };

        let acc_codex = AccountQuota {
            prefix: "tavocodex".to_string(),
            auth_file: "tavocodex.json".to_string(),
            reset_5h: "plus".to_string(),
            reset_wk: "plus".to_string(),
            rem_5h: 100,
            rem_wk: 100,
            pools: vec![QuotaPool {
                label: "Codex Plus".to_string(),
                percent: 100,
                reset: "plus".to_string(),
                pace: "Suscripción".to_string(),
            }],
            ..Default::default()
        };

        state.snapshot.accounts = vec![acc_tavo, acc_codex];

        let orch_tavo = OrchestratorProfile {
            name: "fleet-tavo".to_string(),
            display_name: "tavoclaude".to_string(),
            model: "tavoclaude/claude-opus-5-5".to_string(),
            model_short: "claude-opus-5-5".to_string(),
            prefix: Some("tavoclaude".to_string()),
            status: OrchestratorStatus::Working,
            subagents: vec![hudr2::models::SubAgentProfile {
                role: "worker".to_string(),
                model: "claude-opus-5-5".to_string(),
                model_short: "claude-opus-5-5".to_string(),
                prefix: Some("tavoclaude".to_string()),
                thinking: None,
            }],
            ..Default::default()
        };
        state.snapshot.orchestrators = vec![orch_tavo];

        // Sesión activa con 10 tareas completadas y $2.50 acumulados (promedio $0.25/req)
        state.snapshot.agent_session = AgentSession {
            session_id: Some("sess-01".to_string()),
            agent: "fleet-tavo".to_string(),
            agent_status: "working".to_string(),
            local_prefix: Some("tavoclaude".to_string()),
            model: "tavoclaude/claude-opus-5-5".to_string(),
            model_short: "claude-opus-5-5".to_string(),
            cost: 2.50,
            last_turn_cost: 0.25,
            turn_count: 10,
            ..Default::default()
        };

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 54);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));
        // Verificar Card Saldos API
        assert!(screen.contains("Saldos API · 1 perfil"));
        assert!(screen.contains("tavoclaude"));
        assert!(screen.contains("87%")); // (17.50 / 20.0) * 100 = 87.5% -> 87%
        assert!(screen.contains("$17.50 disp"));
        assert!(screen.contains("[editar]"));

        // Proyección dinámica de tareas restantes: $17.50 / $0.25 = ~70 reqs
        assert!(screen.contains("sesión: $2.500"));
        assert!(screen.contains("$0.250/req"));
        assert!(screen.contains("70 reqs disp"));

        // Consola oficial de facturación con link
        assert!(screen.contains("Anthropic Console"));
        assert!(screen.contains("[visitar ->]"));

        // Bounds interactivos registrados
        assert!(
            render_out
                .bounds
                .iter()
                .any(|b| b.action_type == Some("edit_balance")
                    && b.action_path.as_deref() == Some("tavoclaude"))
        );
        assert!(
            render_out
                .bounds
                .iter()
                .any(|b| b.action_type == Some("open_billing_url")
                    && b.action_path.as_deref() == Some("https://platform.claude.com/spending"))
        );

        // Verificar sincronización en Cuota de Modelos
        assert!(screen.contains("Cuota de Modelos · tavoclaude"));
        assert!(screen.contains("$17.50 disp"));

        // Ahora simular agotamiento de saldo ($21.00 de gasto superando los $20.00 de presupuesto)
        state.snapshot.agent_session.cost = 21.00;
        state.snapshot.agent_session.turn_count = 84;
        let render_exhausted = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 54);
        let screen_exhausted = ui::strip_ansi(&render_exhausted.lines.join("\n"));

        assert!(screen_exhausted.contains("[SALDO AGOTADO]"));

        unsafe {
            std::env::remove_var("HUDR2_CONFIG_PATH");
        }
    }

    #[test]
    fn test_saldos_api_does_not_project_gemini_cost_onto_unused_api_account() {
        use hudr2::models::{AccountQuota, AgentSession, OrchestratorProfile, OrchestratorStatus};

        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("hudr2.json");
        unsafe {
            std::env::set_var("HUDR2_CONFIG_PATH", &cfg_path);
        }
        let _ = hudr2::state::save_account_budget(
            &cfg_path,
            "tavoclaude",
            Some(35.55),
            0.0,
            Some("gemini-session".to_string()),
        );

        let mut state = AppState::with_config(Some(cfg_path), None);
        state.collapsed_cards[5] = false;
        state.collapsed_cards[6] = false;
        state.snapshot.accounts = vec![AccountQuota {
            prefix: "tavoclaude".to_string(),
            auth_file: "config.yaml#tavoclaude".to_string(),
            reset_5h: "api-key".to_string(),
            reset_wk: "api-key".to_string(),
            rem_5h: 100,
            rem_wk: 100,
            ..Default::default()
        }];
        state.snapshot.orchestrators = vec![OrchestratorProfile {
            name: "sdd-orchestrator-agy2".to_string(),
            display_name: "sdd-agy2".to_string(),
            model: "cliproxy/agy2/gemini-3.8-flash-high".to_string(),
            model_short: "gemini-3.8-flash".to_string(),
            prefix: Some("agy2".to_string()),
            status: OrchestratorStatus::Working,
            subagents: vec![hudr2::models::SubAgentProfile {
                role: "explore".to_string(),
                model: "tavoclaude/claude-opus-5-5".to_string(),
                model_short: "claude-opus-5-5".to_string(),
                prefix: Some("tavoclaude".to_string()),
                thinking: None,
            }],
            ..Default::default()
        }];
        state.snapshot.agent_session = AgentSession {
            session_id: Some("gemini-session".to_string()),
            agent: "sdd-orchestrator-agy2".to_string(),
            agent_status: "working".to_string(),
            local_prefix: Some("agy2".to_string()),
            model: "cliproxy/agy2/gemini-3.8-flash-high".to_string(),
            model_short: "gemini-3.8-flash".to_string(),
            cost: 0.068,
            turn_count: 34,
            ..Default::default()
        };

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 54);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));
        assert!(screen.contains("tavoclaude"));
        assert!(screen.contains("$35.55 disp"));
        assert!(!screen.contains("sesión: $0.068"));

        unsafe {
            std::env::remove_var("HUDR2_CONFIG_PATH");
        }
    }

    #[test]
    fn test_cuota_de_modelos_rejects_same_role_with_wrong_account() {
        use hudr2::models::{AccountQuota, AgentSession, OrchestratorProfile, OrchestratorStatus};

        let mut state = AppState::new();
        state.collapsed_cards[5] = false;
        state.snapshot.accounts = vec![
            AccountQuota {
                prefix: "agy2".to_string(),
                rem_5h: 99,
                rem_wk: 91,
                ..Default::default()
            },
            AccountQuota {
                prefix: "tavoclaude".to_string(),
                auth_file: "config.yaml#tavoclaude".to_string(),
                reset_5h: "api-key".to_string(),
                reset_wk: "api-key".to_string(),
                ..Default::default()
            },
        ];
        state.snapshot.orchestrators = vec![OrchestratorProfile {
            name: "sdd-orchestrator-agy2".to_string(),
            display_name: "sdd-agy2".to_string(),
            model: "cliproxy/agy2/gemini-3.8-flash-high".to_string(),
            model_short: "gemini-3.8-flash".to_string(),
            prefix: Some("agy2".to_string()),
            status: OrchestratorStatus::Working,
            subagents: vec![hudr2::models::SubAgentProfile {
                role: "worker".to_string(),
                model: "tavoclaude/claude-opus-5-5".to_string(),
                model_short: "claude-opus-5-5".to_string(),
                prefix: Some("tavoclaude".to_string()),
                thinking: None,
            }],
            ..Default::default()
        }];
        state.snapshot.agent_session = AgentSession {
            agent: "worker".to_string(),
            agent_status: "working".to_string(),
            local_prefix: Some("agy2".to_string()),
            model: "cliproxy/agy2/gemini-3.8-flash-high".to_string(),
            model_short: "gemini-3.8-flash".to_string(),
            ..Default::default()
        };

        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 54);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));
        assert!(
            screen.contains("○ worker"),
            "wrong-account worker must stay inactive"
        );
        assert!(
            !screen.contains("> worker"),
            "role alone must not mark a subagent active"
        );
    }

    #[test]
    fn test_opencode_api_child_sessions_drive_subagent_activity_and_api_cost() {
        use hudr2::models::{AccountQuota, AgentSession};

        let accounts = vec![
            AccountQuota {
                prefix: "agy2".to_string(),
                ..Default::default()
            },
            AccountQuota {
                prefix: "tavoclaude".to_string(),
                auth_file: "config.yaml#tavoclaude".to_string(),
                reset_5h: "api-key".to_string(),
                reset_wk: "api-key".to_string(),
                ..Default::default()
            },
        ];
        let payload = serde_json::json!({
            "data": [
                {
                    "id": "ses_root",
                    "agent": "sdd-orchestrator-agy2",
                    "model": {"providerID": "cliproxy", "id": "agy2/gemini-3.8-flash-high"},
                    "cost": 0.0,
                    "tokens": {"input": 1000, "output": 100, "cache": {"read": 0, "write": 0}},
                    "time": {"created": 10, "updated": 50, "busy": 11},
                    "location": {"directory": "D:/workspace"}
                },
                {
                    "id": "ses_worker",
                    "parentID": "ses_root",
                    "agent": "worker",
                    "model": {"providerID": "cliproxy", "id": "tavoclaude/claude-opus-5-5"},
                    "cost": 0.42,
                    "tokens": {"input": 500, "output": 50, "cache": {"read": 25, "write": 0}},
                    "time": {"created": 20, "updated": 40, "busy": 21},
                    "location": {"directory": "D:/workspace"}
                },
                {
                    "id": "ses_explore",
                    "parentID": "ses_root",
                    "agent": "explore",
                    "model": {"providerID": "cliproxy", "id": "agy2/gemini-3.8-flash-high"},
                    "cost": 0.0,
                    "tokens": {"input": 200, "output": 20, "cache": {"read": 0, "write": 0}},
                    "time": {"created": 25, "updated": 35, "busy": 26, "idle": 36},
                    "location": {"directory": "D:/workspace"}
                }
            ]
        });
        let mut session = AgentSession::default();

        hudr2::opencode::apply_api_session_telemetry(
            &mut session,
            "D:/workspace",
            &accounts,
            &payload,
        );

        assert_eq!(session.subagent_invocations.len(), 2);
        let worker = session
            .subagent_invocations
            .iter()
            .find(|item| item.role == "worker")
            .unwrap();
        assert_eq!(worker.status, "working");
        assert_eq!(worker.local_prefix.as_deref(), Some("tavoclaude"));
        assert!((worker.cost - 0.42).abs() < f64::EPSILON);
        assert_eq!(worker.started_at, Some(20));
        assert_eq!(worker.finished_at, None);

        let explore = session
            .subagent_invocations
            .iter()
            .find(|item| item.role == "explore")
            .unwrap();
        assert_eq!(explore.status, "idle");
        assert_eq!(explore.finished_at, Some(36));

        let (cost, tasks) =
            hudr2::quotas::cliproxy::api_account_attributed_usage(&accounts[1], &session, &[]);
        assert!((cost - 0.42).abs() < f64::EPSILON);
        assert_eq!(tasks, 1);

        let orch = hudr2::models::OrchestratorProfile {
            name: "sdd-orchestrator-agy2".to_string(),
            display_name: "sdd-agy2".to_string(),
            model: "cliproxy/agy2/gemini-3.8-flash-high".to_string(),
            model_short: "gemini-3.8-flash".to_string(),
            prefix: Some("agy2".to_string()),
            status: hudr2::models::OrchestratorStatus::Working,
            subagents: vec![
                hudr2::models::SubAgentProfile {
                    role: "worker".to_string(),
                    model: "tavoclaude/claude-opus-5-5".to_string(),
                    model_short: "claude-opus-5-5".to_string(),
                    prefix: Some("tavoclaude".to_string()),
                    thinking: None,
                },
                hudr2::models::SubAgentProfile {
                    role: "explore".to_string(),
                    model: "agy2/gemini-3.8-flash-high".to_string(),
                    model_short: "gemini-3.8-flash".to_string(),
                    prefix: Some("agy2".to_string()),
                    thinking: None,
                },
            ],
            ..Default::default()
        };
        let mut state = AppState::new();
        state.collapsed_cards[5] = false;
        state.snapshot.accounts = accounts;
        state.snapshot.orchestrators = vec![orch];
        state.snapshot.agent_session = session;
        let render_out = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 80);
        let screen = ui::strip_ansi(&render_out.lines.join("\n"));
        assert!(screen.contains("> worker"));
        assert!(screen.contains("● explore"));
        assert!(screen.contains("ini 00:00:20 · fin activo"));
        assert!(screen.contains("$0.420 · 550 tok"));
        assert!(screen.contains("ini 00:00:25 · fin 00:00:36"));
        assert!(screen.contains("220 tok"));
    }

    #[test]
    fn test_zenwarp_usage_is_actor_driven_dynamic_and_route_aware() {
        use hudr2::models::AgentSession;
        let payload = serde_json::json!({ "data": [
            { "id": "ses_root", "agent": "gentle-orchestrator", "model": {"providerID": "zenwarp", "id": "new-model-free"}, "time": {"created": 10, "updated": 50, "busy": 11}, "location": {"directory": "D:/workspace"} },
            { "id": "ses_worker_a", "parentID": "ses_root", "agent": "worker-a", "model": {"providerID": "cliproxy", "id": "zen/glm-5.3-flash-free"}, "time": {"created": 20, "updated": 40, "busy": 21}, "location": {"directory": "D:/workspace"} },
            { "id": "ses_worker_b", "parentID": "ses_root", "agent": "worker-b", "model": {"providerID": "cliproxy", "id": "zen/glm-5.3-flash-free"}, "time": {"created": 25, "updated": 45, "busy": 26, "idle": 46}, "location": {"directory": "D:/workspace"} },
            { "id": "ses_unrelated", "parentID": "ses_root", "agent": "explore", "model": {"providerID": "cliproxy", "id": "zenith/not-zenwarp"}, "time": {"created": 30, "updated": 35, "busy": 31}, "location": {"directory": "D:/workspace"} },
            { "id": "ses_similar_provider", "parentID": "ses_root", "agent": "fix", "model": {"providerID": "zenwarp-proxy", "id": "almost-zen"}, "time": {"created": 31, "updated": 36, "busy": 32}, "location": {"directory": "D:/workspace"} },
            { "id": "ses_direct_qualified", "parentID": "ses_root", "agent": "verify", "model": {"id": "zenwarp/future-model-free"}, "time": {"created": 32, "updated": 34, "busy": 33}, "location": {"directory": "D:/workspace"} }
        ]});
        let mut session = AgentSession::default();
        hudr2::opencode::apply_api_session_telemetry(&mut session, "D:/workspace", &[], &payload);
        let usages = session.zenwarp_usages();
        assert_eq!(usages.len(), 4, "only observed ZenWarp actors create rows");
        assert_eq!(
            (&usages[0].actor_id, &usages[0].model_id, &usages[0].route),
            (
                &"ses_root".to_string(),
                &"new-model-free".to_string(),
                &"direct".to_string()
            )
        );
        assert_eq!(usages[1].parent_actor_id.as_deref(), Some("ses_root"));
        assert_eq!(
            (&usages[1].model_id, &usages[1].route),
            (&"glm-5.3-flash-free".to_string(), &"cliproxy".to_string())
        );
        assert_eq!(
            (
                &usages[2].actor_id,
                &usages[2].activity_state,
                usages[2].finished_at
            ),
            (&"ses_worker_b".to_string(), &"idle".to_string(), Some(46))
        );
        assert_eq!(
            (&usages[3].model_id, &usages[3].route),
            (&"future-model-free".to_string(), &"direct".to_string())
        );
    }

    #[test]
    fn test_cuota_de_modelos_ignores_runtime_only_zenwarp_rows() {
        use hudr2::models::AgentSession;
        let mut state = AppState::new();
        state.collapsed_cards[5] = false;
        state.snapshot.agent_session = AgentSession {
            session_id: Some("ses_root".to_string()),
            provider_name: "opencode".to_string(),
            agent: "gentle-orchestrator".to_string(),
            model: "zenwarp/new-model-free".to_string(),
            model_short: "new-model-free".to_string(),
            agent_status: "working".to_string(),
            subagent_invocations: vec![hudr2::models::SubagentInvocation {
                session_id: "ses_worker".to_string(),
                role: "worker".to_string(),
                model: "cliproxy/zen/glm-5.3-flash-free".to_string(),
                model_short: "glm-5.3-flash-free".to_string(),
                status: "idle".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let rendered = ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 120);
        let pools = rendered
            .bounds
            .iter()
            .find(|bound| bound.id == "pools")
            .unwrap();
        let screen = ui::strip_ansi(&rendered.lines[pools.start_line..=pools.end_line].join("\n"));
        assert!(!screen.contains("ZenWarp usage"));
        assert!(!screen.contains("new-model-free"));
        assert!(!screen.contains("glm-5.3-flash-free"));
        state.snapshot.agent_session = AgentSession::default();
        let empty = ui::strip_ansi(
            &ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 120)
                .lines
                .join("\n"),
        );
        assert!(!empty.contains("ZenWarp usage"));
    }

    #[test]
    fn test_tabbed_sidebar_rendering_and_card_distribution() {
        use hudr2::state::ActiveTab;
        let mut state = AppState::new();
        state.collapsed_cards = [false; 13];

        // 1. Pestaña 1: Sesión -> Estado, Contexto, MCP
        let out_sesion = ui::render_screen_tabbed(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
            hudr2::theme::active_theme(),
            Some(ActiveTab::Sesion),
        );
        let text_sesion = out_sesion.lines.join("\n");
        assert!(text_sesion.contains("Estado"), "Sesión debe incluir Estado");
        assert!(
            text_sesion.contains("Contexto"),
            "Sesión debe incluir Contexto"
        );
        assert!(text_sesion.contains("MCP"), "Sesión debe incluir MCP");
        assert!(
            !text_sesion.contains("Integraciones"),
            "Sesión no debe incluir Integraciones"
        );
        assert!(
            !text_sesion.contains("Orquestadores"),
            "Sesión no debe incluir Orquestadores"
        );
        assert!(
            !text_sesion.contains("Cambios"),
            "Sesión no debe incluir Cambios (Git)"
        );
        assert!(
            !text_sesion.contains("Engram"),
            "Sesión no debe incluir Engram"
        );

        // 2. Pestaña 2: Código -> Proyecto, Cambios, Herramientas en ese orden exacto
        let out_codigo = ui::render_screen_tabbed(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
            hudr2::theme::active_theme(),
            Some(ActiveTab::Codigo),
        );
        let text_codigo = out_codigo.lines.join("\n");
        assert!(
            text_codigo.contains("Proyecto"),
            "Código debe incluir Proyecto"
        );
        assert!(
            text_codigo.contains("Cambios"),
            "Código debe incluir Cambios (Git)"
        );
        assert!(
            text_codigo.contains("Herramientas"),
            "Código debe incluir Herramientas"
        );
        assert!(
            !text_codigo.contains("Estado"),
            "Código no debe incluir Estado"
        );
        assert!(
            !text_codigo.contains("Integraciones"),
            "Código no debe incluir Integraciones"
        );
        assert!(
            !text_codigo.contains("Orquestadores"),
            "Código no debe incluir Orquestadores"
        );

        // Verificar orden relativo en Código: Proyecto < Cambios < Herramientas
        let pos_proj = text_codigo.find("Proyecto").unwrap();
        let pos_git = text_codigo.find("Cambios").unwrap();
        let pos_tools = text_codigo.find("Herramientas").unwrap();
        assert!(
            pos_proj < pos_git,
            "Proyecto debe aparecer antes de Cambios"
        );
        assert!(
            pos_git < pos_tools,
            "Cambios debe aparecer antes de Herramientas"
        );

        // 3. Pestaña 3: Modelos -> Integraciones, Orquestadores, Cuota de Modelos, Saldos API
        state.snapshot.accounts.push(hudr2::models::AccountQuota {
            prefix: "tavoclaude".to_string(),
            auth_file: "tavoclaude.json".to_string(),
            rem_5h: 100,
            reset_5h: "api-key".to_string(),
            rem_wk: 100,
            reset_wk: "api-key".to_string(),
            ..Default::default()
        });
        let out_modelos = ui::render_screen_tabbed(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
            hudr2::theme::active_theme(),
            Some(ActiveTab::Modelos),
        );
        let text_modelos = out_modelos.lines.join("\n");
        assert!(
            text_modelos.contains("Integraciones"),
            "Modelos debe incluir Integraciones"
        );
        assert!(
            text_modelos.contains("Orquestadores"),
            "Modelos debe incluir Orquestadores"
        );
        assert!(
            text_modelos.contains("Cuota de Modelos"),
            "Modelos debe incluir Cuota de Modelos"
        );
        assert!(
            text_modelos.contains("Saldos API"),
            "Modelos debe incluir Saldos API"
        );
        assert!(
            !text_modelos.contains("Estado"),
            "Modelos no debe incluir Estado"
        );
        assert!(
            !text_modelos.contains("Proyecto"),
            "Modelos no debe incluir Proyecto"
        );
        assert!(
            !text_modelos.contains("Cambios"),
            "Modelos no debe incluir Cambios"
        );

        let pos_int = text_modelos.find("Integraciones").unwrap();
        let pos_orch = text_modelos.find("Orquestadores").unwrap();
        let pos_pools = text_modelos.find("Cuota de Modelos").unwrap();
        let pos_saldos = text_modelos.find("Saldos API").unwrap();
        assert!(
            pos_int < pos_orch,
            "Integraciones debe aparecer antes de Orquestadores"
        );
        assert!(
            pos_orch < pos_pools,
            "Orquestadores debe aparecer antes de Cuota de Modelos"
        );
        assert!(
            pos_pools < pos_saldos,
            "Cuota de Modelos debe aparecer antes de Saldos API"
        );

        // 4. Pestaña 4: Stack -> Engram, Stack · Ecosistema
        let out_stack = ui::render_screen_tabbed(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
            hudr2::theme::active_theme(),
            Some(ActiveTab::Stack),
        );
        let text_stack = out_stack.lines.join("\n");
        assert!(text_stack.contains("Engram"), "Stack debe incluir Engram");
        assert!(
            text_stack.contains("Stack"),
            "Stack debe incluir Stack · Ecosistema"
        );
        assert!(
            !text_stack.contains("Estado"),
            "Stack no debe incluir Estado"
        );
        assert!(
            !text_stack.contains("Proyecto"),
            "Stack no debe incluir Proyecto"
        );

        // 5. Pestaña 5: Ajustes -> Personalizar (siempre expandida)
        let out_ajustes = ui::render_screen_tabbed(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
            hudr2::theme::active_theme(),
            Some(ActiveTab::Ajustes),
        );
        let text_ajustes = out_ajustes.lines.join("\n");
        assert!(
            text_ajustes.contains("Personalizar"),
            "Ajustes debe incluir Personalizar"
        );
        assert!(
            text_ajustes.contains("Nombre"),
            "Ajustes debe mostrar la fila Nombre"
        );
        assert!(
            text_ajustes.contains("Tema"),
            "Ajustes debe mostrar la fila Tema"
        );
        assert!(
            text_ajustes.contains("Updates"),
            "Ajustes debe mostrar la fila Updates"
        );
        assert!(
            text_ajustes.contains("Alertas sonoras"),
            "Ajustes debe mostrar la fila Alertas sonoras"
        );
    }

    #[test]
    fn test_tabbed_sidebar_mouse_click_switching() {
        use hudr2::state::ActiveTab;
        let mut state = AppState::new();
        assert_eq!(state.active_tab, ActiveTab::Sesion);

        // Renderizar pestaña Sesión en ancho 48
        let out = ui::render_screen_tabbed(
            &state.snapshot,
            &state.collapsed_cards,
            None,
            None,
            &[],
            &state.hidden_cards,
            48,
            hudr2::theme::active_theme(),
            Some(state.active_tab),
        );
        state.tab_bounds = out.tab_bounds;
        state.card_bounds = out.bounds;

        // Encontrar bound de pestaña Código
        let codigo_bound = state
            .tab_bounds
            .iter()
            .find(|b| b.tab == ActiveTab::Codigo)
            .expect("bound para Código debe existir");

        // Simular clic en la columna media de Código
        let click_col = (codigo_bound.col_start + codigo_bound.col_end) / 2;
        let switched = state.handle_tab_click(click_col, codigo_bound.line, 0);
        assert_eq!(switched, Some(ActiveTab::Codigo));
        assert_eq!(state.active_tab, ActiveTab::Codigo);
        assert!(state.needs_redraw);

        // Encontrar bound de Modelos
        let modelos_bound = state
            .tab_bounds
            .iter()
            .find(|b| b.tab == ActiveTab::Modelos)
            .expect("bound para Modelos debe existir");
        let click_modelos = (modelos_bound.col_start + modelos_bound.col_end) / 2;
        let switched_modelos = state.handle_tab_click(click_modelos, modelos_bound.line, 0);
        assert_eq!(switched_modelos, Some(ActiveTab::Modelos));
        assert_eq!(state.active_tab, ActiveTab::Modelos);

        // Encontrar bound de Ajustes
        let ajustes_bound = state
            .tab_bounds
            .iter()
            .find(|b| b.tab == ActiveTab::Ajustes)
            .expect("bound para Ajustes debe existir");
        let click_ajustes = (ajustes_bound.col_start + ajustes_bound.col_end) / 2;
        let switched_ajustes = state.handle_tab_click(click_ajustes, ajustes_bound.line, 0);
        assert_eq!(switched_ajustes, Some(ActiveTab::Ajustes));
        assert_eq!(state.active_tab, ActiveTab::Ajustes);
    }

    #[test]
    fn test_tabbed_sidebar_cycle_next_prev() {
        use hudr2::state::ActiveTab;
        let mut state = AppState::new();
        assert_eq!(state.active_tab, ActiveTab::Sesion);

        assert_eq!(state.next_tab(), ActiveTab::Codigo);
        assert_eq!(state.next_tab(), ActiveTab::Modelos);
        assert_eq!(state.next_tab(), ActiveTab::Stack);
        assert_eq!(state.next_tab(), ActiveTab::Ajustes);
        assert_eq!(state.next_tab(), ActiveTab::Sesion); // wrap around

        assert_eq!(state.prev_tab(), ActiveTab::Ajustes);
        assert_eq!(state.prev_tab(), ActiveTab::Stack);
        assert_eq!(state.prev_tab(), ActiveTab::Modelos);
        assert_eq!(state.prev_tab(), ActiveTab::Codigo);
        assert_eq!(state.prev_tab(), ActiveTab::Sesion);
    }

    #[test]
    fn test_tabbed_view_config_persistence_and_toggle() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg_path = tmp.path().join("hudr2.json");

        let mut st = AppState::with_config(Some(cfg_path.clone()), None);
        assert!(st.tabbed, "Tabbed mode debe ser true por omisión");
        assert!(st.snapshot.tabbed_view);

        // Asegurar tarjeta personalizar expandida
        if st.collapsed_cards[12] {
            st.toggle_card(12);
        }
        let render_out = ui::render_screen(&st.snapshot, &st.collapsed_cards, None, 48);
        st.card_bounds = render_out.bounds;

        let toggle_line = st
            .card_bounds
            .iter()
            .find(|b| b.id == "toggle_tabbed_view")
            .map(|b| b.start_line)
            .expect("bound para toggle_tabbed_view debe existir en Personalizar");

        // Simular clic en toggle_tabbed_view: true -> false (modo vertical)
        let action = st.handle_mouse_click(toggle_line, 0);
        assert_eq!(action, Some(ClickAction::ToggleTabbedView(false)));
        assert!(!st.tabbed);
        assert!(!st.snapshot.tabbed_view);

        // Verificar persistencia en disco
        let st2 = AppState::with_config(Some(cfg_path.clone()), None);
        assert!(
            !st2.tabbed,
            "El modo vertical (tabbed: false) debe persistir en hudr2.json"
        );
        assert!(!st2.snapshot.tabbed_view);

        // Simular segundo clic: false -> true (modo pestañas)
        let action2 = st.handle_mouse_click(toggle_line, 0);
        assert_eq!(action2, Some(ClickAction::ToggleTabbedView(true)));
        assert!(st.tabbed);
        assert!(st.snapshot.tabbed_view);

        let st3 = AppState::with_config(Some(cfg_path), None);
        assert!(
            st3.tabbed,
            "El modo pestañas (tabbed: true) debe persistir en hudr2.json"
        );
        assert!(st3.snapshot.tabbed_view);
    }
}
