use crate::models::{EngramTelemetry, HandoffSummary};
use crate::paths::resolve_engram_db_path;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
struct EngramCacheEntry {
    mtime: SystemTime,
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
    let req = format!(
        "GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nAccept: application/json\r\n\r\n"
    );
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
    content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
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

/// Consulta instantánea en SQLite de Engram + estado del daemon y último session_summary (Handoff) con caché mtime
pub fn read_engram_telemetry(project_dir: &str) -> EngramTelemetry {
    let project = Path::new(project_dir)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("proyecto")
        .to_string();

    let mut tele = EngramTelemetry {
        project: project.clone(),
        online: false,
        daemon_online: false,
        obs_count: 0,
        enrolled: false,
        cloud_host: None,
        sync_status: None,
        last_handoff: None,
    };

    let db_path = resolve_engram_db_path();
    let metadata = match std::fs::metadata(&db_path) {
        Ok(m) => m,
        Err(_) => return tele,
    };

    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    if let Ok(guard) = ENGRAM_CACHE.lock()
        && let Some(entry) = guard.get(&project)
        && entry.mtime == mtime
    {
        return entry.telemetry.clone();
    }

    let conn = match Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return tele,
    };

    tele.online = true;
    let _ = conn.busy_timeout(Duration::from_millis(1500));

    // 1. Contar observaciones activas
    if let Ok(count) = conn.query_row(
        "SELECT count(*) FROM observations WHERE project = ?1 AND deleted_at IS NULL",
        [&tele.project],
        |row| row.get::<_, usize>(0),
    ) {
        tele.obs_count = count;
    }

    // 2. Comprobar si está sincronizado en cloud
    if let Ok(enrolled) = conn.query_row(
        "SELECT count(*) FROM sync_enrolled_projects WHERE project = ?1",
        [&tele.project],
        |row| row.get::<_, usize>(0),
    ) {
        tele.enrolled = enrolled > 0;
    }

    // 3. Último resumen de sesión (Active Handoff: type = 'session_summary')
    let now = now_epoch_secs();
    if let Ok(handoff) = conn.query_row(
        "SELECT id, content, CAST(strftime('%s', created_at) AS INTEGER)
         FROM observations
         WHERE project = ?1 AND type = 'session_summary' AND deleted_at IS NULL
         ORDER BY id DESC LIMIT 1",
        [&tele.project],
        |row| {
            let id: i64 = row.get(0)?;
            let content: String = row.get(1)?;
            let at: Option<i64> = row.get(2).ok();
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

    // 4. Estado opcional del daemon local (solo si ENGRAM_PORT está definido o no estamos en un DB de test aislado)
    let is_custom_test_db = std::env::var("ENGRAM_DB_PATH").is_ok()
        && std::env::var("ENGRAM_PORT").is_err();
    if !is_custom_test_db {
        let port: u16 = std::env::var("ENGRAM_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(7437);
        if let Some(health) = http_json_local(port, "/health")
            && health.get("status").and_then(|s| s.as_str()) == Some("ok")
        {
            tele.daemon_online = true;
            if tele.enrolled
                && let Some(sync_v) = http_json_local(
                    port,
                    &format!("/sync/status?project={}", query_escape(&tele.project)),
                )
            {
                let phase = sync_v
                    .get("phase")
                    .and_then(|s| s.as_str())
                    .unwrap_or("ok");
                let last_sync_epoch = sync_v
                    .get("last_sync_at")
                    .and_then(|s| s.as_str())
                    .and_then(|ts| {
                        conn.query_row(
                            "SELECT CAST(strftime('%s', ?1) AS INTEGER)",
                            [ts],
                            |r| r.get::<_, i64>(0),
                        )
                        .ok()
                    });
                let ago_str = format_relative_time(last_sync_epoch, now);
                tele.sync_status = Some(format!("{phase} ({ago_str})"));
            }
        }
    }

    if let Ok(mut guard) = ENGRAM_CACHE.lock() {
        guard.insert(
            project,
            EngramCacheEntry {
                mtime,
                telemetry: tele.clone(),
            },
        );
    }

    tele
}
