use crate::models::GitTelemetry;
use std::process::Command;

/// Recolección de Git limpia y ligera
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

    git
}
