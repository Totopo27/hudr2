use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

/// Representa una especificación cromática de HUDR2 para terminal 24-bit Truecolor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub border: &'static str,
    pub border_muted: &'static str,
    pub accent_primary: &'static str,
    pub accent_secondary: &'static str,
    pub title_color: &'static str,
    pub label_color: &'static str,
    pub text_primary: &'static str,
    pub text_muted: &'static str,
    pub text_dim: &'static str,
    pub mint: &'static str,
    pub amber: &'static str,
    pub coral: &'static str,
    pub cyan: &'static str,
}

// ==============================================================================
// 1. TEMAS NATIVOS PROPIOS DE HUDR2 (5 Creaciones Autorales)
// ==============================================================================

// 0. Linear Studio (Moderno) — Calibrado
pub const LINEAR_STUDIO: Theme = Theme {
    id: "linear",
    name: "Linear Studio (Moderno)",
    border: "\x1b[38;2;88;63;120m",            // #583F78 (violeta visible)
    border_muted: "\x1b[38;2;52;34;71m",      // #342247
    accent_primary: "\x1b[38;2;123;138;255m", // #7B8AFF (periwinkle Linear)
    accent_secondary: "\x1b[38;2;240;149;200m", // #F095C8 (rosa Linear para cursor > / *)
    title_color: "\x1b[38;2;123;138;255m",    // #7B8AFF (títulos periwinkle)
    label_color: "\x1b[38;2;148;163;184m",    // #94A3B8 (slate plata, 0% dorado)
    text_primary: "\x1b[38;2;248;250;252m",   // #F8FAFC (blanco platino)
    text_muted: "\x1b[38;2;138;143;152m",     // #8A8F98
    text_dim: "\x1b[38;2;75;70;88m",          // #4B4658
    mint: "\x1b[38;2;16;185;129m",            // #10B981 (esmeralda Linear)
    amber: "\x1b[38;2;245;158;11m",           // #F59E0B
    coral: "\x1b[38;2;244;63;94m",            // #F43F5E
    cyan: "\x1b[38;2;165;180;252m",           // #A5B4FC (lavanda suave)
};

// 1. Electric Cyber (Clear Minds)
pub const ELECTRIC_CYBER: Theme = Theme {
    id: "cyber",
    name: "Electric Cyber",
    border: "\x1b[38;2;30;41;59m",             // #1E293B
    border_muted: "\x1b[38;2;22;32;50m",       // #162032
    accent_primary: "\x1b[38;2;163;230;53m",  // #A3E635 (electric lime)
    accent_secondary: "\x1b[38;2;56;189;248m",// #38BDF8 (cyan neón)
    title_color: "\x1b[38;2;163;230;53m",     // #A3E635
    label_color: "\x1b[38;2;148;163;184m",    // #94A3B8
    text_primary: "\x1b[38;2;243;244;246m",   // #F3F4F6 (cool white)
    text_muted: "\x1b[38;2;148;163;184m",
    text_dim: "\x1b[38;2;71;85;105m",         // #475569
    mint: "\x1b[38;2;163;230;53m",            // #A3E635
    amber: "\x1b[38;2;245;158;11m",           // #F59E0B
    coral: "\x1b[38;2;244;63;94m",            // #F43F5E
    cyan: "\x1b[38;2;56;189;248m",            // #38BDF8
};

// 2. Bauhaus Minimal (Less is More)
pub const BAUHAUS_MINIMAL: Theme = Theme {
    id: "bauhaus",
    name: "Bauhaus Minimal",
    border: "\x1b[38;2;15;118;110m",          // #0F766E (teal arquitectónico)
    border_muted: "\x1b[38;2;19;78;74m",      // #134E4A
    accent_primary: "\x1b[38;2;249;115;22m",  // #F97316 (naranja bauhaus)
    accent_secondary: "\x1b[38;2;56;189;248m",// #38BDF8 (cyan neón)
    title_color: "\x1b[38;2;249;115;22m",     // #F97316
    label_color: "\x1b[38;2;168;162;158m",    // #A8A29E (sand muted)
    text_primary: "\x1b[38;2;255;251;235m",   // #FFFBEB (warm ivory)
    text_muted: "\x1b[38;2;156;163;175m",     // #9CA3AF
    text_dim: "\x1b[38;2;87;83;78m",          // #57534E
    mint: "\x1b[38;2;45;212;191m",            // #2DD4BF
    amber: "\x1b[38;2;251;146;60m",           // #FB923C
    coral: "\x1b[38;2;239;68;68m",            // #EF4444
    cyan: "\x1b[38;2;56;189;248m",            // #38BDF8
};

