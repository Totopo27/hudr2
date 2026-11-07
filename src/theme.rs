use std::path::PathBuf;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

/// Representa una especificación cromática de HUD2 para terminal 24-bit Truecolor.
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

pub static ALL_THEMES: [&Theme; 5] = [
    &LINEAR_STUDIO,
    &ELECTRIC_CYBER,
    &BAUHAUS_MINIMAL,
    &SYNTHWAVE_SUNSET,
    &EDITORIAL_VELVET,
];

/// Busca un tema por ID o alias amigable.
pub fn find_theme(name: &str) -> Option<&'static Theme> {
    let lower = name.trim().to_lowercase().replace([' ', '_'], "-");
    match lower.as_str() {
        "linear" | "studio" | "linear-studio" | "default" | "base" => Some(&LINEAR_STUDIO),
        "cyber" | "electric" | "electric-cyber" | "cyberpunk" => Some(&ELECTRIC_CYBER),
        "bauhaus" | "minimal" | "bauhaus-minimal" => Some(&BAUHAUS_MINIMAL),
        "synthwave" | "sunset" | "synthwave-sunset" | "retrowave" => Some(&SYNTHWAVE_SUNSET),
        "editorial" | "velvet" | "editorial-velvet" | "luxe" => Some(&EDITORIAL_VELVET),
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
        if in_theme_section && clean_line.starts_with("name") {
            if let Some((_, raw_val)) = clean_line.split_once('=') {
                let clean = raw_val.trim().trim_matches(['"', '\'']);
                if !clean.is_empty() {
                    return Some(clean.to_string());
                }
            }
        }
    }
    None
}

/// Mapea el tema configurado en Herdr / Telar a una de las identidades nativas calibradas de HUDR2.
pub fn map_herdr_theme(herdr_name: &str) -> &'static Theme {
    let normalized = herdr_name.trim().to_lowercase().replace([' ', '_'], "-");
    match normalized.as_str() {
        // Familia Cyberpunk / High-Tech
        "cyberdream" | "tokyo-night" | "tokyonight" | "nightfly" | "gentleman" | "cyber" => {
            &ELECTRIC_CYBER
        }
        // Familia Arquitectura / Industrial / Cálido
        "gruvbox" | "gruvbox-dark" | "gruvbox-light" | "everforest" | "everforest-dark"
        | "everforest-light" | "solarized" | "solarized-dark" | "solarized-light" => {
            &BAUHAUS_MINIMAL
        }
        // Familia Retro / Gélido / Vaporwave
        "nord" | "poimandres" | "ayu-mirage" | "ayu" | "tokyo-night-day" | "tokyo-day" => {
            &SYNTHWAVE_SUNSET
        }
        // Familia Luxe / Tipografía / Suizo
        "melange" | "oxocarbon" | "oxocarbon-dark" | "zenbones" | "monokai-pro-octagon"
        | "kanagawa" | "kanagawa-lotus" => &EDITORIAL_VELVET,
        // Familia Linear / Base / Default (Catppuccin, Dracula, Rose-Pine, One-Dark, etc.)
        _ => &LINEAR_STUDIO,
    }
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
        cached_theme: &LINEAR_STUDIO,
    })
});

/// Resuelve el tema adaptativo de Herdr con caché por `mtime` (0% CPU, 0 subprocesos).
pub fn resolve_adaptive_theme() -> &'static Theme {
    let mut cache = match ADAPTIVE_CACHE.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    if cache.path.is_none() {
        cache.path = resolve_herdr_config_path();
    }

    let Some(path) = cache.path.clone() else {
        return &LINEAR_STUDIO;
    };

    let current_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();

    if current_mtime == cache.last_mtime && cache.last_mtime.is_some() {
        return cache.cached_theme;
    }

    cache.last_mtime = current_mtime;

    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Some(herdr_name) = parse_herdr_theme_name(&content) {
            let mapped = map_herdr_theme(&herdr_name);
            cache.cached_theme = mapped;
            return mapped;
        }
    }

    cache.cached_theme = &LINEAR_STUDIO;
    &LINEAR_STUDIO
}

// Puntero atómico para el tema activo en tiempo de ejecución
static ACTIVE_THEME_PTR: AtomicPtr<Theme> =
    AtomicPtr::new(&LINEAR_STUDIO as *const Theme as *mut Theme);

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
            name = "cyberdream"
            auto_switch = false
        "#;
        assert_eq!(
            parse_herdr_theme_name(toml).as_deref(),
            Some("cyberdream")
        );
    }

    #[test]
    fn test_parse_herdr_theme_name_with_quotes_and_comments() {
        let toml = r#"
            [session]
            resume = true

            [theme] # configuración de tema
            name = 'gruvbox-dark' # tema retro

            [other]
            name = "ignore_this"
        "#;
        assert_eq!(
            parse_herdr_theme_name(toml).as_deref(),
            Some("gruvbox-dark")
        );
    }

    #[test]
    fn test_parse_herdr_theme_name_missing() {
        let toml = r#"
            onboarding = false
            [session]
            resume = true
        "#;
        assert_eq!(parse_herdr_theme_name(toml), None);
    }

    #[test]
    fn test_map_herdr_theme_categories() {
        assert_eq!(map_herdr_theme("cyberdream").id, "cyber");
        assert_eq!(map_herdr_theme("tokyo-night").id, "cyber");
        assert_eq!(map_herdr_theme("gruvbox").id, "bauhaus");
        assert_eq!(map_herdr_theme("everforest").id, "bauhaus");
        assert_eq!(map_herdr_theme("nord").id, "synthwave");
        assert_eq!(map_herdr_theme("poimandres").id, "synthwave");
        assert_eq!(map_herdr_theme("oxocarbon").id, "editorial");
        assert_eq!(map_herdr_theme("melange").id, "editorial");
        assert_eq!(map_herdr_theme("catppuccin").id, "linear");
        assert_eq!(map_herdr_theme("dracula").id, "linear");
        assert_eq!(map_herdr_theme("unknown-theme").id, "linear");
    }

    #[test]
    fn test_find_theme_by_alias() {
        assert_eq!(find_theme("cyber").unwrap().id, "cyber");
        assert_eq!(find_theme("bauhaus").unwrap().id, "bauhaus");
        assert_eq!(find_theme("synthwave").unwrap().id, "synthwave");
        assert_eq!(find_theme("editorial").unwrap().id, "editorial");
        assert_eq!(find_theme("linear").unwrap().id, "linear");
        assert_eq!(find_theme("studio").unwrap().id, "linear");
        assert!(find_theme("nonexistent").is_none());
    }

    #[test]
    fn test_set_active_theme_threadsafe() {
        set_active_theme(&BAUHAUS_MINIMAL);
        assert_eq!(active_theme().id, "bauhaus");
        set_active_theme(&LINEAR_STUDIO);
        assert_eq!(active_theme().id, "linear");
    }
}
