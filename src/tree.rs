use crate::models::{McpConfigShape, McpScope, McpServer, ProjectTreeData, TreeNode};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const TREE_SKIP_NAMES: [&str; 5] = ["node_modules", "dist", "coverage", "build", "target"];

const TREE_MAX_ENTRIES: usize = 2000;
const MCP_CONFIG_MAX_BYTES: u64 = 1024 * 1024;

struct RankedDirEntry {
    entry: fs::DirEntry,
    is_dir: bool,
    can_descend: bool,
    name: String,
    sort_name: String,
    encounter_index: usize,
}

impl RankedDirEntry {
    fn new(entry: fs::DirEntry, encounter_index: usize) -> Option<Self> {
        let name = entry.file_name().to_str()?.to_string();
        if name.starts_with('.') || TREE_SKIP_NAMES.contains(&name.as_str()) {
            return None;
        }
        let ft = entry.file_type().ok()?;
        let (is_dir, can_descend) = if ft.is_symlink() {
            match fs::metadata(entry.path()).ok() {
                Some(m) if m.is_dir() => (true, false),
                Some(m) if m.is_file() => (false, false),
                _ => return None,
            }
        } else if ft.is_dir() {
            (true, true)
        } else if ft.is_file() {
            (false, false)
        } else {
            return None;
        };
        Some(Self {
            is_dir,
            can_descend,
            sort_name: name.to_lowercase(),
            name,
            entry,
            encounter_index,
        })
    }
}

impl PartialEq for RankedDirEntry {
    fn eq(&self, other: &Self) -> bool {
        self.is_dir == other.is_dir
            && self.sort_name == other.sort_name
            && self.encounter_index == other.encounter_index
    }
}

impl Eq for RankedDirEntry {}

impl PartialOrd for RankedDirEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedDirEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.is_dir, other.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => self
                .sort_name
                .cmp(&other.sort_name)
                .then_with(|| self.encounter_index.cmp(&other.encounter_index)),
        }
    }
}

fn select_best_entries(path: &Path, limit: usize) -> Vec<RankedDirEntry> {
    if limit == 0 {
        return Vec::new();
    }

    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };
    let mut best = BinaryHeap::with_capacity(limit);
    for entry in entries
        .flatten()
        .enumerate()
        .filter_map(|(index, entry)| RankedDirEntry::new(entry, index))
    {
        if best.len() < limit {
            best.push(entry);
        } else if best.peek().is_some_and(|worst| entry < *worst) {
            best.pop();
            best.push(entry);
        }
    }

    best.into_sorted_vec()
}

fn read_mcp_config(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MCP_CONFIG_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MCP_CONFIG_MAX_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

pub fn read_project_tree(target_dir: &str) -> ProjectTreeData {
    let mut data = ProjectTreeData {
        root_path: target_dir.to_string(),
        ..Default::default()
    };

    let p = Path::new(target_dir);
    if p.is_dir() {
        let top_entries = select_best_entries(p, TREE_MAX_ENTRIES);
        let mut child_budget = TREE_MAX_ENTRIES.saturating_sub(top_entries.len());
        let mut dirs_left = top_entries.iter().filter(|e| e.can_descend).count();

        for entry in top_entries {
            let is_dir = entry.is_dir;
            let name = entry.name;

            if is_dir {
                data.total_dirs += 1;
                let mut children = Vec::new();
                if entry.can_descend {
                    let per_dir_cap = if dirs_left > 0 {
                        child_budget.div_ceil(dirs_left)
                    } else {
                        0
                    };
                    dirs_left = dirs_left.saturating_sub(1);
                    for child in select_best_entries(&entry.entry.path(), per_dir_cap) {
                        child_budget = child_budget.saturating_sub(1);
                        if child.is_dir {
                            data.total_dirs += 1;
                        } else {
                            data.total_files += 1;
                        }
                        children.push(TreeNode {
                            name: child.name,
                            is_dir: child.is_dir,
                            children: Vec::new(),
                        });
                    }
                }
                data.tree.push(TreeNode {
                    name,
                    is_dir,
                    children,
                });
            } else {
                data.total_files += 1;
                data.tree.push(TreeNode {
                    name,
                    is_dir,
                    children: Vec::new(),
                });
            }
        }
    }

    data
}

/// Estado efectivo de un servidor MCP según la forma de configuración.
///
/// OpenCode V2 marca lo deshabilitado con `disabled: true`; la forma V1
/// compatible usa `enabled: false`. Sin ninguno de los dos campos el servidor
/// está habilitado.
pub fn mcp_state_from_value(shape: McpConfigShape, val: &serde_json::Value) -> bool {
    match shape {
        McpConfigShape::Servers => !val
            .get("disabled")
            .and_then(|b| b.as_bool())
            .unwrap_or(false),
        McpConfigShape::Legacy => val.get("enabled").and_then(|b| b.as_bool()).unwrap_or(true),
        McpConfigShape::AgentHost => val.get("enabled").and_then(|b| b.as_bool()).unwrap_or(true),
    }
}

fn resolve_against_cwd(raw: &Path) -> PathBuf {
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(raw)
    } else {
        raw.to_path_buf()
    };
    let mut clean = PathBuf::new();
    for comp in joined.components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                let _ = clean.pop();
            }
            other => clean.push(other.as_os_str()),
        }
    }
    clean
}

