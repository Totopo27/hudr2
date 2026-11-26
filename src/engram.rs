use crate::models::{EngramTelemetry, HandoffSummary};
use crate::paths::{db_effective_mtime, resolve_engram_db_path};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ENGRAM_CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
struct EngramCacheEntry {
    mtime: Option<SystemTime>,
    cached_at: Instant,
    sync_phase: Option<String>,
    sync_at_epoch: Option<i64>,
    telemetry: EngramTelemetry,
}

static ENGRAM_CACHE: LazyLock<Mutex<HashMap<String, EngramCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn clear_engram_cache() {
    if let Ok(mut guard) = ENGRAM_CACHE.lock() {
        guard.clear();
    }
}

/// Consulta HTTP/1.0 ligera contra el daemon local de Engram (`127.0.0.1:<port><path>`)
/// sin pasar por el pool pesado de reqwest.
pub fn http_json_local(port: u16, path: &str) -> Option<Value> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(120)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(350)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(150)));
    let req = format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nAccept: application/json\r\n\r\n");
    stream.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    stream.take(128 * 1024).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let (head, body) = text.split_once("\r\n\r\n")?;
    let is_200 = head
        .lines()
        .next()
        .is_some_and(|l| l.split_whitespace().nth(1) == Some("200"));
    if is_200 {
        serde_json::from_str(body).ok()
    } else {
        None
    }
}

fn query_escape(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

pub fn format_relative_time(at_epoch: Option<i64>, now_epoch: i64) -> String {
    let Some(at) = at_epoch else {
        return "nunca".to_string();
    };
    let s = (now_epoch - at).max(0);
    if s < 10 {
        "recién".to_string()
    } else if s < 60 {
        format!("hace {s}s")
    } else if s < 3600 {
        format!("hace {}m", s / 60)
    } else if s < 86_400 {
        format!("hace {}h", s / 3600)
    } else {
        format!("hace {}d", s / 86_400)
    }
}

pub fn extract_handoff_lines(content: &str) -> Vec<String> {
    let trimmed: Vec<&str> = content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let non_headings: Vec<&str> = trimmed
        .iter()
        .copied()
        .filter(|l| !l.starts_with('#'))
        .collect();
    let chosen = if non_headings.is_empty() {
        trimmed
    } else {
        non_headings
    };
    chosen
        .into_iter()
        .take(12)
        .map(|l| l.chars().take(180).collect())
        .collect()
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_engram_timestamp(ts: &str) -> Option<i64> {
    let trimmed = ts.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(num) = trimmed.parse::<i64>() {
        return Some(if num.abs() > 10_000_000_000 {
            num / 1000
        } else {
            num
        });
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.timestamp());
    }
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
    ] {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Some(ndt.and_utc().timestamp());
        }
    }
    None
}

fn push_unique_candidate(out: &mut Vec<String>, candidate: &str) {
    let clean = candidate.trim();
    if !clean.is_empty()
        && clean != "."
        && clean != ".."
        && !out.iter().any(|existing| existing == clean)
    {
        out.push(clean.to_string());
    }
}

fn read_engram_config_project(dir: &Path) -> Option<String> {
    let cfg_path = dir.join(".engram").join("config.json");
    if !cfg_path.is_file() {
        return None;
    }
    let content = crate::paths::read_config_string(&cfg_path).ok()?;
    let parsed: Value = serde_json::from_str(&content).ok()?;
    parsed
        .get("project")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn parse_git_origin_url(url: &str) -> Vec<String> {
    let mut results = Vec::new();
    let trimmed = url.trim().trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let path_part = if let Some((_, after_scheme)) = without_git.split_once("://") {
        after_scheme
            .split_once('/')
            .map(|(_, path)| path)
            .unwrap_or("")
    } else if let Some((_, path)) = without_git.split_once(':') {
        path
    } else {
        without_git
    };
    let parts: Vec<&str> = path_part
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() >= 2 {
        let owner_repo = format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]);
        results.push(owner_repo);
    }
    if let Some(repo) = parts.last() {
        results.push((*repo).to_string());
    }
    results
}

fn read_git_origin_candidates_from_config(cfg_path: &Path) -> Vec<String> {
    let Ok(content) = crate::paths::read_config_string(cfg_path) else {
        return Vec::new();
    };
    let mut in_origin = false;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_origin = line.eq_ignore_ascii_case("[remote \"origin\"]");
            continue;
        }
        if in_origin
            && let Some((key, val)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
        {
            return parse_git_origin_url(val);
        }
    }
    Vec::new()
}

