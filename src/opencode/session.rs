//! Acceso de solo lectura a `opencode.db`: resolución de la sesión asociada a un pane
//! de Herdr y extracción de rutas desde las llamadas a herramientas.

use super::model_info::{
    clean_model_name, clean_opencode_terminal_title, resolve_model_context_limit,
};
use crate::models::{HerdrPane, OpenCodeSession};
use rusqlite::{Connection, Row};

/// Condición SQL de sesión "viva": no archivada, archivada en 0, o actualizada después de archivarse.
const ALIVE_CONDITION: &str = "(time_archived IS NULL OR time_archived = 0 OR (time_updated IS NOT NULL AND time_archived IS NOT NULL AND time_updated > time_archived))";

#[derive(Debug, Clone)]
pub struct DbSessionRecord {
    pub id: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub directory: Option<String>,
    pub title: Option<String>,
    pub time_updated: Option<i64>,
    pub time_archived: Option<i64>,
    pub tokens_input: Option<i64>,
    pub tokens_output: Option<i64>,
    pub cost: Option<f64>,
}

impl DbSessionRecord {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            agent: row.get(1)?,
            model: row.get(2)?,
            directory: row.get(3)?,
            title: row.get(4)?,
            time_updated: row.get(5)?,
            time_archived: row.get(6)?,
            tokens_input: row.get(7)?,
            tokens_output: row.get(8)?,
            cost: row.get(9)?,
        })
    }

    /// Copia agente, modelo (y su límite de contexto), tokens y costo al resumen de sesión.
    pub(super) fn apply_to(&self, session: &mut OpenCodeSession) {
        if let Some(ref a) = self.agent {
            if !a.trim().is_empty() {
                session.agent = a.clone();
            }
        } else if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(self.model.as_deref().unwrap_or(""))
            && let Some(agent_val) = parsed
                .get("agent")
                .or_else(|| parsed.get("role"))
                .and_then(|a| a.as_str())
        {
            session.agent = agent_val.to_string();
        }
        if let Some(ref m) = self.model {
            let clean = clean_model_name(m);
            session.model_short = clean.split('/').next_back().unwrap_or(&clean).to_string();
            session.context_limit = resolve_model_context_limit(&clean);
            session.model = clean;
        }
        if let Some(ti) = self.tokens_input {
            session.tokens_input = ti;
        }
        if let Some(to) = self.tokens_output {
            session.tokens_output = to;
        }
        if let Some(c) = self.cost {
            session.cost = c;
        }
    }
}

pub fn table_exists(conn: &Connection, table_name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?",
        [table_name],
        |_| Ok(()),
    )
    .is_ok()
}

pub fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
    let sql = format!("PRAGMA table_info({table})");
    if let Ok(mut stmt) = conn.prepare(&sql)
        && let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1))
    {
        return rows.flatten().any(|name| name.eq_ignore_ascii_case(col));
    }
    false
}

