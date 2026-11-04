use crate::models::HerdrPane;
use std::process::Command;

/// Deep module para interactuar con Herdr / Telar.
/// Prioriza la llamada socket o CLI `herdr pane list` / `herdr pane current`.
pub struct HerdrClient;

impl HerdrClient {
    pub fn get_panes() -> Vec<HerdrPane> {
        let mut panes = Vec::new();

        // 1. Intentar vía CLI 'herdr pane list'
        if let Ok(output) = Command::new("herdr").args(["pane", "list"]).output()
            && output.status.success()
            && let Ok(raw) = String::from_utf8(output.stdout)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
            && let Some(arr) = val
                .get("result")
                .and_then(|r| r.get("panes"))
                .and_then(|p| p.as_array())
        {
            for item in arr {
                let pane_id = item
                    .get("pane_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let tab_id = item
                    .get("tab_id")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                let workspace_id = item
                    .get("workspace_id")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                let cwd = item
                    .get("cwd")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                let terminal_title = item
                    .get("terminal_title")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                let terminal_title_stripped = item
                    .get("terminal_title_stripped")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                let focused = item
                    .get("focused")
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                let agent_status = item
                    .get("agent_status")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());
                let agent = item
                    .get("agent")
                    .and_then(|a| a.as_str())
                    .map(|s| s.to_string());
                let agent_session_id = item
                    .get("agent_session")
                    .and_then(|s| s.get("value"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if !pane_id.is_empty() {
                    panes.push(HerdrPane {
                        pane_id,
                        tab_id,
                        workspace_id,
                        cwd,
                        terminal_title,
                        terminal_title_stripped,
                        focused,
                        agent_status,
                        agent,
                        agent_session_id,
                    });
                }
            }
        }

        panes
    }

    pub fn get_current_pane_and_tab(panes: &[HerdrPane]) -> (Option<String>, Option<String>) {
        let env_pane_id = std::env::var("HERDR_PANE_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(ref my_pid) = env_pane_id {
            let my_tab = panes
                .iter()
                .find(|p| p.pane_id == *my_pid)
                .and_then(|p| p.tab_id.clone());
            return (Some(my_pid.clone()), my_tab);
        }

        if let Ok(output) = Command::new("herdr").args(["pane", "current"]).output()
            && output.status.success()
            && let Ok(raw) = String::from_utf8(output.stdout)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
        {
            let cur_pane = val.get("result").and_then(|r| r.get("pane"));
            let p_id = cur_pane
                .and_then(|p| p.get("pane_id"))
                .and_then(|id| id.as_str())
                .map(|s| s.to_string());
            let t_id = cur_pane
                .and_then(|p| p.get("tab_id"))
                .and_then(|id| id.as_str())
                .map(|s| s.to_string());
            return (p_id, t_id);
        }

        // Si no se pudo obtener explícitamente, buscar el pane con focused == true
        if let Some(focused) = panes.iter().find(|p| p.focused) {
            return (Some(focused.pane_id.clone()), focused.tab_id.clone());
        }

        (None, None)
    }

    pub fn get_current_pane_id() -> Option<String> {
        if let Ok(val) = std::env::var("HERDR_PANE_ID")
            && !val.trim().is_empty()
        {
            return Some(val.trim().to_string());
        }

        if let Ok(output) = Command::new("herdr").args(["pane", "current"]).output()
            && output.status.success()
            && let Ok(raw) = String::from_utf8(output.stdout)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
        {
            return val
                .get("result")
                .and_then(|r| r.get("pane"))
                .and_then(|p| p.get("pane_id"))
                .and_then(|id| id.as_str())
                .map(|s| s.to_string());
        }

        None
    }

    pub fn close_pane(pane_id: &str) -> bool {
        Command::new("herdr")
            .args(["pane", "close", pane_id])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn close_workspace(workspace_id: &str) -> bool {
        Command::new("herdr")
            .args(["workspace", "close", workspace_id])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
