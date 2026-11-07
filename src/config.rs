// Paleta Linear ANSI Truecolor 24-bit (compatibilidad y fallback)
pub const BORDER: &str = "\x1b[38;2;88;63;120m"; // #583F78 (violeta visible calibrado)
pub const BORDER_MUTED: &str = "\x1b[38;2;52;34;71m"; // #342247 (separador sutil)
pub const ACCENT_PRIMARY: &str = "\x1b[38;2;123;138;255m"; // #7B8AFF (periwinkle Linear)
pub const ACCENT_PINK: &str = "\x1b[38;2;240;149;200m"; // #F095C8 (indicador activo > / *)
pub const GOLD: &str = "\x1b[38;2;148;163;184m"; // #94A3B8 (slate plata limpio, sustituye al viejo dorado)
pub const AMBER: &str = "\x1b[38;2;245;158;11m"; // #F59E0B (aviso 70%)
pub const MINT: &str = "\x1b[38;2;16;185;129m"; // #10B981 (success green)
pub const CORAL: &str = "\x1b[38;2;244;63;94m"; // #F43F5E (error / crítico 85%+)
pub const TEXT_PRIMARY: &str = "\x1b[38;2;248;250;252m"; // #F8FAFC (blanco platino)
pub const TEXT_MUTED: &str = "\x1b[38;2;138;143;152m"; // #8A8F98 (muted)
pub const TEXT_DIM: &str = "\x1b[38;2;75;70;88m"; // #4B4658 (detalles tenues)
pub const CYAN: &str = "\x1b[38;2;165;180;252m"; // #A5B4FC (lavanda suave)
pub const MAGENTA: &str = "\x1b[38;2;192;132;252m"; // #C084FC (purple)
pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";

pub const QUOTA_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";
pub const USER_AGENT: &str =
    "antigravity/hub/2.29.0 (aidev_client; os_type=darwin; arch=arm64; cl=1013060670)";

/// HTTP client compartido — reutiliza connection pool y sesiones TLS
pub static HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .user_agent(USER_AGENT)
        .build()
        .expect("Failed to create HTTP client")
});
