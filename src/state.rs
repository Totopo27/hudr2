use crate::models::TelemetrySnapshot;
use crate::ui::CardBounds;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const CARD_KEYS: [&str; 11] = [
    "estado",
    "contexto",
    "proyecto",
    "integraciones",
    "pools",
    "git",
    "engram",
    "tools",
    "mcp",
    "ecosystem",
    "tarjetas",
];

pub const DEFAULT_COLLAPSED: [bool; 11] = [
    false, false, true, false, false, false, false, false, false, false, true,
];

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigData {
    pub collapsed: [bool; 11],
    pub hidden: HashSet<String>,
}

/// Carga la configuración (tarjetas colapsadas y ocultas) desde un archivo JSON.
pub fn load_config(path: &Path) -> ConfigData {
    let Ok(bytes) = std::fs::read(path) else {
        return ConfigData {
            collapsed: DEFAULT_COLLAPSED,
            hidden: HashSet::new(),
        };
    };
    let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
        return ConfigData {
            collapsed: DEFAULT_COLLAPSED,
            hidden: HashSet::new(),
        };
    };

    let mut collapsed = DEFAULT_COLLAPSED;
    if let Some(arr) = v.get("collapsed").and_then(Value::as_array) {
        collapsed = [false; 11];
        for item in arr.iter().filter_map(Value::as_str) {
            if let Some(idx) = CARD_KEYS.iter().position(|&k| k == item) {
                collapsed[idx] = true;
            }
        }
    }

    let mut hidden = HashSet::new();
    if let Some(arr) = v.get("hidden").and_then(Value::as_array) {
        for item in arr.iter().filter_map(Value::as_str) {
            if item != "tarjetas" && CARD_KEYS.contains(&item) {
                hidden.insert(item.to_string());
            }
        }
    }

    ConfigData { collapsed, hidden }
}

