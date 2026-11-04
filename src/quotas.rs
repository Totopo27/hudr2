use crate::config::{QUOTA_URL, USER_AGENT, resolve_auths_dir};
use crate::models::{AccountQuota, QuotaPool};
use std::fs;
use std::path::Path;

pub struct QuotaCollector;

impl QuotaCollector {
    pub fn discover_accounts(target_list: &mut Vec<AccountQuota>) {
        let auths_dir = resolve_auths_dir();
        if !auths_dir.exists() {
            return;
        }

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
                    if let Some(existing) = target_list
                        .iter_mut()
                        .find(|a| a.auth_file == path_str || a.prefix == prefix)
                    {
                        existing.prefix = prefix;
                        existing.email = email;
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

        target_list.sort_by(|a, b| a.prefix.cmp(&b.prefix));
    }

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

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(6))
            .build();

        if let Ok(client) = client {
            let body = serde_json::json!({ "project": project_id });
            if let Ok(res) = client
                .post(QUOTA_URL)
                .header("Authorization", format!("Bearer {}", token))
                .header("User-Agent", USER_AGENT)
                .json(&body)
                .send()
                .await
                && res.status().is_success()
                && let Ok(data) = res.json::<serde_json::Value>().await
            {
                acc.has_error = false;
                acc.error_msg.clear();

                if let Some(groups) = data.get("groups").and_then(|g| g.as_array()) {
                    let mut gem_5h = 100;
                    let mut gem_wk = 100;
                    let mut gem_5h_reset = "4h 59m".to_string();
                    let mut gem_wk_reset = "4d 23h".to_string();
                    let mut cla_5h = 100;
                    let mut cla_wk = 100;
                    let mut cla_5h_reset = "4h 59m".to_string();
                    let mut cla_wk_reset = "6d 23h".to_string();

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
                                        }
                                    } else if window == "weekly" || window.contains("week") {
                                        gem_wk = pct;
                                        if let Some(rt) = reset_time {
                                            gem_wk_reset = format_relative_reset(rt);
                                        }
                                    }
                                } else if name.contains("claude") || name.contains("gpt") {
                                    if window == "5h" || window.contains('5') {
                                        cla_5h = pct;
                                        if let Some(rt) = reset_time {
                                            cla_5h_reset = format_relative_reset(rt);
                                        }
                                    } else if window == "weekly" || window.contains("week") {
                                        cla_wk = pct;
                                        if let Some(rt) = reset_time {
                                            cla_wk_reset = format_relative_reset(rt);
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

                    let worst_used = std::cmp::max(
                        std::cmp::max(100u8.saturating_sub(gem_5h), 100u8.saturating_sub(gem_wk)),
                        std::cmp::max(100u8.saturating_sub(cla_5h), 100u8.saturating_sub(cla_wk)),
                    );
                    acc.is_critical = worst_used >= 85;
                    acc.is_warning = (70..85).contains(&worst_used);

                    acc.pools = vec![
                        QuotaPool {
                            label: "Gemini Wk".to_string(),
                            percent: gem_wk,
                            reset: gem_wk_reset,
                            pace: "↑19%".to_string(),
                        },
                        QuotaPool {
                            label: "Gemini 5h".to_string(),
                            percent: gem_5h,
                            reset: gem_5h_reset,
                            pace: "↑8%".to_string(),
                        },
                        QuotaPool {
                            label: "Claude Wk".to_string(),
                            percent: cla_wk,
                            reset: cla_wk_reset,
                            pace: "~0%".to_string(),
                        },
                        QuotaPool {
                            label: "Claude 5h".to_string(),
                            percent: cla_5h,
                            reset: cla_5h_reset,
                            pace: "~0%".to_string(),
                        },
                    ];
                }
            }
        }
    }
}

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
        return if rem_hours > 0 {
            format!("{}d {}h", days, rem_hours)
        } else {
            format!("{}d", days)
        };
    }
    "4h 30m".to_string()
}