// 3. Synthwave Sunset (Tired of Doing Nothing)
pub const SYNTHWAVE_SUNSET: Theme = Theme {
    id: "synthwave",
    name: "Synthwave Sunset",
    border: "\x1b[38;2;14;116;144m",          // #0E7490 (ocean teal)
    border_muted: "\x1b[38;2;21;94;117m",     // #155E75
    accent_primary: "\x1b[38;2;251;113;133m", // #FB7185 (coral pink)
    accent_secondary: "\x1b[38;2;56;189;248m",// #38BDF8 (cyan neón)
    title_color: "\x1b[38;2;251;113;133m",    // #FB7185
    label_color: "\x1b[38;2;148;163;184m",    // #94A3B8
    text_primary: "\x1b[38;2;236;254;255m",   // #ECFEFF (ice blue)
    text_muted: "\x1b[38;2;148;163;184m",
    text_dim: "\x1b[38;2;71;85;105m",         // #475569
    mint: "\x1b[38;2;45;212;191m",            // #2DD4BF
    amber: "\x1b[38;2;251;146;60m",           // #FB923C
    coral: "\x1b[38;2;251;113;133m",          // #FB7185
    cyan: "\x1b[38;2;56;189;248m",            // #38BDF8
};

// 4. Editorial Velvet (Look Like A Pro)
pub const EDITORIAL_VELVET: Theme = Theme {
    id: "editorial",
    name: "Editorial Velvet",
    border: "\x1b[38;2;190;18;60m",           // #BE123C (crimson red)
    border_muted: "\x1b[38;2;88;13;29m",      // #580D1D
    accent_primary: "\x1b[38;2;253;186;116m", // #FDBA74 (peach orange)
    accent_secondary: "\x1b[38;2;56;189;248m",// #38BDF8 (cyan neón)
    title_color: "\x1b[38;2;253;186;116m",    // #FDBA74
    label_color: "\x1b[38;2;161;161;170m",    // #A1A1AA (zinc)
    text_primary: "\x1b[38;2;255;247;237m",   // #FFF7ED (warm cream)
    text_muted: "\x1b[38;2;161;161;170m",
    text_dim: "\x1b[38;2;82;82;91m",          // #52525B
    mint: "\x1b[38;2;52;211;153m",            // #34D399
    amber: "\x1b[38;2;251;146;60m",           // #FB923C
    coral: "\x1b[38;2;244;63;94m",            // #F43F5E
    cyan: "\x1b[38;2;56;189;248m",            // #38BDF8
};

pub static HUDR2_NATIVE_THEMES: [&Theme; 5] = [
    &LINEAR_STUDIO,
    &ELECTRIC_CYBER,
    &BAUHAUS_MINIMAL,
    &SYNTHWAVE_SUNSET,
    &EDITORIAL_VELVET,
];

// ==============================================================================
// 2. TEMAS ADOPTADOS DE HERDR (13 Opciones Oficiales de Ajustes + Gentleman)
// ==============================================================================

// H1. Catppuccin (Mocha) — Tema por defecto en Herdr
pub const HERDR_CATPPUCCIN: Theme = Theme {
    id: "catppuccin",
    name: "Catppuccin (Mocha)",
    border: "\x1b[38;2;69;71;90m",            // #45475A (surface1)
    border_muted: "\x1b[38;2;49;50;68m",      // #313244 (surface0)
    accent_primary: "\x1b[38;2;137;180;250m", // #89B4FA (blue)
    accent_secondary: "\x1b[38;2;203;166;247m", // #CBA6F7 (mauve)
    title_color: "\x1b[38;2;137;180;250m",    // #89B4FA
    label_color: "\x1b[38;2;166;173;200m",    // #A6ADC8 (subtext0)
    text_primary: "\x1b[38;2;205;214;244m",   // #CDD6F4 (text)
    text_muted: "\x1b[38;2;108;112;134m",     // #6C7086 (overlay0)
    text_dim: "\x1b[38;2;69;71;90m",          // #45475A (surface1)
    mint: "\x1b[38;2;166;227;161m",           // #A6E3A1 (green)
    amber: "\x1b[38;2;249;226;175m",          // #F9E2AF (yellow)
    coral: "\x1b[38;2;243;139;168m",          // #F38BA8 (red)
    cyan: "\x1b[38;2;148;226;213m",           // #94E2D5 (teal)
};

// H2. Catppuccin Latte (Claro)
pub const HERDR_CATPPUCCIN_LATTE: Theme = Theme {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    border: "\x1b[38;2;188;192;204m",         // #BCC0CC (surface1)
    border_muted: "\x1b[38;2;204;208;218m",   // #CCD0DA (surface0)
    accent_primary: "\x1b[38;2;30;102;245m",  // #1E66F5 (blue)
    accent_secondary: "\x1b[38;2;136;57;239m", // #8839EF (mauve)
    title_color: "\x1b[38;2;30;102;245m",
    label_color: "\x1b[38;2;108;111;133m",    // #6C6F85 (subtext0)
    text_primary: "\x1b[38;2;76;79;105m",     // #4C4F69 (text)
    text_muted: "\x1b[38;2;156;160;176m",     // #9CA0B0 (overlay0)
    text_dim: "\x1b[38;2;188;192;204m",
    mint: "\x1b[38;2;64;160;43m",             // #40A02B (green)
    amber: "\x1b[38;2;223;142;29m",           // #DF8E1D (yellow)
    coral: "\x1b[38;2;210;15;57m",            // #D20F39 (red)
    cyan: "\x1b[38;2;23;146;153m",            // #179299 (teal)
};

