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
    head_short: Option<String>,
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

    if status.success() { Some(stdout) } else { None }
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

/// Resuelve el directorio común de Git (`commondir`) cuando `.git` pertenece a un worktree,
/// o devuelve `git_dir` si es un repositorio estándar.
fn resolve_common_git_dir(git_dir: &Path) -> PathBuf {
    let commondir_file = git_dir.join("commondir");
    if let Ok(raw) = std::fs::read_to_string(&commondir_file) {
        let rel = raw.trim();
        if !rel.is_empty() {
            let p = Path::new(rel);
            let resolved = if p.is_absolute() {
                p.to_path_buf()
            } else {
                git_dir.join(p)
            };
            if resolved.exists() {
                return resolved;
            }
        }
    }
    git_dir.to_path_buf()
}

/// Lee el short SHA (7 caracteres) de `HEAD` directamente desde `.git/HEAD`, `refs/`
/// o `packed-refs` sin lanzar ningún subproceso (`git`). Soporta repositorios normales y worktrees.
pub fn read_git_head_short(git_dir: &Path) -> Option<String> {
    let head_raw = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let trimmed = head_raw.trim();

    if let Some(branch_ref) = trimmed.strip_prefix("ref:") {
        let branch_ref = branch_ref.trim();
        if branch_ref.is_empty() {
            return None;
        }
        let common_dir = resolve_common_git_dir(git_dir);

        // 1. Intentar leer la referencia suelta en git_dir o commondir
        for base in [git_dir, common_dir.as_path()] {
            let ref_path = base.join(branch_ref);
            if let Ok(ref_content) = std::fs::read_to_string(&ref_path) {
                let sha = ref_content.trim();
                if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(sha[..7].to_string());
                }
            }
        }

        // 2. Fallback: buscar en packed-refs sin lanzar git
        for base in [git_dir, common_dir.as_path()] {
            if let Ok(packed) = std::fs::read_to_string(base.join("packed-refs")) {
                for line in packed.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
                        continue;
                    }
                    let mut parts = line.split_whitespace();
                    if let (Some(sha), Some(rname)) = (parts.next(), parts.next())
                        && rname == branch_ref
                        && sha.len() >= 7
                        && sha.chars().all(|c| c.is_ascii_hexdigit())
                    {
                        return Some(sha[..7].to_string());
                    }
                }
            }
        }
        None
    } else if trimmed.len() >= 7 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(trimmed[..7].to_string())
    } else {
        None
    }
}

