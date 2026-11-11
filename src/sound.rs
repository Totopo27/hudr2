use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundMode {
    #[serde(rename = "off", alias = "silencio")]
    Off,
    #[default]
    #[serde(rename = "bell", alias = "campana")]
    Bell,
    #[serde(rename = "native", alias = "nativo", alias = "sistema")]
    Native,
}

impl SoundMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Bell,
            Self::Bell => Self::Native,
            Self::Native => Self::Off,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Bell => "bell",
            Self::Native => "native",
        }
    }

    pub fn from_str_loose(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "off" | "silencio" | "mute" | "none" => Self::Off,
            "bell" | "campana" => Self::Bell,
            "native" | "nativo" | "sistema" | "system" => Self::Native,
            _ => Self::Bell,
        }
    }

    pub fn display_badge(self) -> (&'static str, bool) {
        match self {
            Self::Off => ("[ ] silencio", false),
            Self::Bell => ("[x] campana", true),
            Self::Native => ("[x] nativo", true),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// La orquestación y sus subagentes concluyeron exitosamente la tarea.
    FleetCompleted,
    /// La cuota del orquestador activo entró en zona crítica (>85% o 429).
    QuotaCritical,
}

static LAST_ALERT_TIMESTAMP_MS: AtomicU64 = AtomicU64::new(0);
const COOLDOWN_MS: u64 = 1500;

fn check_cooldown_and_stamp() -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let last = LAST_ALERT_TIMESTAMP_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < COOLDOWN_MS {
        return false;
    }
    LAST_ALERT_TIMESTAMP_MS.store(now, Ordering::Relaxed);
    true
}

/// Emite una alerta sonora según el modo activo y el tipo de evento.
pub fn emit_alert(mode: SoundMode, kind: AlertKind) {
    if mode == SoundMode::Off {
        return;
    }

    if !check_cooldown_and_stamp() {
        return;
    }

    match mode {
        SoundMode::Off => {}
        SoundMode::Bell => ring_bell(kind),
        SoundMode::Native => play_native_sound(kind),
    }
}

/// Campana ANSI universal en terminal (`\x07` BEL).
/// - FleetCompleted: 1 pulso limpio.
/// - QuotaCritical: 2 pulsos rápidos.
pub fn ring_bell(kind: AlertKind) {
    match kind {
        AlertKind::FleetCompleted => {
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(b"\x07");
            let _ = stdout.flush();
        }
        AlertKind::QuotaCritical => {
            let mut stdout = std::io::stdout();
            let _ = stdout.write_all(b"\x07");
            let _ = stdout.flush();
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(120));
                let mut stdout = std::io::stdout();
                let _ = stdout.write_all(b"\x07");
                let _ = stdout.flush();
            });
        }
    }
}

/// Reproducción nativa en el sistema operativo.
fn play_native_sound(kind: AlertKind) {
    #[cfg(target_os = "windows")]
    {
        platform_windows::play_system_alert(kind);
    }
    #[cfg(target_os = "linux")]
    {
        platform_linux::play_system_alert(kind);
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        ring_bell(kind);
    }
}

#[cfg(target_os = "windows")]
mod platform_windows {
    use super::AlertKind;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    const SND_ALIAS: u32 = 0x00010000;
    const SND_ASYNC: u32 = 0x00000001;
    const SND_NODEFAULT: u32 = 0x00000002;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn PlaySoundW(pszSound: *const u16, hmod: *mut std::ffi::c_void, fdwSound: u32) -> i32;
    }

    pub fn play_system_alert(kind: AlertKind) {
        let alias = match kind {
            AlertKind::FleetCompleted => "Notification.Default",
            AlertKind::QuotaCritical => "SystemExclamation",
        };

        let wide: Vec<u16> = OsStr::new(alias)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            let res = PlaySoundW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                SND_ALIAS | SND_ASYNC | SND_NODEFAULT,
            );
            // Fallback suave si el alias no está configurado en el registro de Windows
            if res == 0 && kind == AlertKind::FleetCompleted {
                let fallback: Vec<u16> = OsStr::new("SystemAsterisk")
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                PlaySoundW(
                    fallback.as_ptr(),
                    std::ptr::null_mut(),
                    SND_ALIAS | SND_ASYNC | SND_NODEFAULT,
                );
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform_linux {
    use super::AlertKind;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    fn wait_child_bounded(mut child: Child, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return false;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(_) => return false,
            }
        }
    }

    pub fn play_system_alert(kind: AlertKind) {
        std::thread::spawn(move || {
            let (xdg_sound, canberra_id) = match kind {
                AlertKind::FleetCompleted => ("complete.oga", "complete"),
                AlertKind::QuotaCritical => ("dialog-warning.oga", "dialog-warning"),
            };

            let paplay_path = format!("/usr/share/sounds/freedesktop/stereo/{xdg_sound}");
            let timeout = Duration::from_secs(2);

            // 1. Probar paplay (PulseAudio / PipeWire) con límite de tiempo estricto
            if std::path::Path::new(&paplay_path).exists() {
                if let Ok(child) = Command::new("paplay")
                    .arg(&paplay_path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                {
                    if wait_child_bounded(child, timeout) {
                        return;
                    }
                }
            }

            // 2. Probar canberra-gtk-play con límite de tiempo estricto
            if let Ok(child) = Command::new("canberra-gtk-play")
                .args(["-i", canberra_id])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                if wait_child_bounded(child, timeout) {
                    return;
                }
            }

            // 3. Fallback a campana ANSI si ningún reproductor tuvo éxito
            super::ring_bell(kind);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_mode_cycle_and_parsing() {
        assert_eq!(SoundMode::Off.next(), SoundMode::Bell);
        assert_eq!(SoundMode::Bell.next(), SoundMode::Native);
        assert_eq!(SoundMode::Native.next(), SoundMode::Off);

        assert_eq!(SoundMode::from_str_loose("off"), SoundMode::Off);
        assert_eq!(SoundMode::from_str_loose("silencio"), SoundMode::Off);
        assert_eq!(SoundMode::from_str_loose("bell"), SoundMode::Bell);
        assert_eq!(SoundMode::from_str_loose("campana"), SoundMode::Bell);
        assert_eq!(SoundMode::from_str_loose("native"), SoundMode::Native);
        assert_eq!(SoundMode::from_str_loose("nativo"), SoundMode::Native);
        assert_eq!(SoundMode::from_str_loose("unknown"), SoundMode::Bell);

        assert_eq!(SoundMode::Off.as_str(), "off");
        assert_eq!(SoundMode::Bell.as_str(), "bell");
        assert_eq!(SoundMode::Native.as_str(), "native");

        let (b_off, a_off) = SoundMode::Off.display_badge();
        assert_eq!(b_off, "[ ] silencio");
        assert!(!a_off);

        let (b_bell, a_bell) = SoundMode::Bell.display_badge();
        assert_eq!(b_bell, "[x] campana");
        assert!(a_bell);

        let (b_nat, a_nat) = SoundMode::Native.display_badge();
        assert_eq!(b_nat, "[x] nativo");
        assert!(a_nat);
    }
}
