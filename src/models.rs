use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HerdrPane {
    pub pane_id: String,
    pub tab_id: Option<String>,
    pub workspace_id: Option<String>,
    pub cwd: Option<String>,
    pub terminal_title: Option<String>,
    pub terminal_title_stripped: Option<String>,
    pub focused: bool,
    pub agent_status: Option<String>,
    pub agent: Option<String>,
    pub agent_session_id: Option<String>,
    #[serde(default)]
    pub tokens: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AttributedChanges {
    pub files_count: usize,
    pub additions: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionTodo {
    pub title: String,
    pub status: String, // "pending" | "in_progress" | "done"
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AgentSession {
    pub provider_name: String, // "opencode", "pi", "gentle-shell"
    pub agent: String,
    pub model: String,
    pub model_short: String,
    pub thinking_level: Option<String>,
    pub local_prefix: Option<String>,
    pub tokens_input: i64,
    pub tokens_output: i64,
    pub active_turn_tokens: i64,
    pub context_limit: i64,
    pub cost: f64,
    pub tool_reads: usize,
    pub tool_writes: usize,
    pub tool_bash: usize,
    pub tool_engram: usize,
    pub tool_total: usize,
    pub agent_status: String, // "idle" | "working"
    pub all_active_prefixes: HashSet<String>,
    pub attributed_changes: AttributedChanges,
    pub todos: Vec<SessionTodo>,
}

impl AgentSession {
    /// Devuelve el identificador del modelo activo (`model` si no está vacío, sino `model_short`).
    pub fn effective_model(&self) -> &str {
        if !self.model.is_empty() {
            &self.model
        } else {
            &self.model_short
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HandoffSummary {
    pub id: i64,
    pub created_at_epoch: Option<i64>,
    pub relative_time: String,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngramTelemetry {
    pub project: String,
    pub online: bool,
    pub daemon_online: bool,
    pub obs_count: usize,
    pub enrolled: bool,
    pub sync_status: Option<String>,
    pub last_handoff: Option<HandoffSummary>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GitFileChange {
    pub path: String,
    pub status_code: String, // "M", "A", "D", "?", "U"
    pub added: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GitTelemetry {
    pub branch: String,
    pub head_short: Option<String>,
    pub modified: usize,
    pub untracked: usize,
    pub staged: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub is_clean: bool,
    pub has_git: bool,
    pub recent_commits: Vec<String>,
    pub ahead_behind: Option<(usize, usize)>,
    pub files: Vec<GitFileChange>,
    pub cross_session_conflicts: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuotaPool {
    pub label: String,
    pub percent: u8,
    pub reset: String,
    pub pace: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccountQuota {
    pub prefix: String,
    pub auth_file: String,
    pub rem_5h: u8,
    pub reset_5h: String,
    pub rem_wk: u8,
    pub reset_wk: String,
    pub is_critical: bool,
    pub is_warning: bool,
    pub has_error: bool,
    pub error_msg: String,
    pub pools: Vec<QuotaPool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct McpServer {
    pub name: String,
    pub mcp_type: String, // "local" | "remote"
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TreeNode {
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectTreeData {
    pub root_path: String,
    pub total_dirs: usize,
    pub total_files: usize,
    pub tree: Vec<TreeNode>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolVersionInfo {
    pub name: String,
    pub version: Option<String>,
    pub is_installed: bool,
    pub upstream_version: Option<String>,
    pub update_available: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EcosystemTelemetry {
    pub tools: Vec<ToolVersionInfo>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TelemetrySnapshot {
    pub project_dir: String,
    pub agent_session: AgentSession,
    pub engram: EngramTelemetry,
    pub git: GitTelemetry,
    pub accounts: Vec<AccountQuota>,
    pub mcps: Vec<McpServer>,
    pub tree: ProjectTreeData,
    pub ecosystem: EcosystemTelemetry,
    pub my_pane_id: Option<String>,
    pub my_tab_id: Option<String>,
    pub sidebar_name: Option<String>,
    pub theme_name: Option<String>,
    pub is_editing_name: bool,
    pub name_input_buffer: String,
    pub check_updates: bool,
}
