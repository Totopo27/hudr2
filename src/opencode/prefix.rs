//! Atribución de sesiones a cuentas: extracción de prefijos desde agente/modelo,
//! resolución de pools (`pool-N`) y subagentes SDD declarados en la config de OpenCode.

use super::model_info::clean_model_name;
use regex::Regex;
use std::sync::LazyLock;

static RE_POOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:^|[/\\'])pool-0?([0-9]+)").unwrap());
static RE_AGENT_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)sdd-[a-z0-9]+-([a-z0-9_-]+)").unwrap());
static RE_MODEL_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:^|[/"'\\\\])([a-z0-9_-]+)/(?:gemini|claude|antigravity|[a-z0-9.-]+)"#)
        .unwrap()
});

pub const KNOWN_PROXY_PREFIXES: &[&str] = &["cliproxy/", "opencode/"];

/// Limpia y normaliza un identificador de modelo eliminando prefijos de vendor o proxy conocidos
/// como `cliproxy/`, `opencode/`, etc.
pub fn normalize_model_str(model_str: &str) -> String {
    let mut clean = model_str.to_string();
    for pfx in KNOWN_PROXY_PREFIXES {
        loop {
            let lower = clean.to_lowercase();
            if let Some(idx) = lower.find(pfx) {
                // Mapear el índice de bytes en minúsculas al índice de caracteres original
                let char_idx = lower[..idx].chars().count();
                let byte_start = clean.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(idx);
                let byte_end = clean.char_indices().nth(char_idx + pfx.chars().count()).map(|(i, _)| i).unwrap_or(byte_start + pfx.len());
                clean.drain(byte_start..byte_end);
            } else {
                break;
            }
        }
    }
    clean
}

pub fn resolve_pool_account_prefixes(model_str: &str, known_prefixes: &[String]) -> Vec<String> {
    if model_str.is_empty() {
        return Vec::new();
    }
    let model_id = clean_model_name(model_str);
    let clean = normalize_model_str(&model_id).to_lowercase();

    if let Some(caps) = RE_POOL.captures(&clean)
        && let Some(num_match) = caps.get(1)
        && let Ok(pool_num) = num_match.as_str().parse::<u32>()
    {
        let base_prefix = if pool_num == 1 {
            "agy".to_string()
        } else {
            format!("agy{pool_num}")
        };
        let mut matching = Vec::new();
        for pfx in known_prefixes {
            let p_low = pfx.to_lowercase();
            if pool_num == 1 {
                if p_low == "agy" || p_low.starts_with("agy_") {
                    matching.push(p_low);
                }
            } else if p_low == base_prefix || p_low.starts_with(&format!("{base_prefix}_")) {
                matching.push(p_low);
            }
        }
        if matching.is_empty() {
            matching.push(base_prefix);
        }
        return matching;
    }
    Vec::new()
}

pub fn extract_prefix(agent: &str, model: &str, sorted_prefixes: &[String]) -> Option<String> {
    // 1. Del agente (ej: sdd-orchestrator-xen1, sdd-apply-gianni, sdd-orchestrator-nina)
    if !agent.is_empty() {
        if let Some(caps) = RE_AGENT_PREFIX.captures(agent)
            && let Some(c_match) = caps.get(1)
        {
            let candidate = c_match.as_str().to_lowercase();
            if sorted_prefixes.is_empty()
                || sorted_prefixes
                    .iter()
                    .any(|p| p.to_lowercase() == candidate)
            {
                return Some(candidate);
            }
        }

        let lower_agent = agent.to_lowercase();
        for pfx in sorted_prefixes {
            let p_low = pfx.to_lowercase();
            let pat_dash = format!("-{p_low}");
            let pat_under = format!("_{p_low}");
            if lower_agent == p_low
                || lower_agent.ends_with(&pat_dash)
                || lower_agent.ends_with(&pat_under)
            {
                return Some(p_low);
            }
        }
    }

    // 2. Del modelo (ej: "nina/gemini-3.8-flash-high", "cliproxy/nina/...")
    if !model.is_empty() {
        let clean_model = normalize_model_str(&clean_model_name(model)).to_lowercase();

        let pool_prefixes = resolve_pool_account_prefixes(&clean_model, sorted_prefixes);
        if let Some(first_pool) = pool_prefixes.into_iter().next() {
            return Some(first_pool);
        }

        if let Some(caps) = RE_MODEL_PREFIX.captures(&clean_model)
            && let Some(c_match) = caps.get(1)
        {
            let candidate = c_match.as_str().to_lowercase();
            if candidate != "cliproxy"
                && candidate != "opencode"
                && (sorted_prefixes.is_empty()
                    || sorted_prefixes
                        .iter()
                        .any(|p| p.to_lowercase() == candidate))
            {
                return Some(candidate);
            }
        }

        for pfx in sorted_prefixes {
            let p_low = pfx.to_lowercase();
            if clean_model.starts_with(&format!("{p_low}/"))
                || clean_model.contains(&format!("/{p_low}/"))
                || clean_model.contains(&format!(":{p_low}/"))
                || clean_model.contains(&format!("\"{p_low}/"))
                || clean_model.ends_with(&format!("/{p_low}"))
            {
                return Some(p_low);
            }
        }
    }

    None
}

