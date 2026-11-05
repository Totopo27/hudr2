//! Adaptador CLIProxyAPI para cuotas y cuentas.
//!
//! Maneja el parseo de credenciales en `resolve_auths_dir()`,
//! la normalización de prefijos (`antigravity-*`, `google-*`, `oauth-*`),
//! y la consulta HTTP a `QUOTA_URL`.

use crate::config::{QUOTA_URL, USER_AGENT, resolve_auths_dir};
use crate::models::{AccountQuota, QuotaPool};
use std::fs;
use std::path::Path;

/// Comprueba si el directorio de autenticación existe y contiene credenciales JSON válidas.
pub fn is_available() -> bool {
    let auths_dir = resolve_auths_dir();
    if !auths_dir.exists() {
        return false;
    }

    let Ok(entries) = fs::read_dir(auths_dir) else {
        return false;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(file_name) = path.file_name().and_then(|f| f.to_str())
            && file_name.ends_with(".json")
            && !file_name.starts_with('.')
            && let Ok(content) = fs::read_to_string(&path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            let has_token_or_email = val.get("access_token").is_some()
                || val.get("email").is_some()
                || val.get("token").is_some()
                || val.get("type").is_some();

            if has_token_or_email {
                return true;
            }
        }
    }

    false
}

/// Descubre y registra cuentas desde los archivos de credenciales de CLIProxy.
pub fn discover_accounts(target_list: &mut Vec<AccountQuota>) {
    let auths_dir = resolve_auths_dir();
    if !auths_dir.exists() {
        target_list.clear();
        return;
    }

    let mut seen_files = std::collections::HashSet::new();

    if let Ok(entries) = fs::read_dir(auths_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|f| f.to_str())
                && file_name.ends_with(".json")
                && !file_name.starts_with('.')
                && let Ok(content) = fs::read_to_string(&path)
                && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
            {
                // Solo considerar archivos con estructura de credenciales
                let has_token_or_email = val.get("access_token").is_some()
                    || val.get("email").is_some()
                    || val.get("token").is_some()
                    || val.get("type").is_some();

                if !has_token_or_email {
                    continue;
                }

                let email = val
                    .get("email")
                    .and_then(|e| e.as_str())
                    .unwrap_or("")
                    .to_string();
                let raw_stem = file_name.trim_end_matches(".json");
                let clean_stem = if let Some(stripped) = raw_stem.strip_prefix("antigravity-") {
                    stripped
                } else if let Some(stripped) = raw_stem.strip_prefix("google-") {
                    stripped
                } else if let Some(stripped) = raw_stem.strip_prefix("oauth-") {
                    stripped
                } else {
                    raw_stem
                };

                let prefix = val
                    .get("prefix")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        if !clean_stem.is_empty() {
                            clean_stem.to_string()
                        } else if !email.is_empty() {
                            email.split('@').next().unwrap_or("acc").to_string()
                        } else {
                            "acc".to_string()
                        }
                    });

                let path_str = path.to_string_lossy().to_string();
                seen_files.insert(path_str.clone());
                if let Some(existing) = target_list
                    .iter_mut()
                    .find(|a| a.auth_file == path_str || a.prefix == prefix)
                {
                    existing.prefix = prefix;
                    existing.email = email;
                    existing.auth_file = path_str;
                } else {
                    target_list.push(AccountQuota {
                        prefix,
                        email,
                        auth_file: path_str,
                        rem_5h: 100,
                        reset_5h: "4h 59m".to_string(),
                        rem_wk: 100,
                        reset_wk: "4d 23h".to_string(),
                        is_critical: false,
                        is_warning: false,
                        has_error: false,
                        error_msg: String::new(),
                        pools: vec![
                            QuotaPool {
                                label: "Gemini Wk".to_string(),
                                percent: 100,
                                reset: "4d 23h".to_string(),
                                pace: "~0%".to_string(),
                            },
                            QuotaPool {
                                label: "Gemini 5h".to_string(),
                                percent: 100,
                                reset: "4h 59m".to_string(),
                                pace: "~0%".to_string(),
                            },
                            QuotaPool {
                                label: "Claude Wk".to_string(),
                                percent: 100,
                                reset: "6d 23h".to_string(),
                                pace: "~0%".to_string(),
                            },
                            QuotaPool {
                                label: "Claude 5h".to_string(),
                                percent: 100,
                                reset: "4h 59m".to_string(),
                                pace: "~0%".to_string(),
                            },
                        ],
                    });
                }
            }
        }
    }

    // Purgar cuentas huérfanas cuyos archivos JSON fueron eliminados del directorio auths
    target_list.retain(|a| seen_files.contains(&a.auth_file));
    target_list.sort_by(|a, b| a.prefix.cmp(&b.prefix));
}

