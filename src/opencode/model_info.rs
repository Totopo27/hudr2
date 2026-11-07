//! Normalización de nombres de modelo, títulos de terminal y límites de contexto.

fn strip_oc_prefix(title: &str) -> Option<&str> {
    let t = title.trim_start();
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("opencode") {
        let rest = t[8..].trim_start();
        if let Some(after_pipe) = rest.strip_prefix('|') {
            return Some(after_pipe.trim_start());
        }
    } else if lower.starts_with("oc") {
        let rest = t[2..].trim_start();
        if let Some(after_pipe) = rest.strip_prefix('|') {
            return Some(after_pipe.trim_start());
        }
    }
    None
}

pub fn clean_opencode_terminal_title(title: &str) -> String {
    strip_oc_prefix(title).unwrap_or(title).trim().to_string()
}

pub fn is_opencode_terminal_title(title: &str) -> bool {
    let trimmed = title.trim();
    trimmed.eq_ignore_ascii_case("opencode")
        || trimmed.eq_ignore_ascii_case("oc")
        || strip_oc_prefix(trimmed).is_some()
}

fn strip_account_annotation(s: &str) -> &str {
    let trimmed = s.trim_end();
    let Some(last_ch) = trimmed.chars().last() else {
        return trimmed;
    };
    let open_ch = match last_ch {
        ')' => '(',
        ']' => '[',
        _ => return trimmed,
    };
    if let Some(open_idx) = trimmed.rfind(open_ch) {
        let inside = &trimmed[open_idx + 1..trimmed.len() - 1];
        if !inside.is_empty()
            && inside
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return trimmed[..open_idx].trim_end();
        }
    }
    trimmed
}

pub fn clean_model_name(model_raw: &str) -> String {
    let mut s = model_raw.trim().to_string();
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&s) {
        let nested_model = parsed.get("model").and_then(|m| {
            m.as_str().or_else(|| {
                m.get("id")
                    .or_else(|| m.get("modelID"))
                    .or_else(|| m.get("model_id"))
                    .or_else(|| m.get("name"))
                    .and_then(|i| i.as_str())
            })
        });
        if let Some(id_val) = parsed
            .get("id")
            .or_else(|| parsed.get("modelID"))
            .or_else(|| parsed.get("model_id"))
            .and_then(|i| i.as_str())
            .or(nested_model)
            .or_else(|| parsed.get("name").and_then(|i| i.as_str()))
        {
            s = id_val.to_string();
        }
    }
    strip_account_annotation(&s).trim().to_string()
}

pub fn resolve_model_context_limit(model_id: &str) -> i64 {
    let lower = model_id.to_lowercase();
    if lower.contains("qwen") {
        128_000
    } else if lower.contains("claude-3-5")
        || lower.contains("claude-3.5")
        || lower.contains("haiku")
    {
        200_000
    } else {
        1_000_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_opencode_terminal_title_matches_variants() {
        assert!(is_opencode_terminal_title("OC | hudr2"));
        assert!(is_opencode_terminal_title("opencode | fix"));
        assert!(is_opencode_terminal_title("OpenCode"));
        assert!(is_opencode_terminal_title("OC"));
        assert!(!is_opencode_terminal_title("pi - hudr2"));
        assert!(!is_opencode_terminal_title("PowerShell"));
    }

    #[test]
    fn clean_model_name_strips_annotations() {
        assert_eq!(
            clean_model_name("gemini-3.8-flash-high (cuenta4)"),
            "gemini-3.8-flash-high"
        );
        assert_eq!(
            clean_model_name("claude-3-5-sonnet [pool-01]"),
            "claude-3-5-sonnet"
        );
        assert_eq!(clean_model_name("gemini-1.5-pro"), "gemini-1.5-pro");
    }
}
