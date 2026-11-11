use crate::models::TelemetrySnapshot;
use crate::ui::CardBounds;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const CARD_KEYS: [&str; 12] = [
    "estado",
    "contexto",
    "proyecto",
    "integraciones",
    "orquestadores",
    "pools",
    "git",
    "engram",
    "tools",
    "mcp",
    "ecosystem",
    "personalizar",
];

pub const DEFAULT_COLLAPSED: [bool; 12] = [
    false, false, true, false, false, false, false, false, false, false, false, true,
];

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigData {
    pub collapsed: [bool; 12],
    pub hidden: HashSet<String>,
    pub theme: Option<String>,
    pub sidebar_name: Option<String>,
    pub check_updates: bool,
    pub account_budgets: std::collections::HashMap<String, f64>,
}

/// Carga la configuración (tarjetas colapsadas y ocultas, tema y nombre de sidebar) desde un archivo JSON.
pub fn load_config(path: &Path) -> ConfigData {
    let Ok(bytes) = std::fs::read(path) else {
        return ConfigData {
            collapsed: DEFAULT_COLLAPSED,
            hidden: HashSet::new(),
            theme: None,
            sidebar_name: None,
            check_updates: true,
            account_budgets: std::collections::HashMap::new(),
        };
    };
    let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
        return ConfigData {
            collapsed: DEFAULT_COLLAPSED,
            hidden: HashSet::new(),
            theme: None,
            sidebar_name: None,
            check_updates: true,
            account_budgets: std::collections::HashMap::new(),
        };
    };

    let mut collapsed = DEFAULT_COLLAPSED;
    if let Some(arr) = v.get("collapsed").and_then(Value::as_array) {
        collapsed = [false; 12];
        for item in arr.iter().filter_map(Value::as_str) {
            let normalized = if item == "tarjetas" { "personalizar" } else { item };
            if let Some(idx) = CARD_KEYS.iter().position(|&k| k == normalized) {
                collapsed[idx] = true;
            }
        }
    }

    let mut hidden = HashSet::new();
    if let Some(arr) = v.get("hidden").and_then(Value::as_array) {
        for item in arr.iter().filter_map(Value::as_str) {
            let normalized = if item == "tarjetas" { "personalizar" } else { item };
            if normalized != "personalizar" && CARD_KEYS.contains(&normalized) {
                hidden.insert(normalized.to_string());
            }
        }
    }

    let theme = v
        .get("theme")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let sidebar_name = v
        .get("sidebar_name")
        .or_else(|| v.get("name"))
        .or_else(|| v.get("title"))
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let check_updates = v
        .get("check_updates")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let mut account_budgets = std::collections::HashMap::new();
    if let Some(map) = v.get("account_budgets").and_then(Value::as_object) {
        for (k, val) in map {
            if let Some(num) = val.as_f64() {
                account_budgets.insert(k.clone(), num);
            }
        }
    }

    ConfigData {
        collapsed,
        hidden,
        theme,
        sidebar_name,
        check_updates,
        account_budgets,
    }
}

/// Guarda de forma atómica (`<file>.<pid>.tmp` + `rename`) las tarjetas colapsadas, ocultas, tema, nombre y updates.
pub fn save_config(
    path: &Path,
    collapsed: &[bool; 12],
    hidden: &HashSet<String>,
    theme: Option<&str>,
    sidebar_name: Option<&str>,
    check_updates: bool,
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

    let mut payload = if let Ok(existing_bytes) = std::fs::read(path)
        && let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(&existing_bytes)
    {
        Value::Object(map)
    } else {
        json!({})
    };

    payload["collapsed"] = json!(collapsed_names);
    payload["hidden"] = json!(hidden_names);
    payload["check_updates"] = json!(check_updates);
    if let Some(t) = theme.filter(|s| !s.trim().is_empty()) {
        payload["theme"] = json!(t);
    }
    if let Some(n) = sidebar_name.filter(|s| !s.trim().is_empty()) {
        payload["sidebar_name"] = json!(n);
    }
    let body = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    crate::paths::atomic_write_bytes(path, &body).map_err(|e| e.to_string())
}

