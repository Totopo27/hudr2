//! Telemetría del Stack / Ecosistema: detecta las versiones instaladas
//! de las herramientas clave de desarrollo (pi, hudr2, gentle-shell, gentle-ai, engram, herdr).

use crate::models::{EcosystemTelemetry, ToolVersionInfo};
use std::process::Stdio;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

static ECOSYSTEM_CACHE: LazyLock<Mutex<Option<EcosystemTelemetry>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(not(test))]
const UPDATE_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(test)]
const UPDATE_TIMEOUT: Duration = Duration::from_secs(2);

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
    run_version_cmd_until(bin, args, Instant::now() + Duration::from_secs(3))
}

fn run_version_cmd_until(bin: &str, args: &[&str], deadline: Instant) -> Option<String> {
    let deadline = deadline.min(Instant::now() + Duration::from_secs(3));
    if Instant::now() >= deadline {
        return None;
    }
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

    let (status, output, _) = wait_for_output(cmd.spawn().ok()?, deadline, |_, _| {}).ok()?;

    if status.success() {
        let trimmed = output.trim();
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

    if v.len() >= 7 && v[..7].eq_ignore_ascii_case("version") {
        let after = &v[7..];
        if after.is_empty()
            || after.starts_with(|c: char| c.is_whitespace() || c == ':' || c == '=')
        {
            v = after.trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '=');
        }
    }

    let clean = v
        .strip_prefix('v')
        .or_else(|| v.strip_prefix('V'))
        .unwrap_or(v)
        .trim();
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
            .and_then(|p| crate::paths::read_config_string(p).ok())
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
/// Nota: `hudr2` se actualiza de forma asistida fuera del HUD en ejecución para evitar bloqueos
/// de archivo en Windows (`os error 5`) y saturación de CPU por compilación de Rust.
pub fn update_cmd_for_tool(name: &str) -> Option<(&'static str, Vec<&'static str>)> {
    match name {
        "pi" => Some((
            "npm",
            vec![
                "install",
                "-g",
                "--force",
                "@earendil-works/pi-coding-agent@latest",
            ],
        )),
        "gentle-shell" => Some(("npm", vec!["install", "-g", "--force", "gentle-pi@latest"])),
        "gentle-ai" => {
            if cfg!(target_os = "windows") {
                Some((
                    "go",
                    vec![
                        "install",
                        "github.com/gentleman-programming/gentle-ai/v4/cmd/gentle-ai@latest",
                    ],
                ))
            } else {
                Some(("gentle-ai", vec!["upgrade", "gentle-ai"]))
            }
        }
        "engram" => Some(("gentle-ai", vec!["upgrade", "engram"])),
        "herdr" => Some(("herdr", vec!["update"])),
        _ => None,
    }
}

/// Resuelve binario y argumentos de actualización considerando `target_version`
/// para soportar cambios de ruta mayor en módulos Go de Windows (`gentle-ai/vN`).
pub fn update_cmd_for_tool_target(
    name: &str,
    target_version: Option<&str>,
) -> Option<(String, Vec<String>)> {
    if name == "gentle-ai" && cfg!(target_os = "windows") {
        let major = target_version
            .and_then(|v| {
                let core = v.trim().trim_start_matches(['v', 'V']);
                core.split('.').next()?.parse::<u64>().ok()
            })
            .unwrap_or(4);
        let pkg = if major >= 2 {
            format!("github.com/gentleman-programming/gentle-ai/v{major}/cmd/gentle-ai@latest")
        } else {
            "github.com/gentleman-programming/gentle-ai/cmd/gentle-ai@latest".to_string()
        };
        return Some(("go".to_string(), vec!["install".to_string(), pkg]));
    }
    update_cmd_for_tool(name).map(|(bin, args)| {
        (
            bin.to_string(),
            args.into_iter().map(str::to_string).collect(),
        )
    })
}

/// Devuelve instrucciones asistidas para herramientas que deben actualizarse fuera
/// del proceso activo (como hudr2, para evitar bloqueos de archivo en Windows y sobrecarga de CPU).
pub fn assisted_update_notice(name: &str) -> Option<&'static str> {
    match name {
        "hudr2" => Some(
            "hudr2: para actualizar, ejecute en otra terminal: cargo install --git https://github.com/Totopo27/hudr2",
        ),
        _ => None,
    }
}

