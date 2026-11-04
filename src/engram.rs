use crate::config::resolve_engram_db_path;
use crate::models::EngramTelemetry;
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

/// Consulta instantánea en SQLite de Engram
pub fn read_engram_telemetry(project_dir: &str) -> EngramTelemetry {
    let mut tele = EngramTelemetry {
        project: Path::new(project_dir)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("proyecto")
            .to_string(),
        online: false,
        obs_count: 0,
        enrolled: false,
        cloud_host: None,
    };

    let db_path = resolve_engram_db_path();
    if !db_path.exists() {
        return tele;
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

    tele
}