/// Consulta el endpoint de cuotas de CLIProxy para la cuenta especificada.
pub async fn fetch_quota_for_account(acc: &mut AccountQuota) {
    if !Path::new(&acc.auth_file).exists() {
        return;
    }

    let raw = match fs::read_to_string(&acc.auth_file) {
        Ok(r) => r,
        Err(_) => return,
    };

    let val: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return,
    };

    let token = match val.get("access_token").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => return,
    };

    let project_id = val.get("project_id").and_then(|p| p.as_str()).unwrap_or("");

    let client = &*crate::config::HTTP_CLIENT;

    let body = serde_json::json!({ "project": project_id });
    if let Ok(res) = client
        .post(QUOTA_URL)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", USER_AGENT)
        .json(&body)
        .send()
        .await
    {
        if res.status().is_success() {
            if let Ok(data) = res.json::<serde_json::Value>().await {
                apply_quota_data(acc, &data);
            }
        } else {
            let status = res.status().as_u16();
            let err_val = res
                .json::<serde_json::Value>()
                .await
                .unwrap_or(serde_json::Value::Null);
            apply_quota_error(acc, status, &err_val);
        }
    }
}

/// Registra estados de error HTTP (401 expirado, 403 TOS/verificación, 429 rate limit) en la cuenta.
pub fn apply_quota_error(acc: &mut AccountQuota, status: u16, err_val: &serde_json::Value) {
    acc.has_error = true;
    let msg = err_val
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    let err_status = err_val
        .get("error")
        .and_then(|e| e.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or("");
    let has_reason = |target: &str| -> bool {
        err_val
            .get("error")
            .and_then(|e| e.get("details"))
            .and_then(|d| d.as_array())
            .is_some_and(|arr| {
                arr.iter()
                    .any(|item| item.get("reason").and_then(|r| r.as_str()) == Some(target))
            })
    };

    if status == 401 || msg.contains("expired") || err_status == "UNAUTHENTICATED" {
        acc.error_msg = "token expirado".to_string();
    } else if has_reason("TOS_VIOLATION") || msg.contains("Terms of Service") {
        acc.error_msg = "tos suspendida".to_string();
    } else if has_reason("VALIDATION_REQUIRED") || msg.contains("Verify your account") {
        acc.error_msg = "verif. requerida".to_string();
    } else if status == 429 || err_status == "RESOURCE_EXHAUSTED" {
        acc.error_msg = "rate limited".to_string();
    } else {
        acc.error_msg = format!("err {status}");
    }
}

/// Aplica los grupos y buckets recibidos de CLIProxy a la estructura `AccountQuota`.
pub fn apply_quota_data(acc: &mut AccountQuota, data: &serde_json::Value) {
    acc.has_error = false;
    acc.error_msg.clear();

    if let Some(groups) = data.get("groups").and_then(|g| g.as_array()) {
        let mut gem_5h = 100;
        let mut gem_wk = 100;
        let mut gem_5h_reset = "4h 59m".to_string();
        let mut gem_wk_reset = "4d 23h".to_string();
        let mut gem_5h_iso: Option<String> = None;
        let mut gem_wk_iso: Option<String> = None;
        let mut cla_5h = 100;
        let mut cla_wk = 100;
        let mut cla_5h_reset = "4h 59m".to_string();
        let mut cla_wk_reset = "6d 23h".to_string();
        let mut cla_5h_iso: Option<String> = None;
        let mut cla_wk_iso: Option<String> = None;

        for grp in groups {
            let name = grp
                .get("displayName")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_lowercase();
            if let Some(buckets) = grp.get("buckets").and_then(|b| b.as_array()) {
                for b in buckets {
                    let window = b.get("window").and_then(|w| w.as_str()).unwrap_or("");
                    let frac = b
                        .get("remainingFraction")
                        .and_then(|f| f.as_f64())
                        .unwrap_or(1.0);
                    let pct = (frac * 100.0).clamp(0.0, 100.0).round() as u8;
                    let reset_time = b.get("resetTime").and_then(|r| r.as_str());

                    if name.contains("gemini") {
                        if window == "5h" || window.contains('5') {
                            gem_5h = pct;
                            if let Some(rt) = reset_time {
                                gem_5h_reset = format_relative_reset(rt);
                                gem_5h_iso = Some(rt.to_string());
                            }
                        } else if window == "weekly" || window.contains("week") {
                            gem_wk = pct;
                            if let Some(rt) = reset_time {
                                gem_wk_reset = format_relative_reset(rt);
                                gem_wk_iso = Some(rt.to_string());
                            }
                        }
                    } else if name.contains("claude") || name.contains("gpt") {
                        if window == "5h" || window.contains('5') {
                            cla_5h = pct;
                            if let Some(rt) = reset_time {
                                cla_5h_reset = format_relative_reset(rt);
                                cla_5h_iso = Some(rt.to_string());
                            }
                        } else if window == "weekly" || window.contains("week") {
                            cla_wk = pct;
                            if let Some(rt) = reset_time {
                                cla_wk_reset = format_relative_reset(rt);
                                cla_wk_iso = Some(rt.to_string());
                            }
                        }
                    }
                }
            }
        }

        acc.rem_5h = gem_5h;
        acc.rem_wk = gem_wk;
        acc.reset_5h = gem_5h_reset.clone();
        acc.reset_wk = gem_wk_reset.clone();

        // Gemini es la métrica soberana para salud y alertas de migración.
        let worst_used =
            std::cmp::max(100u8.saturating_sub(gem_5h), 100u8.saturating_sub(gem_wk));
        acc.is_critical = worst_used >= 85;
        acc.is_warning = (70..85).contains(&worst_used);

        let gem_wk_pace = calculate_quota_pace(gem_wk, true, gem_wk_iso.as_deref());
        let gem_5h_pace = calculate_quota_pace(gem_5h, false, gem_5h_iso.as_deref());
        let cla_wk_pace = calculate_quota_pace(cla_wk, true, cla_wk_iso.as_deref());
        let cla_5h_pace = calculate_quota_pace(cla_5h, false, cla_5h_iso.as_deref());

        acc.pools = vec![
            QuotaPool {
                label: "Gemini Wk".to_string(),
                percent: gem_wk,
                reset: gem_wk_reset,
                pace: gem_wk_pace,
            },
            QuotaPool {
                label: "Gemini 5h".to_string(),
                percent: gem_5h,
                reset: gem_5h_reset,
                pace: gem_5h_pace,
            },
            QuotaPool {
                label: "Claude Wk".to_string(),
                percent: cla_wk,
                reset: cla_wk_reset,
                pace: cla_wk_pace,
            },
            QuotaPool {
                label: "Claude 5h".to_string(),
                percent: cla_5h,
                reset: cla_5h_reset,
                pace: cla_5h_pace,
            },
        ];
    }
}

/// Calcula el ritmo de consumo (pacing) comparando el % usado frente al % de tiempo transcurrido en la ventana.
pub fn calculate_quota_pace(rem_percent: u8, is_weekly: bool, iso_time: Option<&str>) -> String {
    let Some(iso) = iso_time else {
        return "~0%".to_string();
    };
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return "~0%".to_string();
    };
    let now = chrono::Utc::now();
    let remaining_secs = (dt.with_timezone(&chrono::Utc) - now).num_seconds();
    let window_secs: i64 = if is_weekly { 7 * 24 * 3600 } else { 5 * 3600 };
    if remaining_secs <= 0 || remaining_secs > window_secs {
        return "~0%".to_string();
    }
    let elapsed_secs = window_secs - remaining_secs;
    let elapsed_pct = (elapsed_secs as f64 / window_secs as f64) * 100.0;
    let used_pct = 100.0 - f64::from(rem_percent);
    let pace_diff = (used_pct - elapsed_pct).round() as i32;
    if pace_diff > 5 {
        format!("↓{pace_diff}%")
    } else if pace_diff < -5 {
        format!("↑{}%", pace_diff.abs())
    } else {
        "~0%".to_string()
    }
}

/// Formatea marcas temporales ISO en expresiones relativas breves (ej. `4h 59m`, `4d 23h`).
pub fn format_relative_reset(iso_time: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(iso_time) {
        let now = chrono::Utc::now();
        let dur = dt.with_timezone(&chrono::Utc) - now;
        let total_mins = dur.num_minutes();
        if total_mins <= 0 {
            return "reseteando…".to_string();
        }
        if total_mins < 60 {
            return format!("{}m", total_mins);
        }
        let hours = total_mins / 60;
        let mins = total_mins % 60;
        if hours < 24 {
            return if mins > 0 {
                format!("{}h {}m", hours, mins)
            } else {
                format!("{}h", hours)
            };
        }
        let days = hours / 24;
        let rem_hours = hours % 24;
        if rem_hours > 0 {
            format!("{}d {}h", days, rem_hours)
        } else {
            format!("{}d", days)
        }
    } else {
        "4h 30m".to_string()
    }
}