/// Guarda de forma atómica (`<file>.<pid>.tmp` + `rename`) las tarjetas colapsadas y ocultas.
pub fn save_config(
    path: &Path,
    collapsed: &[bool; 11],
    hidden: &HashSet<String>,
) -> Result<(), String> {
    let collapsed_names: Vec<&str> = CARD_KEYS
        .iter()
        .enumerate()
        .filter_map(|(i, &k)| if collapsed[i] { Some(k) } else { None })
        .collect();
    let hidden_names: Vec<&str> = CARD_KEYS
        .iter()
        .copied()
        .filter(|&k| k != "tarjetas" && hidden.contains(k))
        .collect();
    let payload = json!({
        "collapsed": collapsed_names,
        "hidden": hidden_names,
    });
    let body = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    crate::paths::atomic_write_bytes(path, &body).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct QuickPeekState {
    pub title: String,
    pub lines: Vec<String>,
    pub scroll: usize,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub snapshot: TelemetrySnapshot,
    pub collapsed_cards: [bool; 11],
    pub hidden_cards: HashSet<String>,
    pub scroll_offset: usize,
    pub card_bounds: Vec<CardBounds>,
    pub flash_notice: Option<String>,
    pub flash_notice_until: Option<std::time::Instant>,
    pub needs_redraw: bool,
    pub config_path: Option<PathBuf>,
    pub selected_diff_file: Option<String>,
    pub selected_diff_lines: Vec<String>,
    pub quick_peek: Option<QuickPeekState>,
}

impl AppState {
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());

        Self {
            snapshot: TelemetrySnapshot {
                project_dir: cwd,
                ..Default::default()
            },
            // [estado, contexto, proyecto, integraciones, pools, git, engram, tools, mcp, ecosystem]
            // proyecto (idx 2) viene colapsado por defecto como en hudr original
            collapsed_cards: DEFAULT_COLLAPSED,
            hidden_cards: HashSet::new(),
            scroll_offset: 0,
            card_bounds: Vec::new(),
            flash_notice: None,
            flash_notice_until: None,
            needs_redraw: true,
            config_path: None,
            selected_diff_file: None,
            selected_diff_lines: Vec::new(),
            quick_peek: None,
        }
    }

    pub fn with_config(config_path: Option<PathBuf>, custom_cwd: Option<String>) -> Self {
        let mut s = Self::new();
        if let Some(cwd) = custom_cwd {
            s.snapshot.project_dir = cwd;
        }
        if let Some(ref p) = config_path {
            let cfg = load_config(p);
            s.collapsed_cards = cfg.collapsed;
            s.hidden_cards = cfg.hidden;
        }
        s.config_path = config_path;
        s
    }

    fn persist_config_if_configured(&self) {
        if let Some(ref path) = self.config_path {
            let _ = save_config(path, &self.collapsed_cards, &self.hidden_cards);
        }
    }

    pub fn set_flash_notice(&mut self, notice: Option<String>, duration_ms: u64) {
        if let Some(msg) = notice {
            self.flash_notice = Some(msg);
            self.flash_notice_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(duration_ms));
        } else {
            self.flash_notice = None;
            self.flash_notice_until = None;
        }
        self.needs_redraw = true;
    }

    pub fn check_flash_notice_expiry(&mut self) -> bool {
        if let Some(until) = self.flash_notice_until
            && std::time::Instant::now() >= until
        {
            self.flash_notice = None;
            self.flash_notice_until = None;
            self.needs_redraw = true;
            return true;
        }
        false
    }

    pub fn toggle_card(&mut self, index: usize) {
        if index < self.collapsed_cards.len() {
            self.collapsed_cards[index] = !self.collapsed_cards[index];
            self.needs_redraw = true;
            self.persist_config_if_configured();
        }
    }

    pub fn toggle_all_collapse(&mut self) {
        let any_open = self.collapsed_cards.iter().any(|&c| !c);
        self.collapsed_cards.fill(any_open);
        self.needs_redraw = true;
        self.persist_config_if_configured();
    }

    pub fn toggle_card_visibility(&mut self, card_key: &str) {
        if card_key == "tarjetas" {
            return; // Tarjetas es fija
        }
        if self.hidden_cards.contains(card_key) {
            self.hidden_cards.remove(card_key);
        } else {
            self.hidden_cards.insert(card_key.to_string());
        }
        self.needs_redraw = true;
        self.persist_config_if_configured();
    }

    pub fn is_card_hidden(&self, card_key: &str) -> bool {
        if card_key == "tarjetas" {
            false
        } else {
            self.hidden_cards.contains(card_key)
        }
    }

    pub fn toggle_git_file_diff(&mut self, rel_path: &str) {
        if self.selected_diff_file.as_deref() == Some(rel_path) {
            self.selected_diff_file = None;
            self.selected_diff_lines.clear();
        } else {
            self.selected_diff_file = Some(rel_path.to_string());
            self.selected_diff_lines =
                crate::git::read_file_diff_preview(&self.snapshot.project_dir, rel_path);
        }
        self.needs_redraw = true;
    }

    pub fn open_quick_peek(&mut self, rel_path: &str) {
        let lines = crate::git::read_file_quick_peek(&self.snapshot.project_dir, rel_path);
        self.quick_peek = Some(QuickPeekState {
            title: rel_path.to_string(),
            lines,
            scroll: 0,
        });
        self.needs_redraw = true;
    }

    pub fn close_quick_peek(&mut self) {
        if self.quick_peek.is_some() {
            self.quick_peek = None;
            self.needs_redraw = true;
        }
    }

    pub fn toggle_quick_peek(&mut self) {
        if self.quick_peek.is_some() {
            self.close_quick_peek();
            return;
        }
        if let Some(path) = self.selected_diff_file.clone() {
            self.open_quick_peek(&path);
        } else if let Some(first) = self.snapshot.git.files.first() {
            let path = first.path.clone();
            self.open_quick_peek(&path);
        }
    }

    pub fn scroll_up(&mut self) {
        if let Some(ref mut peek) = self.quick_peek {
            if peek.scroll > 0 {
                peek.scroll = peek.scroll.saturating_sub(2);
                self.needs_redraw = true;
            }
            return;
        }
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(2);
            self.needs_redraw = true;
        }
    }

    pub fn scroll_down(&mut self) {
        if let Some(ref mut peek) = self.quick_peek {
            let max_scroll = peek.lines.len().saturating_sub(1);
            if peek.scroll < max_scroll {
                peek.scroll = (peek.scroll + 2).min(max_scroll);
                self.needs_redraw = true;
            }
            return;
        }
        self.scroll_offset = self.scroll_offset.saturating_add(2);
        self.needs_redraw = true;
    }

    pub fn handle_mouse_click(
        &mut self,
        clicked_row: usize,
        actual_scroll_offset: usize,
    ) -> Option<ClickAction> {
        let absolute_line = clicked_row + actual_scroll_offset;

        // 1. Prioridad: Acciones especiales exactas (abrir carpeta, diff, quick peek o visibilidad)
        let mut matched_special: Option<(&'static str, Option<String>)> = None;
        for bound in &self.card_bounds {
            if let Some(action) = bound.action_type
                && absolute_line >= bound.start_line
                && absolute_line <= bound.end_line
            {
                matched_special = Some((action, bound.action_path.clone()));
                break;
            }
        }
        if let Some((action, action_path)) = matched_special {
            if action == "open_folder"
                && let Some(p) = action_path
            {
                return Some(ClickAction::OpenFolder(p));
            } else if action == "git_file_diff"
                && let Some(p) = action_path
            {
                self.toggle_git_file_diff(&p);
                return Some(ClickAction::ToggleGitFileDiff(p));
            } else if action == "open_quick_peek"
                && let Some(p) = action_path
            {
                self.open_quick_peek(&p);
                return Some(ClickAction::OpenQuickPeek(p));
            } else if action == "close_quick_peek" {
                self.close_quick_peek();
                return Some(ClickAction::CloseQuickPeek);
            } else if action == "toggle_card_visibility"
                && let Some(p) = action_path
            {
                self.toggle_card_visibility(&p);
                return Some(ClickAction::ToggleCardVisibility(p));
            }
        }

        // 2. Clic en tarjetas para colapsar / expandir
        // - Si la tarjeta está colapsada, hacer clic en cualquiera de sus filas la expande.
        // - Si la tarjeta está abierta, sólo hacer clic en su encabezado (start_line) la colapsa.
        //   Esto previene colisiones accidentales entre el borde inferior (end_line) de una
        //   tarjeta abierta (como Contexto) y el encabezado de la tarjeta siguiente (como Proyecto).
        for bound in &self.card_bounds {
            if bound.action_type.is_none()
                && let Some(idx) = CARD_KEYS.iter().position(|&k| k == bound.id)
            {
                let is_collapsed = self.collapsed_cards[idx];
                let hit = if is_collapsed {
                    absolute_line >= bound.start_line && absolute_line <= bound.end_line
                } else {
                    absolute_line == bound.start_line
                };

                if hit {
                    self.toggle_card(idx);
                    return Some(ClickAction::ToggledCard(idx));
                }
            }
        }

        None
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClickAction {
    OpenFolder(String),
    ToggledCard(usize),
    ToggleGitFileDiff(String),
    OpenQuickPeek(String),
    CloseQuickPeek,
    ToggleCardVisibility(String),
}
