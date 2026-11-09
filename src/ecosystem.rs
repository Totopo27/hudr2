//! Telemetría del Stack / Ecosistema: detecta las versiones instaladas
//! de las herramientas clave de desarrollo (pi, hudr2, gentle-shell, gentle-ai, engram, herdr).

use crate::models::{EcosystemTelemetry, ToolVersionInfo};
use std::io::Read;
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

static ECOSYSTEM_CACHE: LazyLock<Mutex<Option<EcosystemTelemetry>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn clear_ecosystem_cache() {
    if let Ok(mut guard) = ECOSYSTEM_CACHE.lock() {
        *guard = None;
    }
}

/// Sondeo one-shot de las versiones del ecosistema: se ejecuta una sola vez por sesión
/// (y solo se invalida manualmente al presionar `r` -> `clear_ecosystem_cache()`).
pub fn read_ecosystem_telemetry() -> EcosystemTelemetry {
    if let Ok(guard) = ECOSYSTEM_CACHE.lock()
        && let Some(ref tele) = *guard
    {
        return tele.clone();
    }

    let tele = collect_ecosystem_telemetry();

    if let Ok(mut guard) = ECOSYSTEM_CACHE.lock() {
        *guard = Some(tele.clone());
    }

    tele
}

fn run_version_cmd(bin: &str, args: &[&str]) -> Option<String> {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", bin]);
        for arg in args {
            c.arg(arg);
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            c.creation_flags(CREATE_NO_WINDOW);
        }
        c
    } else {
        let mut c = std::process::Command::new(bin);
        for arg in args {
            c.arg(arg);
        }
        c
    };

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let mut child = cmd.spawn().ok()?;
    let stdout_pipe = child.stdout.take()?;
    let out_handle = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout_pipe.take(16 * 1024).read_to_string(&mut s);
        s
    });

    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => return None,
        }
    };

    if status.success() {
        let s = out_handle.join().unwrap_or_default();
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

/// Normaliza salidas tipo "herdr 0.9.5", "0.87.0", "v1.5.0", "gentle-shell 3.7.0"
/// al formato canónico `vX.Y.Z[-tag]`.
pub fn clean_version_output(tool_name: &str, raw: &str) -> String {
    let first_line = raw.lines().next().unwrap_or(raw).trim();
    let mut v = first_line;

    // Quitar prefijo con el nombre de la herramienta o derivados si aparece
    let known_prefixes = [
        tool_name,
        "engram",
        "herdr",
        "gentle-ai",
        "gentle-shell",
        "pi",
        "hudr2",
    ];
    for p in known_prefixes {
        if let Some(rest) = v.strip_prefix(p) {
            v = rest.trim();
        }
    }

    let clean = v.strip_prefix('v').unwrap_or(v).trim();
    if clean.is_empty() {
        "-".to_string()
    } else {
        format!("v{clean}")
    }
}

pub fn collect_ecosystem_telemetry() -> EcosystemTelemetry {
    let mut tools = Vec::new();

    // 1. gentle-shell (y captura de fallback para pi)
    let gs_raw = run_version_cmd("gentle-shell", &["--version"]);
    let mut pi_fallback_ver: Option<String> = None;
    if let Some(ref raw) = gs_raw {
        for line in raw.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("pi ") {
                pi_fallback_ver = Some(rest.trim().to_string());
            }
        }
    }

    // 1. pi
    let pi_raw = run_version_cmd("pi", &["--version"]);
    let pi_info = match pi_raw.or(pi_fallback_ver) {
        Some(raw) => ToolVersionInfo {
            name: "pi".to_string(),
            version: Some(clean_version_output("pi", &raw)),
            is_installed: true,
            ..Default::default()
        },
        None => ToolVersionInfo {
            name: "pi".to_string(),
            version: None,
            is_installed: false,
            ..Default::default()
        },
    };
    tools.push(pi_info);

    // 2. hudr2: sondea binario global en PATH (`hudr2 --version`); si no existe o falla,
    // toma la versión del binario en ejecución compilada en Rust (`env!("CARGO_PKG_VERSION")`).
    let hudr2_raw = run_version_cmd("hudr2", &["--version"]);
    let hud_version = hudr2_raw
        .map(|raw| clean_version_output("hudr2", &raw))
        .filter(|v| v != "-")
        .unwrap_or_else(|| format!("v{}", env!("CARGO_PKG_VERSION")));

    let hud_info = ToolVersionInfo {
        name: "hudr2".to_string(),
        version: Some(hud_version),
        is_installed: true,
        ..Default::default()
    };
    tools.push(hud_info);

    // 3. gentle-shell
    let gs_info = match gs_raw {
        Some(raw) => ToolVersionInfo {
            name: "gentle-shell".to_string(),
            version: Some(clean_version_output("gentle-shell", &raw)),
            is_installed: true,
            ..Default::default()
        },
        None => ToolVersionInfo {
            name: "gentle-shell".to_string(),
            version: None,
            is_installed: false,
            ..Default::default()
        },
    };
    tools.push(gs_info);

    // 4. gentle-ai (intenta CLI y fallback a state.json en ~/.gentle-ai)
    let gai_raw = run_version_cmd("gentle-ai", &["--version"]);
    let gai_fallback_ver = if gai_raw.is_none() {
        dirs::home_dir()
            .map(|h| h.join(".gentle-ai").join("state.json"))
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("installed_binary_version")
                    .and_then(|ibv| ibv.as_str())
                    .map(|s| s.to_string())
            })
    } else {
        None
    };

    let gai_info = match gai_raw.or(gai_fallback_ver) {
        Some(raw) => ToolVersionInfo {
            name: "gentle-ai".to_string(),
            version: Some(clean_version_output("gentle-ai", &raw)),
            is_installed: true,
            ..Default::default()
        },
        None => ToolVersionInfo {
            name: "gentle-ai".to_string(),
            version: None,
            is_installed: false,
            ..Default::default()
        },
    };
    tools.push(gai_info);

    // 5. engram
    let engram_raw = run_version_cmd("engram", &["--version"]);
    let engram_info = match engram_raw {
        Some(raw) => ToolVersionInfo {
            name: "engram".to_string(),
            version: Some(clean_version_output("engram", &raw)),
            is_installed: true,
            ..Default::default()
        },
        None => ToolVersionInfo {
            name: "engram".to_string(),
            version: None,
            is_installed: false,
            ..Default::default()
        },
    };
    tools.push(engram_info);

    // 6. herdr
    let herdr_raw = run_version_cmd("herdr", &["--version"]);
    let herdr_info = match herdr_raw {
        Some(raw) => ToolVersionInfo {
            name: "herdr".to_string(),
            version: Some(clean_version_output("herdr", &raw)),
            is_installed: true,
            ..Default::default()
        },
        None => ToolVersionInfo {
            name: "herdr".to_string(),
            version: None,
            is_installed: false,
            ..Default::default()
        },
    };
    tools.push(herdr_info);

    EcosystemTelemetry { tools }
}

