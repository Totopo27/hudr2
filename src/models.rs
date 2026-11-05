use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Default)]
pub struct AttributedChanges {
    pub files_count: usize,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Default)]
pub struct AgentSession {
    pub provider_name: String, // "opencode", "pi", "gentle-shell"
    pub agent: String,
    pub model: String,
    pub model_short: String,
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
    pub tool_other: usize,
    pub tool_total: usize,
    pub agent_status: String, // "idle" | "working"
    pub all_active_prefixes: HashSet<String>,
    pub attributed_changes: AttributedChanges,
}

/// Alias para mantener retrocompatibilidad con el adaptador de OpenCode
pub type OpenCodeSession = AgentSession;

#[derive(Debug, Clone, Default)]
pub struct EngramTelemetry {
    pub project: String,
    pub online: bool,
    pub obs_count: usize,
    pub enrolled: bool,
    pub cloud_host: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GitTelemetry {
    pub branch: String,
    pub modified: usize,
    pub untracked: usize,
    pub staged: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub is_clean: bool,
    pub has_git: bool,
    pub recent_commits: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QuotaPool {
    pub label: String,
    pub percent: u8,
    pub reset: String,
    pub pace: String,
}

#[derive(Debug, Clone, Default)]
pub struct AccountQuota {
    pub prefix: String,
    pub email: String,
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

#[derive(Debug, Clone, Default)]
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

#[derive(Debug, Clone, Default)]
pub struct TelemetrySnapshot {
    pub project_dir: String,
    pub agent_session: AgentSession,
    pub engram: EngramTelemetry,
    pub git: GitTelemetry,
    pub accounts: Vec<AccountQuota>,
    pub mcps: Vec<McpServer>,
    pub tree: ProjectTreeData,
    pub my_pane_id: Option<String>,
    pub my_tab_id: Option<String>,
}
