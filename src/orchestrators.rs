use crate::models::{AccountQuota, AgentSession, HerdrPane, OrchestratorProfile, OrchestratorStatus};
use crate::paths::{gentle_ai_profiles_candidates, opencode_config_candidates};
use std::fs;

/// Descubre y evalúa los perfiles de orquestador configurados en Gentle AI (`profiles.json`)
/// y OpenCode (`opencode.json`), determinando su estado operativo (`Working`, `Idle`, `Offline`)
/// y su salud de cuota soberana basada estrictamente en el modelo asignado al orquestador.
pub fn discover_orchestrators(
    target_dir: &str,
    accounts: &[AccountQuota],
    active_session: &AgentSession,
    panes: &[HerdrPane],
) -> Vec<OrchestratorProfile> {
    let mut profiles = Vec::new();
    let mut seen_names = std::collections::HashSet::new();
    let known_prefixes = crate::quotas::sorted_known_prefixes(accounts, Some(target_dir));

    // 1. Descubrimiento desde profiles.json de Gentle AI
    for cfg_path in gentle_ai_profiles_candidates(target_dir) {
        if let Ok(content) = fs::read_to_string(&cfg_path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            let default_name = val
                .get("default")
                .and_then(|d| d.as_str())
                .unwrap_or("");

            if let Some(map) = val.get("profiles").and_then(|p| p.as_object()) {
                for (name, prof) in map {
                    let name_low = name.to_lowercase();
                    if seen_names.contains(&name_low) {
                        continue;
                    }
                    seen_names.insert(name_low);

                    let orch_obj = prof.get("orchestrator");
                    let model_raw = orch_obj
                        .and_then(|o| o.get("model"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("gpt-5.6-sol");
                    let thinking = orch_obj
                        .and_then(|o| o.get("thinking"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string());

                    let clean = crate::opencode::normalize_model_str(model_raw);
                    let (prefix, model_short) = resolve_prefix_and_short_model(name, &clean, &known_prefixes);

                    let (is_critical, is_warning, rem_pct) =
                        evaluate_orchestrator_quota_health(accounts, prefix.as_deref(), &model_short, &clean);

                    let mut subagents = Vec::new();
                    let mut seen_roles = std::collections::HashSet::new();
                    if let Some(prof_map) = prof.as_object() {
                        if let Some(agents_map) = prof_map.get("agents").and_then(|a| a.as_object()) {
                            for (role, a_val) in agents_map {
                                let role_low = role.to_lowercase();
                                if seen_roles.insert(role_low)
                                    && let Some(sub) = parse_subagent_entry(role, a_val, &known_prefixes)
                                {
                                    subagents.push(sub);
                                }
                            }
                        }
                        for (role, a_val) in prof_map {
                            let role_low = role.to_lowercase();
                            if role != "orchestrator"
                                && role != "agents"
                                && a_val.is_object()
                                && seen_roles.insert(role_low)
                                && let Some(sub) = parse_subagent_entry(role, a_val, &known_prefixes)
                            {
                                subagents.push(sub);
                            }
                        }
                    }

                    let status = resolve_orchestrator_status(
                        name,
                        prefix.as_deref(),
                        &model_short,
                        &clean,
                        &subagents,
                        active_session,
                        panes,
                    );

                    profiles.push(OrchestratorProfile {
                        name: name.clone(),
                        model: clean,
                        model_short,
                        prefix,
                        thinking,
                        is_default: name == default_name,
                        status,
                        is_critical,
                        is_warning,
                        rem_pct,
                        subagents,
                        recently_finished: false,
                    });
                }
            }
        }
    }

    // 2. Descubrimiento desde opencode.json (agentes de tipo orquestador)
    for cfg_path in opencode_config_candidates(target_dir) {
        if let Ok(content) = fs::read_to_string(&cfg_path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
            && let Some(agent_map) = val.get("agent").and_then(|a| a.as_object())
        {
            for (name, agent_val) in agent_map {
                let name_low = name.to_lowercase();
                if seen_names.contains(&name_low) {
                    continue;
                }
                // Solo agentes explícitamente orquestadores
                let is_orch = name_low.contains("orchestrator")
                    || agent_val
                        .get("permission")
                        .and_then(|p| p.get("task"))
                        .is_some();
                if !is_orch {
                    continue;
                }
                seen_names.insert(name_low);

                let model_raw = agent_val
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("gpt-5.6-sol");
                let clean = crate::opencode::normalize_model_str(model_raw);
                let (prefix, model_short) = resolve_prefix_and_short_model(name, &clean, &known_prefixes);

                let (is_critical, is_warning, rem_pct) =
                    evaluate_orchestrator_quota_health(accounts, prefix.as_deref(), &model_short, &clean);

                let mut subagents = Vec::new();
                let mut seen_roles = std::collections::HashSet::new();
                for (sub_name, sub_val) in agent_map {
                    let sub_low = sub_name.to_lowercase();
                    if sub_name != name
                        && sub_val.is_object()
                        && seen_roles.insert(sub_low)
                        && let Some(sub) = parse_subagent_entry(sub_name, sub_val, &known_prefixes)
                    {
                        subagents.push(sub);
                    }
                }

                let status = resolve_orchestrator_status(
                    name,
                    prefix.as_deref(),
                    &model_short,
                    &clean,
                    &subagents,
                    active_session,
                    panes,
                );

                profiles.push(OrchestratorProfile {
                    name: name.clone(),
                    model: clean,
                    model_short,
                    prefix,
                    thinking: None,
                    is_default: false,
                    status,
                    is_critical,
                    is_warning,
                    rem_pct,
                    subagents,
                    recently_finished: false,
                });
            }
        }
    }

    // Ordenamiento: Primero los que están Working, luego Idle, luego Offline.
    // Entre el mismo estado: los default primero, y luego orden alfabético.
    profiles.sort_by(|a, b| {
        let status_order = |s: OrchestratorStatus| match s {
            OrchestratorStatus::Working => 0,
            OrchestratorStatus::Idle => 1,
            OrchestratorStatus::Offline => 2,
        };
        status_order(a.status)
            .cmp(&status_order(b.status))
            .then_with(|| b.is_default.cmp(&a.is_default))
            .then_with(|| a.name.cmp(&b.name))
    });

    profiles
}

fn parse_subagent_entry(
    role: &str,
    val: &serde_json::Value,
    known_prefixes: &[String],
) -> Option<crate::models::SubAgentProfile> {
    let model_raw = val.get("model").and_then(|m| m.as_str())?;
    let thinking = val
        .get("thinking")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let clean = crate::opencode::normalize_model_str(model_raw);
    let (prefix, model_short) = resolve_prefix_and_short_model(role, &clean, known_prefixes);

    Some(crate::models::SubAgentProfile {
        role: role.to_string(),
        model: clean,
        model_short,
        prefix,
        thinking,
    })
}

fn resolve_prefix_and_short_model(
    agent_name: &str,
    clean_model: &str,
    known_prefixes: &[String],
) -> (Option<String>, String) {
    if let Some((pfx, rest)) = clean_model.split_once('/') {
        let p_low = pfx.to_ascii_lowercase();
        if !matches!(p_low.as_str(), "cliproxy" | "opencode" | "cpamc" | "relay") {
            return (Some(pfx.to_string()), rest.to_string());
        }
        if let Some((sub_pfx, sub_rest)) = rest.split_once('/') {
            return (Some(sub_pfx.to_string()), sub_rest.to_string());
        }
        return (None, rest.to_string());
    }

    let pfx = crate::opencode::extract_prefix(agent_name, clean_model, known_prefixes);
    (pfx, clean_model.to_string())
}

/// Evalúa la salud de cuota de forma SOBERANA para el modelo específico del orquestador.
fn evaluate_orchestrator_quota_health(
    accounts: &[AccountQuota],
    prefix_opt: Option<&str>,
    model_short: &str,
    full_model: &str,
) -> (bool, bool, u8) {
    // 1. Buscar la cuenta que provee el modelo del orquestador
    let target_acc = if let Some(pfx) = prefix_opt {
        accounts.iter().find(|a| a.prefix.eq_ignore_ascii_case(pfx))
    } else {
        accounts.iter().find(|a| {
            if full_model.to_lowercase().contains(&a.prefix.to_lowercase()) {
                return true;
            }
            if a.prefix.eq_ignore_ascii_case("codex")
                && (full_model.contains("gpt-5") || full_model.contains("codex"))
            {
                return true;
            }
            false
        })
    };

    if let Some(acc) = target_acc {
        let eff = crate::quotas::evaluate_account_for_model(acc, model_short);
        (eff.is_critical, eff.is_warning, eff.rem_5h)
    } else {
        (false, false, 100)
    }
}

/// Determina si un perfil de orquestador está Working, Idle o Offline,
/// evaluando tanto la identidad del orquestador como la pertenencia estricta de subagentes activos.
fn resolve_orchestrator_status(
    profile_name: &str,
    prefix_opt: Option<&str>,
    model_short: &str,
    full_model: &str,
    subagents: &[crate::models::SubAgentProfile],
    active_session: &AgentSession,
    panes: &[HerdrPane],
) -> OrchestratorStatus {
    // 1. Coincidencia directa con panes de Herdr (fuente de verdad si hay perfil asignado al pane)
    for p in panes {
        let pane_prof = p.tokens.get("profile").map(|s| s.as_str());
        let pane_agent = p.agent.as_deref();
        let hit = pane_prof.map(|pr| pr.eq_ignore_ascii_case(profile_name)).unwrap_or(false)
            || pane_agent.map(|ag| ag.eq_ignore_ascii_case(profile_name)).unwrap_or(false);

        if hit {
            if p.agent_status.as_deref() == Some("working") {
                return OrchestratorStatus::Working;
            }
            return OrchestratorStatus::Idle;
        }
    }

    // 2. Coincidencia directa de la sesión activa con el orquestador mismo
    let is_orch_name = active_session.agent.eq_ignore_ascii_case(profile_name)
        || active_session
            .agent
            .to_lowercase()
            .contains(&profile_name.to_lowercase());

    let is_generic_orch = active_session.agent.eq_ignore_ascii_case("orchestrator")
        || active_session.agent.eq_ignore_ascii_case("gentle-orchestrator")
        || active_session.agent.eq_ignore_ascii_case("pi")
        || active_session.agent.eq_ignore_ascii_case("gentle-shell");

    let orch_model_match = if let Some(pfx) = prefix_opt {
        active_session
            .local_prefix
            .as_deref()
            .map(|lp| lp.eq_ignore_ascii_case(pfx))
            .unwrap_or(false)
            && (active_session.model_short.eq_ignore_ascii_case(model_short)
                || active_session.model.eq_ignore_ascii_case(full_model))
    } else {
        active_session.model_short.eq_ignore_ascii_case(model_short)
            || active_session.model.eq_ignore_ascii_case(full_model)
    };

    let direct_orch_match = is_orch_name || (is_generic_orch && orch_model_match);

    // 3. Coincidencia estricta con subagentes configurados en este perfil
    let subagent_match = subagents.iter().any(|sub| {
        let sess_agent = active_session.agent.to_lowercase();
        let full_lower = sub.role.to_lowercase();
        let clean_role = crate::ui::clean_subagent_role(&sub.role);
        let sess_clean = crate::ui::clean_subagent_role(&sess_agent);

        let role_hit = sess_agent == full_lower
            || sess_agent == clean_role
            || sess_clean == clean_role
            || sess_agent.contains(&full_lower)
            || sess_agent.contains(&clean_role);

        if !role_hit {
            return false;
        }

        // Si coincide el rol, verificar que coincida el modelo/cuenta asignada a este subagente en este perfil
        if let Some(ref spfx) = sub.prefix {
            active_session
                .local_prefix
                .as_deref()
                .map(|lp| lp.eq_ignore_ascii_case(spfx))
                .unwrap_or(false)
                || active_session.model.to_lowercase().contains(&spfx.to_lowercase())
        } else {
            active_session.model_short.eq_ignore_ascii_case(&sub.model_short)
                || active_session.model.eq_ignore_ascii_case(&sub.model)
        }
    });

    if direct_orch_match || subagent_match {
        if active_session.agent_status == "working" {
            return OrchestratorStatus::Working;
        }
        if active_session.agent_status == "idle"
            && (active_session.turn_count > 0
                || active_session.tokens_input > 0
                || active_session.session_id.is_some())
        {
            return OrchestratorStatus::Idle;
        }
    }

    OrchestratorStatus::Offline
}
