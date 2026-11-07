//! Normalización de nombres de modelo, títulos de terminal y límites de contexto.

use regex::Regex;
use std::sync::LazyLock;

static RE_OC_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:OC|opencode)\s*\|\s*").unwrap());

pub fn clean_opencode_terminal_title(title: &str) -> String {
    RE_OC_TITLE.replace(title, "").trim().to_string()
}

pub fn is_opencode_terminal_title(title: &str) -> bool {
    let trimmed = title.trim();
    trimmed.eq_ignore_ascii_case("opencode")
        || trimmed.eq_ignore_ascii_case("oc")
        || RE_OC_TITLE.is_match(trimmed)
}

static RE_ACCOUNT_ANNOTATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s*[\(\[][a-zA-Z0-9_\-]+[\)\]]\s*$").unwrap());

pub fn clean_model_name(model_raw: &str) -> String {
    let mut s = model_raw.trim().to_string();
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&s)
        && let Some(id_val) = parsed
            .get("id")
            .or_else(|| parsed.get("modelID"))
            .or_else(|| parsed.get("model_id"))
            .or_else(|| parsed.get("model"))
            .or_else(|| parsed.get("name"))
            .and_then(|i| i.as_str())
    {
        s = id_val.to_string();
    }
    RE_ACCOUNT_ANNOTATION.replace(&s, "").trim().to_string()
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
}