fn resolve_git_root_and_origin(abs_path: &Path) -> (Option<std::path::PathBuf>, Vec<String>) {
    let mut cur = Some(abs_path);
    while let Some(dir) = cur {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            let remotes = read_git_origin_candidates_from_config(&dot_git.join("config"));
            return (Some(dir.to_path_buf()), remotes);
        } else if dot_git.is_file() {
            if let Ok(pointer) = crate::paths::read_config_string(&dot_git)
                && let Some(raw_gitdir) = pointer
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("gitdir:"))
            {
                let rel_gitdir = Path::new(raw_gitdir.trim());
                let gitdir = if rel_gitdir.is_absolute() {
                    rel_gitdir.to_path_buf()
                } else {
                    dir.join(rel_gitdir)
                };
                let mut remotes = read_git_origin_candidates_from_config(&gitdir.join("config"));
                if remotes.is_empty()
                    && let Ok(common) = crate::paths::read_config_string(gitdir.join("commondir"))
                {
                    let rel_common = Path::new(common.trim());
                    let common_dir = if rel_common.is_absolute() {
                        rel_common.to_path_buf()
                    } else {
                        gitdir.join(rel_common)
                    };
                    remotes = read_git_origin_candidates_from_config(&common_dir.join("config"));
                }
                return (Some(dir.to_path_buf()), remotes);
            }
            return (Some(dir.to_path_buf()), Vec::new());
        }
        cur = dir.parent();
    }
    (None, Vec::new())
}

fn lexical_abs_path(raw: &str) -> std::path::PathBuf {
    let p = Path::new(raw);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(p)
    } else {
        p.to_path_buf()
    };
    let mut out = std::path::PathBuf::new();
    for comp in joined.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resuelve la lista ordenada de identificadores de proyecto Engram para `project_dir`:
/// `.engram/config.json`, basename canónico (incluyendo `.`/`..`) y slug/nombre de `origin`.
pub fn resolve_engram_project_candidates(project_dir: &str) -> Vec<String> {
    let trimmed = project_dir.trim();
    let abs_path = if trimmed.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    } else {
        std::fs::canonicalize(Path::new(trimmed)).unwrap_or_else(|_| lexical_abs_path(trimmed))
    };

    let mut candidates = Vec::new();
    let (git_root, git_remotes) = resolve_git_root_and_origin(&abs_path);
    let home = dirs::home_dir();

    // 1. `.engram/config.json` del directorio o ancestros hasta la raíz git (sin cruzar a `$HOME`)
    if let Some(ref root) = git_root {
        let mut cur = Some(abs_path.as_path());
        while let Some(dir) = cur {
            if home.as_deref() != Some(dir)
                && let Some(cfg_proj) = read_engram_config_project(dir)
            {
                push_unique_candidate(&mut candidates, &cfg_proj);
                break;
            }
            if dir == root.as_path() {
                break;
            }
            cur = dir.parent();
        }
    } else if home.as_deref() != Some(abs_path.as_path())
        && let Some(cfg_proj) = read_engram_config_project(&abs_path)
    {
        push_unique_candidate(&mut candidates, &cfg_proj);
    }

    // 2. Nombre canónico de la carpeta del workspace
    if let Some(name) = abs_path.file_name().and_then(|n| n.to_str()) {
        push_unique_candidate(&mut candidates, name);
    }

    // 3. Slug (`owner/repo`) y nombre de repositorio desde remoto `origin`
    for remote_cand in git_remotes {
        push_unique_candidate(&mut candidates, &remote_cand);
    }

    candidates
}

fn resolve_db_project_match(conn: &Connection, candidates: &[String]) -> Option<String> {
    for pass_ci in [false, true] {
        for cand in candidates {
            let found_obs: Option<String> = if !pass_ci {
                conn.query_row(
                    "SELECT project FROM observations WHERE project = ?1 AND deleted_at IS NULL LIMIT 1",
                    [cand],
                    |r| r.get(0),
                )
                .ok()
            } else {
                conn.query_row(
                    "SELECT project FROM observations WHERE LOWER(project) = LOWER(?1) AND deleted_at IS NULL LIMIT 1",
                    [cand],
                    |r| r.get(0),
                )
                .ok()
            };
            if let Some(p) = found_obs {
                return Some(p);
            }
            let found_sync: Option<String> = if !pass_ci {
                conn.query_row(
                    "SELECT project FROM sync_enrolled_projects WHERE project = ?1 LIMIT 1",
                    [cand],
                    |r| r.get(0),
                )
                .ok()
            } else {
                conn.query_row(
                    "SELECT project FROM sync_enrolled_projects WHERE LOWER(project) = LOWER(?1) LIMIT 1",
                    [cand],
                    |r| r.get(0),
                )
                .ok()
            };
            if let Some(p) = found_sync {
                return Some(p);
            }
        }
    }
    None
}

