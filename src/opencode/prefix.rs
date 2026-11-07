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

pub const KNOWN_PROXY_PREFIXES: &[&str] = &[
    "cliproxy/",
    "opencode/",
    "cpamc/",
    "cliproxy:",
    "opencode:",
    "cpamc:",
];

/// Limpia y normaliza un identificador de modelo eliminando prefijos de vendor o proxy conocidos
/// como `cliproxy/`, `opencode/`, etc. (comparación ASCII case-insensitive sin asignaciones intermedias).
pub fn normalize_model_str(model_str: &str) -> String {
    let mut rest = model_str;
    'strip: loop {
        for pfx in KNOWN_PROXY_PREFIXES {
            if let Some(head) = rest.get(..pfx.len())
                && head.eq_ignore_ascii_case(pfx)
            {
                rest = &rest[pfx.len()..];
                continue 'strip;
            }
        }
        break;
    }
    rest.to_string()
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
            let pat_paren = format!("({p_low})");
            let pat_bracket = format!("[{p_low}]");
            if lower_agent == p_low
                || lower_agent.ends_with(&pat_dash)
                || lower_agent.ends_with(&pat_under)
                || lower_agent.contains(&pat_paren)
                || lower_agent.contains(&pat_bracket)
            {
                return Some(p_low);
            }
        }
    }

    // 2. Del modelo (ej: "nina/gemini-3.8-flash-high", "cliproxy/nina/...", "cpamc/cuenta4/...", "gemini-3.8-flash-high (cuenta4)")
    if !model.is_empty() {
        let raw_model = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(model) {
            parsed
                .get("id")
                .or_else(|| parsed.get("modelID"))
                .or_else(|| parsed.get("model_id"))
                .or_else(|| parsed.get("model"))
                .or_else(|| parsed.get("name"))
                .and_then(|i| i.as_str())
                .unwrap_or(model)
                .to_string()
        } else {
            model.to_string()
        };
        let clean_model = normalize_model_str(&raw_model).to_lowercase();

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
                && candidate != "cpamc"
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
                || clean_model.starts_with(&format!("{p_low}:"))
                || clean_model.contains(&format!("/{p_low}/"))
                || clean_model.contains(&format!(":{p_low}:"))
                || clean_model.contains(&format!(":{p_low}/"))
                || clean_model.contains(&format!("/{p_low}:"))
                || clean_model.contains(&format!("\"{p_low}/"))
                || clean_model.ends_with(&format!("/{p_low}"))
                || clean_model.ends_with(&format!(":{p_low}"))
                || clean_model.contains(&format!("({p_low})"))
                || clean_model.contains(&format!("[{p_low}]"))
                || clean_model.contains(&format!(" {p_low} "))
                || clean_model.ends_with(&format!(" {p_low}"))
                || clean_model.ends_with(&format!("({p_low})"))
                || clean_model.ends_with(&format!("[{p_low}]"))
            {
                return Some(p_low);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_model_str_handles_mixed_case_nested_and_unicode() {
        assert_eq!(
            normalize_model_str("CLIPROXY/OpenCode/agy1/gemini-3.8-flash-high"),
            "agy1/gemini-3.8-flash-high"
        );
        assert_eq!(
            normalize_model_str("opencode/cliproxy/ñandu-model"),
            "ñandu-model"
        );
        assert_eq!(
            normalize_model_str("cpamc/cuenta4/gemini-3.8-flash-high"),
            "cuenta4/gemini-3.8-flash-high"
        );
        assert_eq!(normalize_model_str("ñandu"), "ñandu");
        assert_eq!(normalize_model_str(""), "");
    }

    #[test]
    fn extract_prefix_supports_cpamc_and_parentheses_syntax() {
        let accounts = vec![
            "cuenta1".to_string(),
            "cuenta2".to_string(),
            "cuenta3".to_string(),
            "cuenta4".to_string(),
        ];

        // 1. Con prefijo de proxy cpamc/
        let pfx1 = extract_prefix(
            "Gentle-Orchestrator",
            "cpamc/cuenta4/gemini-3.8-flash-high",
            &accounts,
        );
        assert_eq!(pfx1.as_deref(), Some("cuenta4"));

        // 2. Con formato de paréntesis en el modelo: "gemini-3.8-flash-high (cuenta4)"
        let pfx2 = extract_prefix(
            "Gentle-Orchestrator",
            "gemini-3.8-flash-high (cuenta4)",
            &accounts,
        );
        assert_eq!(pfx2.as_deref(), Some("cuenta4"));

        // 3. Con formato de paréntesis en el agente
        let pfx3 = extract_prefix(
            "Gentle-Orchestrator (cuenta4)",
            "gemini-3.8-flash-high",
            &accounts,
        );
        assert_eq!(pfx3.as_deref(), Some("cuenta4"));
    }
}