// H3. Terminal (ANSI 16 colores estándar)
pub const HERDR_TERMINAL: Theme = Theme {
    id: "terminal",
    name: "Terminal (16-Color)",
    border: "\x1b[38;5;8m",                   // DarkGray
    border_muted: "\x1b[38;5;8m",
    accent_primary: "\x1b[38;5;4m",           // Blue
    accent_secondary: "\x1b[38;5;5m",         // Magenta
    title_color: "\x1b[38;5;4m",
    label_color: "\x1b[38;5;7m",              // Gray
    text_primary: "\x1b[38;5;15m",            // White
    text_muted: "\x1b[38;5;8m",
    text_dim: "\x1b[38;5;8m",
    mint: "\x1b[38;5;2m",                     // Green
    amber: "\x1b[38;5;3m",                    // Yellow
    coral: "\x1b[38;5;1m",                    // Red
    cyan: "\x1b[38;5;6m",                     // Cyan
};

// H4. Tokyo Night
pub const HERDR_TOKYO_NIGHT: Theme = Theme {
    id: "tokyo-night",
    name: "Tokyo Night",
    border: "\x1b[38;2;65;72;104m",           // #414868 (surface1)
    border_muted: "\x1b[38;2;41;46;66m",      // #292E42 (surface_dim)
    accent_primary: "\x1b[38;2;122;162;247m", // #7AA2F7 (blue)
    accent_secondary: "\x1b[38;2;187;154;247m", // #BB9AF7 (mauve)
    title_color: "\x1b[38;2;122;162;247m",
    label_color: "\x1b[38;2;169;177;214m",    // #A9B1D6 (subtext0)
    text_primary: "\x1b[38;2;192;202;245m",   // #C0CAF5 (text)
    text_muted: "\x1b[38;2;86;95;137m",       // #565F89 (overlay0)
    text_dim: "\x1b[38;2;65;72;104m",
    mint: "\x1b[38;2;158;206;106m",           // #9ECE6A (green)
    amber: "\x1b[38;2;224;175;104m",          // #E0AF68 (yellow)
    coral: "\x1b[38;2;247;118;142m",          // #F7768E (red)
    cyan: "\x1b[38;2;125;207;255m",           // #7DCFFF (teal)
};

// H5. Tokyo Night Day (Claro)
pub const HERDR_TOKYO_NIGHT_DAY: Theme = Theme {
    id: "tokyo-night-day",
    name: "Tokyo Night Day",
    border: "\x1b[38;2;168;174;203m",         // #A8AECB (surface1)
    border_muted: "\x1b[38;2;210;211;218m",   // #D2D3DA (surface_dim)
    accent_primary: "\x1b[38;2;46;125;233m",  // #2E7DE9 (blue)
    accent_secondary: "\x1b[38;2;120;71;189m", // #7847BD (mauve)
    title_color: "\x1b[38;2;46;125;233m",
    label_color: "\x1b[38;2;97;114;176m",     // #6172B0 (subtext0)
    text_primary: "\x1b[38;2;55;96;191m",     // #3760BF (text)
    text_muted: "\x1b[38;2;137;144;179m",     // #8990B3 (overlay0)
    text_dim: "\x1b[38;2;168;174;203m",
    mint: "\x1b[38;2;88;117;57m",             // #587539 (green)
    amber: "\x1b[38;2;140;108;62m",           // #8C6C3E (yellow)
    coral: "\x1b[38;2;245;42;101m",           // #F52A65 (red)
    cyan: "\x1b[38;2;17;140;116m",            // #118C74 (teal)
};

// H6. Dracula
pub const HERDR_DRACULA: Theme = Theme {
    id: "dracula",
    name: "Dracula",
    border: "\x1b[38;2;98;114;164m",          // #6272A4 (surface1 / overlay0)
    border_muted: "\x1b[38;2;58;62;80m",      // #3A3E50 (surface_dim)
    accent_primary: "\x1b[38;2;189;147;249m", // #BD93F9 (purple)
    accent_secondary: "\x1b[38;2;255;121;198m", // #FF79C6 (mauve pink)
    title_color: "\x1b[38;2;189;147;249m",
    label_color: "\x1b[38;2;210;210;220m",    // #D2D2DC (subtext0)
    text_primary: "\x1b[38;2;248;248;242m",   // #F8F8F2 (text)
    text_muted: "\x1b[38;2;98;114;164m",      // #6272A4 (overlay0)
    text_dim: "\x1b[38;2;68;71;90m",          // #44475A (surface0)
    mint: "\x1b[38;2;80;250;123m",            // #50FA7B (green)
    amber: "\x1b[38;2;241;250;140m",          // #F1FA8C (yellow)
    coral: "\x1b[38;2;255;85;85m",            // #FF5555 (red)
    cyan: "\x1b[38;2;139;233;253m",           // #8BE9FD (cyan)
};