/// Detecta colisiones de archivos en caliente entre dos o más sesiones activas.
/// Devuelve la lista ordenada y sin duplicados de archivos que están siendo modificados
/// simultáneamente por más de una sesión.
pub fn detect_cross_session_conflicts(session_file_sets: &[Vec<String>]) -> Vec<String> {
    if session_file_sets.len() < 2 {
        return Vec::new();
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut conflicts = std::collections::BTreeSet::new();

    for (idx, files) in session_file_sets.iter().enumerate() {
        for file in files {
            let norm = file.trim().replace('\\', "/");
            let clean = norm.strip_prefix("./").unwrap_or(&norm).trim();
            if clean.is_empty() {
                continue;
            }
            if let Some(prev_idx) = seen.insert(clean.to_string(), idx)
                && prev_idx != idx
            {
                conflicts.insert(clean.to_string());
            }
        }
    }
    conflicts.into_iter().collect()
}

fn normalize_canonical(p: &Path) -> String {
    let canon = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    crate::paths::normalize_path(&canon.to_string_lossy())
}

/// Recolecta colisiones de archivos en caliente comparando el directorio actual con otros panes/worktrees del mismo repositorio en Herdr.
pub fn collect_cross_session_conflicts(
    primary_dir: &str,
    primary_files: &[GitFileChange],
    panes: &[crate::models::HerdrPane],
) -> Vec<String> {
    let primary_set: std::collections::HashSet<String> = primary_files
        .iter()
        .filter(|f| f.status_code != "?")
        .map(|f| {
            let norm = f.path.trim().replace('\\', "/");
            norm.strip_prefix("./").unwrap_or(&norm).trim().to_string()
        })
        .filter(|s| !s.is_empty())
        .collect();
    if primary_set.is_empty() || panes.is_empty() {
        return Vec::new();
    }

    let Some((primary_root, primary_git_dir)) = find_git_root_and_dir(primary_dir) else {
        return Vec::new();
    };
    let primary_common = normalize_canonical(&resolve_common_git_dir(&primary_git_dir));

    let mut seen_roots = std::collections::HashSet::new();
    seen_roots.insert(normalize_canonical(&primary_root));

    let mut conflicts = std::collections::BTreeSet::new();

    for pane in panes {
        let Some(ref cwd) = pane.cwd else {
            continue;
        };
        if !Path::new(cwd).exists() {
            continue;
        }
        let Some((other_root, other_git_dir)) = find_git_root_and_dir(cwd) else {
            continue;
        };
        let other_common = normalize_canonical(&resolve_common_git_dir(&other_git_dir));
        if other_common != primary_common {
            continue;
        }
        if !seen_roots.insert(normalize_canonical(&other_root)) {
            continue;
        }
        let other_git = read_git_telemetry(other_root.to_str().unwrap_or(cwd));
        for f in other_git.files.into_iter().filter(|f| f.status_code != "?") {
            let norm = f.path.trim().replace('\\', "/");
            let clean = norm.strip_prefix("./").unwrap_or(&norm).trim();
            if primary_set.contains(clean) {
                conflicts.insert(clean.to_string());
            }
        }
    }

    conflicts.into_iter().collect()
}

/// Detecta si una ruta corresponde a un archivo sensible (credenciales, claves, .env)
/// cuyo contenido o diff jamás debe leerse ni mostrarse en la barra lateral.
pub fn is_sensitive_file(path: &str) -> bool {
    let norm = path.replace('\\', "/");
    let lower_path = norm.to_lowercase();
    let mut parts = lower_path.rsplit('/');
    let name = parts.next().unwrap_or(&lower_path);

    if parts.any(|seg| {
        matches!(
            seg,
            ".ssh" | ".aws" | ".kube" | ".gnupg" | "secrets" | "credentials" | "auths" | ".env"
        )
    }) {
        return true;
    }

    if name.starts_with(".env")
        || name.ends_with(".env")
        || matches!(name, ".netrc" | ".npmrc" | ".pgpass" | ".pypirc")
        || name.contains(".tfstate.")
        || [
            ".pem",
            ".key",
            ".p12",
            ".pfx",
            ".keystore",
            ".jks",
            ".tfvars",
            ".tfstate",
            ".kdbx",
        ]
        .iter()
        .any(|ext| name.ends_with(ext))
        || ["id_rsa", "id_ed25519", "id_ecdsa", "id_dsa"]
            .iter()
            .any(|k| name == *k || name.starts_with(&format!("{k}.")))
    {
        return true;
    }

    let tokens: Vec<&str> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();

    if tokens.iter().any(|t| {
        matches!(
            *t,
            "secret"
                | "secrets"
                | "credential"
                | "credentials"
                | "token"
                | "tokens"
                | "password"
                | "passwords"
                | "passwd"
                | "passw"
                | "apikey"
                | "privatekey"
                | "keychain"
        )
    }) {
        return true;
    }

    if (tokens.contains(&"private") && tokens.contains(&"key"))
        || (tokens.contains(&"api") && tokens.contains(&"key"))
    {
        return true;
    }

    if tokens.contains(&"auth") {
        let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        let is_code_or_doc = matches!(
            ext,
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "go"
                | "java"
                | "kt"
                | "c"
                | "cc"
                | "cpp"
                | "h"
                | "hpp"
                | "cs"
                | "rb"
                | "php"
                | "swift"
                | "md"
        );
        return !is_code_or_doc;
    }

    false
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
const DIFF_PREVIEW_MAX_LINES: usize = 12;
const QUICK_PEEK_MAX_LINES: usize = 120;

fn format_diff_preview_lines(diff_out: &str, max_lines: usize) -> Vec<String> {
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
    let mut lines: Vec<String> = all_lines
        .into_iter()
        .take(max_lines)
        .map(str::to_string)
        .collect();
    if total > max_lines {
        lines.push(format!("... (+{} líneas más)", total - max_lines));
    }
    lines
}

/// Obtiene una previsualización segura y acotada del diff de un archivo en el árbol de trabajo.
/// Nunca lee archivos sensibles ni sigue enlaces simbólicos.
pub fn read_file_diff_preview(target_dir: &str, rel_path: &str) -> Vec<String> {
    read_file_preview_with_limit(target_dir, rel_path, DIFF_PREVIEW_MAX_LINES)
}

/// Obtiene una previsualización extendida y segura (hasta 120 líneas) para el modal flotante Quick Peek.
/// Protege estrictamente archivos sensibles (`is_sensitive_file`) y enlaces simbólicos.
pub fn read_file_quick_peek(target_dir: &str, rel_path: &str) -> Vec<String> {
    read_file_preview_with_limit(target_dir, rel_path, QUICK_PEEK_MAX_LINES)
}

fn read_file_preview_with_limit(target_dir: &str, rel_path: &str, max_lines: usize) -> Vec<String> {
    let p = Path::new(rel_path);
    if p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return vec!["[ruta inválida fuera del repositorio]".to_string()];
    }

    if is_sensitive_file(rel_path) {
        return vec!["[archivo sensible protegido - diff oculto]".to_string()];
    }

    let Some((repo_root, _)) = find_git_root_and_dir(target_dir) else {
        return vec!["[no es un repositorio git]".to_string()];
    };

    let full_path = repo_root.join(rel_path);
    if let Ok(canon_root) = repo_root.canonicalize()
        && let Ok(canon_path) = full_path.canonicalize()
        && !canon_path.starts_with(&canon_root)
    {
        return vec!["[ruta inválida fuera del repositorio]".to_string()];
    }

    let head_diff = run_git_cmd(
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
    );
    if let Some(ref diff_out) = head_diff
        && !diff_out.trim().is_empty()
    {
        return format_diff_preview_lines(diff_out, max_lines);
    }
    if head_diff.is_none()
        && let Some(cached_out) = run_git_cmd(
            &repo_root,
            &[
                "diff",
                "--cached",
                "--no-renames",
                "--end-of-options",
                "--",
                rel_path,
            ],
            3,
        )
        && !cached_out.trim().is_empty()
    {
        return format_diff_preview_lines(&cached_out, max_lines);
    }

    let is_untracked = run_git_cmd(
        &repo_root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--no-renames",
            "--untracked-files=all",
            "--",
            rel_path,
        ],
        2,
    )
    .is_some_and(|out| out.starts_with("?? "));

    if !is_untracked {
        return vec!["[sin diferencias respecto a HEAD]".to_string()];
    }

    // Archivo nuevo sin seguimiento (untracked)
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
                    for l in all_lines.into_iter().take(max_lines) {
                        out.push(format!("+{l}"));
                    }
                    if total > max_lines {
                        out.push(format!("... (+{} líneas más)", total - max_lines));
                    }
                    out
                }
            }
            Err(_) => vec!["[no se pudo leer el archivo]".to_string()],
        },
        Err(_) => vec!["[sin diferencias respecto a HEAD]".to_string()],
    }
}