/// Determina el alcance real de un candidato de configuración.
pub fn mcp_scope_for(path: &Path, target_dir: &str, is_agent_host: bool) -> McpScope {
    if is_agent_host {
        return McpScope::Agent;
    }
    let resolved_target = resolve_against_cwd(Path::new(target_dir));
    let resolved_path = resolve_against_cwd(path);
    let normalized_target = crate::paths::normalize_path(&resolved_target.display().to_string());
    let normalized_path = crate::paths::normalize_path(&resolved_path.display().to_string());
    if normalized_target != "/"
        && !normalized_target.is_empty()
        && normalized_path.starts_with(&format!("{normalized_target}/"))
    {
        McpScope::Project
    } else {
        McpScope::Global
    }
}

pub fn read_mcp_servers(target_dir: &str) -> Vec<McpServer> {
    let mut mcps: Vec<McpServer> = Vec::new();

    let mut opencode_candidates = crate::paths::opencode_config_candidates(target_dir);
    let opencode_count = opencode_candidates.len();
    opencode_candidates.extend(crate::paths::pi_mcp_config_candidates(target_dir));

    for (idx, c) in opencode_candidates.iter().enumerate() {
        let is_agent_host = idx >= opencode_count;
        if !c.exists() {
            continue;
        }
        let Some(content) = read_mcp_config(c) else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        let scope = mcp_scope_for(c, target_dir, is_agent_host);
        let source = c.display().to_string();

        let mut register_mcp = |name: &str, entry: &serde_json::Value, shape: McpConfigShape| {
            if name.is_empty() || !entry.is_object() {
                return;
            }
            if mcps.iter().any(|m: &McpServer| m.name == name) {
                return;
            }

            let enabled = mcp_state_from_value(shape, entry);
            let mcp_type = entry
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or_else(|| {
                    if entry.get("url").is_some() {
                        "remote"
                    } else {
                        "local"
                    }
                })
                .to_string();

            mcps.push(McpServer {
                name: name.to_string(),
                mcp_type,
                enabled,
                scope,
                shape,
                source: source.clone(),
            });
        };

        if let Some(mcp) = val.get("mcp").and_then(|m| m.as_object()) {
            // Priorizar siempre V2 (`mcp.servers`) antes que las entradas legacy (`mcp.<name>`)
            // sin depender del orden alfabético en BTreeMap.
            if let Some(srv) = mcp.get("servers").and_then(|v| v.as_object()) {
                for (sk, sv) in srv {
                    register_mcp(sk, sv, McpConfigShape::Servers);
                }
            }
            for (k, v) in mcp {
                if k != "servers" {
                    register_mcp(k, v, McpConfigShape::Legacy);
                }
            }
        }
        if let Some(mcp_srv) = val.get("mcpServers").and_then(|m| m.as_object()) {
            for (k, v) in mcp_srv {
                register_mcp(k, v, McpConfigShape::AgentHost);
            }
        }
    }

    mcps
}

/// Clave y rango de bytes del valor de un miembro JSON dentro de `text`.
struct JsonSpan {
    key: String,
    key_span: std::ops::Range<usize>,
    value_span: std::ops::Range<usize>,
}

