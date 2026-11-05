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

struct ModelTemplate {
    context_limit: i64,
    output_limit: i64,
    base_name: &'static str,
    reasoning_field: Option<&'static str>,
}

fn get_model_template(key: &str) -> Option<ModelTemplate> {
    match key {
        "claude-opus-4-6-thinking" => Some(ModelTemplate {
            context_limit: 200_000,
            output_limit: 64_000,
            base_name: "Claude Opus 4.6 (thinking)",
            reasoning_field: Some("reasoning_content"),
        }),
        "claude-sonnet-4-6" => Some(ModelTemplate {
            context_limit: 200_000,
            output_limit: 64_000,
            base_name: "Claude Sonnet 4.6",
            reasoning_field: Some("reasoning_content"),
        }),
        "gemini-3.8-flash-high" => Some(ModelTemplate {
            context_limit: 1_048_576,
            output_limit: 65_536,
            base_name: "Gemini 3.8 Flash (high)",
            reasoning_field: Some("reasoning_content"),
        }),
        "gemini-pro-agent" => Some(ModelTemplate {
            context_limit: 1_048_576,
            output_limit: 65_536,
            base_name: "Gemini Pro Agent",
            reasoning_field: Some("reasoning_content"),
        }),
        "gpt-oss-120b-medium" => Some(ModelTemplate {
            context_limit: 131_072,
            output_limit: 128_000,
            base_name: "GPT-OSS 120B (medium)",
            reasoning_field: Some("reasoning_content"),
        }),
        _ => None,
    }
}

pub async fn sync_integrations_native() -> Result<SyncResult, String> {
    // 1. Localizar opencode.json activo
    let candidates = crate::paths::opencode_config_candidates(".");
    let target_path = candidates
        .into_iter()
        .find(|c| c.exists())
        .ok_or_else(|| "No se encontró ningún opencode.json para sincronizar.".to_string())?;

    let content = fs::read_to_string(&target_path)
        .map_err(|e| format!("Error leyendo {}: {e}", target_path.display()))?;

    let mut raw_config: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Error parseando JSON de {}: {e}", target_path.display()))?;

    // 2. Extraer o crear cliproxy provider
    let providers_key = if raw_config.get("providers").is_some() {
        "providers"
    } else if raw_config.get("provider").is_some() {
        "provider"
    } else {
        return Err("No se encontró la sección 'providers' en opencode.json".to_string());
    };

    let providers_obj = raw_config[providers_key]
        .as_object_mut()
        .ok_or_else(|| "La sección de providers no es un objeto JSON válido".to_string())?;

    let cliproxy = providers_obj
        .get_mut("cliproxy")
        .and_then(|c| c.as_object_mut())
        .ok_or_else(|| "No se encontró el provider 'cliproxy' en opencode.json".to_string())?;

    let base_url = cliproxy
        .get("settings")
        .and_then(|s| s.get("baseURL"))
        .and_then(|u| u.as_str())
        .or_else(|| {
            cliproxy
                .get("options")
                .and_then(|o| o.get("baseURL"))
                .and_then(|u| u.as_str())
        })
        .unwrap_or("http://127.0.0.1:8317/v1");

    let api_key = cliproxy
        .get("settings")
        .and_then(|s| s.get("apiKey"))
        .and_then(|k| k.as_str())
        .or_else(|| {
            cliproxy
                .get("options")
                .and_then(|o| o.get("apiKey"))
                .and_then(|k| k.as_str())
        })
        .unwrap_or("");

    // 3. Consultar modelos remotos de Cliproxy
    let models_endpoint = if base_url.ends_with("/models") {
        base_url.to_string()
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
        .map_err(|e| format!("No se pudo conectar con Cliproxy ({models_endpoint}): {e}"))?;

    if !res.status().is_success() {
        return Err(format!("Cliproxy retornó status HTTP {}", res.status()));
    }

    let remote_data: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Error decodificando respuesta de Cliproxy: {e}"))?;

    let remote_models = remote_data
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| "Formato inválido de modelos remotos de Cliproxy".to_string())?;

    // 4. Actualizar modelos existentes
    if !cliproxy.contains_key("models") || !cliproxy["models"].is_object() {
        cliproxy.insert("models".to_string(), json!({}));
    }
    let models_map = cliproxy["models"]
        .as_object_mut()
        .ok_or_else(|| "El campo 'models' no es un objeto válido".to_string())?;

    let mut added_models_count = 0;
    for remote in remote_models {
        if let Some(id) = remote.get("id").and_then(|i| i.as_str())
            && !models_map.contains_key(id)
        {
            let (prefix, base_key) = if id.contains('/') {
                let mut parts = id.splitn(2, '/');
                (parts.next().unwrap_or(""), parts.next().unwrap_or(id))
            } else {
                ("", id)
            };

            if let Some(tpl) = get_model_template(base_key) {
                let name = if !prefix.is_empty() {
                    format!("{} [{}]", tpl.base_name, prefix)
                } else {
                    tpl.base_name.to_string()
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
    }

    // Ordenar alfabéticamente los modelos
    let mut sorted_models = BTreeMap::new();
    for (k, v) in models_map.iter() {
        sorted_models.insert(k.clone(), v.clone());
    }
    cliproxy.insert("models".to_string(), json!(sorted_models));

    // 5. Auditar orquestadores SDD
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
            if agent_name.starts_with("sdd-orchestrator-")
                && let Some(agent_obj) = agent_val.as_object_mut()
            {
                let current_model = agent_obj
                    .get("model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("");
                if current_model.is_empty() {
                    let profile = agent_name.trim_start_matches("sdd-orchestrator-");
                    let target = format!("cliproxy/{profile}/gemini-3.8-flash-high");
                    agent_obj.insert("model".to_string(), json!(target));
                    audited_count += 1;
                }
            }
        }
    }

    // 6. Guardar cambios en el archivo
    let formatted = serde_json::to_string_pretty(&raw_config)
        .map_err(|e| format!("Error formateando JSON: {e}"))?;

    fs::write(&target_path, formatted)
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