/// Lee de forma continua fragmentos de bytes de un pipe (stdout o stderr) y emite líneas
/// separadas tanto por `\n` como por `\r` (usado comúnmente en terminales para spinners y progreso).
fn stream_pipe_chunks<R: std::io::Read>(
    pipe: Option<R>,
    tx: std::sync::mpsc::SyncSender<(Option<String>, bool)>,
    is_err: bool,
) {
    if let Some(mut r) = pipe {
        let mut buf = [0u8; 512];
        let mut cur = Vec::new();
        while let Ok(n) = r.read(&mut buf) {
            if n == 0 {
                break;
            }
            for &b in &buf[..n] {
                if b == b'\n' || b == b'\r' {
                    if !cur.is_empty() {
                        let line = String::from_utf8_lossy(&cur).trim().to_string();
                        if !line.is_empty() && tx.send((Some(line), is_err)).is_err() {
                            return;
                        }
                        cur.clear();
                    }
                } else if cur.len() < 16 * 1024 {
                    cur.push(b);
                }
            }
        }
        if !cur.is_empty() {
            let line = String::from_utf8_lossy(&cur).trim().to_string();
            if !line.is_empty() {
                let _ = tx.send((Some(line), is_err));
            }
        }
    }
    let _ = tx.send((None, is_err));
}

/// The deadline covers child exit AND EOF; inherited output handles cannot extend it.
fn wait_for_output(
    mut child: std::process::Child,
    deadline: Instant,
    mut on_line: impl FnMut(Option<&str>, u64),
) -> Result<(std::process::ExitStatus, String, String), String> {
    #[cfg(windows)]
    let _job = windows_job::Job::attach(&mut child)?;
    let (tx, rx) = std::sync::mpsc::sync_channel(32);
    for (pipe, is_err) in [
        (child.stdout.take().map(Pipe::Out), false),
        (child.stderr.take().map(Pipe::Err), true),
    ] {
        let tx = tx.clone();
        std::thread::spawn(move || match pipe {
            Some(Pipe::Out(pipe)) => stream_pipe_chunks(Some(pipe), tx, is_err),
            Some(Pipe::Err(pipe)) => stream_pipe_chunks(Some(pipe), tx, is_err),
            None => {
                let _ = tx.send((None, is_err));
            }
        });
    }
    drop(tx);
    let started = Instant::now();
    let mut status = None;
    let mut closed = 0;
    let (mut stdout, mut stderr) = (String::new(), String::new());
    loop {
        if Instant::now() >= deadline {
            if status.is_none() && child.kill().is_ok() {
                let _ = child.wait();
            }
            return Err("operation timed out while installing or collecting output".to_string());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining.min(Duration::from_millis(100))) {
            Ok((Some(line), is_err)) => {
                on_line(Some(&line), started.elapsed().as_secs());
                let output = if is_err { &mut stderr } else { &mut stdout };
                // Keep a bounded diagnostic tail rather than the entire installer transcript.
                output.push_str(&line);
                output.push('\n');
                if output.len() > 16 * 1024 {
                    let mut start = output.len() - 16 * 1024;
                    while !output.is_char_boundary(start) {
                        start += 1;
                    }
                    output.drain(..start);
                }
            }
            Ok((None, _)) => closed += 1,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(remaining.min(Duration::from_millis(20)));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
        on_line(None, started.elapsed().as_secs());
        if status.is_none() {
            status = child
                .try_wait()
                .map_err(|e| format!("could not wait for installer: {e}"))?;
        }
        if let Some(status) = status
            && closed == 2
        {
            return Ok((status, stdout, stderr));
        }
    }
}

enum Pipe {
    Out(std::process::ChildStdout),
    Err(std::process::ChildStderr),
}

#[cfg(windows)]
mod windows_job {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        process_time: i64,
        job_time: i64,
        flags: u32,
        min_working_set: usize,
        max_working_set: usize,
        active_processes: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io_counters: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            class: i32,
            info: *const c_void,
            size: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }

    pub(super) struct Job(*mut c_void);

    impl Job {
        pub(super) fn attach(child: &mut std::process::Child) -> Result<Self, String> {
            // A private, unnamed job owns only this invocation and its descendants.
            // Closing its non-inherited handle terminates survivors, never user sessions.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            let job = Self(handle);
            let limits = ExtendedLimits {
                basic: BasicLimits {
                    flags: 0x2000,
                    ..Default::default()
                },
                ..Default::default()
            };
            let configured = !handle.is_null()
                && unsafe {
                    SetInformationJobObject(
                        handle,
                        9,
                        (&limits as *const ExtendedLimits).cast(),
                        std::mem::size_of::<ExtendedLimits>() as u32,
                    ) != 0
                        && AssignProcessToJobObject(handle, child.as_raw_handle()) != 0
                };
            if !configured {
                if child.kill().is_ok() {
                    let _ = child.wait();
                }
                return Err("could not supervise installer process tree".to_string());
            }
            Ok(job)
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CloseHandle(self.0) };
            }
        }
    }
}

