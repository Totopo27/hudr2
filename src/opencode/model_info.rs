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

/// Extrae el nombre o identificador del modelo desde el JSON de un mensaje o evento de OpenCode.
pub fn extract_model_name_from_val(val: &serde_json::Value) -> Option<String> {
    if let Some(m_val) = val.get("model") {
        if let Some(s) = m_val.as_str()
            && !s.trim().is_empty()
        {
            return Some(clean_model_name(s));
        } else if m_val.is_object()
            && let Some(id) = m_val
                .get("id")
                .or_else(|| m_val.get("modelID"))
                .or_else(|| m_val.get("model_id"))
                .and_then(|v| v.as_str())
            && !id.trim().is_empty()
        {
            return Some(clean_model_name(id));
        }
    }
    if let Some(id) = val
        .get("modelID")
        .or_else(|| val.get("model_id"))
        .and_then(|v| v.as_str())
        && !id.trim().is_empty()
    {
        return Some(clean_model_name(id));
    }
    None
}

/// Calcula el costo estimado en dólares ($) basado en los tokens de entrada, salida y caché.
/// Aplica las tarifas oficiales estándar por millón de tokens según la familia de modelo.
pub fn estimate_tokens_cost(
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    cache_read: i64,
    cache_write: i64,
) -> f64 {
    let m = model.to_ascii_lowercase();

    // Tarifas por 1 millón de tokens (input, output, cache_read, cache_write)
    let (in_rate, out_rate, cr_rate, cw_rate) =
        if m.contains("claude-3-opus") || m == "opus" || m.contains("claude-opus-3") {
            // Claude 3 Opus oficial real: $15 / $75 / $1.50 / $18.75
            (15.0, 75.0, 1.50, 18.75)
        } else if m.contains("haiku") {
            // Claude 3 / 3.5 Haiku: $0.80 / $4.00 / $0.08 / $1.00
            (0.80, 4.00, 0.08, 1.00)
        } else if m.contains("sonnet") || m.contains("claude") {
            // Claude 3.5 / 3.7 Sonnet (y perfiles orquestados como claude-opus-5-5): $3.00 / $15.00 / $0.30 / $3.75
            (3.00, 15.00, 0.30, 3.75)
        } else if m.contains("deepseek") || m.contains("deep-seek") {
            if m.contains("r1") || m.contains("reasoner") {
                // DeepSeek R1: $0.55 / $2.19 / $0.14 / $0.55
                (0.55, 2.19, 0.14, 0.55)
            } else {
                // DeepSeek V3: $0.14 / $0.28 / $0.014 / $0.14
                (0.14, 0.28, 0.014, 0.14)
            }
        } else if m.contains("gpt-4o-mini") {
            // GPT-4o-mini: $0.15 / $0.60 / $0.075 / $0.15
            (0.15, 0.60, 0.075, 0.15)
        } else if m.contains("gpt-4o") || m.contains("chatgpt-4o") {
            // GPT-4o: $2.50 / $10.00 / $1.25 / $2.50
            (2.50, 10.00, 1.25, 2.50)
        } else if m.contains("o1") {
            // o1: $15.00 / $60.00 / $7.50 / $15.00
            (15.0, 60.0, 7.50, 15.0)
        } else if m.contains("o3-mini") {
            // o3-mini: $1.10 / $4.40 / $0.55 / $1.10
            (1.10, 4.40, 0.55, 1.10)
        } else if m.contains("flash") {
            // Gemini Flash: $0.10 / $0.40 / $0.025 / $0.10
            (0.10, 0.40, 0.025, 0.10)
        } else if m.contains("gemini") || m.contains("pro") {
            // Gemini Pro: $1.25 / $5.00 / $0.3125 / $1.25
            (1.25, 5.00, 0.3125, 1.25)
        } else if m.contains("kimi") || m.contains("moonshot") {
            // Kimi: $1.00 / $2.00 / $0.20 / $1.00
            (1.00, 2.00, 0.20, 1.00)
        } else {
            // Estándar / default (Sonnet-tier): $3.00 / $15.00 / $0.30 / $3.75
            (3.00, 15.00, 0.30, 3.75)
        };

    let cost_in = (input_tokens.max(0) as f64) * (in_rate / 1_000_000.0);
    let cost_out = (output_tokens.max(0) as f64) * (out_rate / 1_000_000.0);
    let cost_cr = (cache_read.max(0) as f64) * (cr_rate / 1_000_000.0);
    let cost_cw = (cache_write.max(0) as f64) * (cw_rate / 1_000_000.0);

    cost_in + cost_out + cost_cr + cost_cw
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

    #[test]
    fn test_estimate_tokens_cost_various_models() {
        // 1M in + 1M out de Claude 3 Opus real: $15 + $75 = $90
        let opus_cost = estimate_tokens_cost("claude-3-opus", 1_000_000, 1_000_000, 0, 0);
        assert!((opus_cost - 90.0).abs() < 0.001);

        // Perfiles orquestados como claude-opus-5-5 mapean a tarifas Sonnet ($3 + $15 = $18)
        let proxy_opus5_cost = estimate_tokens_cost("claude-opus-5-5", 1_000_000, 1_000_000, 0, 0);
        assert!((proxy_opus5_cost - 18.0).abs() < 0.001);

        // Claude Sonnet: 100k out = $1.50, 10k cache read = $0.003
        let sonnet_cost = estimate_tokens_cost("claude-3-5-sonnet", 0, 100_000, 10_000, 0);
        assert!((sonnet_cost - 1.503).abs() < 0.001);

        // DeepSeek V3: 1M in + 1M out = $0.42
        let ds_cost = estimate_tokens_cost("deepseek-chat", 1_000_000, 1_000_000, 0, 0);
        assert!((ds_cost - 0.42).abs() < 0.001);

        // Turno vacío = $0.0
        assert_eq!(estimate_tokens_cost("claude-opus-5-5", 0, 0, 0, 0), 0.0);
    }
}
