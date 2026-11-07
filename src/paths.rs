use std::path::{Path, PathBuf};

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

/// Escribe `body` de forma atómica usando un archivo temporal con sufijo `.{pid}.tmp` + `rename`
/// para evitar archivos truncados o colisiones entre múltiples instancias del sidebar.
pub fn atomic_write_bytes(path: &Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = PathBuf::from(format!("{}.{}.tmp", path.display(), std::process::id()));
    if let Err(err) = std::fs::write(&tmp, body).and_then(|_| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

/// Resuelve el directorio de credenciales / autenticaciones de Cliproxy o CPAMC
pub fn resolve_auths_dir() -> PathBuf {
    resolve_auths_dir_for(".")
}

/// Resuelve el directorio de credenciales considerando el directorio de proyecto activo (`target_dir`).
pub fn resolve_auths_dir_for(target_dir: &str) -> PathBuf {
    // 1. Variable de entorno explícita (mayor prioridad para cualquier usuario o CI)
    for env_key in ["CLIPROXY_AUTHS_DIR", "CPAMC_AUTHS_DIR"] {
        if let Ok(dir) = std::env::var(env_key) {
            let p = PathBuf::from(dir.trim());
            if p.exists() {
                return p;
            }
        }
    }

    // 2. Extraer 'authsDir' / 'auths_dir' desde opencode.json (compatible con provider/providers y cpamc/cliproxy)
    let candidates_cfg = opencode_config_candidates(target_dir);
    for cfg_path in candidates_cfg {
        if cfg_path.exists()
            && let Ok(content) = std::fs::read_to_string(&cfg_path)
            && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        {
            for section in ["providers", "provider"] {
                if let Some(prov_map) = val.get(section).and_then(|p| p.as_object()) {
                    for preferred in ["cpamc", "cliproxy"] {
                        if let Some(auth_dir) = prov_map
                            .get(preferred)
                            .and_then(|c| c.get("authsDir").or_else(|| c.get("auths_dir")))
                            .and_then(|s| s.as_str())
                        {
                            let p = PathBuf::from(auth_dir.trim());
                            if p.exists() {
                                return p;
                            }
                        }
                    }
                    for prov_val in prov_map.values() {
                        if let Some(auth_dir) = prov_val
                            .get("authsDir")
                            .or_else(|| prov_val.get("auths_dir"))
                            .and_then(|s| s.as_str())
                        {
                            let p = PathBuf::from(auth_dir.trim());
                            if p.exists() {
                                return p;
                            }
                        }
                    }
                }
            }
            for top_key in ["cpamc_auths_dir", "cliproxy_auths_dir"] {
                if let Some(auth_dir) = val.get(top_key).and_then(|s| s.as_str()) {
                    let p = PathBuf::from(auth_dir.trim());
                    if p.exists() {
                        return p;
                    }
                }
            }
        }
    }

    // 3. Rutas estándar en el HOME o CONFIG del usuario
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut candidates = vec![
        home.join(".cliproxy").join("auths"),
        home.join(".cpamc").join("auths"),
        home.join("cliproxyapi").join("auths"),
    ];

    if let Some(config_dir) = dirs::config_dir() {
        candidates.push(config_dir.join("cliproxy").join("auths"));
        candidates.push(config_dir.join("cpamc").join("auths"));
    }

    #[cfg(target_os = "windows")]
    {
        // En Windows, escanear unidades de disco para instalaciones portables
        if let Ok(system_drive) = std::env::var("SystemDrive") {
            candidates.push(PathBuf::from(format!("{system_drive}/cliproxyapi/auths")));
        }
        push_windows_drive_candidates(
            &mut candidates,
            &["cliproxyapi/auths", ".cliproxy/auths", ".cpamc/auths"],
        );
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

    // Si existen múltiples copias, ordenar por actividad real incluyendo el archivo `-wal` de SQLite
    existing.sort_by_key(|a| std::cmp::Reverse(db_effective_mtime(a)));

    existing.remove(0)
}

/// Calcula el `mtime` efectivo de una base SQLite considerando sus archivos adjuntos `-wal` y `-shm`
/// (en modo WAL, las escrituras recientes actualizan `opencode.db-wal` antes del checkpoint).
pub fn db_effective_mtime(db_path: &Path) -> Option<std::time::SystemTime> {
    let base = db_path.metadata().and_then(|m| m.modified()).ok();
    let wal = PathBuf::from(format!("{}-wal", db_path.display()))
        .metadata()
        .and_then(|m| m.modified())
        .ok();
    let shm = PathBuf::from(format!("{}-shm", db_path.display()))
        .metadata()
        .and_then(|m| m.modified())
        .ok();
    [base, wal, shm].into_iter().flatten().max()
}

/// Resuelve el binario ejecutable de Engram en cualquier sistema operativo
pub fn resolve_engram_binary() -> Option<PathBuf> {
    if let Ok(bin) = std::env::var("ENGRAM_BIN") {
        let trimmed = bin.trim();
        if trimmed.is_empty() || matches!(trimmed, "0" | "false" | "off" | "none") {
            return None;
        }
        let p = PathBuf::from(trimmed);
        return p.exists().then_some(p);
    }

    // 1. Verificar si 'engram' está disponible globalmente en PATH sin invocar subprocesos
    if let Some(path_var) = std::env::var_os("PATH") {
        let exe_names = if cfg!(target_os = "windows") {
            &["engram.exe", "engram.cmd", "engram.bat", "engram"][..]
        } else {
            &["engram"][..]
        };
        for dir in std::env::split_paths(&path_var) {
            for &exe in exe_names {
                let candidate = dir.join(exe);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
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
