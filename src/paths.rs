use std::path::{Path, PathBuf};
use std::process::Command;

/// Resuelve el directorio de credenciales / autenticaciones de Cliproxy
pub fn resolve_auths_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLIPROXY_AUTHS_DIR") {
        let p = PathBuf::from(dir.trim());
        if p.exists() {
            return p;
        }
    }

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
        // En Windows, resolver dinámicamente según las unidades de disco presentes o variables de entorno
        if let Ok(system_drive) = std::env::var("SystemDrive") {
            candidates.push(PathBuf::from(format!("{system_drive}/cliproxyapi/auths")));
        }
        for drive in ['C', 'D', 'E', 'F'] {
            candidates.push(PathBuf::from(format!("{drive}:/cliproxyapi/auths")));
        }
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
        for drive in ['C', 'D', 'E', 'F'] {
            candidates.push(PathBuf::from(format!("{drive}:/.engram/engram.db")));
        }
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

    // 3. macOS Library Application Support
    #[cfg(target_os = "macos")]
    {
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join("opencode")
                .join("opencode.db"),
        );
    }

    // 4. Fallbacks locales de Windows para desarrollo o setups multi-disco
    #[cfg(target_os = "windows")]
    {
        for drive in ['C', 'D', 'E', 'F'] {
            candidates.push(PathBuf::from(format!("{drive}:/.local/share/opencode/opencode.db")));
        }
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
        for drive in ['C', 'D', 'E', 'F'] {
            candidates.push(PathBuf::from(format!("{drive}:/.opencode/opencode.json")));
        }
    }

    candidates
}
