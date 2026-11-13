//! Atribución de sesiones a cuentas: extracción de prefijos desde agente/modelo
//! y resolución de pools (`pool-N`).

use super::model_info::clean_model_name;

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

fn extract_pool_number(clean: &str) -> Option<u32> {
    let mut rest = clean;
    while let Some(idx) = rest.find("pool-") {
        let is_boundary = idx == 0 || matches!(rest.as_bytes()[idx - 1], b'/' | b'\\' | b'\'');
        let after = &rest[idx + 5..];
        if is_boundary {
            let digits_end = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            let digits = &after[..digits_end];
            if !digits.is_empty()
                && let Ok(num) = digits.parse::<u32>()
            {
                return Some(num);
            }
        }
        rest = after;
    }
    None
}

fn matches_pool_num(pfx: &str, pool_num: u32) -> bool {
    let p_low = pfx.to_ascii_lowercase();
    let base = if let Some((head, _sub)) = p_low.split_once('_') {
        head
    } else {
        &p_low
    };
    let base_trimmed = base.trim_end_matches(|c: char| c.is_ascii_digit());
    let num_part = &base[base_trimmed.len()..];
    if let Ok(n) = num_part.parse::<u32>() {
        return n == pool_num;
    }
    if pool_num == 1 && num_part.is_empty() {
        return true;
    }
    false
}

pub fn resolve_pool_account_prefixes(model_str: &str, known_prefixes: &[String]) -> Vec<String> {
    if model_str.is_empty() {
        return Vec::new();
    }
    let model_id = clean_model_name(model_str);
    let clean = normalize_model_str(&model_id).to_lowercase();

    if let Some(pool_num) = extract_pool_number(&clean) {
        // 1. Detección dinámica: busca si alguna cuenta conocida coincide numéricamente con el pool
        let mut dynamic_matches = Vec::new();
        for pfx in known_prefixes {
            if matches_pool_num(pfx, pool_num) {
                dynamic_matches.push(pfx.to_ascii_lowercase());
            }
        }
        if !dynamic_matches.is_empty() {
            return dynamic_matches;
        }

        // 2. Fallback histórico a "agy" / "agyN" si no hay coincidencias dinámicas
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

fn extract_agent_candidate(agent: &str) -> Option<String> {
    let lower = agent.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("sdd-")
        .or_else(|| lower.strip_prefix("gentle-"))?;
    let (role, candidate) = rest.split_once('-')?;
    if !role.is_empty()
        && role.chars().all(|c| c.is_ascii_alphanumeric())
        && !candidate.is_empty()
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn extract_model_prefix_candidate(clean_model: &str) -> Option<String> {
    for (idx, ch) in clean_model.char_indices() {
        if ch == '/' {
            let before = &clean_model[..idx];
            let after = &clean_model[idx + 1..];
            let seg = before.rsplit(['/', '\\', '\'', '"']).next().unwrap_or("");
            if !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && !after.is_empty()
            {
                let cand = seg.to_string();
                if cand != "cliproxy" && cand != "opencode" && cand != "cpamc" {
                    return Some(cand);
                }
            }
        }
    }
    None
}

pub fn extract_prefix(agent: &str, model: &str, sorted_prefixes: &[String]) -> Option<String> {
    // 1. Del agente (ej: orchestrator-cuenta1, apply-dev, gentle-orchestrator-acc2)
    if !agent.is_empty() {
        if let Some(candidate) = extract_agent_candidate(agent) {
            if sorted_prefixes.is_empty() {
                return Some(candidate);
            }
            if let Some(matched) = sorted_prefixes.iter().find(|p| {
                let pl = p.to_ascii_lowercase();
                pl == candidate || pl.ends_with(&candidate)
            }) {
                return Some(matched.to_ascii_lowercase());
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
                || (p_low.ends_with("codex") && lower_agent.contains("codex"))
            {
                return Some(p_low);
            }
        }
    }

    // 2. Del modelo (ej: "cuenta1/gemini-3.8-flash-high", "cliproxy/acc-dev/...", "cpamc/cuenta4/...", "gemini-3.8-flash-high (cuenta4)")
    if !model.is_empty() {
        let raw_model = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(model) {
            let nested_model = parsed.get("model").and_then(|m| {
                m.as_str().or_else(|| {
                    m.get("id")
                        .or_else(|| m.get("modelID"))
                        .or_else(|| m.get("model_id"))
                        .or_else(|| m.get("name"))
                        .and_then(|i| i.as_str())
                })
            });
            parsed
                .get("id")
                .or_else(|| parsed.get("modelID"))
                .or_else(|| parsed.get("model_id"))
                .and_then(|i| i.as_str())
                .or(nested_model)
                .or_else(|| parsed.get("name").and_then(|i| i.as_str()))
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

        if let Some(candidate) = extract_model_prefix_candidate(&clean_model)
            && (sorted_prefixes.is_empty()
                || sorted_prefixes
                    .iter()
                    .any(|p| p.to_lowercase() == candidate))
        {
            return Some(candidate);
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
                || (p_low.ends_with("codex") && clean_model.contains("[codex]"))
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

    #[test]
    fn resolve_pool_account_prefixes_without_regex() {
        let known = vec!["agy".to_string(), "agy2".to_string(), "agy2_1".to_string()];
        let p1 = resolve_pool_account_prefixes("pool-01/gemini-1.5", &known);
        assert_eq!(p1, vec!["agy"]);
        let p2 = resolve_pool_account_prefixes("models/pool-2/claude", &known);
        assert_eq!(p2, vec!["agy2", "agy2_1"]);

        // Detección dinámica con prefijos no-agy
        let non_agy = vec![
            "cuenta1".to_string(),
            "cuenta2".to_string(),
            "cuenta2_1".to_string(),
        ];
        let np1 = resolve_pool_account_prefixes("pool-01/gemini-1.5", &non_agy);
        assert_eq!(np1, vec!["cuenta1"]);
        let np2 = resolve_pool_account_prefixes("models/pool-2/claude", &non_agy);
        assert_eq!(np2, vec!["cuenta2", "cuenta2_1"]);
    }
}
