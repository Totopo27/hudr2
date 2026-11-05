use crate::models::GitTelemetry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone)]
struct GitCacheEntry {
    telemetry: GitTelemetry,
    head_mtime: Option<SystemTime>,
    index_mtime: Option<SystemTime>,
    last_check: Instant,
}

static GIT_CACHE: LazyLock<Mutex<HashMap<String, GitCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn clear_git_cache() {
    if let Ok(mut guard) = GIT_CACHE.lock() {
        guard.clear();
    }
}

fn find_git_dir(target_dir: &str) -> Option<PathBuf> {
    let mut curr = Path::new(target_dir).to_path_buf();
    loop {
        let base_git = curr.join(".git");
        if base_git.is_dir() {
            return Some(base_git);
        } else if base_git.is_file() {
            if let Ok(content) = std::fs::read_to_string(&base_git) {
                let line = content.lines().next().unwrap_or("").trim();
                if let Some(rest) = line.strip_prefix("gitdir:") {
                    let p = rest.trim();
                    let candidate = curr.join(p);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                    let abs = Path::new(p);
                    if abs.exists() {
                        return Some(abs.to_path_buf());
                    }
                }
            }
            return Some(base_git);
        }
        if !curr.pop() {
            break;
        }
    }
    None
}

/// Recolección de Git limpia y ligera con caché mtime / 5s idle
pub fn read_git_telemetry(target_dir: &str) -> GitTelemetry {
    let mut git = GitTelemetry {
        branch: "main".to_string(),
        modified: 0,
        untracked: 0,
        staged: 0,
        lines_added: 0,
        lines_deleted: 0,
        is_clean: true,
        has_git: false,
        recent_commits: Vec::new(),
    };

    let git_dir = match find_git_dir(target_dir) {
        Some(d) => d,
        None => return git,
    };

    let head_path = git_dir.join("HEAD");
    let index_path = git_dir.join("index");
    let head_mtime = std::fs::metadata(&head_path)
        .and_then(|m| m.modified())
        .ok();
    let index_mtime = std::fs::metadata(&index_path)
        .and_then(|m| m.modified())
        .ok();
    let now = Instant::now();
    let norm_dir = crate::paths::normalize_path(target_dir);

    // Comprobar caché por mtime (.git/index y .git/HEAD) o ventana de 5s ociosa
    if let Ok(guard) = GIT_CACHE.lock()
        && let Some(entry) = guard.get(&norm_dir)
    {
        let mtime_matches = head_mtime.is_some()
            && entry.head_mtime == head_mtime
            && entry.index_mtime == index_mtime;
        let within_idle_ttl = now.duration_since(entry.last_check) < Duration::from_secs(5);

        if mtime_matches || within_idle_ttl {
            return entry.telemetry.clone();
        }
    }

    // 1. Branch actual
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(target_dir)
        .output()
    {
        if output.status.success() {
            git.has_git = true;
            let b = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !b.is_empty() {
                git.branch = b;
            }
        } else {
            return git;
        }
    } else {
        return git;
    }

    // 2. Status porcelain
    if let Ok(output) = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(target_dir)
        .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.len() >= 2 {
                let index_char = line.chars().next().unwrap_or(' ');
                let work_char = line.chars().nth(1).unwrap_or(' ');
                if index_char == '?' {
                    git.untracked += 1;
                } else {
                    if index_char != ' ' && index_char != '?' {
                        git.staged += 1;
                    }
                    if work_char != ' ' && work_char != '?' {
                        git.modified += 1;
                    }
                }
            }
        }
        git.is_clean = git.modified == 0 && git.untracked == 0 && git.staged == 0;
    }

    // 3. Log reciente (últimos 2 commits)
    if let Ok(output) = Command::new("git")
        .args(["log", "--oneline", "-n", "2"])
        .current_dir(target_dir)
        .output()
        && output.status.success()
    {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if !line.trim().is_empty() {
                git.recent_commits.push(line.trim().to_string());
            }
        }
    }

    if let Ok(mut guard) = GIT_CACHE.lock() {
        guard.insert(
            norm_dir,
            GitCacheEntry {
                telemetry: git.clone(),
                head_mtime,
                index_mtime,
                last_check: now,
            },
        );
    }

    git
}