/// Guarda de forma atómica el saldo prepago/presupuesto de una cuenta en `hudr2.json`,
/// asociándolo a la línea base de costo y sesión actual para evitar restar consumo histórico.
pub fn save_account_budget(
    path: &Path,
    prefix: &str,
    amount: Option<f64>,
    baseline_cost: f64,
    session_id: Option<String>,
) -> Result<(), String> {
    let mut payload = if let Ok(existing_bytes) = std::fs::read(path)
        && let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(&existing_bytes)
    {
        Value::Object(map)
    } else {
        json!({})
    };

    let mut budgets_map = payload
        .get("account_budgets")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    if let Some(amt) = amount {
        let entry = json!({
            "budget": amt,
            "baseline_cost": baseline_cost,
            "session_id": session_id,
        });
        budgets_map.insert(prefix.to_string(), entry);
    } else {
        budgets_map.remove(prefix);
    }

    payload["account_budgets"] = Value::Object(budgets_map);
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
    pub collapsed_cards: [bool; 12],
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
    pub theme: Option<String>,
    pub sidebar_name: Option<String>,
    pub is_editing_sidebar_name: bool,
    pub name_input_buffer: String,
    pub check_updates: bool,
    pub is_editing_balance: bool,
    pub editing_balance_prefix: Option<String>,
    pub balance_input_buffer: String,
    pub last_click_instant: Option<std::time::Instant>,
    pub last_click_row: Option<usize>,
}

