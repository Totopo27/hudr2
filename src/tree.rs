use crate::models::{McpServer, ProjectTreeData, TreeNode};
use std::fs;
use std::path::Path;

pub fn read_project_tree(target_dir: &str) -> ProjectTreeData {
    let mut data = ProjectTreeData {
        root_path: target_dir.to_string(),
        ..Default::default()
    };

    let p = Path::new(target_dir);
    if let Ok(entries) = fs::read_dir(p) {
        let mut list: Vec<_> = entries
            .flatten()
            .filter(|e| {
                if let Some(name) = e.file_name().to_str() {
                    !name.starts_with('.')
                        && name != "node_modules"
                        && name != "target"
                        && name != "dist"
                } else {
                    false
                }
            })
            .collect();

        list.sort_by_key(|e| e.file_name());

        for entry in list {
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().to_string();

            if is_dir {
                data.total_dirs += 1;
                let mut children = Vec::new();
                if let Ok(sub) = fs::read_dir(entry.path()) {
                    for s in sub.flatten() {
                        let s_name = s.file_name().to_string_lossy().to_string();
                        if !s_name.starts_with('.') && s_name != "target" {
                            let s_is_dir = s.file_type().map(|t| t.is_dir()).unwrap_or(false);
                            if s_is_dir {
                                data.total_dirs += 1;
                            } else {
                                data.total_files += 1;
                            }
                            children.push(TreeNode {
                                name: s_name,
                                is_dir: s_is_dir,
                                children: Vec::new(),
                            });
                        }
                    }
                }
                data.tree.push(TreeNode {
                    name,
                    is_dir,
                    children,
                });
            } else {
                data.total_files += 1;
                data.tree.push(TreeNode {
                    name,
                    is_dir,
                    children: Vec::new(),
                });
            }
        }
    }

    data
}

pub fn read_mcp_servers(target_dir: &str) -> Vec<McpServer> {
    let mut mcps = Vec::new();
    let candidates = crate::paths::opencode_config_candidates(target_dir);

    let mut register_mcp = |name: &str, val: &serde_json::Value| {
        if name.is_empty() || !val.is_object() {
            return;
        }
        if mcps.iter().any(|m: &McpServer| m.name == name) {
            return;
        }

        let enabled = val.get("enabled").and_then(|b| b.as_bool()).unwrap_or(true);
        let mcp_type = val
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or_else(|| {
                if val.get("url").is_some() {
                    "remote"
                } else {
                    "local"
                }
            })
            .to_string();

        mcps.push(McpServer {
            name: name.to_string(),
            mcp_type,
            enabled,
        });
    };

    for c in candidates {
        if c.exists()
            && let Ok(content) = fs::read_to_string(&c)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            if let Some(mcp) = val.get("mcp").and_then(|m| m.as_object()) {
                for (k, v) in mcp {
                    if k == "servers" {
                        if let Some(srv) = v.as_object() {
                            for (sk, sv) in srv {
                                register_mcp(sk, sv);
                            }
                        }
                    } else {
                        register_mcp(k, v);
                    }
                }
            }
            if let Some(mcp_srv) = val.get("mcpServers").and_then(|m| m.as_object()) {
                for (k, v) in mcp_srv {
                    register_mcp(k, v);
                }
            }
        }
    }

    mcps
}