/// Resuelve dinámicamente el nombre de la tabla de sesiones activa:
/// OpenCode 1.19+ migró a `session` (y turnos a `part`/`message`), mientras que versiones
/// anteriores usan `session_v2`. Si ambas coexisten, compara la marca de tiempo más reciente.
pub fn resolve_session_table_name(conn: &Connection) -> &'static str {
    let has_session = table_exists(conn, "session");
    let has_session_v2 = table_exists(conn, "session_v2");
    match (has_session, has_session_v2) {
        (true, false) => "session",
        (false, true) => "session_v2",
        (true, true) => {
            let ts_session: i64 = conn
                .query_row(
                    "SELECT MAX(COALESCE(time_updated, time_created, 0)) FROM session",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let ts_v2: i64 = conn
                .query_row(
                    "SELECT MAX(COALESCE(time_updated, time_created, 0)) FROM session_v2",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if ts_session >= ts_v2 {
                "session"
            } else {
                "session_v2"
            }
        }
        _ => "session_v2",
    }
}

pub fn session_select_columns(conn: &Connection, table: &str) -> String {
    let has_agent = has_column(conn, table, "agent");
    let agent_expr = if has_agent { "agent" } else { "NULL AS agent" };
    let has_model = has_column(conn, table, "model");
    let model_expr = if has_model { "model" } else { "NULL AS model" };
    let has_title = has_column(conn, table, "title");
    let title_expr = if has_title { "title" } else { "NULL AS title" };
    let has_dir = has_column(conn, table, "directory");
    let dir_expr = if has_dir { "directory" } else { "NULL AS directory" };
    let has_tu = has_column(conn, table, "time_updated");
    let tu_expr = if has_tu { "time_updated" } else { "NULL AS time_updated" };
    let has_ta = has_column(conn, table, "time_archived");
    let ta_expr = if has_ta { "time_archived" } else { "NULL AS time_archived" };
    let has_ti = has_column(conn, table, "tokens_input");
    let ti_expr = if has_ti { "tokens_input" } else { "NULL AS tokens_input" };
    let has_to = has_column(conn, table, "tokens_output");
    let to_expr = if has_to { "tokens_output" } else { "NULL AS tokens_output" };
    let has_cost = has_column(conn, table, "cost");
    let cost_expr = if has_cost { "cost" } else { "NULL AS cost" };

    format!("id, {agent_expr}, {model_expr}, {dir_expr}, {title_expr}, {tu_expr}, {ta_expr}, {ti_expr}, {to_expr}, {cost_expr}")
}

/// Empareja las tablas de mensajes y partes correspondientes a la tabla de sesiones activa.
/// Evita leer tablas huérfanas `session_message`/`session_part` cuando la sesión activa vive en `session`.
pub fn paired_message_and_part_tables(
    conn: &Connection,
    session_table: &str,
) -> (Option<&'static str>, Option<&'static str>) {
    let msg_table = if session_table == "session" {
        if table_exists(conn, "message") {
            Some("message")
        } else if table_exists(conn, "session_message") {
            Some("session_message")
        } else {
            None
        }
    } else if table_exists(conn, "session_message") {
        Some("session_message")
    } else if table_exists(conn, "message") {
        Some("message")
    } else {
        None
    };

    let part_table = if session_table == "session" {
        if table_exists(conn, "part") {
            Some("part")
        } else if table_exists(conn, "session_part") {
            Some("session_part")
        } else {
            None
        }
    } else if table_exists(conn, "session_part") {
        Some("session_part")
    } else if table_exists(conn, "part") {
        Some("part")
    } else {
        None
    };

    (msg_table, part_table)
}

fn extract_model_json_from_message_value(val: &serde_json::Value) -> Option<String> {
    let top_provider = val
        .get("providerID")
        .or_else(|| val.get("provider_id"))
        .and_then(|p| p.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let (model_id, nested_provider) = if let Some(m_val) = val.get("model") {
        if let Some(m_str) = m_val.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            (Some(m_str), None)
        } else if m_val.is_object() {
            let mid = m_val
                .get("modelID")
                .or_else(|| m_val.get("model_id"))
                .or_else(|| m_val.get("id"))
                .and_then(|i| i.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let pid = m_val
                .get("providerID")
                .or_else(|| m_val.get("provider_id"))
                .and_then(|p| p.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            (mid, pid)
        } else {
            (None, None)
        }
    } else {
        let mid = val
            .get("modelID")
            .or_else(|| val.get("model_id"))
            .and_then(|i| i.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        (mid, None)
    };

    let mid = model_id?;
    if let Some(pid) = nested_provider.or(top_provider) {
        Some(
            serde_json::json!({
                "id": mid,
                "providerID": pid,
            })
            .to_string(),
        )
    } else {
        Some(mid.to_string())
    }
}

/// En el esquema `session` de OpenCode 1.19+, las columnas `agent` y `model` no viven en `session`,
/// sino dentro del JSON `message.data` de los turnos de la sesión. Hidrata esos metadatos al vuelo.
pub fn hydrate_session_from_messages(
    conn: &Connection,
    session_table: &str,
    rec: &mut DbSessionRecord,
) {
    if rec.agent.is_some() && rec.model.is_some() {
        return;
    }
    let (Some(msg_tbl), _) = paired_message_and_part_tables(conn, session_table) else {
        return;
    };
    if !has_column(conn, msg_tbl, "data") {
        return;
    }
    let order_col = if has_column(conn, msg_tbl, "time_created") {
        "time_created"
    } else {
        "rowid"
    };
    let sql = format!(
        "SELECT data FROM {msg_tbl} WHERE session_id = ? ORDER BY {order_col} DESC LIMIT 10"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return;
    };
    let Ok(rows) = stmt.query_map([&rec.id], |r| r.get::<_, String>(0)) else {
        return;
    };

    for raw in rows.flatten() {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        if rec.agent.is_none()
            && let Some(ag) = val
                .get("agent")
                .or_else(|| val.get("mode"))
                .and_then(|a| a.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            rec.agent = Some(ag.to_string());
        }
        if rec.model.is_none()
            && let Some(m_json) = extract_model_json_from_message_value(&val)
        {
            rec.model = Some(m_json);
        }
        if rec.agent.is_some() && rec.model.is_some() {
            break;
        }
    }
}

/// Ejecuta `SELECT <columnas> FROM <table> <where_clause>` y devuelve la primera fila.
fn query_one<P: rusqlite::Params>(
    conn: &Connection,
    where_clause: &str,
    params: P,
) -> Option<DbSessionRecord> {
    let table = resolve_session_table_name(conn);
    let cols = session_select_columns(conn, table);
    let sql = format!("SELECT {cols} FROM {table} {where_clause}");
    let mut stmt = conn.prepare(&sql).ok()?;
    let mut rec = stmt.query_row(params, DbSessionRecord::from_row).ok()?;
    hydrate_session_from_messages(conn, table, &mut rec);
    Some(rec)
}

/// Sesiones vivas más recientes (hasta 30), usadas como respaldo cuando ningún pane resuelve.
pub(super) fn recent_alive_sessions(conn: &Connection) -> Vec<DbSessionRecord> {
    let table = resolve_session_table_name(conn);
    let cols = session_select_columns(conn, table);
    let sql = format!(
        "SELECT {cols} FROM {table} WHERE {ALIVE_CONDITION} ORDER BY time_updated DESC LIMIT 30"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], DbSessionRecord::from_row) else {
        return Vec::new();
    };
    let mut list: Vec<DbSessionRecord> = rows.flatten().collect();
    for rec in &mut list {
        hydrate_session_from_messages(conn, table, rec);
    }
    list
}

fn direct_or_nested_path(val: &serde_json::Value) -> Option<String> {
    val.get("input")
        .and_then(|i| {
            i.get("path")
                .or_else(|| i.get("file_path"))
                .or_else(|| i.get("filePath"))
        })
        .or_else(|| val.get("path"))
        .or_else(|| val.get("file_path"))
        .or_else(|| val.get("filePath"))
        .and_then(|p| p.as_str())
        .map(str::to_string)
}

fn extract_path_from_field(field: &serde_json::Value) -> Option<String> {
    if field.is_object() {
        direct_or_nested_path(field)
    } else if let Some(raw) = field.as_str()
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw)
    {
        direct_or_nested_path(&parsed)
    } else {
        None
    }
}

pub(super) fn extract_tool_path(tool_obj: &serde_json::Value) -> Option<String> {
    tool_obj
        .get("state")
        .and_then(extract_path_from_field)
        .or_else(|| tool_obj.get("input").and_then(extract_path_from_field))
}

pub fn resolve_session_for_pane(conn: &Connection, pane: &HerdrPane) -> Option<DbSessionRecord> {
    if let Some(ref ses_id) = pane.agent_session_id
        && let Some(rec) = query_one(conn, "WHERE id = ?", [ses_id])
    {
        return Some(rec);
    }

    if let Some(ref raw_cwd) = pane.cwd {
        let norm_cwd = raw_cwd.replace('\\', "/");
        let raw_title = pane
            .terminal_title_stripped
            .as_deref()
            .or(pane.terminal_title.as_deref())
            .unwrap_or("");
        let title_clean = clean_opencode_terminal_title(raw_title);

        // 1. Coincidencia exacta por título y directorio (sin filtrar por time_archived, ya que el pane está vivo)
        if !title_clean.is_empty()
            && let Some(rec) = query_one(
                conn,
                "WHERE (directory = ? OR directory = ?) AND title = ? ORDER BY time_updated DESC LIMIT 1",
                rusqlite::params![raw_cwd, norm_cwd, title_clean],
            )
        {
            return Some(rec);
        }

        // 2. Coincidencia por directorio en sesiones activas (no archivadas o actualizadas después de archivarse)
        if let Some(rec) = query_one(
            conn,
            &format!(
                "WHERE {ALIVE_CONDITION} AND (directory = ? OR directory = ?) ORDER BY time_updated DESC LIMIT 1"
            ),
            rusqlite::params![raw_cwd, norm_cwd],
        ) {
            return Some(rec);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_tool_path_supports_object_and_serialized_json_shapes() {
        assert_eq!(
            extract_tool_path(&json!({"state": {"input": {"file_path": "src/a.rs"}}})).as_deref(),
            Some("src/a.rs")
        );
        assert_eq!(
            extract_tool_path(&json!({"state": r#"{"path":"src/b.rs"}"#})).as_deref(),
            Some("src/b.rs")
        );
        assert_eq!(
            extract_tool_path(&json!({"input": r#"{"file_path":"src/c.rs"}"#})).as_deref(),
            Some("src/c.rs")
        );
        assert_eq!(extract_tool_path(&json!({"type": "tool"})), None);
    }
}