pub fn repo_for_tool(name: &str) -> Option<&'static str> {
    match name {
        "pi" => Some("earendil-works/pi"),
        "hudr2" => Some("Totopo27/hudr2"),
        "gentle-shell" => Some("Gentleman-Programming/gentle-shell"),
        "gentle-ai" => Some("Gentleman-Programming/gentle-ai"),
        "engram" => Some("Gentleman-Programming/engram"),
        "herdr" => Some("herdrdev/herdr"),
        _ => None,
    }
}

/// Define el comando oficial de actualización para cada herramienta del ecosistema.
pub fn update_cmd_for_tool(name: &str) -> Option<(&'static str, Vec<&'static str>)> {
    match name {
        "pi" => Some(("npm", vec!["install", "-g", "@earendil-works/pi-coding-agent@latest"])),
        "gentle-shell" => Some(("npm", vec!["install", "-g", "gentle-pi@latest"])),
        "gentle-ai" => Some(("gentle-ai", vec!["upgrade", "gentle-ai"])),
        "engram" => Some(("gentle-ai", vec!["upgrade", "engram"])),
        "herdr" => Some(("herdr", vec!["update"])),
        "hudr2" => Some(("cargo", vec!["install", "--git", "https://github.com/Totopo27/hudr2"])),
        _ => None,
    }
}