/// Analiza líneas emitidas por instaladores (gentle-ai, npm, herdr, cargo) y aproxima el progreso y etapa.
pub fn parse_line_progress(line: &str, current_pct: u8) -> (u8, String) {
    let lower = line.to_ascii_lowercase();

    // 1. Porcentaje numérico explícito si la herramienta lo emite (e.g. " 45%" o "45%")
    if let Some(pos) = lower.find('%') {
        let prefix = &lower[..pos];
        let num_str: String = prefix
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if let Ok(pct) = num_str.parse::<u8>() {
            return (pct.clamp(5, 95), "descargando".to_string());
        }
    }

    // 2. Mapeo de etapas conocidas en el ecosistema
    if lower.contains("checking") || lower.contains("comprobando") || lower.contains("buscando") {
        (15.max(current_pct), "verificando".to_string())
    } else if lower.contains("backup") || lower.contains("copia") {
        (30.max(current_pct), "backup".to_string())
    } else if lower.contains("download") || lower.contains("descarg") || lower.contains("fetch") {
        (50.max(current_pct), "descargando".to_string())
    } else if lower.contains("upgrading")
        || lower.contains("install")
        || lower.contains("go-install")
        || lower.contains("reify")
        || lower.contains("binary")
    {
        (75.max(current_pct), "instalando".to_string())
    } else if lower.contains("extract") || lower.contains("unzip") || lower.contains("descompr") {
        (85.max(current_pct), "extrayendo".to_string())
    } else if lower.contains("verif") || lower.contains("clean") || lower.contains("limp") {
        (92.max(current_pct), "limpiando".to_string())
    } else if lower.contains("[ok]")
        || lower.starts_with("added ")
        || lower.starts_with("changed ")
        || lower.contains("up to date")
        || lower.contains("success")
        || lower.contains("succeeded")
    {
        (100, "completado".to_string())
    } else {
        (current_pct, "procesando".to_string())
    }
}

/// En Windows, sincroniza el binario recién instalado en GOPATH/bin con la ruta activa que resuelve PATH
/// si ésta se encuentra en otra carpeta (ej. GOROOT/bin u otra instalación previa).
#[cfg(target_os = "windows")]
fn sync_active_windows_binary(bin_name: &str, deadline: Instant) {
    let active_path = run_version_cmd_until("where.exe", &[bin_name], deadline)
        .and_then(|out| out.lines().next().map(|l| l.trim().to_string()))
        .map(std::path::PathBuf::from);

    let gopath = run_version_cmd_until("go", &["env", "GOPATH"], deadline)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join("go")));

    if let (Some(gopath_dir), Some(dest)) = (gopath, active_path) {
        if Instant::now() >= deadline {
            return;
        }
        let src = gopath_dir.join("bin").join(format!("{bin_name}.exe"));
        if src.exists() && src != dest && std::fs::copy(&src, &dest).is_err() {
            let old = dest.with_extension("old.exe");
            let _ = std::fs::rename(&dest, &old);
            let _ = std::fs::copy(&src, &dest);
        }
    }
}

/// Ejecuta el comando de actualización para una herramienta específica en un subproceso
/// seguro y no bloqueante con timeout de 120s, reportando progreso en tiempo real a `on_progress`.
pub fn execute_tool_update_with_progress<F>(
    name: &str,
    on_progress: F,
) -> Result<String, String>
where
    F: FnMut(u8, &str, usize, u64) + Send,
{
    execute_tool_update_with_progress_target(name, None, on_progress)
}

