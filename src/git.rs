use crate::models::{GitFileChange, GitTelemetry};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

/// Ejecuta un subproceso git con `GIT_OPTIONAL_LOCKS=0` (nunca crea `.git/index.lock`),
/// drenado concurrente de stdout/stderr y timeout estricto.
pub fn run_git_cmd(cwd: &Path, args: &[&str], timeout_secs: u64) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().ok()?;
    let stdout_pipe = child.stdout.take()?;
    let stderr_pipe = child.stderr.take()?;

    let out_handle = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout_pipe.take(512 * 1024).read_to_string(&mut s);
        s
    });
    let err_handle = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr_pipe.take(64 * 1024).read_to_string(&mut s);
        s
    });

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => return None,
        }
    };

    let stdout = out_handle.join().unwrap_or_default();
    let _stderr = err_handle.join().unwrap_or_default();

    if status.success() {
        Some(stdout)
    } else {
        None
    }
}

/// Resuelve `(repo_root, git_dir)` subiendo por los directorios sin lanzar subprocesos.
pub fn find_git_root_and_dir(target_dir: &str) -> Option<(PathBuf, PathBuf)> {
    let mut curr = Path::new(target_dir).to_path_buf();
    if (curr.as_os_str().is_empty() || target_dir == ".")
        && let Ok(cwd) = std::env::current_dir()
    {
        curr = cwd;
    }
    loop {
        let base_git = curr.join(".git");
        if base_git.is_dir() {
            return Some((curr, base_git));
        } else if base_git.is_file() {
            if let Ok(content) = std::fs::read_to_string(&base_git) {
                let line = content.lines().next().unwrap_or("").trim();
                if let Some(rest) = line.strip_prefix("gitdir:") {
                    let p = rest.trim();
                    let candidate = curr.join(p);
                    if candidate.exists() {
                        return Some((curr, candidate));
                    }
                    let abs = Path::new(p);
                    if abs.exists() {
                        return Some((curr, abs.to_path_buf()));
                    }
                }
            }
            return Some((curr, base_git));
        }
        if !curr.pop() {
            break;
        }
    }
    None
}

pub fn find_git_dir_for_watch(target_dir: &str) -> Option<PathBuf> {
    find_git_root_and_dir(target_dir).map(|(_, gd)| gd)
}

/// Extrae el nombre de la rama desde el contenido de `.git/HEAD` sin lanzar subproceso.
pub fn parse_head_branch(head_content: &str) -> Option<String> {
    let s = head_content.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(b) = s.strip_prefix("ref: refs/heads/") {
        Some(b.trim().to_string())
    } else if s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("detached {}", &s[..8.min(s.len())]))
    } else {
        None
    }
}

/// Detecta si una ruta corresponde a un archivo sensible (credenciales, claves, .env)
/// cuyo contenido o diff jamás debe leerse ni mostrarse en la barra lateral.
pub fn is_sensitive_file(path: &str) -> bool {
    let norm = path.replace('\\', "/");
    let name = norm.rsplit('/').next().unwrap_or(&norm).to_lowercase();
    name.starts_with(".env")
        || [
            "secret",
            "credential",
            "token",
            "passw",
            "auth",
            ".pem",
            ".key",
            ".p12",
            ".pfx",
            "id_rsa",
            "id_ed25519",
            ".netrc",
            ".npmrc",
        ]
        .iter()
        .any(|k| name.contains(k))
}

/// Parsea la salida de `git status --porcelain=v1 -z --no-renames --untracked-files=all`.
pub fn parse_porcelain_z(out: &str) -> (Vec<GitFileChange>, usize, usize, usize) {
    let mut files = Vec::new();
    let mut staged = 0usize;
    let mut modified = 0usize;
    let mut untracked = 0usize;

    let mut it = out.split('\0');
    while let Some(entry) = it.next() {
        if entry.len() < 4 {
            continue;
        }
        let code = &entry[..2];
        let path = &entry[3..];
        let x = code.as_bytes()[0];
        let y = code.as_bytes()[1];
        if matches!(x, b'R' | b'C') {
            let _ = it.next();
        }
        let status_code = if code == "??" {
            untracked += 1;
            "?".to_string()
        } else if x == b'U' || y == b'U' || code == "AA" || code == "DD" {
            modified += 1;
            "U".to_string()
        } else {
            if x != b' ' && x != b'?' {
                staged += 1;
            }
            if y != b' ' && y != b'?' {
                modified += 1;
            }
            if x == b'A' || y == b'A' {
                "A".to_string()
            } else if x == b'D' || y == b'D' {
                "D".to_string()
            } else {
                "M".to_string()
            }
        };

        files.push(GitFileChange {
            path: path.to_string(),
            status_code,
            added: 0,
            deleted: 0,
        });
    }

    (files, staged, modified, untracked)
}