/// Ejecuta el comando de actualización para una herramienta específica en un subproceso
/// seguro y no bloqueante con timeout de 120s.
pub fn execute_tool_update(name: &str) -> Result<String, String> {
    let (bin, args) = update_cmd_for_tool(name)
        .ok_or_else(|| format!("no hay comando de actualización definido para '{name}'"))?;

    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", bin]);
        for arg in &args {
            c.arg(arg);
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            c.creation_flags(CREATE_NO_WINDOW);
        }
        c
    } else {
        let mut c = std::process::Command::new(bin);
        for arg in &args {
            c.arg(arg);
        }
        c
    };

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("no se pudo iniciar el proceso '{bin}': {e}"))?;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let out_handle = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = stdout_pipe {
            let _ = p.take(32 * 1024).read_to_string(&mut s);
        }
        s
    });

    let err_handle = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = stderr_pipe {
            let _ = p.take(32 * 1024).read_to_string(&mut s);
        }
        s
    });

    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("tiempo límite de actualización agotado (120s) para '{name}'"));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("error esperando proceso: {e}")),
        }
    };

    let stdout_str = out_handle.join().unwrap_or_default();
    let stderr_str = err_handle.join().unwrap_or_default();

    if status.success() {
        Ok(format!("{name} actualizado correctamente"))
    } else {
        let err_detail = stderr_str
            .lines()
            .chain(stdout_str.lines())
            .map(|l| l.trim())
            .rfind(|l| !l.is_empty())
            .unwrap_or("falló el comando de instalación");
        Err(format!("{name}: {err_detail}"))
    }
}

/// Compara dos versiones semver normalizadas (`vX.Y.Z[-tag]`).
/// Devuelve `true` si `upstream` es estrictamente mayor que `current`.
pub fn is_newer_version(current: &str, upstream: &str) -> bool {
    let parse_nums = |v: &str| -> Vec<u64> {
        let v = v.strip_prefix('v').unwrap_or(v);
        let core = v.split(['-', '+']).next().unwrap_or(v);
        core.split('.')
            .filter_map(|part| part.parse::<u64>().ok())
            .collect()
    };
    let cur_parts = parse_nums(current);
    let up_parts = parse_nums(upstream);
    if cur_parts.is_empty() || up_parts.is_empty() {
        return false;
    }
    up_parts > cur_parts
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct UpstreamCache {
    pub last_checked_unix: u64,
    pub versions: std::collections::HashMap<String, Option<String>>,
}

pub fn upstream_cache_path_for(config_path: Option<&std::path::Path>) -> std::path::PathBuf {
    if let Some(p) = config_path {
        p.with_file_name("upstream_cache.json")
    } else {
        crate::paths::resolve_hudr2_config_path().with_file_name("upstream_cache.json")
    }
}

pub fn load_upstream_cache(path: &std::path::Path) -> Option<UpstreamCache> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save_upstream_cache(path: &std::path::Path, cache: &UpstreamCache) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(cache)?;
    crate::paths::atomic_write_bytes(path, &bytes)
}

pub const UPSTREAM_CACHE_TTL_SECS: u64 = 3 * 3600; // 3 horas

pub fn is_upstream_cache_expired(config_path: Option<&std::path::Path>) -> bool {
    let path = upstream_cache_path_for(config_path);
    if let Some(cache) = load_upstream_cache(&path) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(cache.last_checked_unix) >= UPSTREAM_CACHE_TTL_SECS
    } else {
        true
    }
}

