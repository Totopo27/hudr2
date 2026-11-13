use crate::models::HerdrPane;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

static RESOLVED_HERDR_BIN: LazyLock<Mutex<Option<Option<&'static str>>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn clear_herdr_binary_cache() {
    if let Ok(mut guard) = RESOLVED_HERDR_BIN.lock() {
        *guard = None;
    }
}

fn resolve_available_herdr_binary() -> Option<&'static str> {
    if let Ok(guard) = RESOLVED_HERDR_BIN.lock()
        && let Some(cached) = *guard
    {
        return cached;
    }

    let found = (|| {
        let path_var = std::env::var_os("PATH")?;
        for &bin in &["herdr", "telardev", "telar"] {
            for dir in std::env::split_paths(&path_var) {
                #[cfg(target_os = "windows")]
                {
                    for ext in [".exe", ".cmd", ".bat", ""] {
                        if dir.join(format!("{bin}{ext}")).is_file() {
                            return Some(bin);
                        }
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    if dir.join(bin).is_file() {
                        return Some(bin);
                    }
                }
            }
        }
        None
    })();

    if let Ok(mut guard) = RESOLVED_HERDR_BIN.lock() {
        *guard = Some(found);
    }
    found
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum HerdrWireEvent {
    SubscriptionStarted,
    PaneUpdated(HerdrPane),
    PaneClosed(String),
    StatusChanged {
        pane_id: String,
        agent_status: String,
    },
    WorkspaceFocused,
    Disconnected,
}

/// Deep module para interactuar con Herdr / Telar.
/// Prioriza la llamada socket/pipe IPC directo (NDJSON) y usa el CLI `herdr` como respaldo.
pub struct HerdrClient;

impl HerdrClient {
    #[cfg(target_os = "windows")]
    fn quote_powershell_arg(value: &str) -> String {
        format!("'{}'", value.replace('\'', "''"))
    }

    pub fn resolve_ipc_path() -> Option<PathBuf> {
        for env_key in ["HERDR_SOCKET_PATH", "TELAR_SOCKET_PATH"] {
            if let Ok(p) = std::env::var(env_key) {
                let trimmed = p.trim();
                if !trimmed.is_empty() {
                    return Some(PathBuf::from(trimmed));
                }
            }
        }
        #[cfg(unix)]
        {
            let mut bases = Vec::new();
            if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
                bases.push(PathBuf::from(runtime));
            }
            if let Some(config) = dirs::config_dir() {
                bases.push(config);
            }
            if let Some(home) = dirs::home_dir() {
                bases.push(home.join(".config"));
            }
            for base in bases {
                for (dir, file) in [
                    ("herdr", "herdr.sock"),
                    ("telardev", "telardev.sock"),
                    ("telar", "telar.sock"),
                ] {
                    let sock = base.join(dir).join(file);
                    if sock.exists() {
                        return Some(sock);
                    }
                }
            }
        }
        #[cfg(windows)]
        {
            for candidate in [
                r"\\.\pipe\herdr",
                r"\\.\pipe\herdr.sock",
                r"\\.\pipe\telardev",
                r"\\.\pipe\telar",
            ] {
                let p = PathBuf::from(candidate);
                if p.exists() {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Realiza una petición NDJSON directa al socket/pipe de Herdr (`{"id":"hud","method":...}`).
    pub fn request_ipc_at(socket_path: &Path, method: &str, params: Value) -> Option<Value> {
        #[cfg(unix)]
        {
            use std::os::unix::net::UnixStream;
            let mut stream = UnixStream::connect(socket_path).ok()?;
            let _ = stream.set_read_timeout(Some(Duration::from_millis(1500)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(1500)));
            writeln!(
                stream,
                "{}",
                json!({"id": "hud", "method": method, "params": params})
            )
            .ok()?;
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line).ok()?;
            let v: Value = serde_json::from_str(&line).ok()?;
            if !v["error"].is_null() {
                return None;
            }
            Some(v["result"].clone())
        }
        #[cfg(windows)]
        {
            let path_str = socket_path.to_string_lossy();
            let pipe_name = if path_str.starts_with(r"\\.\pipe\") {
                path_str.into_owned()
            } else {
                return None;
            };
            let (tx, rx) = std::sync::mpsc::channel();
            let method_str = method.to_string();
            let params_val = params.clone();
            std::thread::spawn(move || {
                let run = || -> Option<Value> {
                    let mut file = std::fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&pipe_name)
                        .ok()?;
                    writeln!(
                        file,
                        "{}",
                        json!({"id": "hud", "method": method_str, "params": params_val})
                    )
                    .ok()?;
                    file.flush().ok()?;
                    let mut line = String::new();
                    BufReader::new(file).read_line(&mut line).ok()?;
                    let v: Value = serde_json::from_str(&line).ok()?;
                    if !v["error"].is_null() {
                        return None;
                    }
                    Some(v["result"].clone())
                };
                let _ = tx.send(run());
            });
            rx.recv_timeout(Duration::from_millis(1500)).ok().flatten()
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (socket_path, method, params);
            None
        }
    }

    pub fn request_ipc(method: &str, params: Value) -> Option<Value> {
        let sock = Self::resolve_ipc_path()?;
        Self::request_ipc_at(&sock, method, params)
    }

    pub fn parse_pane_json(item: &Value) -> Option<HerdrPane> {
        let pane_id = item
            .get("pane_id")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        if pane_id.is_empty() {
            return None;
        }
        let tab_id = item
            .get("tab_id")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let workspace_id = item
            .get("workspace_id")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let cwd = item
            .get("cwd")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let terminal_title = item
            .get("terminal_title")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let terminal_title_stripped = item
            .get("terminal_title_stripped")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let focused = item
            .get("focused")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let agent_status = item
            .get("agent_status")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let agent = item
            .get("agent")
            .and_then(|a| a.as_str())
            .map(|s| s.to_string());
        let agent_session_id = item
            .get("agent_session")
            .and_then(|s| s.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let mut tokens = std::collections::HashMap::new();
        if let Some(tok_obj) = item.get("tokens").and_then(Value::as_object) {
            for (k, v) in tok_obj {
                if let Some(s) = v.as_str() {
                    tokens.insert(k.clone(), s.to_string());
                }
            }
        }

        Some(HerdrPane {
            pane_id,
            tab_id,
            workspace_id,
            cwd,
            terminal_title,
            terminal_title_stripped,
            focused,
            agent_status,
            agent,
            agent_session_id,
            tokens,
        })
    }

    /// Parsea un mensaje NDJSON del stream `events.subscribe` de Herdr.
    /// Soporta tanto `pane_agent_status_changed` como `pane.agent_status_changed` (con punto).
    pub fn parse_wire_event(v: &Value) -> Option<HerdrWireEvent> {
        if !v["error"].is_null() {
            return None;
        }
        if v["result"]["type"].as_str() == Some("subscription_started") {
            return Some(HerdrWireEvent::SubscriptionStarted);
        }
        let ev_name = v.get("event").and_then(Value::as_str).unwrap_or("");
        match ev_name {
            "pane_updated" | "pane.updated" => {
                let pane = Self::parse_pane_json(&v["data"]["pane"])?;
                Some(HerdrWireEvent::PaneUpdated(pane))
            }
            "pane_closed" | "pane.closed" => {
                let pid = v["data"]["pane_id"].as_str()?.to_string();
                Some(HerdrWireEvent::PaneClosed(pid))
            }
            "pane_agent_status_changed" | "pane.agent_status_changed" => {
                let pid = v["data"]["pane_id"].as_str()?.to_string();
                let st = v["data"]["agent_status"].as_str()?.to_string();
                Some(HerdrWireEvent::StatusChanged {
                    pane_id: pid,
                    agent_status: st,
                })
            }
            "workspace_focused" | "workspace.focused" => Some(HerdrWireEvent::WorkspaceFocused),
            _ => None,
        }
    }

    /// Extrae el `pane_id` devuelto por `herdr pane split` (`.result.pane.pane_id`, `.pane.pane_id` o `pane_id`).
    pub fn extract_split_pane_id(val: &Value) -> Option<String> {
        val.pointer("/result/pane/pane_id")
            .or_else(|| val.pointer("/pane/pane_id"))
            .or_else(|| val.get("pane_id"))
            .and_then(|v| v.as_str())
            .or_else(|| val.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
    }

    /// Construye la línea de comando para ejecutar `hudr2` dentro del nuevo pane split.
    /// En Windows (donde Herdr lanza PowerShell por defecto), antepone el operador `&`
    /// para que PowerShell invoque correctamente ejecutables entrecomillados con argumentos.
    pub fn build_attach_cmd_line(
        exe_path: &Path,
        cwd: &str,
        parent_pane: Option<&str>,
        config_path: Option<&Path>,
    ) -> String {
        let exe_str = exe_path.to_string_lossy();
        #[cfg(target_os = "windows")]
        let mut cmd_line = format!(
            "& {} --cwd {}",
            Self::quote_powershell_arg(&exe_str),
            Self::quote_powershell_arg(cwd)
        );
        #[cfg(not(target_os = "windows"))]
        let mut cmd_line = format!("\"{exe_str}\" --cwd \"{cwd}\"");

        if let Some(parent) = parent_pane {
            let trimmed = parent.trim();
            if !trimmed.is_empty() {
                #[cfg(target_os = "windows")]
                cmd_line.push_str(&format!(" --pane {}", Self::quote_powershell_arg(trimmed)));
                #[cfg(not(target_os = "windows"))]
                cmd_line.push_str(&format!(" --pane \"{trimmed}\""));
            }
        }
        if let Some(cfg) = config_path {
            #[cfg(target_os = "windows")]
            cmd_line.push_str(&format!(
                " --config {}",
                Self::quote_powershell_arg(&cfg.to_string_lossy())
            ));
            #[cfg(not(target_os = "windows"))]
            cmd_line.push_str(&format!(" --config \"{}\"", cfg.to_string_lossy()));
        }
        cmd_line
    }

    /// Crea un split derecho del 20% en Herdr/Telar y lanza `hudr2` dentro del nuevo panel sin secuestrar el actual.
    pub fn attach_sidebar_split(exe_path: &Path, cwd: &str, config_path: Option<&Path>) -> bool {
        if std::env::var_os("HERDR_PANE_ID").is_none() && Self::resolve_ipc_path().is_none() {
            return false;
        }

        let split_res = Self::run_herdr_cli(&[
            "pane",
            "split",
            "--current",
            "--direction",
            "right",
            "--ratio",
            "0.78",
            "--cwd",
            cwd,
            "--no-focus",
        ]);
        let Some(ref val) = split_res else {
            return false;
        };
        let Some(new_pane_id) = Self::extract_split_pane_id(val) else {
            return false;
        };

        let parent_pane = std::env::var("HERDR_PANE_ID").ok();
        let cmd_line =
            Self::build_attach_cmd_line(exe_path, cwd, parent_pane.as_deref(), config_path);

        let _ = Self::run_herdr_cli(&["pane", "run", &new_pane_id, &cmd_line]);
        true
    }

    fn run_herdr_cli(args: &[&str]) -> Option<Value> {
        let bin = resolve_available_herdr_binary()?;
        let mut cmd = Command::new(bin);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().ok()?;
        let stdout_pipe = child.stdout.take()?;
        let out_handle = std::thread::spawn(move || {
            let mut s = String::new();
            let _ = stdout_pipe.take(512 * 1024).read_to_string(&mut s);
            s
        });

        let deadline = Instant::now() + Duration::from_millis(1500);
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

        if !status.success() {
            return None;
        }

        let raw = out_handle.join().unwrap_or_default();
        if raw.trim().is_empty() {
            Some(Value::Null)
        } else {
            serde_json::from_str::<Value>(&raw).ok()
        }
    }

    pub fn get_panes() -> Vec<HerdrPane> {
        // 1. Prioridad: IPC directo (socket / pipe) sin crear subproceso
        if let Some(res) = Self::request_ipc("pane.list", json!({}))
            && let Some(arr) = res
                .get("panes")
                .and_then(|p| p.as_array())
                .or_else(|| res.as_array())
        {
            return arr.iter().filter_map(Self::parse_pane_json).collect();
        }

        // 2. Fallback: CLI 'herdr pane list'
        let mut panes = Vec::new();
        if let Some(val) = Self::run_herdr_cli(&["pane", "list"])
            && let Some(arr) = val
                .get("result")
                .and_then(|r| r.get("panes"))
                .and_then(|p| p.as_array())
        {
            for item in arr {
                if let Some(p) = Self::parse_pane_json(item) {
                    panes.push(p);
                }
            }
        }

        panes
    }

    pub fn get_current_pane_and_tab(panes: &[HerdrPane]) -> (Option<String>, Option<String>) {
        let env_pane_id = std::env::var("HERDR_PANE_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(ref my_pid) = env_pane_id {
            let my_tab = panes
                .iter()
                .find(|p| p.pane_id == *my_pid)
                .and_then(|p| p.tab_id.clone());
            return (Some(my_pid.clone()), my_tab);
        }

        // Si hay panes cargados y alguno tiene foco, resolver en memoria sin lanzar otro subproceso
        if let Some(focused) = panes.iter().find(|p| p.focused) {
            return (Some(focused.pane_id.clone()), focused.tab_id.clone());
        }

        if let Some(val) = Self::run_herdr_cli(&["pane", "current"]) {
            let cur_pane = val.get("result").and_then(|r| r.get("pane"));
            let p_id = cur_pane
                .and_then(|p| p.get("pane_id"))
                .and_then(|id| id.as_str())
                .map(|s| s.to_string());
            let t_id = cur_pane
                .and_then(|p| p.get("tab_id"))
                .and_then(|id| id.as_str())
                .map(|s| s.to_string());
            return (p_id, t_id);
        }

        (None, None)
    }

    pub fn close_pane(pane_id: &str) -> bool {
        if Self::request_ipc("pane.close", json!({ "pane_id": pane_id })).is_some() {
            return true;
        }
        Self::run_herdr_cli(&["pane", "close", pane_id]).is_some()
    }

    /// Inicia un hilo suscriptor al stream NDJSON `events.subscribe` de Herdr cuando el socket/pipe existe.
    pub fn spawn_event_subscriber<F>(bound_pane_id: Option<String>, on_event: F)
    where
        F: Fn(HerdrWireEvent) + Send + 'static,
    {
        let Some(socket_path) = Self::resolve_ipc_path() else {
            return;
        };

        std::thread::spawn(move || {
            let mut backoff_secs = 1u64;
            loop {
                let mut subs = vec![
                    json!({"type": "pane.updated"}),
                    json!({"type": "pane.closed"}),
                    json!({"type": "workspace.focused"}),
                ];
                if let Some(ref pid) = bound_pane_id {
                    subs.push(json!({"type": "pane.agent_status_changed", "pane_id": pid}));
                } else {
                    subs.push(json!({"type": "pane.agent_status_changed"}));
                }

                let subscribe_msg = json!({
                    "id": "events",
                    "method": "events.subscribe",
                    "params": { "subscriptions": subs }
                });

                #[cfg(unix)]
                let connected = {
                    if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&socket_path) {
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
                        if writeln!(stream, "{subscribe_msg}").is_ok() {
                            backoff_secs = 1;
                            for line in BufReader::new(stream).lines() {
                                let Ok(l) = line else { break };
                                let Ok(v) = serde_json::from_str::<Value>(&l) else {
                                    continue;
                                };
                                if let Some(ev) = Self::parse_wire_event(&v) {
                                    let is_bound_closed = matches!(
                                        (&ev, &bound_pane_id),
                                        (HerdrWireEvent::PaneClosed(closed_id), Some(bound)) if closed_id == bound
                                    );
                                    on_event(ev);
                                    if is_bound_closed {
                                        return;
                                    }
                                }
                            }
                        }
                        true
                    } else {
                        false
                    }
                };

                #[cfg(windows)]
                let connected = {
                    let path_str = socket_path.to_string_lossy();
                    if path_str.starts_with(r"\\.\pipe\")
                        && let Ok(mut file) = std::fs::OpenOptions::new()
                            .read(true)
                            .write(true)
                            .open(path_str.as_ref())
                    {
                        if writeln!(file, "{subscribe_msg}").is_ok() && file.flush().is_ok() {
                            backoff_secs = 1;
                            for line in BufReader::new(file).lines() {
                                let Ok(l) = line else { break };
                                let Ok(v) = serde_json::from_str::<Value>(&l) else {
                                    continue;
                                };
                                if let Some(ev) = Self::parse_wire_event(&v) {
                                    let is_bound_closed = matches!(
                                        (&ev, &bound_pane_id),
                                        (HerdrWireEvent::PaneClosed(closed_id), Some(bound)) if closed_id == bound
                                    );
                                    on_event(ev);
                                    if is_bound_closed {
                                        return;
                                    }
                                }
                            }
                        }
                        true
                    } else {
                        false
                    }
                };

                #[cfg(not(any(unix, windows)))]
                let connected = false;

                if !connected {
                    on_event(HerdrWireEvent::Disconnected);
                }
                std::thread::sleep(Duration::from_secs(backoff_secs));
                backoff_secs = (backoff_secs * 2).min(30);
            }
        });
    }
}