// H7. Nord
pub const HERDR_NORD: Theme = Theme {
    id: "nord",
    name: "Nord",
    border: "\x1b[38;2;67;76;94m",            // #434C5E (surface1)
    border_muted: "\x1b[38;2;59;66;82m",      // #3B4252 (surface_dim)
    accent_primary: "\x1b[38;2;136;192;208m", // #88C0D0 (frost blue)
    accent_secondary: "\x1b[38;2;180;142;173m", // #B48EAD (mauve)
    title_color: "\x1b[38;2;136;192;208m",
    label_color: "\x1b[38;2;216;222;233m",    // #D8DEE9 (subtext0)
    text_primary: "\x1b[38;2;236;239;244m",   // #ECEFF4 (text)
    text_muted: "\x1b[38;2;76;86;106m",       // #4C566A (overlay0)
    text_dim: "\x1b[38;2;67;76;94m",
    mint: "\x1b[38;2;163;190;140m",           // #A3BE8C (green)
    amber: "\x1b[38;2;235;203;139m",          // #EBCB8B (yellow)
    coral: "\x1b[38;2;191;97;106m",           // #BF616A (red)
    cyan: "\x1b[38;2;143;188;187m",           // #8FBCBB (teal)
};

// H8. Gruvbox (Dark)
pub const HERDR_GRUVBOX: Theme = Theme {
    id: "gruvbox",
    name: "Gruvbox (Dark)",
    border: "\x1b[38;2;80;73;69m",            // #504945 (surface1)
    border_muted: "\x1b[38;2;60;56;54m",      // #3C3836 (surface_dim)
    accent_primary: "\x1b[38;2;215;153;33m",  // #D79921 (yellow)
    accent_secondary: "\x1b[38;2;254;128;25m", // #FE8019 (peach orange)
    title_color: "\x1b[38;2;215;153;33m",
    label_color: "\x1b[38;2;213;196;161m",    // #D5C4A1 (subtext0)
    text_primary: "\x1b[38;2;235;219;178m",   // #EBDBB2 (text)
    text_muted: "\x1b[38;2;146;131;116m",     // #928374 (overlay0)
    text_dim: "\x1b[38;2;80;73;69m",
    mint: "\x1b[38;2;184;187;38m",            // #B8BB26 (green)
    amber: "\x1b[38;2;250;189;47m",           // #FABD2F (yellow)
    coral: "\x1b[38;2;251;73;52m",            // #FB4934 (red)
    cyan: "\x1b[38;2;142;192;124m",           // #8EC07C (teal / aqua)
};

// H9. Gruvbox Light
pub const HERDR_GRUVBOX_LIGHT: Theme = Theme {
    id: "gruvbox-light",
    name: "Gruvbox Light",
    border: "\x1b[38;2;213;196;161m",         // #D5C4A1 (surface1)
    border_muted: "\x1b[38;2;235;219;178m",   // #EBDBB2 (surface_dim)
    accent_primary: "\x1b[38;2;181;118;20m",  // #B57614 (yellow)
    accent_secondary: "\x1b[38;2;143;63;113m", // #8F3F71 (mauve)
    title_color: "\x1b[38;2;181;118;20m",
    label_color: "\x1b[38;2;80;73;69m",       // #504945 (subtext0)
    text_primary: "\x1b[38;2;60;56;54m",      // #3C3836 (text)
    text_muted: "\x1b[38;2;168;153;132m",     // #A89984 (overlay0)
    text_dim: "\x1b[38;2;213;196;161m",
    mint: "\x1b[38;2;121;116;14m",            // #79740E (green)
    amber: "\x1b[38;2;181;118;20m",           // #B57614 (yellow)
    coral: "\x1b[38;2;157;0;6m",              // #9D0006 (red)
    cyan: "\x1b[38;2;66;123;88m",             // #427B58 (teal)
};

// H10. One Dark
pub const HERDR_ONE_DARK: Theme = Theme {
    id: "one-dark",
    name: "One Dark",
    border: "\x1b[38;2;62;68;81m",            // #3E4451 (surface1)
    border_muted: "\x1b[38;2;53;59;69m",      // #353B45 (surface_dim)
    accent_primary: "\x1b[38;2;97;175;239m",  // #61AFEF (blue)
    accent_secondary: "\x1b[38;2;198;120;221m", // #C678DD (mauve)
    title_color: "\x1b[38;2;97;175;239m",
    label_color: "\x1b[38;2;150;156;168m",    // #969CA8 (subtext0)
    text_primary: "\x1b[38;2;171;178;191m",   // #ABB2BF (text)
    text_muted: "\x1b[38;2;92;99;112m",       // #5C6370 (overlay0)
    text_dim: "\x1b[38;2;62;68;81m",
    mint: "\x1b[38;2;152;195;121m",           // #98C379 (green)
    amber: "\x1b[38;2;229;192;123m",          // #E5C07B (yellow)
    coral: "\x1b[38;2;224;108;117m",          // #E06C75 (red)
    cyan: "\x1b[38;2;86;182;194m",            // #56B6C2 (teal)
};

