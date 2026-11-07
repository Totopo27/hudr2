//! Atribución de sesiones a cuentas: extracción de prefijos desde agente/modelo
//! y resolución de pools (`pool-N`).

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
