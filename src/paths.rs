use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
const WINDOWS_FALLBACK_DRIVES: [char; 4] = ['C', 'D', 'E', 'F'];

#[cfg(target_os = "windows")]
fn push_windows_drive_candidates(candidates: &mut Vec<PathBuf>, suffixes: &[&str]) {
    for drive in WINDOWS_FALLBACK_DRIVES {
        for suffix in suffixes {
            candidates.push(PathBuf::from(format!("{drive}:/{suffix}")));
        }
    }
}

/// Normaliza un path para comparaciones case-insensitive y cross-platform
pub fn normalize_path(p: &str) -> String {
    p.replace('\\', "/")
        .to_lowercase()
        .trim_end_matches('/')
        .to_string()
}

/// Resuelve el directorio de credenciales / autenticaciones de Cliproxy
pub fn resolve_auths_dir() -> PathBuf {
    // 1. Variable de entorno explícita (mayor prioridad para cualquier usuario o CI)
    if let Ok(dir) = std::env::var("CLIPROXY_AUTHS_DIR") {
        let p = PathBuf::from(dir.trim());
        if p.exists() {
            return p;
        }
    }

    // 2. Extraer 'authsDir' o 'cliproxy_auths_dir' desde opencode.json si está configurado
    let candidates_cfg = opencode_config_candidates(".");
    for cfg_path in candidates_cfg {
        if cfg_path.exists()
            && let Ok(content) = std::fs::read_to_string(&cfg_path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            if let Some(auth_dir) = val
                .get("providers")
                .and_then(|p| p.get("cliproxy"))
                .and_then(|c| c.get("authsDir").or_else(|| c.get("auths_dir")))
                .and_then(|s| s.as_str())
            {
                let p = PathBuf::from(auth_dir.trim());
                if p.exists() {
                    return p;
                }
            }
            if let Some(auth_dir) = val.get("cliproxy_auths_dir").and_then(|s| s.as_str()) {
                let p = PathBuf::from(auth_dir.trim());
                if p.exists() {
                    return p;
                }
            }
        }
    }

    // 3. Rutas estándar en el HOME o CONFIG del usuario
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut candidates = vec![
        home.join(".cliproxy").join("auths"),
        home.join("cliproxyapi").join("auths"),
    ];

    if let Some(config_dir) = dirs::config_dir() {
        candidates.push(config_dir.join("cliproxy").join("auths"));
    }

    #[cfg(target_os = "windows")]
    {
        // En Windows, escanear unidades de disco para instalaciones portables
        if let Ok(system_drive) = std::env::var("SystemDrive") {
            candidates.push(PathBuf::from(format!("{system_drive}/cliproxyapi/auths")));
        }
        push_windows_drive_candidates(&mut candidates, &["cliproxyapi/auths", ".cliproxy/auths"]);
    }

    for c in candidates {
        if c.exists() {
            return c;
        }
    }

    home.join(".cliproxy").join("auths")
}

/// Resuelve la ruta al archivo SQLite de Engram
pub fn resolve_engram_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("ENGRAM_DB_PATH") {
        let p_buf = PathBuf::from(p.trim());
        if p_buf.exists() {
            return p_buf;
        }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut candidates = vec![home.join(".engram").join("engram.db")];

    if let Some(data_dir) = dirs::data_local_dir() {
        candidates.push(data_dir.join("engram").join("engram.db"));
    }

    #[cfg(target_os = "windows")]
    {
        push_windows_drive_candidates(&mut candidates, &[".engram/engram.db"]);
    }

    for c in candidates {
        if c.exists() {
            return c;
        }
    }

    home.join(".engram").join("engram.db")
}

/// Resuelve la ruta al archivo SQLite de OpenCode
pub fn resolve_opencode_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("OPENCODE_DB_PATH") {
        let p_buf = PathBuf::from(p.trim());
        if p_buf.exists() {
            return p_buf;
        }
    }

    if let Ok(dir) = std::env::var("OPENCODE_CONFIG_DIR") {
        let p_buf = PathBuf::from(dir.trim()).join("opencode.db");
        if p_buf.exists() {
            return p_buf;
        }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut candidates = Vec::new();

    // 1. Estándar XDG / Local Data
    if let Some(data_dir) = dirs::data_local_dir() {
        candidates.push(data_dir.join("opencode").join("opencode.db"));
    }

    // 2. ~/.local/share/opencode/opencode.db (común en Linux, macOS y ports Unix en Windows)
    candidates.push(
        home.join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db"),
    );

    // 3. Directorios de configuración estándar y alternativos (~/.config/opencode, ~/.opencode)
    if let Some(config_dir) = dirs::config_dir() {
        candidates.push(config_dir.join("opencode").join("opencode.db"));
    }
    candidates.push(home.join(".config").join("opencode").join("opencode.db"));
    candidates.push(home.join(".opencode").join("opencode.db"));

    // 4. macOS Library Application Support
    #[cfg(target_os = "macos")]
    {
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join("opencode")
                .join("opencode.db"),
        );
    }

    // 5. Fallbacks locales de Windows para desarrollo o setups multi-disco
    #[cfg(target_os = "windows")]
    {
        push_windows_drive_candidates(
            &mut candidates,
            &[
                ".local/share/opencode/opencode.db",
                ".config/opencode/opencode.db",
                ".opencode/opencode.db",
            ],
        );
    }

    let mut existing: Vec<PathBuf> = candidates.into_iter().filter(|c| c.exists()).collect();
    if existing.is_empty() {
        return home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("opencode.db");
    }
    if existing.len() == 1 {
        return existing.remove(0);
    }

    // Si existen múltiples copias, ordenar por fecha de modificación más reciente (activa)
    existing.sort_by(|a, b| {
        let ma = a.metadata().and_then(|m| m.modified()).ok();
        let mb = b.metadata().and_then(|m| m.modified()).ok();
        mb.cmp(&ma)
    });

    existing.remove(0)
}