// H11. One Light
pub const HERDR_ONE_LIGHT: Theme = Theme {
    id: "one-light",
    name: "One Light",
    border: "\x1b[38;2;229;229;230m",         // #E5E5E6 (surface1)
    border_muted: "\x1b[38;2;234;234;235m",   // #EAEAEB (surface_dim)
    accent_primary: "\x1b[38;2;64;120;242m",  // #4078F2 (blue)
    accent_secondary: "\x1b[38;2;166;38;164m", // #A626A4 (mauve)
    title_color: "\x1b[38;2;64;120;242m",
    label_color: "\x1b[38;2;79;82;93m",       // #4F525D (subtext0)
    text_primary: "\x1b[38;2;56;58;66m",      // #383A42 (text)
    text_muted: "\x1b[38;2;160;161;167m",     // #A0A1A7 (overlay0)
    text_dim: "\x1b[38;2;229;229;230m",
    mint: "\x1b[38;2;80;161;79m",             // #50A14F (green)
    amber: "\x1b[38;2;193;132;1m",            // #C18401 (yellow)
    coral: "\x1b[38;2;228;86;73m",            // #E45649 (red)
    cyan: "\x1b[38;2;1;132;188m",             // #0184BC (teal)
};

// H12. Solarized (Dark)
pub const HERDR_SOLARIZED: Theme = Theme {
    id: "solarized",
    name: "Solarized (Dark)",
    border: "\x1b[38;2;88;110;117m",          // #586E75 (base01)
    border_muted: "\x1b[38;2;7;54;66m",       // #073642 (base02)
    accent_primary: "\x1b[38;2;38;139;210m",  // #268BD2 (blue)
    accent_secondary: "\x1b[38;2;211;54;130m", // #D33682 (magenta)
    title_color: "\x1b[38;2;38;139;210m",
    label_color: "\x1b[38;2;131;148;150m",    // #839496 (base0)
    text_primary: "\x1b[38;2;147;161;161m",   // #93A1A1 (base1)
    text_muted: "\x1b[38;2;88;110;117m",      // #586E75 (base01)
    text_dim: "\x1b[38;2;7;54;66m",
    mint: "\x1b[38;2;133;153;0m",             // #859900 (green)
    amber: "\x1b[38;2;181;137;0m",            // #B58900 (yellow)
    coral: "\x1b[38;2;220;50;47m",            // #DC322F (red)
    cyan: "\x1b[38;2;42;161;152m",            // #2AA198 (teal)
};

// H13. Solarized Light
pub const HERDR_SOLARIZED_LIGHT: Theme = Theme {
    id: "solarized-light",
    name: "Solarized Light",
    border: "\x1b[38;2;147;161;161m",         // #93A1A1 (surface1)
    border_muted: "\x1b[38;2;238;232;213m",   // #EEE8D5 (surface_dim)
    accent_primary: "\x1b[38;2;38;139;210m",  // #268BD2 (blue)
    accent_secondary: "\x1b[38;2;211;54;130m", // #D33682 (magenta)
    title_color: "\x1b[38;2;38;139;210m",
    label_color: "\x1b[38;2;131;148;150m",    // #839496 (subtext0)
    text_primary: "\x1b[38;2;101;123;131m",   // #657B83 (text)
    text_muted: "\x1b[38;2;147;161;161m",     // #93A1A1 (overlay0)
    text_dim: "\x1b[38;2;238;232;213m",
    mint: "\x1b[38;2;133;153;0m",             // #859900 (green)
    amber: "\x1b[38;2;181;137;0m",            // #B58900 (yellow)
    coral: "\x1b[38;2;220;50;47m",            // #DC322F (red)
    cyan: "\x1b[38;2;42;161;152m",            // #2AA198 (teal)
};

// H14. Gentleman (Signature Gentle-Shell)
pub const HERDR_GENTLEMAN: Theme = Theme {
    id: "gentleman",
    name: "Gentleman (Signature)",
    border: "\x1b[38;2;85;52;72m",            // #553448 (surface1)
    border_muted: "\x1b[38;2;65;42;58m",      // #412A3A (surface_dim)
    accent_primary: "\x1b[38;2;240;149;200m", // #F095C8 (blush rose)
    accent_secondary: "\x1b[38;2;224;194;122m", // #E0C27A (champagne gold)
    title_color: "\x1b[38;2;240;149;200m",
    label_color: "\x1b[38;2;210;165;185m",    // #D2A5B9 (rose labels)
    text_primary: "\x1b[38;2;246;239;243m",   // #F6EFF3 (pearl soft white)
    text_muted: "\x1b[38;2;141;119;131m",     // #8D7783 (overlay0)
    text_dim: "\x1b[38;2;65;42;58m",
    mint: "\x1b[38;2;180;231;199m",           // #B4E7C7 (mint green)
    amber: "\x1b[38;2;242;184;109m",          // #F2B86D (warm amber)
    coral: "\x1b[38;2;255;113;143m",          // #FF718F (coral)
    cyan: "\x1b[38;2;0;229;255m",             // #00E5FF (electric cyan)
};