/// Consulta instantánea en SQLite de Engram + estado del daemon y último session_summary (Handoff) con caché mtime
pub fn read_engram_telemetry(project_dir: &str) -> EngramTelemetry {
    let candidates = resolve_engram_project_candidates(project_dir);
    let project = candidates
        .first()
        .cloned()
        .unwrap_or_else(|| "proyecto".to_string());

    let db_path = resolve_engram_db_path();
    let mtime = if db_path.exists() {
        db_effective_mtime(&db_path)
    } else {
        None
    };

    let custom_env_db = std::env::var("ENGRAM_DB_PATH").is_ok();
    let custom_port = std::env::var("ENGRAM_PORT").ok();
    let is_custom_test_db = custom_env_db && custom_port.is_none();
    let cache_key = format!(
        "{}|{}|{}",
        crate::paths::normalize_path(&db_path.to_string_lossy()),
        crate::paths::normalize_path(project_dir),
        custom_port.as_deref().unwrap_or("")
    );

    if let Ok(guard) = ENGRAM_CACHE.lock()
        && let Some(entry) = guard.get(&cache_key)
        && entry.mtime == mtime
        && entry.cached_at.elapsed() < ENGRAM_CACHE_TTL
    {
        let mut cached = entry.telemetry.clone();
        let now = now_epoch_secs();
        if let Some(ref mut handoff) = cached.last_handoff {
            handoff.relative_time = format_relative_time(handoff.created_at_epoch, now);
        }
        if let Some(ref phase) = entry.sync_phase {
            let ago_str = format_relative_time(entry.sync_at_epoch, now);
            cached.sync_status = Some(format!("{phase} ({ago_str})"));
        }
        return cached;
    }

    let mut tele = EngramTelemetry {
        project,
        online: false,
        daemon_online: false,
        obs_count: 0,
        enrolled: false,
        sync_status: None,
        last_handoff: None,
    };

    let now = now_epoch_secs();
    if mtime.is_some()
        && let Ok(conn) = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
    {
        tele.online = true;
        let _ = conn.busy_timeout(Duration::from_millis(1500));

        if let Some(matched) = resolve_db_project_match(&conn, &candidates) {
            tele.project = matched;
        }

        // 1. Contar observaciones activas
        if let Ok(count) = conn.query_row(
            "SELECT count(*) FROM observations WHERE project = ?1 AND deleted_at IS NULL",
            [&tele.project],
            |row| row.get::<_, i64>(0),
        ) {
            tele.obs_count = usize::try_from(count).unwrap_or(0);
        }

        // 2. Comprobar si está sincronizado en cloud
        if let Ok(enrolled) = conn.query_row(
            "SELECT count(*) FROM sync_enrolled_projects WHERE project = ?1",
            [&tele.project],
            |row| row.get::<_, i64>(0),
        ) {
            tele.enrolled = enrolled > 0;
        }

        // 3. Último resumen de sesión (Active Handoff: type = 'session_summary')
        if let Ok(handoff) = conn.query_row(
            "SELECT id, content, CAST(created_at AS TEXT)
             FROM observations
             WHERE project = ?1 AND type = 'session_summary' AND deleted_at IS NULL
             ORDER BY id DESC LIMIT 1",
            [&tele.project],
            |row| {
                let id: i64 = row.get(0)?;
                let content: String = row.get(1)?;
                let raw_ts: Option<String> = row.get(2).ok();
                let at = raw_ts.as_deref().and_then(parse_engram_timestamp);
                Ok(HandoffSummary {
                    id,
                    created_at_epoch: at,
                    relative_time: format_relative_time(at, now),
                    lines: extract_handoff_lines(&content),
                })
            },
        ) {
            tele.last_handoff = Some(handoff);
        }
    }

    // 4. Sondeo del daemon local incluso si `engram.db` local aún no existe
    let mut sync_phase = None;
    let mut sync_at_epoch = None;
    if !is_custom_test_db {
        let port: u16 = custom_port
            .as_deref()
            .and_then(|p| p.parse().ok())
            .unwrap_or(7437);
        if let Some(health) = http_json_local(port, "/health")
            && health.get("status").and_then(|s| s.as_str()) == Some("ok")
        {
            tele.daemon_online = true;
            if (tele.enrolled || !tele.online)
                && let Some(sync_v) = http_json_local(
                    port,
                    &format!("/sync/status?project={}", query_escape(&tele.project)),
                )
            {
                let phase = sync_v.get("phase").and_then(|s| s.as_str()).unwrap_or("ok");
                let last_sync_epoch = sync_v
                    .get("last_sync_at")
                    .and_then(|s| s.as_str())
                    .and_then(parse_engram_timestamp);
                let ago_str = format_relative_time(last_sync_epoch, now);
                sync_phase = Some(phase.to_string());
                sync_at_epoch = last_sync_epoch;
                tele.sync_status = Some(format!("{phase} ({ago_str})"));
            }
        }
    }

    if let Ok(mut guard) = ENGRAM_CACHE.lock() {
        guard.insert(
            cache_key,
            EngramCacheEntry {
                mtime,
                cached_at: Instant::now(),
                sync_phase,
                sync_at_epoch,
                telemetry: tele.clone(),
            },
        );
    }

    tele
}