pub fn apply_upstream_cache_to_ecosystem(eco: &mut EcosystemTelemetry, cache: &UpstreamCache) {
    for tool in &mut eco.tools {
        let key = tool.name.as_str();
        if let Some(Some(up_ver)) = cache.versions.get(key) {
            tool.upstream_version = Some(up_ver.clone());
            if let Some(ref cur_ver) = tool.version {
                tool.update_available = is_newer_version(cur_ver, up_ver);
            } else {
                tool.update_available = false;
            }
        }
    }
}

pub fn hydrate_ecosystem_upstream(eco: &mut EcosystemTelemetry, config_path: Option<&std::path::Path>) {
    let path = upstream_cache_path_for(config_path);
    if let Some(cache) = load_upstream_cache(&path) {
        apply_upstream_cache_to_ecosystem(eco, &cache);
    }
}

async fn fetch_single_upstream(
    tool_name: &'static str,
    repo: &'static str,
) -> (String, Option<String>) {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let resp = crate::config::HTTP_CLIENT
        .get(&url)
        .timeout(Duration::from_millis(2500))
        .send()
        .await;
    let tag = match resp {
        Ok(r) if r.status().is_success() => {
            if let Ok(v) = r.json::<serde_json::Value>().await {
                v.get("tag_name")
                    .and_then(|t| t.as_str())
                    .map(|s| clean_version_output(tool_name, s))
            } else {
                None
            }
        }
        _ => None,
    };
    (tool_name.to_string(), tag)
}

