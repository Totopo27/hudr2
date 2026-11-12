use serde_json::json;
use std::collections::BTreeMap;
use std::fs;

#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub total_models: usize,
    pub added_models: usize,
    pub audited_orchestrators: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelTemplate {
    context_limit: i64,
    output_limit: i64,
    base_name: String,
    reasoning_field: Option<&'static str>,
}

fn humanize_model_key(base_key: &str) -> String {
    base_key
        .split(['-', '_'])
        .filter(|seg| !seg.is_empty())
        .map(|seg| match seg.to_ascii_lowercase().as_str() {
            "gpt" => "GPT".to_string(),
            "oss" => "OSS".to_string(),
            "r1" => "R1".to_string(),
            "v3" => "V3".to_string(),
            "thinking" | "high" | "medium" | "low" => format!("({})", seg.to_ascii_lowercase()),
            _ => {
                let mut chars = seg.chars();
                match chars.next() {
                    Some(first) if first.is_ascii_alphabetic() => {
                        let mut out = String::with_capacity(seg.len());
                        out.push(first.to_ascii_uppercase());
                        out.push_str(chars.as_str());
                        out
                    }
                    _ => seg.to_string(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn resolve_sync_model_template(key: &str) -> ModelTemplate {
    match key {
        "claude-opus-4-6-thinking" => ModelTemplate {
            context_limit: 200_000,
            output_limit: 64_000,
            base_name: "Claude Opus 4.6 (thinking)".to_string(),
            reasoning_field: Some("reasoning_content"),
        },
        "claude-sonnet-4-6" => ModelTemplate {
            context_limit: 200_000,
            output_limit: 64_000,
            base_name: "Claude Sonnet 4.6".to_string(),
            reasoning_field: Some("reasoning_content"),
        },
        "gemini-3.8-flash-high" => ModelTemplate {
            context_limit: 1_048_576,
            output_limit: 65_536,
            base_name: "Gemini 3.8 Flash (high)".to_string(),
            reasoning_field: Some("reasoning_content"),
        },
        "gemini-pro-agent" => ModelTemplate {
            context_limit: 1_048_576,
            output_limit: 65_536,
            base_name: "Gemini Pro Agent".to_string(),
            reasoning_field: Some("reasoning_content"),
        },
        "gpt-oss-120b-medium" => ModelTemplate {
            context_limit: 131_072,
            output_limit: 128_000,
            base_name: "GPT-OSS 120B (medium)".to_string(),
            reasoning_field: Some("reasoning_content"),
        },
        _ => {
            let lower = key.to_ascii_lowercase();
            let context_limit = if lower.contains("claude") {
                200_000
            } else if lower.contains("deepseek") || lower.contains("gpt-oss") {
                131_072
            } else if lower.contains("gemini") || lower.contains("argon") {
                1_048_576
            } else {
                crate::opencode::resolve_model_context_limit(key)
            };

            let output_limit = if lower.contains("gemini") || lower.contains("argon") {
                65_536
            } else if lower.contains("claude") || lower.contains("deepseek") {
                64_000
            } else if lower.contains("gpt-oss") {
                128_000
            } else {
                32_768
            };

            let reasoning_field = if lower.contains("thinking")
                || lower.contains("reasoner")
                || lower.contains("deepseek-r1")
                || lower.contains("gemini")
                || lower.contains("argon")
                || lower.contains("gpt-oss")
            {
                Some("reasoning_content")
            } else {
                None
            };

            ModelTemplate {
                context_limit,
                output_limit,
                base_name: humanize_model_key(key),
                reasoning_field,
            }
        }
    }
}

pub fn detect_proxy_provider_key(
    providers_obj: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    for preferred in ["cpamc", "cliproxy"] {
        if providers_obj.get(preferred).is_some_and(|v| v.is_object()) {
            return Some(preferred.to_string());
        }
    }

    for (key, val) in providers_obj {
        let Some(obj) = val.as_object() else {
            continue;
        };
        let base_url = obj
            .get("settings")
            .and_then(|s| s.get("baseURL"))
            .and_then(|u| u.as_str())
            .or_else(|| {
                obj.get("options")
                    .and_then(|o| o.get("baseURL"))
                    .and_then(|u| u.as_str())
            })
            .or_else(|| obj.get("baseURL").and_then(|u| u.as_str()))
            .unwrap_or("");
        if base_url.contains("127.0.0.1:8317") || base_url.contains("localhost:8317") {
            return Some(key.clone());
        }
    }

    None
}

/// Detecta el nombre del provider de proxy local configurado para el directorio indicado (`target_dir`).
pub fn detect_configured_proxy_provider_for(target_dir: &str) -> String {
    for cfg_path in crate::paths::opencode_config_candidates(target_dir) {
        if cfg_path.exists()
            && let Ok(content) = fs::read_to_string(&cfg_path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            for section in ["providers", "provider"] {
                if let Some(map) = val.get(section).and_then(|v| v.as_object())
                    && let Some(found) = detect_proxy_provider_key(map)
                {
                    return found;
                }
            }
        }
    }
    "cliproxy".to_string()
}

fn resolve_default_orchestrator_model<V>(
    provider_key: &str,
    profile: &str,
    sorted_models: &BTreeMap<String, V>,
) -> String {
    let preferred = format!("{profile}/gemini-3.8-flash-high");
    if sorted_models.contains_key(&preferred) {
        return format!("{provider_key}/{preferred}");
    }

    let profile_prefix = format!("{profile}/");
    if let Some(gemini_key) = sorted_models.keys().find(|k| {
        k.starts_with(&profile_prefix)
            && k[profile_prefix.len()..]
                .to_ascii_lowercase()
                .contains("gemini")
    }) {
        return format!("{provider_key}/{gemini_key}");
    }

    if let Some(any_key) = sorted_models.keys().find(|k| k.starts_with(&profile_prefix)) {
        return format!("{provider_key}/{any_key}");
    }

    format!("{provider_key}/{preferred}")
}

fn extract_orchestrator_profile(agent_name: &str) -> Option<&str> {
    for prefix in ["sdd-orchestrator-", "gentle-orchestrator-"] {
        if agent_name.len() > prefix.len()
            && agent_name
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return Some(&agent_name[prefix.len()..]);
        }
    }
    None
}

pub async fn sync_integrations_native_for(target_dir: &str) -> Result<SyncResult, String> {
    let candidates = crate::paths::opencode_config_candidates(target_dir);
    let target_path = candidates
        .into_iter()
        .find(|c| c.exists())
        .ok_or_else(|| "No se encontró ningún opencode.json para sincronizar.".to_string())?;
    sync_integrations_for_path(&target_path).await
}

pub async fn sync_integrations_for_path(
    target_path: &std::path::Path,
) -> Result<SyncResult, String> {
    let content = fs::read_to_string(target_path)
        .map_err(|e| format!("Error leyendo {}: {e}", target_path.display()))?;

    let mut raw_config: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Error parseando JSON de {}: {e}", target_path.display()))?;

    // 2. Extraer provider activo (`cpamc`, `cliproxy` o proxy local en :8317)
    let providers_key = if raw_config.get("providers").is_some() {
        "providers"
    } else if raw_config.get("provider").is_some() {
        "provider"
    } else {
        return Err("No se encontró la sección 'provider'/'providers' en opencode.json".to_string());
    };

    let providers_obj = raw_config[providers_key]
        .as_object_mut()
        .ok_or_else(|| "La sección de providers no es un objeto JSON válido".to_string())?;

    let provider_name = detect_proxy_provider_key(providers_obj).ok_or_else(|| {
        "No se encontró el provider 'cpamc' ni 'cliproxy' en opencode.json".to_string()
    })?;

    let proxy_provider = providers_obj
        .get_mut(&provider_name)
        .and_then(|c| c.as_object_mut())
        .ok_or_else(|| format!("El provider '{provider_name}' no es un objeto válido"))?;

    let base_url = proxy_provider
        .get("settings")
        .and_then(|s| s.get("baseURL"))
        .and_then(|u| u.as_str())
        .or_else(|| {
            proxy_provider
                .get("options")
                .and_then(|o| o.get("baseURL"))
                .and_then(|u| u.as_str())
        })
        .or_else(|| proxy_provider.get("baseURL").and_then(|u| u.as_str()))
        .unwrap_or("http://127.0.0.1:8317/v1")
        .to_string();

    let api_key = proxy_provider
        .get("settings")
        .and_then(|s| s.get("apiKey"))
        .and_then(|k| k.as_str())
        .or_else(|| {
            proxy_provider
                .get("options")
                .and_then(|o| o.get("apiKey"))
                .and_then(|k| k.as_str())
        })
        .or_else(|| proxy_provider.get("apiKey").and_then(|k| k.as_str()))
        .unwrap_or("")
        .to_string();

    // 3. Consultar modelos remotos del proxy local
    let models_endpoint = if base_url.ends_with("/models") {
        base_url
    } else {
        format!("{}/models", base_url.trim_end_matches('/'))
    };

    let client = &*crate::config::HTTP_CLIENT;

    let mut req = client.get(&models_endpoint);
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {api_key}"));
    }

    let res = req
        .send()
        .await
        .map_err(|e| format!("No se pudo conectar con {provider_name} ({models_endpoint}): {e}"))?;

    if !res.status().is_success() {
        return Err(format!("{provider_name} retornó status HTTP {}", res.status()));
    }

    let remote_data: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Error decodificando respuesta de {provider_name}: {e}"))?;

    let remote_models = remote_data
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| format!("Formato inválido de modelos remotos de {provider_name}"))?;

    // 4. Actualizar modelos existentes
    if !proxy_provider.contains_key("models") || !proxy_provider["models"].is_object() {
        proxy_provider.insert("models".to_string(), json!({}));
    }
    let models_map = proxy_provider["models"]
        .as_object_mut()
        .ok_or_else(|| "El campo 'models' no es un objeto válido".to_string())?;

    let mut added_models_count = 0;
    for remote in remote_models {
        if let Some(id) = remote.get("id").and_then(|i| i.as_str())
            && !id.trim().is_empty()
            && !models_map.contains_key(id)
        {
            let (prefix, base_key) = id.split_once('/').unwrap_or(("", id));
            if base_key.trim().is_empty() {
                continue;
            }

            let tpl = resolve_sync_model_template(base_key);
            let name = if !prefix.is_empty() {
                format!("{} [{}]", tpl.base_name, prefix)
            } else {
                tpl.base_name
            };

            let mut entry = json!({
                "name": name,
                "limit": {
                    "context": tpl.context_limit,
                    "output": tpl.output_limit
                },
                "capabilities": {
                    "input": ["text", "image"],
                    "output": ["text"],
                    "tools": true
                }
            });

            if let Some(rf) = tpl.reasoning_field {
                entry["compatibility"] = json!({ "reasoningField": rf });
            }

            models_map.insert(id.to_string(), entry);
            added_models_count += 1;
        }
    }

    // Ordenar alfabéticamente los modelos
    let mut sorted_models = BTreeMap::new();
    for (k, v) in models_map.iter() {
        sorted_models.insert(k.clone(), v.clone());
    }
    proxy_provider.insert("models".to_string(), json!(sorted_models));

    // 5. Auditar orquestadores SDD y Gentle-Orchestrator
    let mut audited_count = 0;
    let agents_key = if raw_config.get("agents").is_some() {
        Some("agents")
    } else if raw_config.get("agent").is_some() {
        Some("agent")
    } else {
        None
    };

    if let Some(key) = agents_key
        && let Some(agents) = raw_config[key].as_object_mut()
    {
        for (agent_name, agent_val) in agents.iter_mut() {
            if let Some(profile) = extract_orchestrator_profile(agent_name)
                && let Some(agent_obj) = agent_val.as_object_mut()
            {
                let current_model = agent_obj
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("");
                if current_model.is_empty() {
                    let target =
                        resolve_default_orchestrator_model(&provider_name, profile, &sorted_models);
                    agent_obj.insert("model".to_string(), json!(target));
                    audited_count += 1;
                }
            }
        }
    }

    // 6. Guardar cambios de forma atómica (.{pid}.tmp + rename)
    let formatted = serde_json::to_string_pretty(&raw_config)
        .map_err(|e| format!("Error formateando JSON: {e}"))?;

    crate::paths::atomic_write_bytes(target_path, formatted.as_bytes())
        .map_err(|e| format!("Error guardando {}: {e}", target_path.display()))?;

    Ok(SyncResult {
        total_models: sorted_models.len(),
        added_models: added_models_count,
        audited_orchestrators: audited_count,
        message: format!(
            "Sincronizados {} modelos (+{} nuevos), {} orquestadores auditados",
            sorted_models.len(),
            added_models_count,
            audited_count
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_sync_model_template_preserves_presets_and_synthesizes_dynamic_models() {
        let preset = resolve_sync_model_template("claude-opus-4-6-thinking");
        assert_eq!(preset.base_name, "Claude Opus 4.6 (thinking)");
        assert_eq!(preset.context_limit, 200_000);
        assert_eq!(preset.output_limit, 64_000);
        assert_eq!(preset.reasoning_field, Some("reasoning_content"));

        let claude_dyn = resolve_sync_model_template("claude-3.7-sonnet");
        assert_eq!(claude_dyn.base_name, "Claude 3.7 Sonnet");
        assert_eq!(claude_dyn.context_limit, 200_000);
        assert_eq!(claude_dyn.output_limit, 64_000);
        assert_eq!(claude_dyn.reasoning_field, None);

        let deepseek_dyn = resolve_sync_model_template("deepseek-r1");
        assert_eq!(deepseek_dyn.base_name, "Deepseek R1");
        assert_eq!(deepseek_dyn.context_limit, 131_072);
        assert_eq!(deepseek_dyn.reasoning_field, Some("reasoning_content"));

        let gemini_dyn = resolve_sync_model_template("gemini-2.5-pro");
        assert_eq!(gemini_dyn.base_name, "Gemini 2.5 Pro");
        assert_eq!(gemini_dyn.context_limit, 1_048_576);
        assert_eq!(gemini_dyn.output_limit, 65_536);

        let qwen_dyn = resolve_sync_model_template("qwen-2.5-coder-32b");
        assert_eq!(qwen_dyn.context_limit, 128_000);
    }

    #[test]
    fn resolve_default_orchestrator_model_prefers_synced_profile_models() {
        let mut models = BTreeMap::new();
        models.insert("agy1/claude-sonnet-4-6".to_string(), json!({}));
        models.insert("agy1/gemini-2.5-pro".to_string(), json!({}));
        models.insert("agy2/claude-sonnet-4-6".to_string(), json!({}));

        assert_eq!(
            resolve_default_orchestrator_model("cliproxy", "agy1", &models),
            "cliproxy/agy1/gemini-2.5-pro"
        );
        assert_eq!(
            resolve_default_orchestrator_model("cpamc", "agy2", &models),
            "cpamc/agy2/claude-sonnet-4-6"
        );
        assert_eq!(
            resolve_default_orchestrator_model("cpamc", "agy3", &models),
            "cpamc/agy3/gemini-3.8-flash-high"
        );
    }
}