pub static HERDR_ADOPTED_THEMES: [&Theme; 14] = [
    &HERDR_CATPPUCCIN,
    &HERDR_CATPPUCCIN_LATTE,
    &HERDR_TERMINAL,
    &HERDR_TOKYO_NIGHT,
    &HERDR_TOKYO_NIGHT_DAY,
    &HERDR_DRACULA,
    &HERDR_NORD,
    &HERDR_GRUVBOX,
    &HERDR_GRUVBOX_LIGHT,
    &HERDR_ONE_DARK,
    &HERDR_ONE_LIGHT,
    &HERDR_SOLARIZED,
    &HERDR_SOLARIZED_LIGHT,
    &HERDR_GENTLEMAN,
];

pub static ALL_THEMES: [&Theme; 19] = [
    // 5 Nativos
    &LINEAR_STUDIO,
    &ELECTRIC_CYBER,
    &BAUHAUS_MINIMAL,
    &SYNTHWAVE_SUNSET,
    &EDITORIAL_VELVET,
    // 14 Adoptados de Herdr
    &HERDR_CATPPUCCIN,
    &HERDR_CATPPUCCIN_LATTE,
    &HERDR_TERMINAL,
    &HERDR_TOKYO_NIGHT,
    &HERDR_TOKYO_NIGHT_DAY,
    &HERDR_DRACULA,
    &HERDR_NORD,
    &HERDR_GRUVBOX,
    &HERDR_GRUVBOX_LIGHT,
    &HERDR_ONE_DARK,
    &HERDR_ONE_LIGHT,
    &HERDR_SOLARIZED,
    &HERDR_SOLARIZED_LIGHT,
    &HERDR_GENTLEMAN,
];

/// Catálogo secuencial de selección interactiva (Auto + 5 Nativos + 14 Herdr).
pub const THEME_CYCLE_KEYS: [&str; 20] = [
    "auto",
    "linear",
    "cyber",
    "bauhaus",
    "synthwave",
    "editorial",
    "catppuccin",
    "catppuccin-latte",
    "terminal",
    "tokyo-night",
    "tokyo-night-day",
    "dracula",
    "nord",
    "gruvbox",
    "gruvbox-light",
    "one-dark",
    "one-light",
    "solarized",
    "solarized-light",
    "gentleman",
];

/// Devuelve la siguiente clave en el ciclo de temas a partir de la clave actual.
pub fn next_theme_in_cycle(current_key: Option<&str>) -> &'static str {
    let cur = current_key.unwrap_or("auto");
    let pos = THEME_CYCLE_KEYS
        .iter()
        .position(|&k| k.eq_ignore_ascii_case(cur))
        .unwrap_or(0);
    THEME_CYCLE_KEYS[(pos + 1) % THEME_CYCLE_KEYS.len()]
}

/// Genera la etiqueta visual limpia para la fila de tema en la card de configuración.
pub fn theme_display_label(theme_key: Option<&str>) -> String {
    match theme_key.unwrap_or("auto") {
        "auto" | "herdr" => {
            let active = active_theme();
            format!("Auto ({})", active.id)
        }
        other => {
            if let Some(th) = find_theme(other) {
                // Nombre legible abreviado si contiene paréntesis
                let clean_name = th.name.split_once(" (").map(|(w, _)| w).unwrap_or(th.name);
                clean_name.to_string()
            } else {
                other.to_string()
            }
        }
    }
}

/// Adopta directamente la especificación cromática de Herdr por su nombre oficial.
pub fn adopt_herdr_theme(herdr_name: &str) -> &'static Theme {
    let lower = herdr_name.trim().to_lowercase().replace([' ', '_'], "-");
    match lower.as_str() {
        "catppuccin" | "catppuccin-mocha" | "mocha" => &HERDR_CATPPUCCIN,
        "catppuccin-latte" | "latte" => &HERDR_CATPPUCCIN_LATTE,
        "terminal" => &HERDR_TERMINAL,
        "tokyo-night" | "tokyonight" => &HERDR_TOKYO_NIGHT,
        "tokyo-night-day" | "tokyo-day" => &HERDR_TOKYO_NIGHT_DAY,
        "dracula" => &HERDR_DRACULA,
        "nord" => &HERDR_NORD,
        "gruvbox" | "gruvbox-dark" => &HERDR_GRUVBOX,
        "gruvbox-light" => &HERDR_GRUVBOX_LIGHT,
        "one-dark" | "onedark" => &HERDR_ONE_DARK,
        "one-light" | "onelight" => &HERDR_ONE_LIGHT,
        "solarized" | "solarized-dark" => &HERDR_SOLARIZED,
        "solarized-light" => &HERDR_SOLARIZED_LIGHT,
        "gentleman" | "gentle" | "gentle-shell" => &HERDR_GENTLEMAN,
        _ => &HERDR_CATPPUCCIN, // Default de Herdr
    }
}