impl AppState {
    pub fn new() -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());

        Self {
            snapshot: TelemetrySnapshot {
                project_dir: cwd,
                check_updates: true,
                ..Default::default()
            },
            // [estado, contexto, proyecto, integraciones, pools, git, engram, tools, mcp, ecosystem, personalizar]
            // proyecto (idx 2) y personalizar (idx 10) vienen colapsados por defecto
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
            theme: None,
            sidebar_name: None,
            is_editing_sidebar_name: false,
            name_input_buffer: String::new(),
            check_updates: true,
            is_editing_balance: false,
            editing_balance_prefix: None,
            balance_input_buffer: String::new(),
            last_click_instant: None,
            last_click_row: None,
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
            s.theme = cfg.theme.clone();
            s.sidebar_name = cfg.sidebar_name.clone();
            s.check_updates = cfg.check_updates;
            s.snapshot.theme_name = cfg.theme;
            s.snapshot.sidebar_name = cfg.sidebar_name;
            s.snapshot.check_updates = cfg.check_updates;
        }
        s.config_path = config_path;
        s
    }

    fn persist_config_if_configured(&self) {
        if let Some(ref path) = self.config_path {
            let _ = save_config(
                path,
                &self.collapsed_cards,
                &self.hidden_cards,
                self.theme.as_deref(),
                self.sidebar_name.as_deref(),
                self.check_updates,
            );
        }
    }

    pub fn toggle_check_updates(&mut self) -> bool {
        self.check_updates = !self.check_updates;
        self.snapshot.check_updates = self.check_updates;
        self.needs_redraw = true;
        self.persist_config_if_configured();
        self.check_updates
    }

    pub fn is_auto_theme(&self) -> bool {
        matches!(self.theme.as_deref(), None | Some("auto") | Some("herdr"))
    }

    pub fn cycle_theme(&mut self) -> String {
        let next_key = crate::theme::next_theme_in_cycle(self.theme.as_deref());
        self.theme = Some(next_key.to_string());
        self.snapshot.theme_name = Some(next_key.to_string());

        if next_key == "auto" {
            let resolved = crate::theme::resolve_adaptive_theme();
            crate::theme::set_active_theme(resolved);
            self.set_flash_notice(
                Some(format!("+ Tema: Auto (Herdr \u{00b7} {})", resolved.id)),
                2500,
            );
        } else if let Some(th) = crate::theme::find_theme(next_key) {
            crate::theme::set_active_theme(th);
            self.set_flash_notice(Some(format!("+ Tema: {}", th.name)), 2500);
        }

        self.needs_redraw = true;
        self.persist_config_if_configured();
        next_key.to_string()
    }

    pub fn set_sidebar_name(&mut self, name: Option<String>) {
        let clean = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        self.sidebar_name = clean.clone();
        self.snapshot.sidebar_name = clean;
        self.needs_redraw = true;
        self.persist_config_if_configured();
    }

    pub fn start_edit_sidebar_name(&mut self) {
        self.is_editing_sidebar_name = true;
        let current = self
            .sidebar_name
            .clone()
            .unwrap_or_else(|| "HUDR2".to_string());
        self.name_input_buffer = current;
        self.snapshot.is_editing_name = true;
        self.snapshot.name_input_buffer = self.name_input_buffer.clone();
        self.needs_redraw = true;
    }

    pub fn update_name_input_buffer(&mut self, new_buffer: String) {
        self.name_input_buffer = new_buffer;
        self.snapshot.name_input_buffer = self.name_input_buffer.clone();
        self.needs_redraw = true;
    }

    pub fn commit_sidebar_name(&mut self) {
        let trimmed = self.name_input_buffer.trim().to_string();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("HUDR2")
            || trimmed.eq_ignore_ascii_case("defecto")
        {
            self.set_sidebar_name(None);
            self.set_flash_notice(
                Some("Nombre restablecido a HUDR2 (defecto)".to_string()),
                3000,
            );
        } else {
            self.set_sidebar_name(Some(trimmed.clone()));
            self.set_flash_notice(
                Some(format!("Nombre actualizado: {trimmed}")),
                3000,
            );
        }
        self.is_editing_sidebar_name = false;
        self.name_input_buffer.clear();
        self.snapshot.is_editing_name = false;
        self.snapshot.name_input_buffer.clear();
        self.needs_redraw = true;
    }

    pub fn cancel_edit_sidebar_name(&mut self) {
        self.is_editing_sidebar_name = false;
        self.name_input_buffer.clear();
        self.snapshot.is_editing_name = false;
        self.snapshot.name_input_buffer.clear();
        self.set_flash_notice(Some("Edición de nombre cancelada".to_string()), 2000);
        self.needs_redraw = true;
    }

    pub fn start_edit_balance(&mut self, prefix: &str) {
        self.is_editing_balance = true;
        self.editing_balance_prefix = Some(prefix.to_string());
        let current_budget = crate::quotas::cliproxy::get_user_configured_budget(prefix);
        self.balance_input_buffer = current_budget
            .map(|b| format!("{b:.2}"))
            .unwrap_or_default();
        self.snapshot.is_editing_balance = true;
        self.snapshot.editing_balance_prefix = Some(prefix.to_string());
        self.snapshot.balance_input_buffer = self.balance_input_buffer.clone();
        self.needs_redraw = true;
    }

    pub fn update_balance_input_buffer(&mut self, new_buffer: String) {
        self.balance_input_buffer = new_buffer;
        self.snapshot.balance_input_buffer = self.balance_input_buffer.clone();
        self.needs_redraw = true;
    }

    pub fn commit_balance(&mut self) {
        if let Some(ref pfx) = self.editing_balance_prefix.clone() {
            let clean = self.balance_input_buffer.trim().trim_start_matches('$');
            let parsed = clean.parse::<f64>().ok().filter(|&v| v > 0.0);
            let cfg_path = crate::paths::resolve_hudr2_config_path();
            let current_cost = self.snapshot.agent_session.cost;
            let current_sid = self.snapshot.agent_session.session_id.clone();
            let _ = save_account_budget(&cfg_path, pfx, parsed, current_cost, current_sid);

            if let Some(val) = parsed {
                self.set_flash_notice(
                    Some(format!("Saldo para {pfx}: ${val:.2}")),
                    3000,
                );
            } else {
                self.set_flash_notice(
                    Some(format!("Saldo restablecido para {pfx}")),
                    3000,
                );
            }
        }
        self.is_editing_balance = false;
        self.editing_balance_prefix = None;
        self.balance_input_buffer.clear();
        self.snapshot.is_editing_balance = false;
        self.snapshot.editing_balance_prefix = None;
        self.snapshot.balance_input_buffer.clear();
        self.needs_redraw = true;
    }

    pub fn cancel_edit_balance(&mut self) {
        self.is_editing_balance = false;
        self.editing_balance_prefix = None;
        self.balance_input_buffer.clear();
        self.snapshot.is_editing_balance = false;
        self.snapshot.editing_balance_prefix = None;
        self.snapshot.balance_input_buffer.clear();
        self.set_flash_notice(Some("Edición de saldo cancelada".to_string()), 2000);
        self.needs_redraw = true;
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
        if card_key == "personalizar" || card_key == "tarjetas" {
            return; // Personalizar es fija
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
        if card_key == "personalizar" || card_key == "tarjetas" {
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
        let now = std::time::Instant::now();
        let is_double_click = if let (Some(last_time), Some(last_r)) =
            (self.last_click_instant, self.last_click_row)
        {
            last_r == clicked_row && now.duration_since(last_time).as_millis() <= 500
        } else {
            false
        };
        self.last_click_instant = Some(now);
        self.last_click_row = Some(clicked_row);

        // Doble clic en el encabezado de Personalizar o en la fila de Nombre inicia edición
        if is_double_click {
            for bound in &self.card_bounds {
                let is_header = (bound.id == "personalizar" || bound.id == "tarjetas")
                    && bound.action_type.is_none()
                    && absolute_line == bound.start_line;
                let is_name_row = (bound.id == "edit_sidebar_name"
                    || bound.action_type == Some("edit_sidebar_name"))
                    && absolute_line >= bound.start_line
                    && absolute_line <= bound.end_line;
                if is_header || is_name_row {
                    self.start_edit_sidebar_name();
                    return Some(ClickAction::StartEditSidebarName);
                }
            }
        }

        // 1. Prioridad: Acciones especiales exactas (abrir carpeta, diff, quick peek, visibilidad, tema o nombre)
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
            if action == "edit_sidebar_name" {
                self.start_edit_sidebar_name();
                return Some(ClickAction::StartEditSidebarName);
            } else if action == "open_folder"
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
            } else if action == "cycle_theme" {
                let next_th = self.cycle_theme();
                return Some(ClickAction::CycleTheme(next_th));
            } else if action == "toggle_check_updates" {
                let now_enabled = self.toggle_check_updates();
                let msg = if now_enabled {
                    "+ Updates: Chequeo online activado (3h)".to_string()
                } else {
                    "+ Updates: Chequeo online apagado".to_string()
                };
                self.set_flash_notice(Some(msg), 2500);
                return Some(ClickAction::ToggleCheckUpdates(now_enabled));
            } else if action == "update_tool"
                && let Some(p) = action_path
            {
                return Some(ClickAction::UpdateTool(p));
            } else if action == "edit_balance"
                && let Some(p) = action_path
            {
                self.start_edit_balance(&p);
                return Some(ClickAction::StartEditBalance(p));
            } else if action == "open_billing_url"
                && let Some(url) = action_path
            {
                return Some(ClickAction::OpenBillingUrl(url));
            }
        }

        // 2. Clic en tarjetas para colapsar / expandir
        // - Si la tarjeta está colapsada, hacer clic en cualquiera de sus filas la expande.
        // - Si la tarjeta está abierta, sólo hacer clic en su encabezado (start_line) la colapsa.
        //   Esto previene colisiones accidentales entre el borde inferior (end_line) de una
        //   tarjeta abierta (como Contexto) y el encabezado de la tarjeta siguiente (como Proyecto).
        for bound in &self.card_bounds {
            if bound.action_type.is_none()
                && let Some(idx) = CARD_KEYS.iter().position(|&k| {
                    k == bound.id || (k == "personalizar" && bound.id == "tarjetas")
                })
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
    CycleTheme(String),
    StartEditSidebarName,
    ToggleCheckUpdates(bool),
    UpdateTool(String),
    StartEditBalance(String),
    OpenBillingUrl(String),
}
