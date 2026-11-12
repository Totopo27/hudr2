use crate::models::{AccountQuota, AgentSession, HerdrPane, OrchestratorProfile, OrchestratorStatus};
use crate::paths::{gentle_ai_profiles_candidates, opencode_config_candidates};

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
        if let Ok(content) = crate::paths::read_config_string(&cfg_path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            let default_name = val
                .get("default")
                .and_then(|d| d.as_str())
                .unwrap_or("");

            if let Some(map) = val.get("profiles").and_then(|p| p.as_object()) {
                for (name, prof) in map {
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
                    let (mut prefix, model_short) = resolve_prefix_and_short_model(name, &clean, &known_prefixes);
                    let (display_name, mapped_prefix) = resolve_friendly_orchestrator_name(name, prefix.as_deref(), &clean, &known_prefixes);
                    if prefix.is_none() && mapped_prefix.is_some() {
                        prefix = mapped_prefix;
                    }

                    let name_low = name.to_lowercase();
                    let disp_low = display_name.to_lowercase();
                    if seen_names.contains(&name_low) || seen_names.contains(&disp_low) {
                        continue;
                    }
                    seen_names.insert(name_low);
                    seen_names.insert(disp_low.clone());

                    let (is_critical, is_warning, rem_pct) =
                        evaluate_orchestrator_quota_health(accounts, prefix.as_deref(), &model_short, &clean);

                    let mut subagents = Vec::new();
                    let mut seen_roles = std::collections::HashSet::new();
                    if let Some(prof_map) = prof.as_object() {
                        if let Some(agents_map) = prof_map.get("agents").and_then(|a| a.as_object()) {
                            for (role, a_val) in agents_map {
                                let role_low = role.to_lowercase();
                                if seen_roles.insert(role_low)
                                    && let Some(mut sub) = parse_subagent_entry(role, a_val, &known_prefixes)
                                {
                                    if sub.prefix.is_none() {
                                        sub.prefix = prefix.clone();
                                    }
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
                                && let Some(mut sub) = parse_subagent_entry(role, a_val, &known_prefixes)
                            {
                                if sub.prefix.is_none() {
                                    sub.prefix = prefix.clone();
                                }
                                subagents.push(sub);
                            }
                        }
                    }

                    let status = resolve_orchestrator_status(
                        name,
                        &display_name,
                        prefix.as_deref(),
                        &model_short,
                        &clean,
                        &subagents,
                        active_session,
                        panes,
                    );

                    profiles.push(OrchestratorProfile {
                        name: name.clone(),
                        display_name,
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

    // 2. Descubrimiento desde opencode.json (agentes de tipo orquestador en "agent" y "agents")
    for cfg_path in opencode_config_candidates(target_dir) {
        if let Ok(content) = crate::paths::read_config_string(&cfg_path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            let mut agent_maps: Vec<&serde_json::Map<String, serde_json::Value>> = Vec::new();
            if let Some(am) = val.get("agent").and_then(|a| a.as_object()) {
                agent_maps.push(am);
            }
            if let Some(ams) = val.get("agents").and_then(|a| a.as_object()) {
                agent_maps.push(ams);
            }

            for agent_map in &agent_maps {
                for (name, agent_val) in *agent_map {
                    let name_low = name.to_lowercase();
                    // Filtro estricto: descartar subagentes conocidos
                    if is_known_subagent(&name_low, agent_val) {
                        continue;
                    }

                    // Debe ser explícitamente un orquestador
                    let is_orch = name_low.contains("orchestrator")
                        || agent_val.get("mode").and_then(|m| m.as_str()) == Some("primary")
                        || (agent_val.get("permission").and_then(|p| p.get("task")).and_then(|t| t.as_object()).is_some());
                    if !is_orch {
                        continue;
                    }

                    let model_raw = agent_val
                        .get("model")
                        .and_then(|m| m.as_str())
                        .unwrap_or("gpt-5.6-sol");
                    let clean = crate::opencode::normalize_model_str(model_raw);
                    let (mut prefix, model_short) = resolve_prefix_and_short_model(name, &clean, &known_prefixes);
                    let (display_name, mapped_prefix) = resolve_friendly_orchestrator_name(name, prefix.as_deref(), &clean, &known_prefixes);
                    if prefix.is_none() && mapped_prefix.is_some() {
                        prefix = mapped_prefix;
                    }

                    let disp_low = display_name.to_lowercase();
                    if seen_names.contains(&name_low) || seen_names.contains(&disp_low) {
                        continue;
                    }
                    seen_names.insert(name_low);
                    seen_names.insert(disp_low.clone());

                    let (is_critical, is_warning, rem_pct) =
                        evaluate_orchestrator_quota_health(accounts, prefix.as_deref(), &model_short, &clean);

                    let mut subagents = Vec::new();
                    let mut seen_roles = std::collections::HashSet::new();

                    // Si es un orquestador SDD (ej. sdd-orchestrator-agy3), poblar su suite SDD correspondiente
                    if let Some(suffix) = name.strip_prefix("sdd-orchestrator-") {
                        let target_suffix = suffix.to_lowercase();
                        for am in &agent_maps {
                            for (sub_name, sub_val) in *am {
                                let sub_low = sub_name.to_lowercase();
                                if sub_low.starts_with("sdd-") && !sub_low.starts_with("sdd-orchestrator-") {
                                    // Verificar si este subagente SDD pertenece a este perfil
                                    if (sub_low.ends_with(&format!("-{target_suffix}")) || sub_low.ends_with(&format!("_{target_suffix}")))
                                        && seen_roles.insert(sub_low.clone())
                                        && let Some(mut sub) = parse_subagent_entry(sub_name, sub_val, &known_prefixes)
                                    {
                                        if sub.prefix.is_none() {
                                            sub.prefix = prefix.clone();
                                        }
                                        subagents.push(sub);
                                    }
                                }
                            }
                        }
                    } else {
                        // Orquestador estándar: añadir subagentes explícitos si los declara
                        for am in &agent_maps {
                            for (sub_name, sub_val) in *am {
                                let sub_low = sub_name.to_lowercase();
                                if sub_name != name
                                    && sub_val.is_object()
                                    && is_known_subagent(&sub_low, sub_val)
                                    && seen_roles.insert(sub_low)
                                    && let Some(mut sub) = parse_subagent_entry(sub_name, sub_val, &known_prefixes)
                                {
                                    if sub.prefix.is_none() {
                                        sub.prefix = prefix.clone();
                                    }
                                    subagents.push(sub);
                                }
                            }
                        }
                    }

                    let status = resolve_orchestrator_status(
                        name,
                        &display_name,
                        prefix.as_deref(),
                        &model_short,
                        &clean,
                        &subagents,
                        active_session,
                        panes,
                    );

                    profiles.push(OrchestratorProfile {
                        name: name.clone(),
                        display_name,
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
    }

    // Ordenamiento: Primero los que están Working, luego Idle, luego Offline.
    // Entre el mismo estado: los default primero, y luego orden alfabético por display_name.
    profiles.sort_by(|a, b| {
        let status_order = |s: OrchestratorStatus| match s {
            OrchestratorStatus::Working => 0,
            OrchestratorStatus::Idle => 1,
            OrchestratorStatus::Offline => 2,
        };
        status_order(a.status)
            .cmp(&status_order(b.status))
            .then_with(|| b.is_default.cmp(&a.is_default))
            .then_with(|| a.display_name().cmp(b.display_name()))
    });

    profiles
}

fn is_known_subagent(name_low: &str, agent_val: &serde_json::Value) -> bool {
    if agent_val.get("mode").and_then(|m| m.as_str()) == Some("subagent") {
        return true;
    }
    if agent_val.get("hidden").and_then(|h| h.as_bool()) == Some(true) {
        return true;
    }
    if agent_val
        .get("permission")
        .and_then(|p| p.get("task"))
        .and_then(|t| t.as_str())
        == Some("deny")
    {
        return true;
    }
    if name_low.starts_with("gentle-ai-")
        || name_low.starts_with("jd-")
        || name_low.starts_with("review-")
        || name_low == "deep-researcher"
        || name_low == "news-researcher"
        || name_low == "ui-designer"
    {
        return true;
    }
    if name_low.starts_with("sdd-") && !name_low.starts_with("sdd-orchestrator-") {
        return true;
    }
    false
}

pub fn resolve_friendly_orchestrator_name(
    raw_name: &str,
    prefix_opt: Option<&str>,
    clean_model: &str,
    known_prefixes: &[String],
) -> (String, Option<String>) {
    let name_low = raw_name.to_lowercase();
    let model_low = clean_model.to_lowercase();
    let pfx_low = prefix_opt.map(|s| s.to_lowercase());

    let matches_id = |id: &str| {
        pfx_low.as_deref() == Some(id)
            || model_low.contains(id)
            || known_prefixes
                .iter()
                .any(|p| p.to_lowercase() == id && (pfx_low.as_deref() == Some(p.as_str()) || model_low.contains(p.as_str())))
    };

    // 1. Gentle AI fleets
    if name_low == "fleet-codex" || name_low == "gentle-orchestrator-codex" || name_low == "codex" {
        let pfx = known_prefixes
            .iter()
            .find(|p| p.contains("codex"))
            .cloned()
            .or_else(|| Some("tavocodex".to_string()));
        return ("tavocodex".to_string(), pfx);
    }
    if name_low == "fleet-gianni" || name_low == "gentle-orchestrator-gianni" || name_low == "gianniclaude" {
        return ("gianniclaude".to_string(), Some("gianniclaude".to_string()));
    }
    if name_low == "fleet-clas" || name_low == "gentle-orchestrator-clas" || name_low == "clasclaude" {
        return ("clasclaude".to_string(), Some("clasclaude".to_string()));
    }
    if name_low == "fleet-tavo" || name_low == "tavoclaude" {
        return ("tavoclaude".to_string(), Some("tavoclaude".to_string()));
    }
    if name_low == "gentle-orchestrator" {
        if matches_id("clasclaude") {
            return ("clasclaude".to_string(), Some("clasclaude".to_string()));
        }
        if matches_id("gianniclaude") {
            return ("gianniclaude".to_string(), Some("gianniclaude".to_string()));
        }
        if matches_id("tavocodex") || model_low.contains("codex") {
            return ("tavocodex".to_string(), Some("tavocodex".to_string()));
        }
        return ("tavoclaude".to_string(), Some("tavoclaude".to_string()));
    }

    // 2. SDD orchestrators (e.g. "sdd-orchestrator-agy3" -> "sdd-agy3")
    if let Some(suffix) = name_low.strip_prefix("sdd-orchestrator-") {
        return (format!("sdd-{suffix}"), Some(suffix.to_string()));
    }
    if let Some(suffix) = name_low.strip_prefix("sdd-")
        && !suffix.starts_with("explore")
        && !suffix.starts_with("apply")
        && !suffix.starts_with("verify")
        && !suffix.starts_with("spec")
        && !suffix.starts_with("tasks")
    {
        return (name_low.to_string(), Some(suffix.to_string()));
    }

    if let Some(rest) = name_low.strip_prefix("fleet-") {
        let pfx = prefix_opt
            .map(|s| s.to_string())
            .or_else(|| known_prefixes.iter().find(|p| p.contains(rest)).cloned());
        return (rest.to_string(), pfx);
    }

    let pfx = prefix_opt.map(|s| s.to_string());
    (raw_name.to_string(), pfx)
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

fn match_boundary(haystack: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs_pos = start + pos;
        let before_ok = abs_pos == 0 || {
            let prev = haystack[..abs_pos].chars().next_back().unwrap();
            !prev.is_alphanumeric()
        };
        let after_pos = abs_pos + needle.len();
        let after_ok = after_pos == haystack.len() || {
            let next = haystack[after_pos..].chars().next().unwrap();
            !next.is_alphanumeric()
        };
        if before_ok && after_ok {
            return true;
        }
        start = abs_pos + needle.len().max(1);
    }
    false
}

/// Determina si un perfil de orquestador está Working, Idle o Offline,
/// evaluando tanto la identidad del orquestador como la pertenencia estricta de subagentes activos.
#[allow(clippy::too_many_arguments)]
fn resolve_orchestrator_status(
    profile_name: &str,
    display_name: &str,
    prefix_opt: Option<&str>,
    model_short: &str,
    full_model: &str,
    subagents: &[crate::models::SubAgentProfile],
    active_session: &AgentSession,
    panes: &[HerdrPane],
) -> OrchestratorStatus {
    let name_low = profile_name.to_lowercase();
    let disp_low = display_name.to_lowercase();

    // 1. Coincidencia directa con panes de Herdr (fuente de verdad si hay perfil asignado al pane)
    for p in panes {
        let pane_prof = p.tokens.get("profile").map(|s| s.as_str());
        let pane_agent = p.agent.as_deref();
        let pane_title = p.terminal_title.as_deref().unwrap_or("");
        let hit = pane_prof.map(|pr| pr.eq_ignore_ascii_case(profile_name) || pr.eq_ignore_ascii_case(display_name) || match_boundary(&pr.to_lowercase(), &name_low) || match_boundary(&pr.to_lowercase(), &disp_low)).unwrap_or(false)
            || pane_agent.map(|ag| ag.eq_ignore_ascii_case(profile_name) || ag.eq_ignore_ascii_case(display_name) || match_boundary(&ag.to_lowercase(), &name_low) || match_boundary(&ag.to_lowercase(), &disp_low)).unwrap_or(false)
            || match_boundary(&pane_title.to_lowercase(), &name_low)
            || match_boundary(&pane_title.to_lowercase(), &disp_low);

        if hit {
            if p.agent_status.as_deref() == Some("working") {
                return OrchestratorStatus::Working;
            }
            return OrchestratorStatus::Idle;
        }
    }

    // 2. Coincidencia directa de la sesión activa con el orquestador mismo
    let sess_low = active_session.agent.to_lowercase();

    let is_orch_name = sess_low == name_low
        || sess_low == disp_low
        || match_boundary(&sess_low, &name_low)
        || match_boundary(&sess_low, &disp_low)
        || (disp_low == "tavocodex" && (match_boundary(&sess_low, "codex") || sess_low == "gentle-orchestrator-codex"))
        || (disp_low == "tavoclaude" && (sess_low == "gentle-orchestrator" || match_boundary(&sess_low, "tavo")))
        || (disp_low == "gianniclaude" && match_boundary(&sess_low, "gianni"))
        || (disp_low == "clasclaude" && match_boundary(&sess_low, "clas"));

    let is_generic_orch = active_session.agent.eq_ignore_ascii_case("orchestrator")
        || active_session.agent.eq_ignore_ascii_case("gentle-orchestrator")
        || active_session.agent.eq_ignore_ascii_case("pi")
        || active_session.agent.eq_ignore_ascii_case("gentle-shell")
        || active_session.agent.to_lowercase().contains("orchestrator");

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
