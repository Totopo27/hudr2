use crate::config::resolve_engram_db_path;
use crate::models::EngramTelemetry;
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

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

/// Consulta instantánea en SQLite de Engram con caché mtime
pub fn read_engram_telemetry(project_dir: &str) -> EngramTelemetry {
    let project = Path::new(project_dir)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("proyecto")
        .to_string();

    let mut tele = EngramTelemetry {
        project: project.clone(),
        online: false,
        obs_count: 0,
        enrolled: false,
        cloud_host: None,
    };

    let db_path = resolve_engram_db_path();
    let metadata = match std::fs::metadata(&db_path) {
        Ok(m) => m,
        Err(_) => return tele,
    };

    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    if let Ok(guard) = ENGRAM_CACHE.lock() {
        if let Some(entry) = guard.get(&project) {
            if entry.mtime == mtime {
                return entry.telemetry.clone();
            }
        }
    }

    let conn = match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(c) => c,
        Err(_) => return tele,
    };

    tele.online = true;
    let _ = conn.busy_timeout(std::time::Duration::from_millis(1500));

    // Contar observaciones activas
    if let Ok(count) = conn.query_row(
        "SELECT count(*) FROM observations WHERE project = ? AND deleted_at IS NULL",
        [&tele.project],
        |row| row.get::<_, usize>(0),
    ) {
        tele.obs_count = count;
    }

    // Comprobar si está sincronizado en cloud
    if let Ok(enrolled) = conn.query_row(
        "SELECT count(*) FROM sync_enrolled_projects WHERE project = ?",
        [&tele.project],
        |row| row.get::<_, usize>(0),
    ) {
        tele.enrolled = enrolled > 0;
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
