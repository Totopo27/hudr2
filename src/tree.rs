use crate::models::{McpServer, ProjectTreeData, TreeNode};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs;
use std::io::Read;
use std::path::Path;

const TREE_SKIP_NAMES: [&str; 9] = [
    ".git",
    "node_modules",
    "dist",
    ".cache",
    ".next",
    "coverage",
    ".turbo",
    "build",
    "target",
];

const TREE_MAX_ENTRIES: usize = 2000;
const MCP_CONFIG_MAX_BYTES: u64 = 1024 * 1024;

fn is_real_dir(entry: &fs::DirEntry) -> bool {
    entry
        .file_type()
        .map(|t| t.is_dir() && !t.is_symlink())
        .unwrap_or(false)
}

struct RankedDirEntry {
    entry: fs::DirEntry,
    is_dir: bool,
    name: String,
    sort_name: String,
    encounter_index: usize,
}

impl RankedDirEntry {
    fn new(entry: fs::DirEntry, encounter_index: usize) -> Option<Self> {
        let name = entry.file_name().to_str()?.to_string();
        if name.starts_with('.') || TREE_SKIP_NAMES.contains(&name.as_str()) {
            return None;
        }
        Some(Self {
            is_dir: is_real_dir(&entry),
            sort_name: name.to_lowercase(),
            name,
            entry,
            encounter_index,
        })
    }
}

impl PartialEq for RankedDirEntry {
    fn eq(&self, other: &Self) -> bool {
        self.is_dir == other.is_dir
            && self.sort_name == other.sort_name
            && self.encounter_index == other.encounter_index
    }
}

impl Eq for RankedDirEntry {}

impl PartialOrd for RankedDirEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedDirEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.is_dir, other.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => self
                .sort_name
                .cmp(&other.sort_name)
                .then_with(|| self.encounter_index.cmp(&other.encounter_index)),
        }
    }
}

fn select_best_entries(path: &Path, limit: usize) -> Vec<RankedDirEntry> {
    if limit == 0 {
        return Vec::new();
    }

    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };
    let mut best = BinaryHeap::with_capacity(limit);
    for entry in entries
        .flatten()
        .enumerate()
        .filter_map(|(index, entry)| RankedDirEntry::new(entry, index))
    {
        if best.len() < limit {
            best.push(entry);
        } else if best.peek().is_some_and(|worst| entry < *worst) {
            best.pop();
            best.push(entry);
        }
    }

    best.into_sorted_vec()
}

fn read_mcp_config(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MCP_CONFIG_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MCP_CONFIG_MAX_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

pub fn read_project_tree(target_dir: &str) -> ProjectTreeData {
    let mut data = ProjectTreeData {
        root_path: target_dir.to_string(),
        ..Default::default()
    };

    let mut budget = TREE_MAX_ENTRIES;
    let p = Path::new(target_dir);
    if p.is_dir() {
        for entry in select_best_entries(p, budget) {
            if budget == 0 {
                break;
            }
            budget -= 1;

            let is_dir = entry.is_dir;
            let name = entry.name;

            if is_dir {
                data.total_dirs += 1;
                let mut children = Vec::new();
                for child in select_best_entries(&entry.entry.path(), budget) {
                    budget -= 1;
                    if child.is_dir {
                        data.total_dirs += 1;
                    } else {
                        data.total_files += 1;
                    }
                    children.push(TreeNode {
                        name: child.name,
                        is_dir: child.is_dir,
                        children: Vec::new(),
                    });
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
    let mut candidates = crate::paths::opencode_config_candidates(target_dir);
    candidates.extend(crate::paths::pi_mcp_config_candidates(target_dir));

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
            && let Some(content) = read_mcp_config(&c)
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
