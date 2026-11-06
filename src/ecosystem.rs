//! Telemetría del Stack / Ecosistema: detecta las versiones instaladas
//! de las herramientas clave de desarrollo (pi, hudr2/gs3-hud, gentle-shell, gentle-ai, engram, herdr).

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
        "gs3-hud",
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

fn collect_ecosystem_telemetry() -> EcosystemTelemetry {
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
        },
        None => ToolVersionInfo {
            name: "pi".to_string(),
            version: None,
            is_installed: false,
        },
    };
    tools.push(pi_info);

    // 2. hudr2 / gs3-hud (prioriza gs3-hud si existe binario dedicado, de lo contrario la versión propia de hudr2)
    let gs3_raw = run_version_cmd("gs3-hud", &["--version"]);
    let hud_info = if let Some(raw) = gs3_raw {
        ToolVersionInfo {
            name: "gs3-hud".to_string(),
            version: Some(clean_version_output("gs3-hud", &raw)),
            is_installed: true,
        }
    } else {
        ToolVersionInfo {
            name: "hudr2".to_string(),
            version: Some(format!("v{}", env!("CARGO_PKG_VERSION"))),
            is_installed: true,
        }
    };
    tools.push(hud_info);

    // 3. gentle-shell
    let gs_info = match gs_raw {
        Some(raw) => ToolVersionInfo {
            name: "gentle-shell".to_string(),
            version: Some(clean_version_output("gentle-shell", &raw)),
            is_installed: true,
        },
        None => ToolVersionInfo {
            name: "gentle-shell".to_string(),
            version: None,
            is_installed: false,
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
        },
        None => ToolVersionInfo {
            name: "gentle-ai".to_string(),
            version: None,
            is_installed: false,
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
        },
        None => ToolVersionInfo {
            name: "engram".to_string(),
            version: None,
            is_installed: false,
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
        },
        None => ToolVersionInfo {
            name: "herdr".to_string(),
            version: None,
            is_installed: false,
        },
    };
    tools.push(herdr_info);

    EcosystemTelemetry { tools }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_version_output() {
        assert_eq!(clean_version_output("pi", "0.87.0"), "v0.87.0");
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
}