/// Aplica `git diff HEAD --numstat -z` a la lista de archivos y devuelve `(total_added, total_deleted)`.
pub fn apply_numstat_z(files: &mut [GitFileChange], out: &str) -> (usize, usize) {
    let mut total_a = 0usize;
    let mut total_d = 0usize;
    for rec in out.split('\0') {
        let mut parts = rec.splitn(3, '\t');
        let (Some(a_str), Some(d_str), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let a = a_str.parse::<usize>().unwrap_or(0);
        let d = d_str.parse::<usize>().unwrap_or(0);
        total_a += a;
        total_d += d;
        if let Some(f) = files.iter_mut().find(|f| f.path == path) {
            f.added = a;
            f.deleted = d;
        }
    }
    (total_a, total_d)
}

const UNTRACKED_PREVIEW_MAX_BYTES: u64 = 64 * 1024;
const DIFF_PREVIEW_MAX_LINES: usize = 24;

/// Obtiene una previsualización segura y acotada del diff de un archivo en el árbol de trabajo.
/// Nunca lee archivos sensibles ni sigue enlaces simbólicos.
pub fn read_file_diff_preview(target_dir: &str, rel_path: &str) -> Vec<String> {
    if is_sensitive_file(rel_path) {
        return vec!["[archivo sensible protegido - diff oculto]".to_string()];
    }

    let Some((repo_root, _)) = find_git_root_and_dir(target_dir) else {
        return vec!["[no es un repositorio git]".to_string()];
    };

    if let Some(diff_out) = run_git_cmd(
        &repo_root,
        &[
            "diff",
            "HEAD",
            "--no-renames",
            "--end-of-options",
            "--",
            rel_path,
        ],
        3,
    ) && !diff_out.trim().is_empty()
    {
        let mut lines = Vec::new();
        let all_lines: Vec<&str> = diff_out
            .lines()
            .filter(|l| {
                !l.starts_with("diff --git")
                    && !l.starts_with("index ")
                    && !l.starts_with("--- ")
                    && !l.starts_with("+++ ")
            })
            .collect();
        let total = all_lines.len();
        for l in all_lines.into_iter().take(DIFF_PREVIEW_MAX_LINES) {
            lines.push(l.to_string());
        }
        if total > DIFF_PREVIEW_MAX_LINES {
            lines.push(format!("... (+{} líneas más)", total - DIFF_PREVIEW_MAX_LINES));
        }
        return lines;
    }

    // Si `git diff HEAD` salió vacío, puede ser un archivo nuevo sin seguimiento (untracked)
    let full_path = repo_root.join(rel_path);
    match std::fs::symlink_metadata(&full_path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            vec!["[enlace simbólico - no se sigue]".to_string()]
        }
        Ok(meta) if meta.is_dir() => vec!["[directorio sin seguimiento]".to_string()],
        Ok(meta) if meta.len() > UNTRACKED_PREVIEW_MAX_BYTES => vec![format!(
            "[archivo nuevo de {} KiB - vista previa omitida]",
            meta.len() / 1024
        )],
        Ok(_) => match std::fs::read(&full_path) {
            Ok(bytes) => {
                if bytes.contains(&0) {
                    vec!["[archivo binario]".to_string()]
                } else {
                    let text = String::from_utf8_lossy(&bytes);
                    let all_lines: Vec<&str> = text.lines().collect();
                    let total = all_lines.len();
                    let mut out = vec!["@@ archivo nuevo @@".to_string()];
                    for l in all_lines.into_iter().take(DIFF_PREVIEW_MAX_LINES) {
                        out.push(format!("+{l}"));
                    }
                    if total > DIFF_PREVIEW_MAX_LINES {
                        out.push(format!("... (+{} líneas más)", total - DIFF_PREVIEW_MAX_LINES));
                    }
                    out
                }
            }
            Err(_) => vec!["[no se pudo leer el archivo]".to_string()],
        },
        Err(_) => vec!["[sin diferencias respecto a HEAD]".to_string()],
    }
}

/// Recolección de Git limpia, sin bloqueos (`GIT_OPTIONAL_LOCKS=0`), con timeout y caché mtime / 5s idle
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
        ahead_behind: None,
        files: Vec::new(),
    };

    let (repo_root, git_dir) = match find_git_root_and_dir(target_dir) {
        Some(pair) => pair,
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

    // 1. Branch actual: lectura directa de .git/HEAD (cero subprocesos) con fallback a git rev-parse
    if let Ok(head_raw) = std::fs::read_to_string(&head_path)
        && let Some(b) = parse_head_branch(&head_raw)
    {
        git.has_git = true;
        git.branch = b;
    } else if let Some(out) = run_git_cmd(&repo_root, &["rev-parse", "--abbrev-ref", "HEAD"], 3) {
        git.has_git = true;
        let b = out.trim().to_string();
        if !b.is_empty() {
            git.branch = b;
        }
    } else {
        return git;
    }

    // 2. Status porcelain en la raíz del repositorio (-z, sin renames, todos los untracked)
    if let Some(status_out) = run_git_cmd(
        &repo_root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--no-renames",
            "--untracked-files=all",
        ],
        3,
    ) {
        let (files, staged, modified, untracked) = parse_porcelain_z(&status_out);
        git.files = files;
        git.staged = staged;
        git.modified = modified;
        git.untracked = untracked;
        git.is_clean = git.modified == 0 && git.untracked == 0 && git.staged == 0;
    }

    // 3. Estadísticas de líneas añadidas/eliminadas (--numstat -z)
    if !git.is_clean
        && let Some(numstat_out) =
            run_git_cmd(&repo_root, &["diff", "HEAD", "--numstat", "-z", "--no-renames"], 3)
    {
        let (added, deleted) = apply_numstat_z(&mut git.files, &numstat_out);
        git.lines_added = added;
        git.lines_deleted = deleted;
    }

    // 4. Ahead / Behind respecto a upstream (si existe)
    if let Some(ab_out) = run_git_cmd(
        &repo_root,
        &["rev-list", "--left-right", "--count", "@{u}...HEAD"],
        3,
    ) {
        let counts: Vec<usize> = ab_out
            .split_whitespace()
            .filter_map(|x| x.parse().ok())
            .collect();
        if counts.len() == 2 {
            git.ahead_behind = Some((counts[1], counts[0]));
        }
    }

    // 5. Log reciente (últimos 2 commits)
    if let Some(log_out) = run_git_cmd(&repo_root, &["log", "--oneline", "-n", "2"], 3) {
        for line in log_out.lines() {
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
