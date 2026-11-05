//! Normalización de nombres de modelo, títulos de terminal y límites de contexto.

use regex::Regex;
use std::sync::LazyLock;

static RE_OC_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:OC|opencode)\s*\|\s*").unwrap());

pub fn clean_opencode_terminal_title(title: &str) -> String {
    RE_OC_TITLE.replace(title, "").trim().to_string()
}

pub fn clean_model_name(model_raw: &str) -> String {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(model_raw)
        && let Some(id_val) = parsed.get("id").and_then(|i| i.as_str())
    {
        return id_val.to_string();
    }
    model_raw.to_string()
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