/// Variante de [`execute_tool_update_with_progress`] que recibe `target_version` upstream
/// para resolver paquetes con sufijo mayor (como `gentle-ai` en Windows).
pub fn execute_tool_update_with_progress_target<F>(
    name: &str,
    target_version: Option<&str>,
    mut on_progress: F,
) -> Result<String, String>
where
    F: FnMut(u8, &str, usize, u64) + Send,
{
    let start_time = Instant::now();
    let deadline = start_time + UPDATE_TIMEOUT;
    if cfg!(target_os = "windows")
        && name == "gentle-ai"
        && run_version_cmd_until("go", &["version"], deadline).is_none()
    {
        return Err(
            "gentle-ai en Windows requiere Go instalado (https://go.dev) para compilar".to_string(),
        );
    }

    let (bin, args) = update_cmd_for_tool_target(name, target_version)
        .ok_or_else(|| format!("no hay comando de actualización definido para '{name}'"))?;

    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", &bin]);
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
        let mut c = std::process::Command::new(&bin);
        for arg in &args {
            c.arg(arg);
        }
        c
    };

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|e| format!("no se pudo iniciar el proceso '{bin}': {e}"))?;

    let pct: u8 = 0;
    let mut stage = "instalando".to_string();
    let mut spinner_idx: usize = 0;
    let mut last_progress_emit = Instant::now();
    let mut files_locked = false;

    on_progress(pct, &stage, spinner_idx, 0);

    let (status, stdout_str, stderr_str) = wait_for_output(child, deadline, |line, _| {
        if let Some(line) = line {
            files_locked |= line.to_ascii_lowercase().contains("ebusy");
            let (_, new_stage) = parse_line_progress(line, pct);
            stage = if new_stage == "completado" {
                "verificando".to_string()
            } else {
                new_stage
            };
        }
        if last_progress_emit.elapsed() >= Duration::from_millis(180) {
            spinner_idx = (spinner_idx + 1) % 4;
            on_progress(pct, &stage, spinner_idx, start_time.elapsed().as_secs());
            last_progress_emit = Instant::now();
        }
    })
    .map_err(|error| format!("{name}: {error} ({}s limit)", UPDATE_TIMEOUT.as_secs()))?;

    let combined = format!("{stdout_str}\n{stderr_str}");
    let combined_lower = combined.to_ascii_lowercase();

    if !status.success() && files_locked {
        return Err(format!(
            "{name}: EBUSY (files in use). Close sessions using this tool, then update from another terminal; no automatic retry."
        ));
    }

    if status.success() {
        if combined_lower.contains("manual update required")
            || combined_lower.contains("no files were changed")
        {
            let reason = combined
                .lines()
                .find(|l| {
                    let low = l.to_ascii_lowercase();
                    low.contains("manual update required") || low.contains("no files were changed")
                })
                .map(|l| l.trim())
                .unwrap_or("requiere actualización manual");
            return Err(format!("{name}: {reason}"));
        }
        #[cfg(target_os = "windows")]
        if name == "gentle-ai" {
            sync_active_windows_binary("gentle-ai", deadline);
        }
        on_progress(0, "verificando", 0, start_time.elapsed().as_secs());
        let version = run_version_cmd_until(name, &["--version"], deadline)
            .map(|raw| clean_version_output(name, &raw))
            .filter(|version| version.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
            .ok_or_else(|| {
                format!("{name}: installation finished, but its version could not be verified")
            })?;
        on_progress(100, "completado", 0, start_time.elapsed().as_secs());
        Ok(format!(
            "{name}: installation finished; active version {version}"
        ))
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

fn parse_semver(v: &str) -> Option<(Vec<u64>, Option<Vec<&str>>)> {
    let trimmed = v.trim();
    let stripped = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let no_build = stripped.split('+').next().unwrap_or(stripped);
    if no_build.is_empty() {
        return None;
    }
    let (core_str, pre_opt) = match no_build.split_once('-') {
        Some((c, p)) if !p.is_empty() => (c, Some(p.split('.').collect::<Vec<&str>>())),
        Some((c, _)) => (c, None),
        None => (no_build, None),
    };
    let mut core = Vec::new();
    for seg in core_str.split('.') {
        if seg.is_empty() {
            return None;
        }
        core.push(seg.parse::<u64>().ok()?);
    }
    if core.is_empty() {
        return None;
    }
    if let Some(ref pre) = pre_opt
        && pre.iter().any(|p| p.is_empty())
    {
        return None;
    }
    Some((core, pre_opt))
}

fn compare_prerelease(cur: &[&str], up: &[&str]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    for (a, b) in cur.iter().zip(up.iter()) {
        let ord = match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(na), Ok(nb)) => na.cmp(&nb),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => a.cmp(b),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    cur.len().cmp(&up.len())
}

/// Compara dos versiones SemVer 2.0.0 normalizadas (`vX.Y.Z[-tag]`).
/// Devuelve `true` si `upstream` es estrictamente mayor que `current`.
pub fn is_newer_version(current: &str, upstream: &str) -> bool {
    use std::cmp::Ordering;
    let (Some((cur_core, cur_pre)), Some((up_core, up_pre))) =
        (parse_semver(current), parse_semver(upstream))
    else {
        return false;
    };
    let max_len = cur_core.len().max(up_core.len());
    for i in 0..max_len {
        let c = *cur_core.get(i).unwrap_or(&0);
        let u = *up_core.get(i).unwrap_or(&0);
        match u.cmp(&c) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
    }
    match (cur_pre.as_deref(), up_pre.as_deref()) {
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => false,
        (Some(cp), Some(up)) => compare_prerelease(cp, up) == Ordering::Less,
    }
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
    let bytes = crate::paths::read_config_bytes(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save_upstream_cache(path: &std::path::Path, cache: &UpstreamCache) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(cache)?;
    crate::paths::atomic_write_bytes(path, &bytes)
}

pub const UPSTREAM_CACHE_TTL_SECS: u64 = 3 * 3600; // 3 horas
pub const UPSTREAM_RETRY_COOLDOWN_SECS: u64 = 300; // 5 minutos ante fallo/offline

static LAST_UPSTREAM_ATTEMPT: LazyLock<Mutex<Option<Instant>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn clear_upstream_attempt_backoff() {
    if let Ok(mut guard) = LAST_UPSTREAM_ATTEMPT.lock() {
        *guard = None;
    }
}

pub fn record_upstream_attempt() {
    if let Ok(mut guard) = LAST_UPSTREAM_ATTEMPT.lock() {
        *guard = Some(Instant::now());
    }
}

pub fn is_upstream_cache_expired(config_path: Option<&std::path::Path>) -> bool {
    let path = upstream_cache_path_for(config_path);
    if let Some(cache) = load_upstream_cache(&path) {
        if !cache.versions.values().any(Option::is_some) {
            return true;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(cache.last_checked_unix) >= UPSTREAM_CACHE_TTL_SECS
    } else {
        true
    }
}

pub fn should_run_upstream_check(
    config_path: Option<&std::path::Path>,
    check_updates: bool,
    force: bool,
) -> bool {
    if !check_updates || !is_upstream_cache_expired(config_path) {
        return false;
    }
    if force {
        return true;
    }
    if let Ok(guard) = LAST_UPSTREAM_ATTEMPT.lock()
        && let Some(last) = *guard
    {
        return last.elapsed() >= Duration::from_secs(UPSTREAM_RETRY_COOLDOWN_SECS);
    }
    true
}

pub fn merge_upstream_fetch_results(
    fetched: Vec<(String, Option<String>)>,
    old_cache: Option<&UpstreamCache>,
    now_unix: u64,
) -> (UpstreamCache, bool) {
    let any_fresh = fetched.iter().any(|(_, tag)| tag.is_some());
    let mut versions = std::collections::HashMap::new();
    for (name, tag) in fetched {
        let resolved = tag.or_else(|| {
            old_cache
                .and_then(|c| c.versions.get(&name))
                .cloned()
                .flatten()
        });
        versions.insert(name, resolved);
    }
    let cache = UpstreamCache {
        last_checked_unix: if any_fresh {
            now_unix
        } else {
            old_cache.map(|c| c.last_checked_unix).unwrap_or(0)
        },
        versions,
    };
    (cache, any_fresh)
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

pub fn hydrate_ecosystem_upstream(
    eco: &mut EcosystemTelemetry,
    config_path: Option<&std::path::Path>,
) {
    let path = upstream_cache_path_for(config_path);
    if let Some(cache) = load_upstream_cache(&path) {
        apply_upstream_cache_to_ecosystem(eco, &cache);
    }
}

/// Preserva el estado de actualización en curso (`is_updating` y `update_progress`)
/// de herramientas que se están actualizando en segundo plano cuando un ciclo de refresco
/// general (`refresh_all`) genera un nuevo `EcosystemTelemetry`.
pub fn preserve_in_flight_update_state(dest: &mut EcosystemTelemetry, source: &EcosystemTelemetry) {
    for current_tool in &source.tools {
        if current_tool.is_updating
            && let Some(dest_tool) = dest.tools.iter_mut().find(|t| t.name == current_tool.name)
        {
            dest_tool.is_updating = true;
            dest_tool.update_progress = current_tool.update_progress.clone();
        }
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

    let old_cache = load_upstream_cache(&path);
    let (cache, should_persist) =
        merge_upstream_fetch_results(vec![t0, t1, t2, t3, t4, t5], old_cache.as_ref(), now);
    if should_persist {
        let _ = save_upstream_cache(&path, &cache);
    }
    cache
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ToolUpdateProgress;

    #[cfg(windows)]
    static UPDATE_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[cfg(windows)]
    struct FakeUpdate {
        dir: tempfile::TempDir,
        old_path: std::ffi::OsString,
        old_search: Option<std::ffi::OsString>,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    #[cfg(windows)]
    impl FakeUpdate {
        fn new(installer: &str, version: &str) -> Self {
            let guard = UPDATE_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(
                dir.path().join("npm.cmd"),
                format!("@echo off\r\n{installer}\r\n"),
            )
            .unwrap();
            for tool in [
                "gentle-shell",
                "pi",
                "hudr2",
                "gentle-ai",
                "engram",
                "herdr",
                "go",
            ] {
                std::fs::write(
                    dir.path().join(format!("{tool}.cmd")),
                    format!("@echo off\r\n{version}\r\n"),
                )
                .unwrap();
            }
            let old_path = std::env::var_os("PATH").unwrap_or_default();
            let old_search = std::env::var_os("NoDefaultCurrentDirectoryInExePath");
            let path = std::env::join_paths(
                std::iter::once(dir.path().to_path_buf()).chain(std::env::split_paths(&old_path)),
            )
            .unwrap();
            // These process fixtures run serially and never invoke a real installer.
            unsafe {
                std::env::set_var("PATH", path);
                std::env::set_var("NoDefaultCurrentDirectoryInExePath", "1");
            }
            Self {
                dir,
                old_path,
                old_search,
                _guard: guard,
            }
        }
    }

    #[cfg(windows)]
    impl Drop for FakeUpdate {
        fn drop(&mut self) {
            unsafe { std::env::set_var("PATH", &self.old_path) };
            unsafe {
                match &self.old_search {
                    Some(value) => std::env::set_var("NoDefaultCurrentDirectoryInExePath", value),
                    None => std::env::remove_var("NoDefaultCurrentDirectoryInExePath"),
                }
            }
        }
    }

    #[test]
    #[cfg(windows)]
    fn update_accepts_successful_skipped_warning_and_reads_installed_version() {
        let fixture = FakeUpdate::new(
            "echo gentle-pi skipped enabling fullscreen: not a pi-managed install\r\nexit /b 0",
            "echo version-probed>> \"%~dp0calls.txt\"\r\necho gentle-shell 4.8.0\r\nexit /b 0",
        );
        let mut progress = Vec::new();
        let result = execute_tool_update_with_progress("gentle-shell", |pct, _, _, _| {
            progress.push(pct);
        });
        assert!(result.is_ok(), "{result:?}");
        assert!(result.unwrap().contains("v4.8.0"));
        assert!(fixture.dir.path().join("calls.txt").exists());
        assert_eq!(progress.last(), Some(&100));
        assert!(progress[..progress.len() - 1].iter().all(|pct| *pct == 0));
    }

    #[test]
    #[cfg(windows)]
    fn update_reports_locked_files_without_retrying_or_exposing_paths() {
        let fixture = FakeUpdate::new(
            "echo attempted>> \"%~dp0attempts.txt\"\r\necho npm error code EBUSY 1>&2\r\necho npm error log C:\\Users\\private-user\\npm.log 1>&2\r\nexit /b 1",
            "echo version-probed>> \"%~dp0calls.txt\"\r\necho gentle-shell 4.7.0",
        );
        let mut progress = Vec::new();
        let error = execute_tool_update_with_progress("gentle-shell", |pct, _, _, _| {
            progress.push(pct);
        })
        .unwrap_err();
        assert_eq!(
            error,
            "gentle-shell: EBUSY (files in use). Close sessions using this tool, then update from another terminal; no automatic retry."
        );
        assert!(!error.contains("private-user"), "{error}");
        assert!(!progress.contains(&100));
        assert!(!fixture.dir.path().join("calls.txt").exists());
        assert_eq!(
            std::fs::read_to_string(fixture.dir.path().join("attempts.txt"))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }

    #[test]
    #[cfg(windows)]
    fn update_does_not_announce_success_before_version_verification() {
        let _fixture = FakeUpdate::new("echo added 107 packages in 1s\r\nexit /b 0", "exit /b 1");
        let mut progress = Vec::new();
        let result = execute_tool_update_with_progress("gentle-shell", |pct, _, _, _| {
            progress.push(pct);
        });
        assert!(result.unwrap_err().contains("version"));
        assert!(!progress.contains(&100));
    }

    #[test]
    #[cfg(windows)]
    fn update_deadline_covers_inherited_output_handles() {
        let _fixture = FakeUpdate::new(
            "start \"\" /b \"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoLogo -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 5\"\r\nexit /b 0",
            "echo gentle-shell 4.8.0",
        );
        let start = Instant::now();
        let mut completed = false;
        let result = execute_tool_update_with_progress("gentle-shell", |pct, _, _, _| {
            completed |= pct == 100;
        });
        assert!(result.is_err(), "{result:?}");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );
        assert!(!completed);
    }

    #[test]
    #[cfg(windows)]
    fn update_deadline_also_covers_version_probe_output() {
        let _fixture = FakeUpdate::new(
            "exit /b 0",
            "echo gentle-shell 4.8.0\r\nstart \"\" /b \"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoLogo -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 5\"\r\nexit /b 0",
        );
        let start = Instant::now();
        let mut completed = false;
        let result = execute_tool_update_with_progress("gentle-shell", |pct, _, _, _| {
            completed |= pct == 100;
        });
        assert!(result.is_err(), "{result:?}");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );
        assert!(!completed);
    }

    #[test]
    #[cfg(windows)]
    fn update_deadline_covers_silent_running_installer() {
        let _fixture = FakeUpdate::new(
            "\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoLogo -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 5\"\r\nexit /b 0",
            "echo gentle-shell 4.8.0",
        );
        let start = Instant::now();
        let mut progress = Vec::new();
        let result = execute_tool_update_with_progress("gentle-shell", |pct, _, _, elapsed| {
            progress.push((pct, elapsed));
        });
        assert!(result.unwrap_err().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(4));
        assert!(progress.iter().any(|(_, elapsed)| *elapsed >= 1));
        assert!(progress.iter().all(|(pct, _)| *pct == 0));
    }

    #[test]
    #[cfg(windows)]
    fn update_deadline_is_shared_between_install_and_version_verification() {
        let _fixture = FakeUpdate::new(
            "\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoLogo -NoProfile -NonInteractive -Command \"Start-Sleep -Milliseconds 900\"",
            "\"%SystemRoot%\\System32\\WindowsPowerShell\\v1.0\\powershell.exe\" -NoLogo -NoProfile -NonInteractive -Command \"Start-Sleep -Seconds 5\"\r\necho gentle-shell 4.8.0",
        );
        let start = Instant::now();
        let mut completed = false;
        let result = execute_tool_update_with_progress("gentle-shell", |pct, _, _, _| {
            completed |= pct == 100;
        });
        assert!(result.is_err(), "{result:?}");
        assert!(
            start.elapsed() < UPDATE_TIMEOUT + Duration::from_millis(800),
            "{:?}",
            start.elapsed()
        );
        assert!(!completed);
    }

    #[test]
    #[cfg(windows)]
    fn update_rejects_an_unparseable_installed_version() {
        let _fixture = FakeUpdate::new("exit /b 0", "echo not-a-version");
        let mut completed = false;
        let result = execute_tool_update_with_progress("gentle-shell", |pct, _, _, _| {
            completed |= pct == 100;
        });
        assert!(
            result
                .unwrap_err()
                .contains("version could not be verified")
        );
        assert!(!completed);
    }

    #[test]
    #[cfg(windows)]
    fn update_shared_version_probes_preserve_each_tools_reported_version() {
        let _fixture = FakeUpdate::new("exit /b 0", "echo 4.8.0\r\nexit /b 0");
        let ecosystem = collect_ecosystem_telemetry();
        for name in [
            "pi",
            "hudr2",
            "gentle-shell",
            "gentle-ai",
            "engram",
            "herdr",
        ] {
            let tool = ecosystem
                .tools
                .iter()
                .find(|tool| tool.name == name)
                .unwrap();
            assert!(tool.is_installed, "{name}");
            assert_eq!(tool.version.as_deref(), Some("v4.8.0"), "{name}");
        }
        for name in ["pi", "gentle-shell", "gentle-ai", "engram", "herdr"] {
            let result = execute_tool_update_with_progress(name, |_, _, _, _| {});
            assert!(result.is_ok(), "{name}: {result:?}");
            assert!(result.unwrap().contains("v4.8.0"), "{name}");
        }
    }

    #[test]
    fn test_clean_version_output() {
        assert_eq!(clean_version_output("pi", "0.87.0"), "v0.87.0");
        assert_eq!(clean_version_output("hudr2", "hudr2 0.1.0"), "v0.1.0");
        assert_eq!(clean_version_output("hudr2", "hudr2 v0.1.0"), "v0.1.0");
        assert_eq!(clean_version_output("hudr2", "0.1.0"), "v0.1.0");
        assert_eq!(clean_version_output("engram", "engram 3.6.2"), "v3.6.2");
        assert_eq!(clean_version_output("herdr", "herdr 0.9.5"), "v0.9.5");
        assert_eq!(
            clean_version_output("gentle-ai", "gentle-ai 4.3.1"),
            "v4.3.1"
        );
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
        assert!(update_cmd_for_tool("hudr2").is_none());
        assert!(assisted_update_notice("hudr2").is_some());
        assert!(update_cmd_for_tool("nonexistent").is_none());

        let (bin_pi, args_pi) = update_cmd_for_tool("pi").unwrap();
        assert_eq!(bin_pi, "npm");
        assert!(args_pi.contains(&"--force"));
        assert!(args_pi.contains(&"@earendil-works/pi-coding-agent@latest"));

        let (bin_gs, args_gs) = update_cmd_for_tool("gentle-shell").unwrap();
        assert_eq!(bin_gs, "npm");
        assert!(args_gs.contains(&"--force"));
        assert!(args_gs.contains(&"gentle-pi@latest"));

        let (bin_engram, args_engram) = update_cmd_for_tool("engram").unwrap();
        assert_eq!(bin_engram, "gentle-ai");
        assert_eq!(args_engram, vec!["upgrade", "engram"]);

        let (bin_gai, args_gai) = update_cmd_for_tool("gentle-ai").unwrap();
        if cfg!(target_os = "windows") {
            assert_eq!(bin_gai, "go");
            assert!(args_gai.contains(&"install"));
        } else {
            assert_eq!(bin_gai, "gentle-ai");
            assert_eq!(args_gai, vec!["upgrade", "gentle-ai"]);
        }
    }

    #[test]
    fn test_parse_line_progress() {
        let (pct1, stage1) = parse_line_progress("Checking for updates...", 10);
        assert_eq!(pct1, 15);
        assert_eq!(stage1, "verificando");

        let (pct2, stage2) = parse_line_progress("Creating pre-upgrade backup...", 15);
        assert_eq!(pct2, 30);
        assert_eq!(stage2, "backup");

        let (pct3, stage3) =
            parse_line_progress("Upgrading engram via binary (3.6.2 -> 3.7.0)...", 30);
        assert_eq!(pct3, 75);
        assert_eq!(stage3, "instalando");

        let (pct4, stage4) = parse_line_progress("Downloaded 65% of release assets", 30);
        assert_eq!(pct4, 65);
        assert_eq!(stage4, "descargando");

        let (pct5, stage5) = parse_line_progress("[ok] engram 3.6.2 -> 3.7.0", 75);
        assert_eq!(pct5, 100);
        assert_eq!(stage5, "completado");

        let (pct6, stage6) =
            parse_line_progress("added 107 packages, and changed 16 packages in 16s", 75);
        assert_eq!(pct6, 100);
        assert_eq!(stage6, "completado");
    }

    #[test]
    fn test_preserve_in_flight_update_state() {
        let source = EcosystemTelemetry {
            tools: vec![
                ToolVersionInfo {
                    name: "pi".to_string(),
                    version: Some("v0.87.0".to_string()),
                    is_installed: true,
                    is_updating: true,
                    update_progress: Some(ToolUpdateProgress {
                        percent: 60,
                        stage: "instalando".to_string(),
                        spinner_idx: 4,
                        elapsed_secs: 5,
                    }),
                    ..Default::default()
                },
                ToolVersionInfo {
                    name: "herdr".to_string(),
                    version: Some("v0.9.5".to_string()),
                    is_installed: true,
                    is_updating: false,
                    update_progress: None,
                    ..Default::default()
                },
            ],
        };

        let mut dest = EcosystemTelemetry {
            tools: vec![
                ToolVersionInfo {
                    name: "pi".to_string(),
                    version: Some("v0.87.0".to_string()),
                    is_installed: true,
                    is_updating: false,
                    update_progress: None,
                    ..Default::default()
                },
                ToolVersionInfo {
                    name: "herdr".to_string(),
                    version: Some("v0.9.5".to_string()),
                    is_installed: true,
                    is_updating: false,
                    update_progress: None,
                    ..Default::default()
                },
            ],
        };

        preserve_in_flight_update_state(&mut dest, &source);

        assert!(dest.tools[0].is_updating);
        assert_eq!(
            dest.tools[0].update_progress,
            source.tools[0].update_progress
        );
        assert!(!dest.tools[1].is_updating);
        assert_eq!(dest.tools[1].update_progress, None);
    }
}
