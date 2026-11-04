use crate::models::TelemetrySnapshot;
use crate::ui::CardBounds;

#[derive(Debug, Clone)]
pub struct AppState {
    pub snapshot: TelemetrySnapshot,
    pub collapsed_cards: [bool; 9],
    pub scroll_offset: usize,
    pub card_bounds: Vec<CardBounds>,
    pub flash_notice: Option<String>,
    pub flash_notice_until: Option<std::time::Instant>,
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
            // [estado, contexto, proyecto, integraciones, pools, git, engram, tools, mcp]
            // proyecto (idx 2) viene colapsado por defecto como en hudr original
            collapsed_cards: [false, false, true, false, false, false, false, false, false],
            scroll_offset: 0,
            card_bounds: Vec::new(),
            flash_notice: None,
            flash_notice_until: None,
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
    }

    pub fn check_flash_notice_expiry(&mut self) -> bool {
        if let Some(until) = self.flash_notice_until
            && std::time::Instant::now() >= until
        {
            self.flash_notice = None;
            self.flash_notice_until = None;
            return true;
        }
        false
    }

    pub fn toggle_card(&mut self, index: usize) {
        if index < self.collapsed_cards.len() {
            self.collapsed_cards[index] = !self.collapsed_cards[index];
        }
    }

    pub fn toggle_all_collapse(&mut self) {
        let any_open = self.collapsed_cards.iter().any(|&c| !c);
        self.collapsed_cards.fill(any_open);
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(2);
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset += 2;
    }

    pub fn handle_mouse_click(
        &mut self,
        clicked_row: usize,
        actual_scroll_offset: usize,
    ) -> Option<ClickAction> {
        let absolute_line = clicked_row + actual_scroll_offset;

        // 1. Prioridad: Acciones especiales exactas (abrir carpeta o sincronizar)
        for bound in &self.card_bounds {
            if let Some(ref action) = bound.action_type
                && absolute_line == bound.start_line
            {
                if action == "sync_integrations" {
                    return Some(ClickAction::SyncIntegrations);
                } else if action == "open_folder"
                    && let Some(ref p) = bound.action_path
                {
                    return Some(ClickAction::OpenFolder(p.clone()));
                }
            }
        }

        // 2. Clic en tarjetas para colapsar / expandir
        // - Si la tarjeta está colapsada, hacer clic en cualquiera de sus filas la expande.
        // - Si la tarjeta está abierta, sólo hacer clic en su encabezado (start_line) la colapsa.
        //   Esto previene colisiones accidentales entre el borde inferior (end_line) de una
        //   tarjeta abierta (como Contexto) y el encabezado de la tarjeta siguiente (como Proyecto).
        let card_keys = [
            "estado",
            "contexto",
            "proyecto",
            "integraciones",
            "pools",
            "git",
            "engram",
            "tools",
            "mcp",
        ];

        for bound in &self.card_bounds {
            if bound.action_type.is_none()
                && let Some(idx) = card_keys.iter().position(|&k| k == bound.id)
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

pub enum ClickAction {
    SyncIntegrations,
    OpenFolder(String),
    ToggledCard(usize),
}