pub async fn fetch_upstream_versions_with_cache(
    config_path: Option<&std::path::Path>,
) -> UpstreamCache {
    let path = upstream_cache_path_for(config_path);
    if !is_upstream_cache_expired(config_path)
        && let Some(cache) = load_upstream_cache(&path)
    {
        return cache;
    }

    let (t0, t1, t2, t3, t4, t5) = tokio::join!(
        fetch_single_upstream("pi", repo_for_tool("pi").unwrap()),
        fetch_single_upstream("hudr2", repo_for_tool("hudr2").unwrap()),
        fetch_single_upstream("gentle-shell", repo_for_tool("gentle-shell").unwrap()),
        fetch_single_upstream("gentle-ai", repo_for_tool("gentle-ai").unwrap()),
        fetch_single_upstream("engram", repo_for_tool("engram").unwrap()),
        fetch_single_upstream("herdr", repo_for_tool("herdr").unwrap()),
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut versions = std::collections::HashMap::new();
    for (name, tag) in [t0, t1, t2, t3, t4, t5] {
        versions.insert(name, tag);
    }

    if let Some(old_cache) = load_upstream_cache(&path) {
        for (name, tag) in &mut versions {
            if tag.is_none()
                && let Some(old_val) = old_cache.versions.get(name)
            {
                *tag = old_val.clone();
            }
        }
    }

    let cache = UpstreamCache {
        last_checked_unix: now,
        versions,
    };
    let _ = save_upstream_cache(&path, &cache);
    cache
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_version_output() {
        assert_eq!(clean_version_output("pi", "0.87.0"), "v0.87.0");
        assert_eq!(clean_version_output("hudr2", "hudr2 0.1.0"), "v0.1.0");
        assert_eq!(clean_version_output("hudr2", "hudr2 v0.1.0"), "v0.1.0");
        assert_eq!(clean_version_output("hudr2", "0.1.0"), "v0.1.0");
        assert_eq!(clean_version_output("engram", "engram 3.6.2"), "v3.6.2");
        assert_eq!(clean_version_output("herdr", "herdr 0.9.5"), "v0.9.5");
        assert_eq!(clean_version_output("gentle-ai", "gentle-ai 4.3.1"), "v4.3.1");
        assert_eq!(
            clean_version_output("gentle-shell", "gentle-shell v4.5.1-main.14f44d\npi 0.87.0"),
            "v4.5.1-main.14f44d"
        );
        assert_eq!(clean_version_output("unknown", "v1.2.3"), "v1.2.3");
        assert_eq!(clean_version_output("empty", ""), "-");
    }

    #[test]
    fn test_collect_ecosystem_telemetry_hudr2_always_present_and_no_gs3() {
        let tele = collect_ecosystem_telemetry();
        assert!(
            !tele.tools.iter().any(|t| t.name == "gs3-hud"),
            "gs3-hud must never be listed in ecosystem tools"
        );
        let hudr2_tool = tele
            .tools
            .iter()
            .find(|t| t.name == "hudr2")
            .expect("hudr2 tool must be present in ecosystem telemetry");
        assert!(hudr2_tool.is_installed);
        let ver = hudr2_tool.version.as_deref().unwrap_or("");
        assert!(
            ver.starts_with('v'),
            "hudr2 version should start with 'v', got: {}",
            ver
        );
    }

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("v3.6.2", "v3.7.0"));
        assert!(!is_newer_version("v3.7.0", "v3.6.2"));
        assert!(!is_newer_version("v0.9.5", "v0.9.5"));
        assert!(is_newer_version("v4.5.1-main.14f44d", "v4.7.0"));
        assert!(is_newer_version("0.87.0", "v1.5.0"));
        assert!(!is_newer_version("invalid", "v1.0.0"));
        assert!(!is_newer_version("v1.0.0", "invalid"));
    }

    #[test]
    fn test_upstream_cache_expiration_and_hydration() {
        let mut versions = std::collections::HashMap::new();
        versions.insert("engram".to_string(), Some("v3.7.0".to_string()));
        versions.insert("herdr".to_string(), Some("v0.9.5".to_string()));

        let cache = UpstreamCache {
            last_checked_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            versions,
        };

        let mut eco = EcosystemTelemetry {
            tools: vec![
                ToolVersionInfo {
                    name: "engram".to_string(),
                    version: Some("v3.6.2".to_string()),
                    is_installed: true,
                    ..Default::default()
                },
                ToolVersionInfo {
                    name: "herdr".to_string(),
                    version: Some("v0.9.5".to_string()),
                    is_installed: true,
                    ..Default::default()
                },
            ],
        };

        apply_upstream_cache_to_ecosystem(&mut eco, &cache);

        assert_eq!(eco.tools[0].upstream_version.as_deref(), Some("v3.7.0"));
        assert!(eco.tools[0].update_available);

        assert_eq!(eco.tools[1].upstream_version.as_deref(), Some("v0.9.5"));
        assert!(!eco.tools[1].update_available);
    }

    #[test]
    fn test_update_cmd_for_tool() {
        assert!(update_cmd_for_tool("pi").is_some());
        assert!(update_cmd_for_tool("gentle-shell").is_some());
        assert!(update_cmd_for_tool("gentle-ai").is_some());
        assert!(update_cmd_for_tool("engram").is_some());
        assert!(update_cmd_for_tool("herdr").is_some());
        assert!(update_cmd_for_tool("hudr2").is_some());
        assert!(update_cmd_for_tool("nonexistent").is_none());

        let (bin_pi, args_pi) = update_cmd_for_tool("pi").unwrap();
        assert_eq!(bin_pi, "npm");
        assert!(args_pi.contains(&"@earendil-works/pi-coding-agent@latest"));

        let (bin_gs, args_gs) = update_cmd_for_tool("gentle-shell").unwrap();
        assert_eq!(bin_gs, "npm");
        assert!(args_gs.contains(&"gentle-pi@latest"));

        let (bin_engram, args_engram) = update_cmd_for_tool("engram").unwrap();
        assert_eq!(bin_engram, "gentle-ai");
        assert_eq!(args_engram, vec!["upgrade", "engram"]);
    }
}