/// Busca un tema por ID o alias amigable entre los 5 de HUDR2 y los 14 de Herdr.
pub fn find_theme(name: &str) -> Option<&'static Theme> {
    let lower = name.trim().to_lowercase().replace([' ', '_'], "-");
    match lower.as_str() {
        // Modo adaptativo
        "herdr" | "auto" | "adopt" => Some(resolve_adaptive_theme()),

        // 5 Temas Nativos de HUDR2
        "linear" | "studio" | "linear-studio" | "default" | "base" => Some(&LINEAR_STUDIO),
        "cyber" | "electric" | "electric-cyber" | "cyberpunk" => Some(&ELECTRIC_CYBER),
        "bauhaus" | "minimal" | "bauhaus-minimal" => Some(&BAUHAUS_MINIMAL),
        "synthwave" | "sunset" | "synthwave-sunset" | "retrowave" => Some(&SYNTHWAVE_SUNSET),
        "editorial" | "velvet" | "editorial-velvet" | "luxe" => Some(&EDITORIAL_VELVET),

        // 14 Temas Adoptados de Herdr
        "catppuccin" | "mocha" | "catppuccin-mocha" => Some(&HERDR_CATPPUCCIN),
        "catppuccin-latte" | "latte" => Some(&HERDR_CATPPUCCIN_LATTE),
        "terminal" => Some(&HERDR_TERMINAL),
        "tokyo-night" | "tokyonight" => Some(&HERDR_TOKYO_NIGHT),
        "tokyo-night-day" | "tokyo-day" => Some(&HERDR_TOKYO_NIGHT_DAY),
        "dracula" => Some(&HERDR_DRACULA),
        "nord" => Some(&HERDR_NORD),
        "gruvbox" | "gruvbox-dark" => Some(&HERDR_GRUVBOX),
        "gruvbox-light" => Some(&HERDR_GRUVBOX_LIGHT),
        "one-dark" | "onedark" => Some(&HERDR_ONE_DARK),
        "one-light" | "onelight" => Some(&HERDR_ONE_LIGHT),
        "solarized" | "solarized-dark" => Some(&HERDR_SOLARIZED),
        "solarized-light" => Some(&HERDR_SOLARIZED_LIGHT),
        "gentleman" | "gentle" | "gentle-shell" => Some(&HERDR_GENTLEMAN),

        _ => None,
    }
}

