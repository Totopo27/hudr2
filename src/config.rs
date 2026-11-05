// Paleta Linear ANSI Truecolor 24-bit
pub const BORDER: &str = "\x1b[38;2;120;75;160m"; // #784ba0 (violeta estructural)
pub const BORDER_MUTED: &str = "\x1b[38;2;70;45;95m"; // #462d5f (separador sutil)
pub const ACCENT_PRIMARY: &str = "\x1b[38;2;123;138;255m"; // #7B8AFF (periwinkle Linear)
pub const ACCENT_PINK: &str = "\x1b[38;2;240;149;200m"; // #F095C8 (indicador activo > / *)
pub const GOLD: &str = "\x1b[38;2;224;194;122m"; // #E0C27A (etiquetas, números)
pub const AMBER: &str = "\x1b[38;2;242;184;109m"; // #F2B86D (aviso 70%)
pub const MINT: &str = "\x1b[38;2;76;183;130m"; // #4CB782 (success green)
pub const CORAL: &str = "\x1b[38;2;235;87;87m"; // #EB5757 (error / crítico 85%+)
pub const TEXT_PRIMARY: &str = "\x1b[38;2;247;248;248m"; // #F7F8F8 (off-white texto)
pub const TEXT_MUTED: &str = "\x1b[38;2;138;143;152m"; // #8A8F98 (muted)
pub const TEXT_DIM: &str = "\x1b[38;2;90;94;102m"; // #5A5E66 (detalles tenues)
pub const CYAN: &str = "\x1b[38;2;139;233;253m"; // #8BE9FD (cyan)
pub const MAGENTA: &str = "\x1b[38;2;189;147;249m"; // #BD93F9 (purple)
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

// Re-exportar funciones de resolución de rutas desde el módulo paths
pub use crate::paths::{resolve_auths_dir, resolve_engram_db_path, resolve_opencode_db_path};