/// Resuelve el binario ejecutable de Engram en cualquier sistema operativo
pub fn resolve_engram_binary() -> Option<PathBuf> {
    if let Ok(bin) = std::env::var("ENGRAM_BIN") {
        let p = PathBuf::from(bin.trim());
        if p.exists() {
            return Some(p);
        }
    }

    // 1. Verificar si 'engram' está disponible globalmente en PATH
    let probe_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    if let Ok(output) = Command::new(probe_cmd).arg("engram").output()
        && output.status.success()
    {
        let path_str = String::from_utf8_lossy(&output.stdout);
        if let Some(first_line) = path_str.lines().next() {
            let p = PathBuf::from(first_line.trim());
            if p.exists() {
                return Some(p);
            }
        }
        return Some(PathBuf::from("engram"));
    }

    // 2. Probar rutas conocidas según el sistema operativo
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut candidates = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Some(d) = dirs::data_local_dir() {
            candidates.push(d.join("engram").join("bin").join("engram.exe"));
        }
        candidates.push(
            home.join("AppData")
                .join("Local")
                .join("engram")
                .join("bin")
                .join("engram.exe"),
        );
        candidates.push(home.join(".local").join("bin").join("engram.exe"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        candidates.push(home.join(".local").join("bin").join("engram"));
        if let Some(d) = dirs::data_local_dir() {
            candidates.push(d.join("engram").join("bin").join("engram"));
        }
        candidates.push(PathBuf::from("/usr/local/bin/engram"));
        candidates.push(PathBuf::from("/usr/bin/engram"));
    }

    candidates.into_iter().find(|c| c.exists())
}

/// Candidatos para archivos de configuración de OpenCode (mcp servers, etc.)
pub fn opencode_config_candidates(target_dir: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    if let Ok(config_dir) = std::env::var("OPENCODE_CONFIG_DIR") {
        candidates.push(Path::new(&config_dir).join("opencode.json"));
    }

    // Proyecto actual
    candidates.push(Path::new(target_dir).join("opencode.json"));
    candidates.push(
        Path::new(target_dir)
            .join(".opencode")
            .join("opencode.json"),
    );

    // Configuración de usuario estándar (XDG / AppData)
    if let Some(config_dir) = dirs::config_dir() {
        candidates.push(config_dir.join("opencode").join("opencode.json"));
    }
    candidates.push(home.join(".config").join("opencode").join("opencode.json"));
    candidates.push(home.join(".opencode").join("opencode.json"));

    #[cfg(target_os = "windows")]
    {
        push_windows_drive_candidates(&mut candidates, &[".opencode/opencode.json"]);
    }

    candidates
}

/// Candidatos para archivos de configuración MCP de Pi / Gentle Shell (`mcp.json`)
pub fn pi_mcp_config_candidates(target_dir: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    if let Ok(agent_dir) = std::env::var("PI_AGENT_DIR") {
        candidates.push(Path::new(&agent_dir).join("mcp.json"));
    }

    candidates.push(Path::new(target_dir).join(".gentle-shell").join("mcp.json"));
    candidates.push(Path::new(target_dir).join(".pi").join("mcp.json"));
    candidates.push(home.join(".gentle-shell").join("agent").join("mcp.json"));
    candidates.push(home.join(".pi").join("agent").join("mcp.json"));

    candidates
}

/// Ruta al archivo de configuración persistente de hudr2 (`hudr2.json`).
/// Solo escribe cuando el directorio padre existe o cuando se especifica explícitamente.
pub fn resolve_hudr2_config_path() -> PathBuf {
    if let Ok(p) = std::env::var("HUDR2_CONFIG_PATH") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if let Some(cfg_dir) = dirs::config_dir() {
        return cfg_dir.join("hudr2").join("hudr2.json");
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".config").join("hudr2").join("hudr2.json")
}