/// Extrae el prefijo de cuenta (`<prefijo>/modelo`) de un identificador de modelo
/// configurado, ignorando los proveedores intermedios `cliproxy/` y `opencode/`.
fn model_account_prefix(model_str: &str) -> Option<String> {
    let clean = normalize_model_str(model_str);
    clean
        .find('/')
        .map(|slash_idx| clean[..slash_idx].to_lowercase())
}

pub fn get_sdd_subagent_account_prefixes(
    orchestrator_agent: &str,
    target_dir: &str,
    known_prefixes: &[String],
) -> Vec<String> {
    let mut prefixes = Vec::new();
    let candidates = crate::paths::opencode_config_candidates(target_dir);

    for c in candidates {
        if !c.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&c) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        // 1. Tabla explícita en system o prompt (gentle-ai:sdd-model-assignments)
        let agent_sections = [val.get("agent"), val.get("agents")];
        for sec in agent_sections.into_iter().flatten() {
            if let Some(orch) = sec.get(orchestrator_agent) {
                let sys_text = orch
                    .get("system")
                    .or_else(|| orch.get("prompt"))
                    .and_then(|s| s.as_str())
                    .unwrap_or("");
                if let Some(start_idx) = sys_text.find("<!-- gentle-ai:sdd-model-assignments -->")
                    && let Some(end_idx) =
                        sys_text.find("<!-- /gentle-ai:sdd-model-assignments -->")
                {
                    let table_part = &sys_text[start_idx..end_idx];
                    for line in table_part.lines() {
                        if !line.contains('|') {
                            continue;
                        }
                        let cols: Vec<&str> = line.split('|').map(|col| col.trim()).collect();
                        if cols.len() >= 3
                            && !cols[1].is_empty()
                            && !cols[1].starts_with("Phase")
                            && !cols[1].starts_with("---")
                            && let Some(pfx) = model_account_prefix(cols[2])
                            && !prefixes.contains(&pfx)
                        {
                            prefixes.push(pfx);
                        }
                    }
                }
            }

            // 2. Mapeo de subagentes por prefijo en la sección agents
            if prefixes.is_empty() {
                let agent_prefix = orchestrator_agent
                    .strip_prefix("sdd-orchestrator-")
                    .map(|s| s.to_lowercase());

                if let Some(obj) = sec.as_object() {
                    for (name, def) in obj {
                        let lower = name.to_lowercase();
                        let is_match = if let Some(ref pfx) = agent_prefix {
                            lower.starts_with("sdd-")
                                && lower.ends_with(&format!("-{pfx}"))
                                && !lower.contains("orchestrator")
                        } else {
                            lower.starts_with("sdd-")
                                && !lower.contains("orchestrator")
                                && !lower.contains('-')
                        };

                        if is_match {
                            let model_str = def
                                .get("model")
                                .and_then(|m| {
                                    m.as_str().or_else(|| m.get("id").and_then(|i| i.as_str()))
                                })
                                .unwrap_or("");
                            if let Some(pfx) = model_account_prefix(model_str)
                                && !prefixes.contains(&pfx)
                            {
                                prefixes.push(pfx);
                            }
                        }
                    }
                }
            }
        }
        if !prefixes.is_empty() {
            break;
        }
    }

    // Filtrar solo los que realmente pertenecen a cuentas conocidas
    prefixes
        .into_iter()
        .filter(|p| {
            known_prefixes.is_empty() || known_prefixes.iter().any(|k| k.to_lowercase() == *p)
        })
        .collect()
}