/// Recolección de Git limpia, sin bloqueos (`GIT_OPTIONAL_LOCKS=0`), con timeout y caché mtime / 2s idle
pub fn read_git_telemetry(target_dir: &str) -> GitTelemetry {
    let mut git = GitTelemetry {
        branch: String::new(),
        head_short: None,
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
        cross_session_conflicts: Vec::new(),
    };

    let (repo_root, git_dir) = match find_git_root_and_dir(target_dir) {
        Some(pair) => pair,
        None => return git,
    };

    git.branch = "main".to_string();

    let head_path = git_dir.join("HEAD");
    let index_path = git_dir.join("index");
    let head_mtime = std::fs::metadata(&head_path)
        .and_then(|m| m.modified())
        .ok();
    let index_mtime = std::fs::metadata(&index_path)
        .and_then(|m| m.modified())
        .ok();
    // Lectura directa (0 subprocesos) del short SHA de HEAD desde .git/HEAD, refs/ o packed-refs
    let current_head_short = read_git_head_short(&git_dir);
    let now = Instant::now();
    let norm_dir = crate::paths::normalize_path(target_dir);

    // Exige coincidencia de mtime (.git/HEAD, .git/index y SHA) Y ventana corta de 2s para refrescar cambios sin stage
    if let Ok(guard) = GIT_CACHE.lock()
        && let Some(entry) = guard.get(&norm_dir)
    {
        let mtime_matches = head_mtime.is_some()
            && entry.head_mtime == head_mtime
            && entry.index_mtime == index_mtime
            && entry.head_short == current_head_short;
        let within_idle_ttl = now.duration_since(entry.last_check) < Duration::from_secs(2)
            && entry.head_short == current_head_short;

        if mtime_matches && within_idle_ttl {
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
    git.head_short = current_head_short.clone();

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
        && let Some(numstat_out) = run_git_cmd(
            &repo_root,
            &["diff", "HEAD", "--numstat", "-z", "--no-renames"],
            3,
        )
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

    // 5. Log reciente (últimos 2 commits) con fallback sin subproceso a head_short
    if let Some(log_out) = run_git_cmd(&repo_root, &["log", "--oneline", "-n", "2"], 3) {
        for line in log_out.lines() {
            if !line.trim().is_empty() {
                git.recent_commits.push(line.trim().to_string());
            }
        }
    }
    if git.recent_commits.is_empty()
        && let Some(ref sha) = git.head_short
    {
        git.recent_commits.push(format!("{sha} HEAD"));
    }

    if let Ok(mut guard) = GIT_CACHE.lock() {
        guard.insert(
            norm_dir,
            GitCacheEntry {
                telemetry: git.clone(),
                head_mtime,
                index_mtime,
                head_short: current_head_short,
                last_check: now,
            },
        );
    }

    git
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_git_test(cwd: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("git debe ejecutarse en prueba");
        assert!(
            out.status.success(),
            "git {:?} falló: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn git_cache_refreshes_unstaged_edits_after_ttl_without_head_or_index_change() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path();
        run_git_test(repo, &["init"]);
        std::fs::write(repo.join("a.txt"), "line1\n").unwrap();
        run_git_test(repo, &["add", "a.txt"]);
        run_git_test(
            repo,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "init",
            ],
        );

        let repo_str = repo.to_string_lossy().to_string();
        let norm_dir = crate::paths::normalize_path(&repo_str);
        let t1 = read_git_telemetry(&repo_str);
        assert!(t1.has_git);
        assert!(t1.is_clean);

        // Unstaged edit does not change .git/HEAD or .git/index mtimes.
        std::fs::write(repo.join("a.txt"), "line1\nline2\n").unwrap();

        if let Ok(mut guard) = GIT_CACHE.lock()
            && let Some(entry) = guard.get_mut(&norm_dir)
        {
            entry.last_check = entry
                .last_check
                .checked_sub(Duration::from_secs(10))
                .expect("Instant de sistema debe permitir retroceder 10s");
        }

        let t2 = read_git_telemetry(&repo_str);
        assert!(
            !t2.is_clean,
            "Edición sin staging tras expirar el TTL de caché debe detectarse sin depender de mtime en .git/index"
        );
        assert!(
            t2.files.iter().any(|f| f.path == "a.txt"),
            "a.txt modificado sin stage debe aparecer tras vencer el TTL"
        );
    }
}
