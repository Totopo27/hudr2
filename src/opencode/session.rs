//! Acceso de solo lectura a `opencode.db`: resolución de la sesión asociada a un pane
//! de Herdr y extracción de rutas desde las llamadas a herramientas.

use super::model_info::{
    clean_model_name, clean_opencode_terminal_title, resolve_model_context_limit,
};
use crate::models::{HerdrPane, OpenCodeSession};
use rusqlite::{Connection, Row};

/// Columnas de `session_v2` en el orden que espera [`DbSessionRecord::from_row`].
const SESSION_COLUMNS: &str = "id, agent, model, directory, title, time_updated, time_archived, tokens_input, tokens_output, cost";

/// Condición SQL de sesión "viva": no archivada, o actualizada después de archivarse.
const ALIVE_CONDITION: &str = "(time_archived IS NULL OR (time_updated IS NOT NULL AND time_archived IS NOT NULL AND time_updated > time_archived))";

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
            session.agent = a.clone();
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

/// Ejecuta `SELECT <columnas> FROM session_v2 <where_clause>` y devuelve la primera fila.
fn query_one<P: rusqlite::Params>(
    conn: &Connection,
    where_clause: &str,
    params: P,
) -> Option<DbSessionRecord> {
    let sql = format!("SELECT {SESSION_COLUMNS} FROM session_v2 {where_clause}");
    let mut stmt = conn.prepare(&sql).ok()?;
    stmt.query_row(params, DbSessionRecord::from_row).ok()
}

/// Sesiones vivas más recientes (hasta 30), usadas como respaldo cuando ningún pane resuelve.
pub(super) fn recent_alive_sessions(conn: &Connection) -> Vec<DbSessionRecord> {
    let sql = format!(
        "SELECT {SESSION_COLUMNS} FROM session_v2 WHERE {ALIVE_CONDITION} ORDER BY time_updated DESC LIMIT 30"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], DbSessionRecord::from_row) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn direct_or_nested_path(val: &serde_json::Value) -> Option<String> {
    val.get("input")
        .and_then(|i| i.get("path").or_else(|| i.get("file_path")))
        .or_else(|| val.get("path"))
        .or_else(|| val.get("file_path"))
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
