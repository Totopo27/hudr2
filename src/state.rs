use crate::models::TelemetrySnapshot;
use crate::ui::CardBounds;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub const CARD_KEYS: [&str; 13] = [
    "estado",
    "contexto",
    "proyecto",
    "integraciones",
    "orquestadores",
    "pools",
    "saldos",
    "git",
    "engram",
    "tools",
    "mcp",
    "ecosystem",
    "personalizar",
];

pub const DEFAULT_COLLAPSED: [bool; 13] = [false; 13];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveTab {
    #[default]
    Sesion,
    Codigo,
    Modelos,
    Stack,
    Ajustes,
}

impl ActiveTab {
    pub fn all() -> [ActiveTab; 5] {
        [
            ActiveTab::Sesion,
            ActiveTab::Codigo,
            ActiveTab::Modelos,
            ActiveTab::Stack,
            ActiveTab::Ajustes,
        ]
    }

    pub fn index(self) -> usize {
        match self {
            ActiveTab::Sesion => 0,
            ActiveTab::Codigo => 1,
            ActiveTab::Modelos => 2,
            ActiveTab::Stack => 3,
            ActiveTab::Ajustes => 4,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        match idx % 5 {
            0 => ActiveTab::Sesion,
            1 => ActiveTab::Codigo,
            2 => ActiveTab::Modelos,
            3 => ActiveTab::Stack,
            4 => ActiveTab::Ajustes,
            _ => unreachable!(),
        }
    }

    pub fn next(self) -> Self {
        Self::from_index(self.index() + 1)
    }

    pub fn prev(self) -> Self {
        Self::from_index((self.index() + 4) % 5)
    }

    pub fn label(self) -> &'static str {
        match self {
            ActiveTab::Sesion => "1: Sesión",
            ActiveTab::Codigo => "2: Código",
            ActiveTab::Modelos => "3: Modelos",
            ActiveTab::Stack => "4: Stack",
            ActiveTab::Ajustes => "☼ Ajustes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabBounds {
    pub tab: ActiveTab,
    pub line: usize,
    pub col_start: usize,
    pub col_end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigData {
    pub collapsed: [bool; 13],
    pub hidden: HashSet<String>,
    pub theme: Option<String>,
    pub sidebar_name: Option<String>,
    pub check_updates: bool,
    pub sound_mode: crate::sound::SoundMode,
    pub account_budgets: std::collections::HashMap<String, f64>,
    pub tabbed: bool,
}

/// Carga la configuración (tarjetas colapsadas y ocultas, tema, nombre, updates, modo de sonido y vista por pestañas) desde un archivo JSON.
pub fn load_config(path: &Path) -> ConfigData {
    let Ok(bytes) = std::fs::read(path) else {
        return ConfigData {
            collapsed: DEFAULT_COLLAPSED,
            hidden: HashSet::new(),
            theme: None,
            sidebar_name: None,
            check_updates: true,
            sound_mode: crate::sound::SoundMode::Bell,
            account_budgets: std::collections::HashMap::new(),
            tabbed: true,
        };
    };
    let Ok(v) = serde_json::from_slice::<Value>(&bytes) else {
        return ConfigData {
            collapsed: DEFAULT_COLLAPSED,
            hidden: HashSet::new(),
            theme: None,
            sidebar_name: None,
            check_updates: true,
            sound_mode: crate::sound::SoundMode::Bell,
            account_budgets: std::collections::HashMap::new(),
            tabbed: true,
        };
    };

    let mut collapsed = DEFAULT_COLLAPSED;
    if let Some(arr) = v.get("collapsed").and_then(Value::as_array) {
        collapsed = [false; 13];
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

    let sound_mode = v
        .get("sound_mode")
        .and_then(Value::as_str)
        .map(crate::sound::SoundMode::from_str_loose)
        .unwrap_or(crate::sound::SoundMode::Bell);

    let mut account_budgets = std::collections::HashMap::new();
    if let Some(map) = v.get("account_budgets").and_then(Value::as_object) {
        for (k, val) in map {
            if let Some(num) = val.as_f64() {
                account_budgets.insert(k.clone(), num);
            }
        }
    }

    let tabbed = v
        .get("tabbed")
        .or_else(|| v.get("tabs"))
        .and_then(Value::as_bool)
        .unwrap_or(true);

    ConfigData {
        collapsed,
        hidden,
        theme,
        sidebar_name,
        check_updates,
        sound_mode,
        account_budgets,
        tabbed,
    }
}

/// Guarda de forma atómica (`<file>.<pid>.tmp` + `rename`) las tarjetas colapsadas, ocultas, tema, nombre, updates, modo de sonido y vista por pestañas.
#[allow(clippy::too_many_arguments)]
pub fn save_config(
    path: &Path,
    collapsed: &[bool; 13],
    hidden: &HashSet<String>,
    theme: Option<&str>,
    sidebar_name: Option<&str>,
    check_updates: bool,
    sound_mode: crate::sound::SoundMode,
    tabbed: bool,
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
    payload["sound_mode"] = json!(sound_mode.as_str());
    payload["tabbed"] = json!(tabbed);
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

#[derive(Debug, Clone, PartialEq)]
pub struct OrchestratorTrackState {
    pub last_status: crate::models::OrchestratorStatus,
    pub was_critical: bool,
    pub working_since: Option<std::time::Instant>,
    pub finished_at: Option<std::time::Instant>,
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
    pub collapsed_cards: [bool; 13],
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
    pub sound_mode: crate::sound::SoundMode,
    pub is_editing_balance: bool,
    pub editing_balance_prefix: Option<String>,
    pub balance_input_buffer: String,
    pub last_click_instant: Option<std::time::Instant>,
    pub last_click_row: Option<usize>,
    pub orch_trackers: std::collections::HashMap<String, OrchestratorTrackState>,
    pub active_tab: ActiveTab,
    pub tab_bounds: Vec<TabBounds>,
    pub tabbed: bool,
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
                sound_mode: crate::sound::SoundMode::Bell,
                tabbed_view: true,
                ..Default::default()
            },
            // [estado, contexto, proyecto, integraciones, pools, git, engram, tools, mcp, ecosystem, personalizar]
            // Todas las cards expandidas por omisión
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
            sound_mode: crate::sound::SoundMode::Bell,
            is_editing_balance: false,
            editing_balance_prefix: None,
            balance_input_buffer: String::new(),
            last_click_instant: None,
            last_click_row: None,
            orch_trackers: std::collections::HashMap::new(),
            active_tab: ActiveTab::default(),
            tab_bounds: Vec::new(),
            tabbed: true,
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
            s.sound_mode = cfg.sound_mode;
            s.tabbed = cfg.tabbed;
            s.snapshot.theme_name = cfg.theme;
            s.snapshot.sidebar_name = cfg.sidebar_name;
            s.snapshot.check_updates = cfg.check_updates;
            s.snapshot.sound_mode = cfg.sound_mode;
            s.snapshot.tabbed_view = cfg.tabbed;
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
                self.sound_mode,
                self.tabbed,
            );
        }
    }

    pub fn toggle_tabbed(&mut self) -> bool {
        self.tabbed = !self.tabbed;
        self.snapshot.tabbed_view = self.tabbed;
        self.needs_redraw = true;
        self.persist_config_if_configured();
        self.tabbed
    }

    pub fn cycle_sound_mode(&mut self) -> crate::sound::SoundMode {
        self.sound_mode = self.sound_mode.next();
        self.snapshot.sound_mode = self.sound_mode;
        self.needs_redraw = true;
        self.persist_config_if_configured();
        self.sound_mode
    }

    pub fn update_orchestrator_alerts(&mut self) {
        let now = std::time::Instant::now();
        let sound_mode = self.sound_mode;

        for orch in &mut self.snapshot.orchestrators {
            let tracker = self
                .orch_trackers
                .entry(orch.name.clone())
                .or_insert_with(|| OrchestratorTrackState {
                    last_status: orch.status,
                    was_critical: orch.is_critical,
                    working_since: if orch.status == crate::models::OrchestratorStatus::Working {
                        Some(now)
                    } else {
                        None
                    },
                    finished_at: None,
                });

            // 1. Transición Working -> Idle (Fin de tarea global de la flota)
            if tracker.last_status == crate::models::OrchestratorStatus::Working
                && orch.status == crate::models::OrchestratorStatus::Idle
            {
                tracker.finished_at = Some(now);
                let duration = tracker
                    .working_since
                    .map(|s| now.saturating_duration_since(s))
                    .unwrap_or_default();
                // Evita sonar en microtareas insignificantes (< 2 segundos)
                if duration >= std::time::Duration::from_secs(2) {
                    crate::sound::emit_alert(sound_mode, crate::sound::AlertKind::FleetCompleted);
                }
            }

            // 2. Transición a Working
            if orch.status == crate::models::OrchestratorStatus::Working {
                if tracker.last_status != crate::models::OrchestratorStatus::Working {
                    tracker.working_since = Some(now);
                }
                // Alerta sonora preventiva de cuota crítica durante ejecución activa
                if orch.is_critical && !tracker.was_critical {
                    crate::sound::emit_alert(sound_mode, crate::sound::AlertKind::QuotaCritical);
                }
            }

            // 3. Refuerzo visual temporal: marcar recently_finished si terminó hace < 4 segundos
            if orch.status == crate::models::OrchestratorStatus::Idle
                && let Some(f_time) = tracker.finished_at
                && now.saturating_duration_since(f_time) < std::time::Duration::from_secs(4)
            {
                orch.recently_finished = true;
            }

            tracker.last_status = orch.status;
            tracker.was_critical = orch.is_critical;
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

    pub fn switch_tab(&mut self, tab: ActiveTab) -> bool {
        if self.active_tab != tab {
            self.active_tab = tab;
            self.needs_redraw = true;
            true
        } else {
            false
        }
    }

    pub fn next_tab(&mut self) -> ActiveTab {
        let next = self.active_tab.next();
        self.switch_tab(next);
        next
    }

    pub fn prev_tab(&mut self) -> ActiveTab {
        let prev = self.active_tab.prev();
        self.switch_tab(prev);
        prev
    }

    pub fn handle_tab_click(
        &mut self,
        clicked_col: usize,
        clicked_row: usize,
        scroll_offset: usize,
    ) -> Option<ActiveTab> {
        let visual_row = clicked_row.saturating_sub(scroll_offset);
        let target_tab = self
            .tab_bounds
            .iter()
            .find(|tb| {
                tb.line == visual_row && clicked_col >= tb.col_start && clicked_col <= tb.col_end
            })
            .map(|tb| tb.tab);

        if let Some(tab) = target_tab {
            self.switch_tab(tab);
            Some(tab)
        } else {
            None
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

    pub fn is_card_collapsed(&self, card_key: &str) -> bool {
        CARD_KEYS
            .iter()
            .position(|&k| k == card_key)
            .map(|idx| self.collapsed_cards[idx])
            .unwrap_or(false)
    }

    pub fn set_card_collapsed(&mut self, card_key: &str, collapsed: bool) {
        if let Some(idx) = CARD_KEYS.iter().position(|&k| k == card_key) {
            self.collapsed_cards[idx] = collapsed;
            self.needs_redraw = true;
            self.persist_config_if_configured();
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
            } else if action == "cycle_sound_mode" {
                let next_sm = self.cycle_sound_mode();
                let (badge, _) = next_sm.display_badge();
                self.set_flash_notice(Some(format!("+ Alertas sonoras: {badge}")), 2500);
                return Some(ClickAction::CycleSoundMode(next_sm));
            } else if action == "toggle_tabbed_view" {
                let now_tabbed = self.toggle_tabbed();
                let msg = if now_tabbed {
                    "+ Vista: Navegación por pestañas activada".to_string()
                } else {
                    "+ Vista: Modo vertical en cascada activado".to_string()
                };
                self.set_flash_notice(Some(msg), 2500);
                return Some(ClickAction::ToggleTabbedView(now_tabbed));
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
    CycleSoundMode(crate::sound::SoundMode),
    ToggleTabbedView(bool),
}