/// Extrae el campo `name` bajo la sección `[theme]` en un archivo TOML (zero-regex).
pub fn parse_herdr_theme_name(content: &str) -> Option<String> {
    let mut in_theme_section = false;
    for line in content.lines() {
        let clean_line = line.split('#').next().unwrap_or("").trim();
        if clean_line.is_empty() {
            continue;
        }
        if clean_line.starts_with('[') && clean_line.ends_with(']') {
            let section = clean_line.trim_matches(['[', ']']).trim();
            in_theme_section = section == "theme";
            continue;
        }
        if in_theme_section
            && clean_line.starts_with("name")
            && let Some((_, raw_val)) = clean_line.split_once('=')
        {
            let clean = raw_val.trim().trim_matches(['"', '\'']);
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

/// Resuelve la ruta del archivo `config.toml` de Herdr según el sistema operativo.
pub fn resolve_herdr_config_path() -> Option<PathBuf> {
    for env_key in ["HERDR_CONFIG_PATH", "TELAR_CONFIG_PATH"] {
        if let Ok(p) = std::env::var(env_key) {
            let pb = PathBuf::from(p);
            if pb.is_file() {
                return Some(pb);
            }
        }
    }

    #[cfg(windows)]
    {
        if let Some(appdata) = dirs::config_dir() {
            for dir in ["herdr", "telardev", "telar"] {
                let candidate = appdata.join(dir).join("config.toml");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Some(config_dir) = dirs::config_dir() {
            for dir in ["herdr", "telardev", "telar"] {
                let candidate = config_dir.join(dir).join("config.toml");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        if let Some(home) = dirs::home_dir() {
            for dir in ["herdr", "telardev", "telar"] {
                let candidate = home.join(".config").join(dir).join("config.toml");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

struct AdaptiveCache {
    path: Option<PathBuf>,
    last_mtime: Option<SystemTime>,
    cached_theme: &'static Theme,
}

static ADAPTIVE_CACHE: LazyLock<Mutex<AdaptiveCache>> = LazyLock::new(|| {
    Mutex::new(AdaptiveCache {
        path: None,
        last_mtime: None,
        cached_theme: &HERDR_CATPPUCCIN,
    })
});

/// Resuelve el tema adoptado directamente de Herdr con caché por `mtime` (0% CPU, 0 subprocesos).
pub fn resolve_adaptive_theme() -> &'static Theme {
    let mut cache = match ADAPTIVE_CACHE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    if cache.path.is_none() {
        cache.path = resolve_herdr_config_path();
    }

    let Some(path) = cache.path.clone() else {
        return &HERDR_CATPPUCCIN;
    };

    let current_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();

    if current_mtime == cache.last_mtime && cache.last_mtime.is_some() {
        return cache.cached_theme;
    }

    cache.last_mtime = current_mtime;

    if let Ok(content) = std::fs::read_to_string(&path)
        && let Some(herdr_name) = parse_herdr_theme_name(&content)
    {
        let adopted = adopt_herdr_theme(&herdr_name);
        cache.cached_theme = adopted;
        return adopted;
    }

    // Si existe archivo de Herdr pero no especifica tema, Herdr usa Catppuccin por defecto
    cache.cached_theme = &HERDR_CATPPUCCIN;
    &HERDR_CATPPUCCIN
}

// Puntero atómico para el tema activo en tiempo de ejecución
static ACTIVE_THEME_PTR: AtomicPtr<Theme> =
    AtomicPtr::new(&HERDR_CATPPUCCIN as *const Theme as *mut Theme);

/// Obtiene el tema activo globalmente en HUDR2.
pub fn active_theme() -> &'static Theme {
    let ptr = ACTIVE_THEME_PTR.load(Ordering::Acquire);
    unsafe { &*ptr }
}

/// Establece manualmente el tema activo globalmente.
pub fn set_active_theme(theme: &'static Theme) {
    ACTIVE_THEME_PTR.store(theme as *const Theme as *mut Theme, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_herdr_theme_name_standard() {
        let toml = r#"
            onboarding = false

            [theme]
            name = "dracula"
            auto_switch = false
        "#;
        assert_eq!(
            parse_herdr_theme_name(toml).as_deref(),
            Some("dracula")
        );
    }

    #[test]
    fn test_parse_herdr_theme_name_with_quotes_and_comments() {
        let toml = r#"
            [session]
            resume = true

            [theme] # configuración de tema
            name = 'gruvbox-light' # tema retro claro

            [other]
            name = "ignore_this"
        "#;
        assert_eq!(
            parse_herdr_theme_name(toml).as_deref(),
            Some("gruvbox-light")
        );
    }

    #[test]
    fn test_adopt_all_13_herdr_themes() {
        assert_eq!(adopt_herdr_theme("catppuccin").id, "catppuccin");
        assert_eq!(adopt_herdr_theme("catppuccin-latte").id, "catppuccin-latte");
        assert_eq!(adopt_herdr_theme("terminal").id, "terminal");
        assert_eq!(adopt_herdr_theme("tokyo-night").id, "tokyo-night");
        assert_eq!(adopt_herdr_theme("tokyo-night-day").id, "tokyo-night-day");
        assert_eq!(adopt_herdr_theme("dracula").id, "dracula");
        assert_eq!(adopt_herdr_theme("nord").id, "nord");
        assert_eq!(adopt_herdr_theme("gruvbox").id, "gruvbox");
        assert_eq!(adopt_herdr_theme("gruvbox-light").id, "gruvbox-light");
        assert_eq!(adopt_herdr_theme("one-dark").id, "one-dark");
        assert_eq!(adopt_herdr_theme("one-light").id, "one-light");
        assert_eq!(adopt_herdr_theme("solarized").id, "solarized");
        assert_eq!(adopt_herdr_theme("solarized-light").id, "solarized-light");
        assert_eq!(adopt_herdr_theme("gentleman").id, "gentleman");
        // Unknown defaults to Herdr default (Catppuccin)
        assert_eq!(adopt_herdr_theme("unknown-flavor").id, "catppuccin");
    }

    #[test]
    fn test_find_theme_by_alias_hudr2_and_herdr() {
        // Hudr2 nativos
        assert_eq!(find_theme("linear").unwrap().id, "linear");
        assert_eq!(find_theme("cyber").unwrap().id, "cyber");
        assert_eq!(find_theme("bauhaus").unwrap().id, "bauhaus");
        assert_eq!(find_theme("synthwave").unwrap().id, "synthwave");
        assert_eq!(find_theme("editorial").unwrap().id, "editorial");

        // Herdr adoptados
        assert_eq!(find_theme("nord").unwrap().id, "nord");
        assert_eq!(find_theme("dracula").unwrap().id, "dracula");
        assert_eq!(find_theme("tokyo-night").unwrap().id, "tokyo-night");
        assert_eq!(find_theme("gruvbox-light").unwrap().id, "gruvbox-light");
        assert_eq!(find_theme("one-dark").unwrap().id, "one-dark");
        assert_eq!(find_theme("solarized-light").unwrap().id, "solarized-light");
        assert_eq!(find_theme("gentleman").unwrap().id, "gentleman");
        assert_eq!(find_theme("catppuccin-latte").unwrap().id, "catppuccin-latte");

        assert!(find_theme("nonexistent").is_none());
    }

    #[test]
    fn test_set_active_theme_threadsafe() {
        set_active_theme(&HERDR_DRACULA);
        assert_eq!(active_theme().id, "dracula");
        set_active_theme(&HERDR_CATPPUCCIN);
        assert_eq!(active_theme().id, "catppuccin");
        set_active_theme(&LINEAR_STUDIO);
        assert_eq!(active_theme().id, "linear");
    }
}