/// Indentación de la línea que contiene el offset `at`.
fn line_indent(text: &str, at: usize) -> String {
    let line_start = text[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    text[line_start..at]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn json_skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    i
}

/// Devuelve el offset inmediatamente posterior al string JSON que empieza en `i`.
fn json_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    if *bytes.get(start)? != b'"' {
        return None;
    }
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// Devuelve el offset inmediatamente posterior al valor JSON que empieza en `i`.
fn json_value_end(bytes: &[u8], start: usize) -> Option<usize> {
    match *bytes.get(start)? {
        b'"' => json_string_end(bytes, start),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut i = start;
            while i < bytes.len() {
                match bytes[i] {
                    b'"' => {
                        i = json_string_end(bytes, i)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(i + 1);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        }
        _ => {
            let mut i = start;
            while i < bytes.len()
                && !matches!(bytes[i], b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
            {
                i += 1;
            }
            (i > start).then_some(i)
        }
    }
}

/// Miembros de primer nivel del objeto que empieza en `start` (posición de `{`).
fn json_object_members(text: &str, start: usize) -> Option<Vec<JsonSpan>> {
    let bytes = text.as_bytes();
    if *bytes.get(start)? != b'{' {
        return None;
    }
    let mut members = Vec::new();
    let mut i = json_skip_ws(bytes, start + 1);
    if *bytes.get(i)? == b'}' {
        return Some(members);
    }
    loop {
        i = json_skip_ws(bytes, i);
        let key_start = i;
        let key_end = json_string_end(bytes, i)?;
        let key: String = serde_json::from_str(&text[key_start..key_end]).ok()?;
        i = json_skip_ws(bytes, key_end);
        if *bytes.get(i)? != b':' {
            return None;
        }
        i = json_skip_ws(bytes, i + 1);
        let value_end = json_value_end(bytes, i)?;
        members.push(JsonSpan {
            key,
            key_span: key_start..key_end,
            value_span: i..value_end,
        });
        i = json_skip_ws(bytes, value_end);
        match *bytes.get(i)? {
            b',' => i += 1,
            b'}' => return Some(members),
            _ => return None,
        }
    }
}

/// Ruta de claves hasta el objeto de un servidor dentro del archivo de config.
fn mcp_member_path(shape: McpConfigShape, name: &str) -> Option<Vec<&str>> {
    match shape {
        McpConfigShape::Servers => Some(vec!["mcp", "servers", name]),
        McpConfigShape::Legacy => Some(vec!["mcp", name]),
        McpConfigShape::AgentHost => None,
    }
}

/// Span del valor JSON ubicado en `path`, relativo a `text`.
fn json_path_span(text: &str, path: &[&str]) -> Option<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let root = json_skip_ws(bytes, 0);
    let mut span = root..json_value_end(bytes, root)?;

    for (idx, segment) in path.iter().enumerate() {
        let member = json_object_members(text, span.start)?
            .into_iter()
            .find(|m| m.key == *segment)?;
        if idx + 1 == path.len() {
            return Some(member.value_span);
        }
        span = member.value_span;
    }
    Some(span)
}

/// Habilita o deshabilita un servidor MCP editando SOLO el campo de control
/// dentro del archivo de origen.
///
/// La edición es quirúrgica sobre el texto original: no se reescribe el JSON
/// completo (eso reordenaría todas las claves porque `serde_json::Map` es un
/// `BTreeMap` sin `preserve_order`). Devuelve `Ok(true)` si el archivo cambió.
pub fn set_mcp_enabled(
    source: &str,
    name: &str,
    shape: McpConfigShape,
    enable: bool,
) -> Result<bool, String> {
    let control_key = shape
        .control_key()
        .ok_or_else(|| "Este entorno no tiene un interruptor verificado.".to_string())?;
    let member_path = mcp_member_path(shape, name)
        .ok_or_else(|| "Este entorno no tiene un interruptor verificado.".to_string())?;

    let path = Path::new(source);
    let text = crate::paths::read_config_string(path)
        .map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|_| "El archivo no es JSON estricto; se niega a editarlo.".to_string())?;

    let object_span = json_path_span(&text, &member_path)
        .ok_or_else(|| format!("No se encontró «{name}» en {}", path.display()))?;
    let members = json_object_members(&text, object_span.start)
        .ok_or_else(|| format!("«{name}» no es un objeto en {}", path.display()))?;

    let desired = if control_key == "disabled" {
        !enable
    } else {
        enable
    };
    let literal = if desired { "true" } else { "false" };

    let edited = if let Some(member) = members.iter().rfind(|m| m.key == control_key) {
        let current = text[member.value_span.clone()].trim();
        if current != "true" && current != "false" {
            return Err(format!(
                "«{control_key}» de «{name}» no es booleano; no se sobrescribe."
            ));
        }
        if current == literal {
            return Ok(false);
        }
        format!(
            "{}{}{}",
            &text[..member.value_span.start],
            literal,
            &text[member.value_span.end..]
        )
    } else {
        let pair = format!("\"{control_key}\": {literal}");
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        match members.last() {
            Some(last) => {
                let pos = last.value_span.end;
                let multiline = text[object_span.start..pos].contains('\n');
                let separator = if multiline {
                    format!(",{newline}{}", line_indent(&text, last.key_span.start))
                } else {
                    ",".to_string()
                };
                format!("{}{separator}{pair}{}", &text[..pos], &text[pos..])
            }
            None => {
                let pos = object_span.start + 1;
                format!("{}{pair}{}", &text[..pos], &text[pos..])
            }
        }
    };

    let reparsed: serde_json::Value = serde_json::from_str(&edited)
        .map_err(|e| format!("La edición produjo JSON inválido: {e}"))?;
    let segment = member_path.last().copied().unwrap_or_default();
    let verified = reparsed
        .get(member_path[0])
        .and_then(|v| {
            member_path[1..]
                .iter()
                .try_fold(v, |acc, key| acc.get(*key))
        })
        .map(|v| mcp_state_from_value(shape, v))
        .ok_or_else(|| format!("La edición no se pudo verificar para «{segment}»."))?;
    if verified != enable {
        return Err(format!(
            "La edición no dejó «{segment}» en el estado pedido."
        ));
    }

    crate::paths::atomic_write_bytes(path, edited.as_bytes())
        .map_err(|e| format!("No se pudo escribir {}: {e}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod mcp_scope_tests {
    use super::*;

    #[test]
    fn mcp_scope_for_resolves_relative_dot_target_dir_as_project() {
        let cwd = std::env::current_dir().unwrap();
        let cfg = cwd.join("opencode.json");
        assert_eq!(mcp_scope_for(&cfg, ".", false), McpScope::Project);
        assert_eq!(mcp_scope_for(&cfg, ".", true), McpScope::Agent);
    }
}
