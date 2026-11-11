use crate::config::{
    ACCENT_PINK, ACCENT_PRIMARY, AMBER, BOLD, CORAL, CYAN, GOLD, MAGENTA, MINT, RESET, TEXT_DIM,
    TEXT_MUTED, TEXT_PRIMARY,
};
use crate::models::TelemetrySnapshot;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct CardBounds {
    pub id: &'static str,
    pub start_line: usize,
    pub end_line: usize,
    pub action_type: Option<&'static str>,
    pub action_path: Option<String>,
}

pub struct RenderOutput {
    pub lines: Vec<String>,
    pub bounds: Vec<CardBounds>,
}

/// Devuelve la longitud en bytes de la secuencia de escape ANSI que empieza en `s`
/// (que debe comenzar con `\x1b`). Soporta:
/// - CSI: `ESC [ <params> <final 0x40..=0x7E>` (colores, cursor, borrado...)
/// - OSC: `ESC ] ... (BEL | ESC \)` (títulos, hipervínculos)
/// - Escapes de 2 bytes: `ESC <char>`
///
/// Si la secuencia está incompleta, consume hasta el final de la cadena.
fn ansi_escape_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    debug_assert!(bytes.first() == Some(&0x1b));
    match bytes.get(1) {
        None => 1,
        Some(b'[') => {
            // CSI: parámetros/intermedios en 0x20..=0x3F, byte final en 0x40..=0x7E
            for (i, &b) in bytes.iter().enumerate().skip(2) {
                if (0x40..=0x7E).contains(&b) {
                    return i + 1;
                }
            }
            bytes.len()
        }
        Some(b']') => {
            // OSC: termina en BEL (0x07) o ST (ESC \)
            let mut i = 2;
            while i < bytes.len() {
                match bytes[i] {
                    0x07 => return i + 1,
                    0x1b if bytes.get(i + 1) == Some(&b'\\') => return i + 2,
                    _ => i += 1,
                }
            }
            bytes.len()
        }
        Some(_) => {
            // Escape de 2 caracteres: avanzar el char completo (puede ser multibyte)
            1 + s[1..].chars().next().map_or(0, char::len_utf8)
        }
    }
}

pub fn strip_ansi(s: &str) -> String {
    // Ruta rápida: sin secuencias de escape no hay nada que limpiar
    if !s.contains('\x1b') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('\x1b') {
        out.push_str(&rest[..pos]);
        let esc_len = ansi_escape_len(&rest[pos..]);
        rest = &rest[pos + esc_len..];
    }
    out.push_str(rest);
    out
}

pub fn string_width(s: &str) -> usize {
    // Evita asignar memoria cuando la cadena no contiene escapes ANSI
    if !s.contains('\x1b') {
        return UnicodeWidthStr::width(s);
    }
    let clean = strip_ansi(s);
    UnicodeWidthStr::width(clean.as_str())
}

pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut current_width = 0;
    let mut out = String::with_capacity(s.len() + RESET.len());
    let mut rest = s;

    'outer: while !rest.is_empty() {
        // Copiar texto visible hasta el próximo escape (o el final)
        let next_esc = rest.find('\x1b').unwrap_or(rest.len());
        for c in rest[..next_esc].chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            if current_width + cw > max_width {
                break 'outer;
            }
            current_width += cw;
            out.push(c);
        }
        rest = &rest[next_esc..];
        if rest.is_empty() {
            break;
        }
        // Preservar la secuencia de escape completa (no ocupa ancho visible)
        let esc_len = ansi_escape_len(rest);
        out.push_str(&rest[..esc_len]);
        rest = &rest[esc_len..];
    }
    out.push_str(RESET);
    out
}

fn truncate_plain(s: &str, max_width: usize) -> String {
    if string_width(s) <= max_width {
        return s.to_string();
    }
    if max_width <= 1 {
        return "\u{2026}".to_string();
    }
    let limit = max_width - 1;
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > limit {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('\u{2026}');
    out
}

pub fn draw_top(title: &str, collapsed: bool, width: usize) -> String {
    draw_top_theme(title, collapsed, width, crate::theme::active_theme())
}

pub fn draw_top_theme(
    title: &str,
    collapsed: bool,
    width: usize,
    theme: &crate::theme::Theme,
) -> String {
    let icon = if collapsed { "+" } else { "-" };
    let icon_badge = format!(
        "{}[{RESET}{}{icon}{RESET}{}]{RESET}",
        theme.text_dim, theme.text_muted, theme.text_dim
    );
    let clean_title = if string_width(title) > width.saturating_sub(12) {
        truncate_to_width(title, width.saturating_sub(12))
    } else {
        title.to_string()
    };
    let t_len = string_width(&clean_title);
    let dash_count = width.saturating_sub(10 + t_len);
    format!(
        "{}\u{256d}\u{2500} {clean_title} {}\u{2500}{icon_badge} {}\u{256e}{RESET}",
        theme.border,
        "\u{2500}".repeat(dash_count),
        theme.border
    )
}

pub fn draw_bottom(width: usize) -> String {
    draw_bottom_theme(width, crate::theme::active_theme())
}

pub fn draw_bottom_theme(width: usize, theme: &crate::theme::Theme) -> String {
    format!(
        "{}\u{2570}{}{}\u{256f}{RESET}",
        theme.border,
        "\u{2500}".repeat(width.saturating_sub(2)),
        theme.border
    )
}

pub fn draw_divider(width: usize) -> String {
    draw_divider_theme(width, crate::theme::active_theme())
}

pub fn draw_divider_theme(width: usize, theme: &crate::theme::Theme) -> String {
    format!(
        "{}\u{251c}{}{}\u{2524}{RESET}",
        theme.border_muted,
        "\u{2500}".repeat(width.saturating_sub(2)),
        theme.border_muted
    )
}

pub fn draw_row(left: &str, right: &str, width: usize) -> String {
    draw_row_theme(left, right, width, crate::theme::active_theme())
}

pub fn draw_row_theme(
    left: &str,
    right: &str,
    width: usize,
    theme: &crate::theme::Theme,
) -> String {
    let target_inner = width.saturating_sub(4);
    let mut clean_left = left.to_string();
    let mut clean_right = right.to_string();

    let mut l_width = string_width(&clean_left);
    let mut r_width = string_width(&clean_right);

    if l_width + r_width + if r_width > 0 { 1 } else { 0 } > target_inner {
        if l_width < target_inner {
            let avail_r = target_inner.saturating_sub(l_width + 1);
            clean_right = truncate_to_width(&clean_right, avail_r);
            r_width = string_width(&clean_right);
        } else {
            clean_left = truncate_to_width(&clean_left, target_inner);
            l_width = string_width(&clean_left);
            clean_right.clear();
            r_width = 0;
        }
    }

    let space_count = target_inner.saturating_sub(l_width + r_width);
    let pad = " ".repeat(space_count);
    format!(
        "{}\u{2502}{RESET} {clean_left}{pad}{clean_right} {}\u{2502}{RESET}",
        theme.border, theme.border
    )
}

pub fn render_gauge_inline(percent: u8, available_cells: usize, color: &str) -> String {
    let p = percent.clamp(0, 100) as usize;
    let mut filled = (p * available_cells) / 100;
    if p > 0 && p < 100 {
        filled = filled.max(1).min(available_cells.saturating_sub(1));
    } else if p == 0 {
        filled = 0;
    } else {
        filled = available_cells;
    }
    let empty = available_cells.saturating_sub(filled);
    format!(
        "{color}{}{RESET}{TEXT_DIM}{}{RESET}",
        "\u{2501}".repeat(filled),
        "\u{2500}".repeat(empty)
    )
}

pub fn format_tokens(num: i64) -> String {
    if num <= 0 {
        return "0.0k".to_string();
    }
    if num >= 1_000_000_000 {
        format!("{:.1}B", num as f64 / 1_000_000_000.0)
    } else if num >= 1_000_000 {
        format!("{:.1}M", num as f64 / 1_000_000.0)
    } else {
        format!("{:.1}k", num as f64 / 1_000.0)
    }
}

pub fn compact_model_label(model: &str) -> String {
    let clean = crate::opencode::normalize_model_str(model);
    let base = clean.rsplit('/').next().unwrap_or(&clean).to_lowercase();

    if base.contains("sonnet") {
        "sonnet-5-5".to_string()
    } else if base.contains("opus") {
        "opus-5-5".to_string()
    } else if base.contains("haiku") {
        "haiku-5-5".to_string()
    } else if base.contains("5.6-sol") || base.contains("sol") {
        "5.6-sol".to_string()
    } else if base.contains("5.6-terra") || base.contains("terra") {
        "5.6-terra".to_string()
    } else if base.contains("5.6-luna") || base.contains("luna") {
        "5.6-luna".to_string()
    } else if base.contains("codex-auto-review") || base.contains("codex-review") || (base.contains("codex") && base.contains("review")) {
        "codex-review".to_string()
    } else if base.contains("gemini-3.8-flash") || base.contains("3.8-flash") || (base.contains("3.8") && base.contains("flash")) || base.contains("flash") {
        "3.8-flash".to_string()
    } else if base.contains("gemini-3.8-pro") || base.contains("3.8-pro") || (base.contains("3.8") && base.contains("pro")) {
        "3.8-pro".to_string()
    } else {
        base.trim_start_matches("gpt-")
            .trim_start_matches("claude-")
            .trim_start_matches("gemini-")
            .to_string()
    }
}

pub fn clean_subagent_role(role: &str) -> String {
    let s = role.to_lowercase();
    // Roles de suite SDD: "sdd-explore-agy3" -> "explore", "sdd-tasks-agy" -> "tasks"
    if let Some(rest) = s.strip_prefix("sdd-") {
        if let Some((phase, _pfx)) = rest.split_once('-') {
            return phase.to_string();
        }
        if let Some((phase, _pfx)) = rest.split_once('_') {
            return phase.to_string();
        }
        return rest.to_string();
    }

    let s = s.trim_start_matches("gentle-ai-");
    let s = s.trim_start_matches("gentle-");
    let s = s.trim_start_matches("agent-");
    let s = s.trim_end_matches("-agent");
    if s.starts_with("jd-judge-") {
        s.replace("jd-judge-", "judge-")
    } else if s == "jd-fix" {
        "fix".to_string()
    } else if s == "review-readability" {
        "review-read".to_string()
    } else if s == "review-reliability" {
        "review-rel".to_string()
    } else if s == "review-resilience" {
        "review-res".to_string()
    } else if s == "deep-researcher" {
        "deep-rsch".to_string()
    } else if s == "news-researcher" {
        "news-rsch".to_string()
    } else if s == "ui-designer" {
        "ui-design".to_string()
    } else {
        s.to_string()
    }
}

pub fn render_screen(
    snap: &TelemetrySnapshot,
    collapsed: &[bool; 12],
    flash_notice: Option<&str>,
    width: usize,
) -> RenderOutput {
    let empty = std::collections::HashSet::new();
    render_screen_with_diff(snap, collapsed, flash_notice, None, &[], &empty, width)
}

pub fn render_screen_with_diff(
    snap: &TelemetrySnapshot,
    collapsed: &[bool; 12],
    flash_notice: Option<&str>,
    selected_diff_file: Option<&str>,
    selected_diff_lines: &[String],
    hidden_cards: &std::collections::HashSet<String>,
    width: usize,
) -> RenderOutput {
    render_screen_with_diff_and_theme(
        snap,
        collapsed,
        flash_notice,
        selected_diff_file,
        selected_diff_lines,
        hidden_cards,
        width,
        crate::theme::active_theme(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_screen_with_diff_and_theme(
    snap: &TelemetrySnapshot,
    collapsed: &[bool; 12],
    flash_notice: Option<&str>,
    selected_diff_file: Option<&str>,
    selected_diff_lines: &[String],
    hidden_cards: &std::collections::HashSet<String>,
    width: usize,
    theme: &crate::theme::Theme,
) -> RenderOutput {
    let mut lines = Vec::new();
    let mut bounds = Vec::new();

    let title_color = theme.title_color;
    let label_color = theme.label_color;
    let accent_primary = theme.accent_primary;
    let accent_secondary = theme.accent_secondary;
    let mint = theme.mint;
    let amber = theme.amber;
    let text_primary = theme.text_primary;
    let text_muted = theme.text_muted;
    let text_dim = theme.text_dim;
    let cyan = theme.cyan;

    let draw_top = |title: &str, col: bool, w: usize| draw_top_theme(title, col, w, theme);
    let draw_bottom = |w: usize| draw_bottom_theme(w, theme);
    let draw_divider = |w: usize| draw_divider_theme(w, theme);
    let draw_row = |l: &str, r: &str, w: usize| draw_row_theme(l, r, w, theme);

    let folder = std::path::Path::new(&snap.project_dir)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("proyecto");

    let active_model = snap.agent_session.effective_model();

    let target_healthy = crate::cleanup::get_best_healthy_target_prefix_for_model(
        &snap.accounts,
        &snap.agent_session.all_active_prefixes,
        active_model,
    );

    // 1. Header Linear (Línea 0 fija siempre)
    let raw_name = snap
        .sidebar_name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("HUDR2");
    let banner_text = format!(":: {raw_name} ::");
    let banner_w = string_width(&banner_text);
    let banner_pad = width.saturating_sub(banner_w) / 2;
    let safe_banner = if banner_w > width {
        truncate_to_width(&banner_text, width)
    } else {
        banner_text
    };
    lines.push(format!("{}{accent_primary}{safe_banner}{RESET}", " ".repeat(banner_pad)));

    // 2. Estado Card (0)
    if !hidden_cards.contains("estado") {
        let estado_start = lines.len();
        let is_working = snap.agent_session.agent_status == "working";
        let petal = if is_working {
            format!("{BOLD}{AMBER}>> WORKING{RESET}")
        } else {
            format!("{TEXT_DIM}-- idle{RESET}")
        };
        let estado_title = format!("{BOLD}{title_color}Estado{RESET} {TEXT_DIM}\u{00b7}{RESET} {petal}");
        lines.push(draw_top(&estado_title, collapsed[0], width));
        if !collapsed[0] {
            let short_proj = truncate_plain(folder, 18);
            lines.push(draw_row(
                &format!("{label_color}Proyecto{RESET}"),
                &format!("{TEXT_PRIMARY}~/.../{short_proj}{RESET}"),
                width,
            ));
            let dirty = if snap.git.is_clean {
                format!("{MINT}limpio{RESET}")
            } else {
                format!(
                    "{AMBER}\u{00b1}{}{RESET}",
                    snap.git.modified + snap.git.staged
                )
            };
            lines.push(draw_row(
                &format!("{label_color}Rama{RESET}"),
                &format!("{CYAN}# {}{RESET} {dirty}", snap.git.branch),
                width,
            ));
            let model_disp = match snap
                .agent_session
                .thinking_level
                .as_deref()
                .filter(|t| !t.is_empty())
            {
                Some(lvl) => format!(
                    "{accent_primary}{}{RESET} {TEXT_MUTED}[{lvl}]{RESET}",
                    snap.agent_session.model_short
                ),
                None => format!("{accent_primary}{}{RESET}", snap.agent_session.model_short),
            };
            lines.push(draw_row(&format!("{label_color}Modelo{RESET}"), &model_disp, width));

            if !snap.agent_session.todos.is_empty() {
                let mut done = 0usize;
                let mut in_prog = 0usize;
                let mut pending = 0usize;
                let mut active_title: Option<&str> = None;
                for td in &snap.agent_session.todos {
                    match td.status.as_str() {
                        "done" | "completed" => done += 1,
                        "in_progress" => {
                            in_prog += 1;
                            if active_title.is_none() && !td.title.is_empty() {
                                active_title = Some(&td.title);
                            }
                        }
                        _ => {
                            pending += 1;
                        }
                    }
                }
                let counts = format!(
                    "{MINT}\u{2713}{done}{RESET} {AMBER}\u{25c9}{in_prog}{RESET} {TEXT_DIM}\u{25cb}{pending}{RESET}"
                );
                let right_todo = if let Some(t) = active_title {
                    let short_t = truncate_plain(t, 14);
                    format!("{counts} {TEXT_MUTED}{short_t}{RESET}")
                } else {
                    counts
                };
                lines.push(draw_row(&format!("{label_color}Tareas{RESET}"), &right_todo, width));
            }

            let active_prof_labels = if let Some(ref lp) = snap.agent_session.local_prefix {
                format!("{accent_secondary}*{RESET} {MINT}{lp}{RESET} {accent_secondary}*{RESET}")
            } else if snap.agent_session.all_active_prefixes.is_empty() {
                format!("{TEXT_MUTED}default{RESET}")
            } else {
                snap.agent_session
                    .all_active_prefixes
                    .iter()
                    .map(|p| format!("{accent_secondary}*{RESET} {MINT}{p}{RESET} {accent_secondary}*{RESET}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let sub_vitals = format!(
                "{accent_primary}mem:{RESET} {TEXT_PRIMARY}{folder}{RESET} {TEXT_DIM}\u{00b7}{RESET} {TEXT_MUTED}{} MCPs{RESET} {TEXT_DIM}\u{00b7}{RESET} {active_prof_labels}",
                snap.mcps.len()
            );
            lines.push(draw_row(
                &sub_vitals,
                &format!("{accent_secondary}>{RESET}"),
                width,
            ));
        }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "estado",
            start_line: estado_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 3. Contexto Card (1)
    if !hidden_cards.contains("contexto") {
        let ctx_start = lines.len();
        lines.push(draw_top(
            &format!("{BOLD}{title_color}Contexto{RESET}"),
            collapsed[1],
            width,
        ));
        if !collapsed[1] {
            let cur_tokens = snap.agent_session.active_turn_tokens;
            let limit = snap.agent_session.context_limit.max(1);
            let pct = (cur_tokens as f64 / limit as f64) * 100.0;
            let (gauge_color, right_st) = if pct >= 80.0 {
                (
                    CORAL,
                    format!("{CORAL}\u{25cf} Saturado {:.1}% [/snew]{RESET}", pct),
                )
            } else if pct >= 50.0 {
                (
                    AMBER,
                    format!("{AMBER}\u{25cf} Maduro {:.1}% [/snew]{RESET}", pct),
                )
            } else {
                (MINT, format!("{MINT}\u{25cf} Optimo {:.1}%{RESET}", pct))
            };
            let left_tok = format!(
                "{BOLD}{gauge_color}{}{RESET} {TEXT_DIM}/{RESET} {TEXT_MUTED}{}{RESET}",
                format_tokens(cur_tokens),
                format_tokens(limit)
            );
            lines.push(draw_row(&left_tok, &right_st, width));

            let inner_w = width.saturating_sub(4);
            lines.push(draw_row(
                &render_gauge_inline(pct.round().clamp(0.0, 100.0) as u8, inner_w, gauge_color),
                "",
                width,
            ));

            let in_t = snap.agent_session.tokens_input;
            let out_t = snap.agent_session.tokens_output;
            let left_io = format!(
                "{TEXT_DIM}^{RESET} {TEXT_PRIMARY}{}{RESET} {TEXT_DIM}\u{00b7}{RESET} {TEXT_DIM}v{RESET} {TEXT_PRIMARY}{}{RESET}",
                format_tokens(in_t),
                format_tokens(out_t)
            );
            let right_cost = format!(
                "{label_color}Costo{RESET} {TEXT_PRIMARY}${:.3}{RESET}",
                snap.agent_session.cost
            );
            lines.push(draw_row(&left_io, &right_cost, width));

            // Fila de telemetría por petición/tarea: turnos, promedio por petición y costo del último turno
            let turns = snap.agent_session.turn_count;
            let avg_cost = snap.agent_session.avg_turn_cost();
            let last_cost = snap.agent_session.last_turn_cost;

            if turns > 0 || last_cost > 0.0 || avg_cost > 0.0 {
                let left_turns = if turns > 0 {
                    let label = if turns == 1 { "turno" } else { "turnos" };
                    if width < 38 {
                        format!("{TEXT_DIM}{turns}t \u{00b7} ${:.3}/r{RESET}", avg_cost)
                    } else if width < 46 {
                        format!("{TEXT_DIM}{turns} {label} \u{00b7} ${:.3}/req{RESET}", avg_cost)
                    } else {
                        format!("{TEXT_DIM}{turns} {label} \u{00b7} prom: ${:.3}/req{RESET}", avg_cost)
                    }
                } else {
                    format!("{TEXT_DIM}prom: ${:.3}/req{RESET}", avg_cost)
                };

                let right_last = if last_cost > 0.0 {
                    if width < 46 {
                        format!("{TEXT_DIM}\u{00fa}lt: ${:.3}{RESET}", last_cost)
                    } else {
                        format!("{TEXT_DIM}\u{00fa}ltimo: ${:.3}{RESET}", last_cost)
                    }
                } else {
                    String::new()
                };

                lines.push(draw_row(&left_turns, &right_last, width));
            }

            if pct >= 50.0 {
                lines.push(draw_row(
                    &format!(
                        "{gauge_color}[aviso] Sesion madura: usa /snew para handover limpio{RESET}"
                    ),
                    "",
                    width,
                ));
            }
        }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "contexto",
            start_line: ctx_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 4. Proyecto Card (Árbol 2 niveles) (2)
    if !hidden_cards.contains("proyecto") {
        let proj_start = lines.len();
        let stats_badge = format!(
            "{TEXT_DIM}{}d \u{00b7} {}f{RESET}",
            snap.tree.total_dirs, snap.tree.total_files
        );
        let proj_title = format!(
            "{accent_primary}~{RESET} {BOLD}{title_color}Proyecto{RESET} {TEXT_DIM}\u{00b7}{RESET} {CYAN}{folder}{RESET} {stats_badge}"
        );
        lines.push(draw_top(&proj_title, collapsed[2], width));
        if !collapsed[2] {
            let open_line_idx = lines.len();
            let raw_proj_path = if snap.tree.root_path.is_empty() {
                &snap.project_dir
            } else {
                &snap.tree.root_path
            };
            let normalized_path = raw_proj_path.replace('\\', "/");
            let btn_label = format!("{label_color}Ruta{RESET} {MINT}[abrir en PC ->]{RESET}");
            let btn_w = string_width(&btn_label);
            let target_inner = width.saturating_sub(4);
            let avail_right = target_inner.saturating_sub(btn_w + 1);

            let disp_path = if string_width(&normalized_path) <= avail_right {
                normalized_path
            } else {
                let short = format!(".../{folder}");
                if string_width(&short) <= avail_right {
                    short
                } else {
                    truncate_to_width(&short, avail_right)
                }
            };
            lines.push(draw_row(
                &btn_label,
                &format!("{TEXT_PRIMARY}{disp_path}{RESET}"),
                width,
            ));
            bounds.push(CardBounds {
                id: "open_folder",
                start_line: open_line_idx,
                end_line: open_line_idx,
                action_type: Some("open_folder"),
                action_path: Some(raw_proj_path.clone()),
            });

            let max_items = 6;
            let total_items = snap.tree.tree.len();
            for item in snap.tree.tree.iter().take(max_items) {
                if item.is_dir {
                    lines.push(draw_row(
                        &format!(
                            "{CYAN}> /{RESET} {TEXT_PRIMARY}{}/{RESET} {TEXT_DIM}({}){RESET}",
                            item.name,
                            item.children.len()
                        ),
                        "",
                        width,
                    ));
                    for sub in item.children.iter().take(2) {
                        let sub_icon = if sub.is_dir {
                            format!("{CYAN}\u{2514} /{RESET}")
                        } else {
                            format!("{TEXT_DIM}\u{2514} {RESET}")
                        };
                        lines.push(draw_row(
                            &format!("   {sub_icon} {TEXT_MUTED}{}{RESET}", sub.name),
                            "",
                            width,
                        ));
                    }
                } else {
                    lines.push(draw_row(
                        &format!("{TEXT_DIM}\u{2022} {RESET}{TEXT_MUTED}{}{RESET}", item.name),
                        "",
                        width,
                    ));
                }
            }
            if total_items > max_items {
                let remaining = total_items - max_items;
                lines.push(draw_row(
                    &format!("{TEXT_DIM}\u{2026} y {remaining} elementos m\u{00e1}s{RESET}"),
                    "",
                    width,
                ));
            }
        }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "proyecto",
            start_line: proj_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 5. Integraciones Card (3)
    if !hidden_cards.contains("integraciones") {
        let int_start = lines.len();
        let count_str = format!("{TEXT_DIM}{} cuentas{RESET}", snap.accounts.len());
    let critical_count = snap
        .accounts
        .iter()
        .filter(|a| {
            crate::quotas::evaluate_account_for_model(a, active_model).is_critical
        })
        .count();
    let int_title = if critical_count > 0 {
        format!(
            "{title_color}Integraciones{RESET} {TEXT_DIM}\u{00b7}{RESET} {BOLD}{CORAL}! {critical_count} cr\u{00ed}tic{RESET}"
        )
    } else {
        format!("{title_color}Integraciones{RESET} {TEXT_DIM}\u{00b7}{RESET} {count_str}")
    };
    lines.push(draw_top(&int_title, collapsed[3], width));
    if !collapsed[3] {
        if snap.accounts.is_empty() {
            lines.push(draw_row(
                &format!("{TEXT_DIM}Sin proveedor de cuotas activo{RESET}"),
                &format!("{TEXT_DIM}[inactivo]{RESET}"),
                width,
            ));
        } else {
            for acc in &snap.accounts {
                let eff =
                    crate::quotas::evaluate_account_for_model(acc, active_model);

                let is_api = crate::quotas::is_api_key(acc);
                let right_normal = if is_api {
                    format!("{TEXT_DIM}[{}]{RESET}", eff.reset_5h)
                } else {
                    format!("{TEXT_DIM}en {}{RESET}", eff.reset_5h)
                };

                let (status_glyph, pct_color, right_text) = if acc.has_error {
                    let err_display = if acc.error_msg.to_lowercase().contains("expirado") {
                        if width < 34 {
                            "exp".to_string()
                        } else {
                            "expirado".to_string()
                        }
                    } else if width < 34 {
                        truncate_to_width(&acc.error_msg, 6)
                    } else if acc.error_msg.len() > 10 {
                        truncate_to_width(&acc.error_msg, 10)
                    } else {
                        acc.error_msg.clone()
                    };
                    (
                        format!("{AMBER}?{RESET}"),
                        AMBER,
                        format!("{AMBER}[{err_display}]{RESET}"),
                    )
                } else if eff.is_critical {
                    (
                        format!("{BOLD}{CORAL}!{RESET}"),
                        CORAL,
                        if is_api {
                            format!("{BOLD}{CORAL}[SALDO AGOTADO]{RESET}")
                        } else {
                            format!("{BOLD}{CORAL}[AGOT\u{00c1}NDOSE]{RESET}")
                        },
                    )
                } else if eff.is_warning {
                    (
                        format!("{BOLD}{AMBER}!{RESET}"),
                        AMBER,
                        if is_api {
                            format!("{AMBER}[AVISO SALDO]{RESET}")
                        } else {
                            format!("{AMBER}[AVISO 70%]{RESET}")
                        },
                    )
                } else {
                    (
                        format!("{MINT}\u{25cf}{RESET}"),
                        MINT,
                        right_normal,
                    )
                };

                let pfx_w = if width < 38 {
                    4
                } else if width < 42 {
                    5
                } else {
                    7
                };
                let pfx_clean = format!("{:<width$}", acc.prefix, width = pfx_w);
                let pfx_disp = format!("{TEXT_PRIMARY}{pfx_clean}{RESET}");
                let left = if is_api {
                    let cost_to_apply = if snap
                        .agent_session
                        .local_prefix
                        .as_deref()
                        .map(|p| p.eq_ignore_ascii_case(&acc.prefix))
                        .unwrap_or(false)
                    {
                        snap.agent_session.cost
                    } else {
                        0.0
                    };
                    let (_budget, rem_balance, _, _) =
                        crate::quotas::cliproxy::calculate_api_key_balance(
                            &acc.prefix,
                            cost_to_apply,
                            snap.agent_session.session_id.as_deref(),
                        );

                    match rem_balance {
                        Some(rem) => {
                            let bar_len = if width < 38 { 2 } else { 3 };
                            let bar = render_gauge_inline(eff.rem_5h, bar_len, pct_color);
                            if width < 38 {
                                format!("{status_glyph} {pfx_disp} {bar} ${rem:.2}")
                            } else {
                                format!("{status_glyph} {pfx_disp} {bar} ${rem:.2} disp")
                            }
                        }
                        None => {
                            if width < 38 {
                                format!("{status_glyph} {pfx_disp} ${cost_to_apply:.2}")
                            } else {
                                format!("{status_glyph} {pfx_disp} ${cost_to_apply:.2} sesi\u{00f3}n")
                            }
                        }
                    }
                } else {
                    let bar_len = if width < 38 { 2 } else { 3 };
                    let bar = render_gauge_inline(eff.rem_5h, bar_len, pct_color);
                    let pct_5h = format!("{BOLD}{pct_color}{:>3}%{RESET}", eff.rem_5h);
                    let wk_color = if eff.is_critical {
                        CORAL
                    } else if eff.is_warning {
                        AMBER
                    } else {
                        MINT
                    };
                    let wk_str = if width < 36 {
                        String::new()
                    } else {
                        let wk_label = if width < 40 { "W:" } else { "Wk:" };
                        format!(
                            " {TEXT_DIM}{wk_label}{RESET}{BOLD}{wk_color}{:>3}%{RESET}",
                            eff.rem_wk
                        )
                    };
                    format!("{status_glyph} {pfx_disp} {bar} {pct_5h}{wk_str}")
                };

                lines.push(draw_row(&left, &right_text, width));
            }
        }
    }

    // Flash notice dentro de Integraciones
    if let Some(notice) = flash_notice {
        if !collapsed[3] {
            lines.push(draw_divider(width));
        }
        lines.push(draw_row(&format!("{BOLD}{MINT}{notice}{RESET}"), "", width));
    }

        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "integraciones",
            start_line: int_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 5. Orquestadores Card (4)
    if !hidden_cards.contains("orquestadores") {
        let orch_start = lines.len();
        let total_count = snap.orchestrators.len();
        let critical_count = snap.orchestrators.iter().filter(|o| o.is_critical).count();
        let count_str = format!("{TEXT_DIM}{total_count} perfiles{RESET}");
        let orch_title = if critical_count > 0 {
            format!(
                "{title_color}Orquestadores{RESET} {TEXT_DIM}\u{00b7}{RESET} {BOLD}{CORAL}! {critical_count} cr\u{00ed}tic{RESET}"
            )
        } else {
            format!("{title_color}Orquestadores{RESET} {TEXT_DIM}\u{00b7}{RESET} {count_str}")
        };
        lines.push(draw_top(&orch_title, collapsed[4], width));
        if !collapsed[4] {
            if snap.orchestrators.is_empty() {
                lines.push(draw_row(
                    &format!("{TEXT_DIM}Sin perfiles de orquestador configurados{RESET}"),
                    &format!("{TEXT_DIM}[inactivo]{RESET}"),
                    width,
                ));
            } else {
                for orch in &snap.orchestrators {
                    let orch_disp = orch.display_name();
                    let (status_glyph, name_disp, status_right) = match orch.status {
                        crate::models::OrchestratorStatus::Working => {
                            let glyph = format!("{accent_secondary}>{RESET}");
                            let name = format!("{BOLD}{TEXT_PRIMARY}{}{RESET}", orch_disp);
                            let right = format!("{MINT}\u{25cf} trabajando{RESET}");
                            (glyph, name, right)
                        }
                        crate::models::OrchestratorStatus::Idle => {
                            if orch.recently_finished {
                                let glyph = format!("{MINT}\u{25cf}{RESET}");
                                let name = format!("{BOLD}{TEXT_PRIMARY}{}{RESET}", orch_disp);
                                let right = format!("{BOLD}{MINT}\u{2713} finalizado{RESET}");
                                (glyph, name, right)
                            } else {
                                let glyph = format!("{AMBER}\u{25cf}{RESET}");
                                let name = format!("{TEXT_PRIMARY}{}{RESET}", orch_disp);
                                let right = format!("{TEXT_DIM}-- en espera{RESET}");
                                (glyph, name, right)
                            }
                        }
                        crate::models::OrchestratorStatus::Offline => {
                            let glyph = format!("{TEXT_DIM}\u{25cb}{RESET}");
                            let name = format!("{TEXT_MUTED}{}{RESET}", orch_disp);
                            let right = format!("{TEXT_DIM}\u{25cb} apagado{RESET}");
                            (glyph, name, right)
                        }
                    };

                    let compact = compact_model_label(&orch.model_short);
                    let model_label = if let Some(ref t) = orch.thinking {
                        format!("{} [{t}]", compact)
                    } else {
                        compact
                    };

                    let left = if width < 42 {
                        format!("{status_glyph} {name_disp}")
                    } else {
                        format!("{status_glyph} {name_disp} {TEXT_DIM}({model_label}){RESET}")
                    };

                    let final_right = if orch.is_critical {
                        format!("{BOLD}{CORAL}[CUOTA CR\u{00cd}TICA]{RESET}")
                    } else if orch.is_warning {
                        format!("{BOLD}{AMBER}[AVISO CUOTA]{RESET}")
                    } else {
                        status_right
                    };

                    lines.push(draw_row(&left, &final_right, width));
                }
            }

            let active_critical_orch = snap
                .orchestrators
                .iter()
                .find(|o| o.status == crate::models::OrchestratorStatus::Working && o.is_critical);

            let has_idle_sessions = snap
                .orchestrators
                .iter()
                .any(|o| o.status == crate::models::OrchestratorStatus::Idle);

            if active_critical_orch.is_some() || has_idle_sessions {
                lines.push(draw_divider(width));
            }

            if let Some(co) = active_critical_orch {
                let target_orch = snap
                    .orchestrators
                    .iter()
                    .find(|o| !o.is_critical && o.name != co.name)
                    .map(|o| o.name.as_str())
                    .unwrap_or("alternativo");

                let mig_text = if width < 46 {
                    format!("{BOLD}{CORAL}! CR\u{00cd}TICO: {} [x: migrar]{RESET}", co.name)
                } else if width < 56 {
                    format!("{BOLD}{CORAL}! CR\u{00cd}TICO: {} [x -> migrar]{RESET}", co.name)
                } else {
                    format!(
                        "{BOLD}{CORAL}! CR\u{00cd}TICO: {} (activo) [presion\u{00e1} 'x' -> migrar a {target_orch}]{RESET}",
                        co.name
                    )
                };

                lines.push(draw_row(&mig_text, "", width));
            }

            if has_idle_sessions {
                let idle_text = if width < 46 {
                    format!("{TEXT_DIM}>> En espera [d: liberar]{RESET}")
                } else if width < 56 {
                    format!("{TEXT_DIM}>> Sesiones en espera [d: liberar]{RESET}")
                } else {
                    format!("{TEXT_DIM}>> Sesiones en espera detectadas [presion\u{00e1} 'd' -> liberar]{RESET}")
                };

                lines.push(draw_row(&idle_text, "", width));
            }
        }

        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "orquestadores",
            start_line: orch_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 6. Pool de Modelos / Cuotas Breakdown Card (5)
    if !hidden_cards.contains("pools") {
        let pools_start = lines.len();

        let active_orch = snap
            .orchestrators
            .iter()
            .find(|o| o.status == crate::models::OrchestratorStatus::Working)
            .or_else(|| {
                snap.orchestrators
                    .iter()
                    .find(|o| o.status == crate::models::OrchestratorStatus::Idle)
            })
            .or_else(|| snap.orchestrators.iter().find(|o| o.is_default))
            .or_else(|| snap.orchestrators.first());

        if let Some(orch) = active_orch {
            let pool_title = format!(
                "{title_color}Cuota de Modelos{RESET} {TEXT_DIM}\u{00b7}{RESET} {MINT}{}{RESET} {accent_secondary}*{RESET}",
                orch.display_name()
            );

            lines.push(draw_top(&pool_title, collapsed[5], width));
            if !collapsed[5] {
                struct FleetRow<'a> {
                    role_label: String,
                    _full_role: &'a str,
                    prefix: Option<&'a str>,
                    model_short: &'a str,
                    is_orch: bool,
                    all_roles: Vec<&'a str>,
                }

                let mut fleet_rows: Vec<FleetRow> = Vec::new();

                // Fila soberana del Orquestador
                fleet_rows.push(FleetRow {
                    role_label: "orch".to_string(),
                    _full_role: orch.name.as_str(),
                    prefix: orch.prefix.as_deref(),
                    model_short: &orch.model_short,
                    is_orch: true,
                    all_roles: vec![orch.name.as_str(), orch.display_name(), "orchestrator"],
                });

                // Agrupar subagentes y colapsar las 4 revisiones (review-risk, readability, reliability, resilience) en 'rev-4r'
                let review_roles = [
                    "review-risk",
                    "review-readability",
                    "review-reliability",
                    "review-resilience",
                ];
                let mut has_review_group = false;
                let mut review_prefix: Option<&str> = None;
                let mut review_model: &str = "claude-sonnet-5-5";

                for sub in &orch.subagents {
                    let r_low = sub.role.to_lowercase();
                    if review_roles.iter().any(|&rr| r_low == rr || r_low.contains(rr)) {
                        has_review_group = true;
                        if review_prefix.is_none() {
                            review_prefix = sub.prefix.as_deref();
                        }
                        review_model = &sub.model_short;
                    } else {
                        let clean_r = clean_subagent_role(&sub.role);
                        let pfx = sub.prefix.as_deref().or(orch.prefix.as_deref());
                        fleet_rows.push(FleetRow {
                            role_label: clean_r,
                            _full_role: sub.role.as_str(),
                            prefix: pfx,
                            model_short: &sub.model_short,
                            is_orch: false,
                            all_roles: vec![sub.role.as_str()],
                        });
                    }
                }

                if has_review_group {
                    let pfx = review_prefix.or(orch.prefix.as_deref());
                    fleet_rows.push(FleetRow {
                        role_label: "rev-4r".to_string(),
                        _full_role: "review-4r",
                        prefix: pfx,
                        model_short: review_model,
                        is_orch: false,
                        all_roles: vec![
                            "review-risk",
                            "review-readability",
                            "review-reliability",
                            "review-resilience",
                            "review-read",
                            "review-rel",
                            "review-res",
                            "rev-4r",
                        ],
                    });
                }

                for row in &fleet_rows {
                    let pfx_str = row.prefix.or(orch.prefix.as_deref()).unwrap_or("local");
                    let target_acc = snap
                        .accounts
                        .iter()
                        .find(|a| a.prefix.eq_ignore_ascii_case(pfx_str));

                    // Determinar estado de ejecución:
                    // 1. Activo (Working):
                    let is_active = if row.is_orch {
                        orch.status == crate::models::OrchestratorStatus::Working
                            && ((snap.agent_session.agent_status == "working"
                                && (snap.agent_session.agent.eq_ignore_ascii_case(&orch.name)
                                    || snap.agent_session.agent.eq_ignore_ascii_case(orch.display_name())
                                    || snap.agent_session.agent.eq_ignore_ascii_case("orchestrator")
                                    || snap.agent_session.agent.eq_ignore_ascii_case("gentle-orchestrator")
                                    || snap.agent_session.agent.eq_ignore_ascii_case("pi")
                                    || snap.agent_session.agent.eq_ignore_ascii_case("gentle-shell")
                                    || (snap.agent_session.local_prefix.as_deref() == orch.prefix.as_deref()
                                        && snap.agent_session.model_short.eq_ignore_ascii_case(&orch.model_short))))
                                || snap.panes.iter().any(|p| {
                                    let p_working = p.agent_status.as_deref() == Some("working")
                                        || p.tokens.get("agent_status").map(|s| s.as_str()) == Some("working");
                                    let hit_name = p.agent.as_deref().map(|ag| ag.eq_ignore_ascii_case(&orch.name) || ag.eq_ignore_ascii_case(orch.display_name())).unwrap_or(false)
                                        || p.tokens.get("profile").map(|pr| pr.eq_ignore_ascii_case(&orch.name) || pr.eq_ignore_ascii_case(orch.display_name())).unwrap_or(false);
                                    p_working && hit_name
                                }))
                    } else {
                        let clean_lower = row.role_label.to_lowercase();
                        let check_subagent_hit = |agent_str: &str, pfx_opt: Option<&str>, model_str: &str| {
                            let sess_agent = agent_str.to_lowercase();
                            let sess_clean = clean_subagent_role(&sess_agent);

                            let role_matches = row.all_roles.iter().any(|&r| {
                                let rl = r.to_lowercase();
                                sess_agent == rl
                                    || sess_clean == rl
                                    || sess_agent.contains(&rl)
                                    || sess_clean.contains(&rl)
                            }) || sess_agent == clean_lower || sess_clean == clean_lower;

                            let model_matches = if let Some(pfx) = row.prefix {
                                pfx_opt.map(|lp| lp.eq_ignore_ascii_case(pfx)).unwrap_or(false)
                                    || model_str.to_lowercase().contains(&pfx.to_lowercase())
                                    || true
                            } else {
                                model_str.is_empty()
                                    || model_str.eq_ignore_ascii_case(row.model_short)
                                    || model_str.to_lowercase().contains(&row.model_short.to_lowercase())
                            };

                            role_matches && model_matches
                        };

                        let session_active = snap.agent_session.agent_status == "working"
                            && check_subagent_hit(
                                &snap.agent_session.agent,
                                snap.agent_session.local_prefix.as_deref(),
                                &snap.agent_session.model_short,
                            );

                        let pane_active = snap.panes.iter().any(|p| {
                            let p_working = p.agent_status.as_deref() == Some("working")
                                || p.tokens.get("agent_status").map(|s| s.as_str()) == Some("working");
                            if !p_working {
                                return false;
                            }
                            let p_agent = p.agent.as_deref()
                                .or_else(|| p.tokens.get("agent").map(|s| s.as_str()))
                                .unwrap_or("");
                            if p_agent.is_empty() {
                                return false;
                            }
                            let p_model = p.tokens.get("model").map(|s| s.as_str()).unwrap_or("");
                            let p_pfx = p.tokens.get("profile").map(|s| s.as_str());
                            check_subagent_hit(p_agent, p_pfx, p_model)
                        });

                        session_active || pane_active
                    };

                    // 2. Pasivo (recién dejó de trabajar / presente en all_active_prefixes / idle):
                    let is_passive = !is_active
                        && (row.prefix
                            .map(|p| snap.agent_session.all_active_prefixes.contains(&p.to_lowercase()))
                            .unwrap_or(false)
                            || (row.is_orch && orch.status == crate::models::OrchestratorStatus::Idle));

                    // 3. Glifos y colores de texto solicitados:
                    // Activos: '>' y texto blanco brillante (BOLD TEXT_PRIMARY)
                    // Pasivos: '●' en MAGENTA (púrpura) y texto blanco (TEXT_PRIMARY)
                    // Inactivos: '○' en gris (TEXT_DIM) y texto gris (TEXT_MUTED)
                    let (status_glyph, text_color) = if is_active {
                        (format!("{accent_secondary}>{RESET}"), format!("{BOLD}{TEXT_PRIMARY}"))
                    } else if is_passive {
                        (format!("{MAGENTA}\u{25cf}{RESET}"), TEXT_PRIMARY.to_string())
                    } else {
                        (format!("{TEXT_DIM}\u{25cb}{RESET}"), TEXT_MUTED.to_string())
                    };

                    let compact_model = compact_model_label(row.model_short);
                    let role_w = if width < 42 { 8 } else { 9 };
                    let role_truncated = truncate_to_width(&row.role_label, role_w);
                    let role_disp = format!("{:<width$}", role_truncated, width = role_w);

                    let left = format!(
                        "{status_glyph} {text_color}{role_disp}{RESET} {TEXT_DIM}({compact_model}){RESET}"
                    );

                    // Proyección de cuota / saldo para esta cuenta y modelo:
                    let bar_len = if width < 42 { 2 } else { 3 };

                    let right = if let Some(acc) = target_acc {
                        let is_api = crate::quotas::is_api_key(acc);
                        if is_api {
                            let session_cost = if is_active || is_passive {
                                snap.agent_session.cost
                            } else {
                                0.0
                            };
                            let (maybe_budget, rem_balance, rem_pct, _cost) =
                                crate::quotas::cliproxy::calculate_api_key_balance(
                                    &acc.prefix,
                                    session_cost,
                                    snap.agent_session.session_id.as_deref(),
                                );
                            let pool_color = if maybe_budget.is_some() {
                                if rem_pct <= 8 {
                                    CORAL
                                } else if rem_pct <= 20 {
                                    AMBER
                                } else {
                                    MINT
                                }
                            } else {
                                MINT
                            };
                            let bar = render_gauge_inline(rem_pct, bar_len, pool_color);
                            if let Some(rem) = rem_balance {
                                if rem <= 0.0 || rem_pct <= 8 {
                                    format!("{bar} {BOLD}{CORAL}[SALDO AGOTADO]{RESET}")
                                } else if width < 42 {
                                    format!("{bar} {BOLD}{pool_color}${rem:.2}{RESET}")
                                } else {
                                    format!("{bar} {BOLD}{pool_color}${rem:.2} disp{RESET}")
                                }
                            } else {
                                format!("{TEXT_DIM}[{}]{RESET}", acc.reset_5h)
                            }
                        } else {
                            let eff = crate::quotas::evaluate_account_for_model(acc, row.model_short);
                            let pool_color = if eff.is_critical {
                                CORAL
                            } else if eff.is_warning {
                                AMBER
                            } else {
                                MINT
                            };
                            let bar = render_gauge_inline(eff.rem_5h, bar_len, pool_color);
                            let pct_5h = format!("{BOLD}{pool_color}{:>3}%{RESET}", eff.rem_5h);
                            let wk_color = if eff.is_critical {
                                CORAL
                            } else if eff.is_warning {
                                AMBER
                            } else {
                                MINT
                            };
                            let wk_str = if width < 40 {
                                format!(" {TEXT_DIM}W:{RESET}{BOLD}{wk_color}{:>3}%{RESET}", eff.rem_wk)
                            } else {
                                format!(" {TEXT_DIM}Wk:{RESET}{BOLD}{wk_color}{:>3}%{RESET}", eff.rem_wk)
                            };

                            if eff.is_critical {
                                format!("{bar} {pct_5h}{wk_str} {CORAL}[AGOTADO]{RESET}")
                            } else {
                                format!("{bar} {pct_5h}{wk_str}")
                            }
                        }
                    } else {
                        format!("{TEXT_DIM}[disp]{RESET}")
                    };

                    lines.push(draw_row(&left, &right, width));
                }

                if snap.is_editing_balance {
                    let input_display = if snap.balance_input_buffer.is_empty() {
                        format!("{TEXT_DIM}_ (ingresa saldo ej: 50.00){RESET}")
                    } else {
                        format!(
                            "{BOLD}{TEXT_PRIMARY}{}{RESET}{MINT}_{RESET}",
                            snap.balance_input_buffer
                        )
                    };
                    let left = format!("{accent_secondary}> {RESET}{BOLD}Saldo $: {input_display}");
                    let right = format!("{AMBER}[Enter|Esc]{RESET}");
                    let row_idx = lines.len();
                    lines.push(draw_row(&left, &right, width));
                    bounds.push(CardBounds {
                        id: "edit_balance",
                        start_line: row_idx,
                        end_line: row_idx,
                        action_type: Some("edit_balance"),
                        action_path: snap.editing_balance_prefix.clone(),
                    });
                }
            }
        } else {
            // Fallback legado si no hay perfiles de orquestador definidos
            let default_prefix = snap
                .accounts
                .first()
                .map(|a| a.prefix.as_str())
                .unwrap_or("local");
            let active_acc_prefix = snap
                .agent_session
                .local_prefix
                .as_deref()
                .unwrap_or(default_prefix);

            let target_account = snap
                .accounts
                .iter()
                .find(|a| a.prefix.to_lowercase() == active_acc_prefix.to_lowercase())
                .or_else(|| snap.accounts.first());

            let pool_title = format!(
                "{title_color}Cuota de Modelos{RESET} {TEXT_DIM}\u{00b7}{RESET} {MINT}{active_acc_prefix} {accent_secondary}*{RESET}"
            );

            lines.push(draw_top(&pool_title, collapsed[5], width));
            if !collapsed[5] {
                if snap.accounts.is_empty() {
                    lines.push(draw_row(
                        &format!("{TEXT_DIM}Sin proveedor de cuotas activo{RESET}"),
                        &format!("{TEXT_DIM}[inactivo]{RESET}"),
                        width,
                    ));
                } else if let Some(acc) = target_account {
                    let is_acc_api = crate::quotas::is_api_key(acc);
                    if is_acc_api {
                        let is_local = snap
                            .agent_session
                            .local_prefix
                            .as_deref()
                            .map(|p| p.eq_ignore_ascii_case(&acc.prefix))
                            .unwrap_or(false);
                        let is_any_active = snap
                            .agent_session
                            .all_active_prefixes
                            .contains(&acc.prefix.to_lowercase());
                        let session_cost = if is_local || is_any_active {
                            snap.agent_session.cost
                        } else {
                            0.0
                        };
                        let maybe_budget = crate::quotas::cliproxy::get_user_configured_budget(&acc.prefix);
                        let (_, rem_balance, rem_pct, cost_since_set) =
                            crate::quotas::cliproxy::calculate_api_key_balance(
                                &acc.prefix,
                                session_cost,
                                snap.agent_session.session_id.as_deref(),
                            );

                        let api_label = acc
                            .pools
                            .first()
                            .map(|p| p.label.as_str())
                            .unwrap_or("API");

                        let pool_color = if maybe_budget.is_some() {
                            if rem_pct <= 8 {
                                CORAL
                            } else if rem_pct <= 20 {
                                AMBER
                            } else {
                                MINT
                            }
                        } else {
                            MINT
                        };
                        let pfx_w = if width < 42 { 8 } else { 12 };
                        let p_label = format!("{:<width$}", api_label, width = pfx_w);
                        let row1_left = if maybe_budget.is_some() {
                            let bar_len = if width < 42 { 3 } else { 6 };
                            let bar = render_gauge_inline(rem_pct, bar_len, pool_color);
                            let pct_fmt = format!("{BOLD}{pool_color}{:>3}%{RESET}", rem_pct);
                            format!("{pool_color}\u{25cf}{RESET} {TEXT_PRIMARY}{p_label}{RESET} {bar} {pct_fmt}")
                        } else {
                            format!("{MINT}\u{25cf}{RESET} {TEXT_PRIMARY}{p_label}{RESET}")
                        };
                        let avg_cost = snap.agent_session.avg_turn_cost();
                        let display_cost = if maybe_budget.is_some() {
                            cost_since_set
                        } else {
                            session_cost
                        };
                        let row1_right = if width < 42 {
                            format!("{BOLD}{TEXT_PRIMARY}${:.3}{RESET}", display_cost)
                        } else if avg_cost > 0.0 {
                            format!(
                                "{TEXT_DIM}sesi\u{00f3}n:{RESET} {BOLD}{TEXT_PRIMARY}${:.3}{RESET} {TEXT_DIM}\u{00b7} ${:.3}/req{RESET}",
                                display_cost, avg_cost
                            )
                        } else {
                            format!(
                                "{TEXT_DIM}sesi\u{00f3}n:{RESET} {BOLD}{TEXT_PRIMARY}${:.3}{RESET}",
                                display_cost
                            )
                        };
                        lines.push(draw_row(&row1_left, &row1_right, width));

                        let saldo_row_idx = lines.len();
                        let is_editing = snap.is_editing_balance
                            && snap
                                .editing_balance_prefix
                                .as_deref()
                                .map(|p| p.eq_ignore_ascii_case(&acc.prefix))
                                .unwrap_or(false);

                        if is_editing {
                            let input_display = if snap.balance_input_buffer.is_empty() {
                                format!("{TEXT_DIM}_ (ingresa saldo ej: 50.00){RESET}")
                            } else {
                                format!(
                                    "{BOLD}{TEXT_PRIMARY}{}{RESET}{MINT}_{RESET}",
                                    snap.balance_input_buffer
                                )
                            };
                            let left = format!("{accent_secondary}> {RESET}{BOLD}Saldo $: {input_display}");
                            let right = format!("{AMBER}[Enter|Esc]{RESET}");
                            lines.push(draw_row(&left, &right, width));
                        } else if let (Some(_b), Some(rem)) = (maybe_budget, rem_balance) {
                            let saldo_color = if rem <= 0.0 || rem_pct <= 8 {
                                CORAL
                            } else if rem_pct <= 20 {
                                AMBER
                            } else {
                                MINT
                            };
                            let bar_len = if width < 42 { 3 } else { 6 };
                            let bar = render_gauge_inline(rem_pct, bar_len, saldo_color);
                            let pfx_w = if width < 42 { 6 } else { 8 };
                            let p_label = format!("{:<width$}", "Saldo", width = pfx_w);
                            let left = format!(
                                "{saldo_color}\u{25cf}{RESET} {TEXT_PRIMARY}{p_label}{RESET} {bar} {BOLD}{saldo_color}${rem:.2}{RESET}"
                            );
                            let right = if rem <= 0.0 {
                                format!("{BOLD}{CORAL}[AGOTADO]{RESET} {TEXT_DIM}[editar]{RESET}")
                            } else if avg_cost > 0.0 && width >= 42 {
                                let count = (rem / avg_cost).round() as u64;
                                let count_str = if count >= 1000 {
                                    format!("~{:.1}k", count as f64 / 1000.0)
                                } else {
                                    format!("~{count}")
                                };
                                format!("{TEXT_DIM}{count_str} reqs{RESET} {TEXT_DIM}[editar]{RESET}")
                            } else {
                                format!("{TEXT_DIM}disp{RESET} {TEXT_DIM}[editar]{RESET}")
                            };
                            lines.push(draw_row(&left, &right, width));
                        } else {
                            let pfx_w = if width < 42 { 6 } else { 8 };
                            let p_label = format!("{:<width$}", "Saldo", width = pfx_w);
                            let left = format!("{TEXT_MUTED}\u{25cf}{RESET} {TEXT_MUTED}{p_label}{RESET}");
                            let right = format!("{MINT}[definir saldo]{RESET}");
                            lines.push(draw_row(&left, &right, width));
                        }
                        bounds.push(CardBounds {
                            id: "edit_balance",
                            start_line: saldo_row_idx,
                            end_line: saldo_row_idx,
                            action_type: Some("edit_balance"),
                            action_path: Some(acc.prefix.clone()),
                        });

                        // 3. Fila de Facturación oficial con [ver ->]
                        let portal_row_idx = lines.len();
                        let billing_url = crate::quotas::cliproxy::provider_billing_url(&acc.prefix);
                        let portal_left = if width < 40 {
                            format!("{TEXT_DIM}\u{00bb} Facturaci\u{00f3}n{RESET}")
                        } else {
                            format!("{TEXT_DIM}\u{00bb} Facturaci\u{00f3}n oficial{RESET}")
                        };
                        let portal_right = format!("{MINT}[ver ->]{RESET}");
                        lines.push(draw_row(&portal_left, &portal_right, width));
                        bounds.push(CardBounds {
                            id: "open_billing_url",
                            start_line: portal_row_idx,
                            end_line: portal_row_idx,
                            action_type: Some("open_billing_url"),
                            action_path: Some(billing_url.to_string()),
                        });
                    } else {
                        for p in &acc.pools {
                            let pool_color = if p.percent <= 15 {
                                CORAL
                            } else if p.percent <= 50 {
                                AMBER
                            } else {
                                MINT
                            };

                            let bar_len = if width < 42 { 3 } else { 6 };
                            let bar = render_gauge_inline(p.percent, bar_len, pool_color);
                            let pct_fmt = format!("{BOLD}{pool_color}{:>3}%{RESET}", p.percent);
                            let pfx_w = if width < 42 { 8 } else { 12 };
                            let p_label = format!("{:<width$}", p.label, width = pfx_w);
                            let left = format!(
                                "{pool_color}\u{25cf}{RESET} {TEXT_PRIMARY}{p_label}{RESET} {bar} {pct_fmt}"
                            );
                            let right = if width < 40 {
                                format!("{pool_color}{}{RESET} {TEXT_DIM}{}{RESET}", p.pace, p.reset)
                            } else {
                                format!(
                                    "{pool_color}{}{RESET} {TEXT_DIM}en {}{RESET}",
                                    p.pace, p.reset
                                )
                            };
                            lines.push(draw_row(&left, &right, width));
                        }
                    }
                }
            }
        }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "pools",
            start_line: pools_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 7. Cambios / Git Card (5)
    if !hidden_cards.contains("git") {
        let git_start = lines.len();
    let ag_changes = if !snap.git.cross_session_conflicts.is_empty() {
        format!(
            "{BOLD}{CORAL}! {} colisi\u{00f3}n{RESET}",
            snap.git.cross_session_conflicts.len()
        )
    } else {
        format!(
            "{TEXT_DIM}\u{00b7} {} por agente{RESET}",
            snap.agent_session.attributed_changes.files_count
        )
    };
    lines.push(draw_top(
        &format!("{BOLD}{title_color}Cambios{RESET} {ag_changes}"),
        collapsed[6],
        width,
    ));
    if !collapsed[6] {
        if !snap.git.cross_session_conflicts.is_empty() {
            let files_joined = snap.git.cross_session_conflicts.join(", ");
            lines.push(draw_row(
                &format!("{BOLD}{CORAL}! CONFLICTO: {files_joined}{RESET}"),
                &format!("{CORAL}[colisi\u{00f3}n]{RESET}"),
                width,
            ));
        }
        let ab_badge = match snap.git.ahead_behind {
            Some((a, b)) if a > 0 || b > 0 => {
                let mut parts = Vec::new();
                if a > 0 {
                    parts.push(format!("\u{25b2}{a}"));
                }
                if b > 0 {
                    parts.push(format!("\u{25bc}{b}"));
                }
                format!(" {GOLD}{}{RESET}", parts.join(""))
            }
            _ => String::new(),
        };
        let clean_st = if snap.git.is_clean {
            format!("{MINT}ok limpio{RESET}")
        } else {
            format!("{AMBER}modificado{RESET}")
        };
        lines.push(draw_row(
            &format!("{CYAN}{}{RESET}{ab_badge}", snap.git.branch),
            &clean_st,
            width,
        ));
        if let Some(latest_commit) = snap.git.recent_commits.first() {
            lines.push(draw_row(
                &format!("{TEXT_DIM}* {latest_commit}{RESET}"),
                "",
                width,
            ));
        } else if let Some(ref head_sha) = snap.git.head_short {
            lines.push(draw_row(
                &format!("{TEXT_DIM}* {head_sha} HEAD{RESET}"),
                "",
                width,
            ));
        } else {
            lines.push(draw_row(
                &format!("{TEXT_DIM}* (sin commits recientes){RESET}"),
                "",
                width,
            ));
        }
        let git_part = if snap.git.is_clean {
            format!("{TEXT_DIM}git:{RESET} {MINT}limpio{RESET}")
        } else if snap.git.lines_added > 0 || snap.git.lines_deleted > 0 {
            format!(
                "{TEXT_DIM}git:{RESET} {MINT}+{}{RESET} {CORAL}-{}{RESET}",
                snap.git.lines_added, snap.git.lines_deleted
            )
        } else {
            format!(
                "{TEXT_DIM}git:{RESET} {AMBER}{} pend{RESET}",
                snap.git.modified + snap.git.staged + snap.git.untracked
            )
        };
        let ag_part = format!(
            "{ACCENT_PRIMARY}ag:{RESET} {MINT}{} archivos (+){RESET}",
            snap.agent_session.attributed_changes.files_count
        );
        lines.push(draw_row(&git_part, &ag_part, width));

        if !snap.git.files.is_empty() {
            lines.push(draw_divider(width));
            let max_files = 6;
            for f in snap.git.files.iter().take(max_files) {
                let file_line_idx = lines.len();
                let is_selected = selected_diff_file == Some(f.path.as_str());
                let badge_color = match f.status_code.as_str() {
                    "A" => MINT,
                    "D" | "U" => CORAL,
                    "?" => CYAN,
                    _ => AMBER,
                };
                let arrow = if is_selected { "v" } else { ">" };
                let left_file = format!(
                    "{TEXT_DIM}{arrow}{RESET} {badge_color}[{}]{RESET} {TEXT_PRIMARY}{}{RESET}",
                    f.status_code, f.path
                );
                let right_stat = if f.added > 0 || f.deleted > 0 {
                    format!("{MINT}+{}{RESET} {CORAL}-{}{RESET}", f.added, f.deleted)
                } else {
                    String::new()
                };
                lines.push(draw_row(&left_file, &right_stat, width));
                bounds.push(CardBounds {
                    id: "git_file_diff",
                    start_line: file_line_idx,
                    end_line: file_line_idx,
                    action_type: Some("git_file_diff"),
                    action_path: Some(f.path.clone()),
                });

                if is_selected {
                    for dl in selected_diff_lines.iter().take(12) {
                        let colored_dl = if dl.starts_with('[') {
                            format!("  {AMBER}{dl}{RESET}")
                        } else if dl.starts_with("@@") {
                            format!("  {CYAN}{dl}{RESET}")
                        } else if dl.starts_with('+') {
                            format!("  {MINT}{dl}{RESET}")
                        } else if dl.starts_with('-') {
                            format!("  {CORAL}{dl}{RESET}")
                        } else {
                            format!("  {TEXT_MUTED}{dl}{RESET}")
                        };
                        lines.push(draw_row(&colored_dl, "", width));
                    }
                    let peek_line_idx = lines.len();
                    lines.push(draw_row(
                        &format!("  {ACCENT_PRIMARY}>> [vista r\u{00e1}pida (Espacio)]{RESET}"),
                        "",
                        width,
                    ));
                    bounds.push(CardBounds {
                        id: "open_quick_peek",
                        start_line: peek_line_idx,
                        end_line: peek_line_idx,
                        action_type: Some("open_quick_peek"),
                        action_path: Some(f.path.clone()),
                    });
                }
            }
            if snap.git.files.len() > max_files {
                lines.push(draw_row(
                    &format!(
                        "{TEXT_DIM}\u{2026} y {} archivos m\u{00e1}s{RESET}",
                        snap.git.files.len() - max_files
                    ),
                    "",
                    width,
                ));
            }
        }
    }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "git",
            start_line: git_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 8. Engram Card (7)
    if !hidden_cards.contains("engram") {
        let engram_start = lines.len();
    lines.push(draw_top(
        &format!("{ACCENT_PRIMARY}Engram:{RESET} {CYAN}{folder}{RESET} {TEXT_DIM}^{RESET}"),
        collapsed[7],
        width,
    ));
    if !collapsed[7] {
        let online = if snap.engram.online {
            format!("{MINT}\u{25cf} En linea{RESET}")
        } else {
            format!("{CORAL}\u{25cb} Offline{RESET}")
        };
        // El estado "en línea" se deriva de poder abrir engram.db (SQLite local),
        // no de un puerto HTTP, así que la etiqueta refleja la fuente real.
        let local_label = if width < 40 {
            "engram.db"
        } else {
            "Local (engram.db)"
        };
        lines.push(draw_row(
            local_label,
            &format!(
                "{online} {TEXT_DIM}\u{00b7}{RESET} {CYAN}{} obs{RESET}",
                snap.engram.obs_count
            ),
            width,
        ));
        let cloud_row = if snap.engram.enrolled {
            if width < 40 {
                "Cloud: sync inscrito"
            } else {
                "Cloud: proyecto inscrito para sync"
            }
        } else if width < 40 {
            "Cloud: solo local"
        } else {
            "Cloud: solo local o no configurado"
        };
        lines.push(draw_row(cloud_row, "", width));
        if snap.engram.daemon_online || snap.engram.sync_status.is_some() {
            let d_badge = if snap.engram.daemon_online {
                format!("{MINT}daemon ok{RESET}")
            } else {
                format!("{TEXT_DIM}daemon off{RESET}")
            };
            let s_badge = snap
                .engram
                .sync_status
                .as_deref()
                .map(|s| format!("{CYAN}{s}{RESET}"))
                .unwrap_or_default();
            lines.push(draw_row(
                &format!("{TEXT_DIM}Servicio:{RESET} {d_badge}"),
                &s_badge,
                width,
            ));
        }
        if let Some(ref handoff) = snap.engram.last_handoff {
            lines.push(draw_divider(width));
            lines.push(draw_row(
                &format!("{label_color}Handoff{RESET} {TEXT_DIM}#{} {RESET}", handoff.id),
                &format!("{TEXT_MUTED}{}{RESET}", handoff.relative_time),
                width,
            ));
            for hl in handoff.lines.iter().take(3) {
                lines.push(draw_row(&format!("{TEXT_MUTED}{hl}{RESET}"), "", width));
            }
        }
    }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "engram",
            start_line: engram_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 8. Herramientas Card (idx 7)
    if !hidden_cards.contains("tools") {
        let tools_start = lines.len();
        let tool_title = format!(
            "{BOLD}{title_color}Herramientas{RESET} {TEXT_DIM}\u{00b7} {} llamadas{RESET}",
            snap.agent_session.tool_total
        );
        lines.push(draw_top(&tool_title, collapsed[8], width));
        if !collapsed[8] {
            let p_read = format!("{MAGENTA}@ {} lecturas{RESET}", snap.agent_session.tool_reads);
            let p_write = format!("{CYAN}@ {} escritu{RESET}", snap.agent_session.tool_writes);
            let p_mem = format!("{CORAL}mem: {}{RESET}", snap.agent_session.tool_engram);
            lines.push(draw_row(&format!("{p_read}  {p_write}"), &p_mem, width));
            if snap.agent_session.tool_bash > 0 {
                let p_bash = format!("{label_color}$ {} cmds{RESET}", snap.agent_session.tool_bash);
                lines.push(draw_row(&p_bash, "", width));
            }
        }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "tools",
            start_line: tools_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 9. MCP Card (idx 9)
    if !hidden_cards.contains("mcp") {
        let mcp_start = lines.len();
        lines.push(draw_top(
            &format!(
                "{BOLD}{title_color}MCP{RESET} {TEXT_DIM}\u{00b7} {} activos{RESET}",
                snap.mcps.len()
            ),
            collapsed[9],
            width,
        ));
        if !collapsed[9] {
            for m in &snap.mcps {
                let dot = if m.enabled {
                    format!("{MINT}\u{25cf}{RESET}")
                } else {
                    format!("{AMBER}\u{25cb}{RESET}")
                };
                let badge = format!("{TEXT_DIM}[{}]{RESET}", m.mcp_type);
                let left = format!("{dot} {BOLD}{TEXT_PRIMARY}{}{RESET} {badge}", m.name);
                let right = if m.enabled {
                    format!("{MINT}en línea{RESET}")
                } else {
                    format!("{TEXT_MUTED}desactivado{RESET}")
                };
                lines.push(draw_row(&left, &right, width));
            }
        }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "mcp",
            start_line: mcp_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 10. Stack · Ecosistema Card (idx 9, tecla 'e')
    if !hidden_cards.contains("ecosystem") {
        let eco_start = lines.len();
        let update_count = if snap.check_updates {
            snap.ecosystem
                .tools
                .iter()
                .filter(|t| t.update_available)
                .count()
        } else {
            0
        };
        let eco_title = if update_count > 0 {
            format!(
                "{BOLD}{title_color}Stack{RESET} {text_dim}\u{00b7}{RESET} {title_color}Ecosistema{RESET} {amber}\u{2191}{update_count}{RESET}"
            )
        } else {
            format!("{BOLD}{title_color}Stack{RESET} {text_dim}\u{00b7}{RESET} {title_color}Ecosistema{RESET}")
        };
        lines.push(draw_top(&eco_title, collapsed[10], width));
        if !collapsed[10] {
            if snap.ecosystem.tools.is_empty() {
                lines.push(draw_row(
                    &format!("{text_dim}escaneando ecosistema...{RESET}"),
                    "",
                    width,
                ));
            } else {
                for tool in &snap.ecosystem.tools {
                    let row_line_idx = lines.len();
                    let left = if tool.is_installed {
                        let ver_str = tool.version.as_deref().unwrap_or("");
                        if tool.is_updating {
                            format!(
                                "{mint}\u{25cf}{RESET} {text_primary}{}{RESET} {text_muted}{}{RESET}",
                                tool.name, ver_str
                            )
                        } else if snap.check_updates && tool.update_available && let Some(ref up_ver) = tool.upstream_version {
                            format!(
                                "{mint}\u{25cf}{RESET} {text_primary}{}{RESET} {text_muted}{}{RESET} {amber}\u{2191}{}{RESET}",
                                tool.name, ver_str, up_ver
                            )
                        } else {
                            format!(
                                "{mint}\u{25cf}{RESET} {text_primary}{}{RESET} {text_muted}{}{RESET}",
                                tool.name, ver_str
                            )
                        }
                    } else if snap.check_updates && let Some(ref up_ver) = tool.upstream_version {
                        format!("{text_dim}\u{25cb} {} - (disp {}){RESET}", tool.name, up_ver)
                    } else {
                        format!("{text_dim}\u{25cb} {} -{RESET}", tool.name)
                    };

                    let right = if tool.is_updating {
                        const SPINNER_FRAMES: &[char] = &[
                            '\u{280b}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283c}',
                            '\u{2834}', '\u{2826}', '\u{2827}', '\u{2807}', '\u{280f}',
                        ];
                        let (spinner, pct_str) = if let Some(ref prog) = tool.update_progress {
                            let sp = SPINNER_FRAMES[prog.spinner_idx % SPINNER_FRAMES.len()];
                            let p = format!("{}%", prog.percent);
                            (sp, p)
                        } else {
                            ('\u{280b}', "--%".to_string())
                        };

                        let monitor = format!("{cyan}[{spinner} {pct_str}]{RESET}");
                        let btn_txt = if width >= 56 {
                            format!("{amber}[actualizando]{RESET}")
                        } else if width >= 40 {
                            format!("{amber}[\u{2191}]{RESET}")
                        } else {
                            String::new()
                        };

                        if btn_txt.is_empty() {
                            monitor
                        } else {
                            format!("{monitor} {btn_txt}")
                        }
                    } else if snap.check_updates && tool.update_available {
                        let btn_txt = if tool.name == "hudr2" {
                            if width >= 44 { "[asistido]" } else { "[\u{2191}]" }
                        } else if width >= 44 {
                            "[actualizar]"
                        } else {
                            "[\u{2191}]"
                        };
                        format!("{amber}{btn_txt}{RESET}")
                    } else {
                        String::new()
                    };

                    lines.push(draw_row(&left, &right, width));

                    if snap.check_updates && tool.update_available && !tool.is_updating {
                        bounds.push(CardBounds {
                            id: "update_tool",
                            start_line: row_line_idx,
                            end_line: row_line_idx,
                            action_type: Some("update_tool"),
                            action_path: Some(tool.name.clone()),
                        });
                    }
                }
            }
        }
        lines.push(draw_bottom(width));
        bounds.push(CardBounds {
            id: "ecosystem",
            start_line: eco_start,
            end_line: lines.len() - 1,
            action_type: None,
            action_path: None,
        });
    }

    // 11. Personalizar Card (10) — Selector interactivo de visibilidad, nombre y tema
    let personalizar_start = lines.len();
    let personalizar_title = format!("{BOLD}{title_color}Personalizar{RESET}");
    lines.push(draw_top(&personalizar_title, collapsed[11], width));
    if !collapsed[11] {
        let cards_info: [(&str, &str); 11] = [
            ("estado", "Estado"),
            ("contexto", "Contexto"),
            ("proyecto", "Proyecto"),
            ("integraciones", "Integraciones"),
            ("orquestadores", "Orquestadores"),
            ("pools", "Cuota de Modelos"),
            ("git", "Cambios (Git)"),
            ("engram", "Engram"),
            ("tools", "Herramientas"),
            ("mcp", "MCP"),
            ("ecosystem", "Stack \u{00b7} Ecosistema"),
        ];

        for (id, label) in cards_info {
            let is_hidden = hidden_cards.contains(id);
            let row_line_idx = lines.len();
            let (left_box, right_status) = if is_hidden {
                (
                    format!("{TEXT_DIM}[ ]{RESET} {TEXT_MUTED}{label}{RESET}"),
                    format!("{TEXT_DIM}oculta{RESET}"),
                )
            } else {
                (
                    format!("{MINT}[x]{RESET} {TEXT_PRIMARY}{label}{RESET}"),
                    format!("{MINT}visible{RESET}"),
                )
            };
            lines.push(draw_row(&left_box, &right_status, width));
            bounds.push(CardBounds {
                id: "toggle_card_visibility",
                start_line: row_line_idx,
                end_line: row_line_idx,
                action_type: Some("toggle_card_visibility"),
                action_path: Some(id.to_string()),
            });
        }

        // Divisor para opciones de personalización (Nombre + Tema)
        lines.push(draw_divider(width));

        // 1. Fila de Nombre personalizado del Sidebar (editable en línea)
        let name_row_idx = lines.len();
        if snap.is_editing_name {
            let input_display = if snap.name_input_buffer.is_empty() {
                format!("{TEXT_DIM}_ (escribe nombre){RESET}")
            } else {
                format!(
                    "{BOLD}{TEXT_PRIMARY}{}{RESET}{MINT}_{RESET}",
                    snap.name_input_buffer
                )
            };
            let name_left = format!("{label_color}Nombre{RESET}  > {input_display}");
            let name_right = format!("{AMBER}[Enter|Esc]{RESET}");
            lines.push(draw_row(&name_left, &name_right, width));
        } else {
            let (name_val, badge_text, badge_color) = match snap
                .sidebar_name
                .as_deref()
                .filter(|s| !s.trim().is_empty())
            {
                Some(n) => (n, "[editar >]", accent_primary),
                None => ("HUDR2", "[editar >]", accent_primary),
            };
            let name_left = format!("{label_color}Nombre{RESET}  {TEXT_PRIMARY}{name_val}{RESET}");
            let name_right = format!("{badge_color}{badge_text}{RESET}");
            lines.push(draw_row(&name_left, &name_right, width));
        }
        bounds.push(CardBounds {
            id: "edit_sidebar_name",
            start_line: name_row_idx,
            end_line: name_row_idx,
            action_type: Some("edit_sidebar_name"),
            action_path: None,
        });

        // 2. Fila interactiva para ciclar temas visuales
        let theme_label = crate::theme::theme_display_label(snap.theme_name.as_deref());
        let theme_left = format!("{label_color}Tema{RESET}    {TEXT_PRIMARY}{theme_label}{RESET}");
        let theme_right = format!("{accent_primary}[cambiar >]{RESET}");
        let theme_row_idx = lines.len();
        lines.push(draw_row(&theme_left, &theme_right, width));
        bounds.push(CardBounds {
            id: "cycle_theme",
            start_line: theme_row_idx,
            end_line: theme_row_idx,
            action_type: Some("cycle_theme"),
            action_path: None,
        });

        // 3. Fila interactiva para activar/desactivar chequeo online de versiones del stack
        let updates_left = format!("{label_color}Updates{RESET} {text_primary}Check online{RESET}");
        let (updates_badge, updates_color) = if snap.check_updates {
            ("[x] activo (3h)", mint)
        } else {
            ("[ ] apagado", text_dim)
        };
        let updates_right = format!("{updates_color}{updates_badge}{RESET}");
        let updates_row_idx = lines.len();
        lines.push(draw_row(&updates_left, &updates_right, width));
        bounds.push(CardBounds {
            id: "toggle_check_updates",
            start_line: updates_row_idx,
            end_line: updates_row_idx,
            action_type: Some("toggle_check_updates"),
            action_path: None,
        });

        // 4. Fila interactiva para configurar alertas sonoras de orquestación
        let sound_left = format!("{label_color}Avisos{RESET}  {text_primary}Alertas sonoras{RESET}");
        let (sound_badge, is_active) = snap.sound_mode.display_badge();
        let sound_color = if is_active { mint } else { text_dim };
        let sound_right = format!("{sound_color}{sound_badge}{RESET}");
        let sound_row_idx = lines.len();
        lines.push(draw_row(&sound_left, &sound_right, width));
        bounds.push(CardBounds {
            id: "cycle_sound_mode",
            start_line: sound_row_idx,
            end_line: sound_row_idx,
            action_type: Some("cycle_sound_mode"),
            action_path: None,
        });
    }
    lines.push(draw_bottom(width));
    bounds.push(CardBounds {
        id: "personalizar",
        start_line: personalizar_start,
        end_line: lines.len() - 1,
        action_type: None,
        action_path: None,
    });

    // Footer de atajos
    if snap.is_editing_name {
        lines.push(truncate_to_width(
            &format!(
                "{AMBER} Enter: guardar \u{00b7} Esc: cancelar \u{00b7} Retroceso: borrar{RESET}"
            ),
            width,
        ));
    } else {
        lines.push(truncate_to_width(
            &format!(
                "{TEXT_DIM} 0-9: colap \u{00b7} p: pers \u{00b7} m: tema \u{00b7} c: todo \u{00b7} d: liberar \u{00b7} o: abrir{RESET}"
            ),
            width,
        ));
    }
    let has_active_critical = snap.orchestrators.iter().any(|o| {
        o.status == crate::models::OrchestratorStatus::Working && o.is_critical
    }) || snap.accounts.iter().any(|a| {
        let eff = crate::quotas::evaluate_account_for_model(a, active_model);
        (eff.is_critical || (100u8.saturating_sub(eff.rem_5h) >= 85 && !a.has_error))
            && snap
                .agent_session
                .all_active_prefixes
                .contains(&a.prefix.to_lowercase())
    });
    let (audit_label, critical_action) = if has_active_critical {
        (
            "a: aud",
            format!("{CORAL}x: migrar ({target_healthy}){TEXT_DIM} \u{00b7} {RESET}"),
        )
    } else {
        ("a: auditar", String::new())
    };
    lines.push(truncate_to_width(
        &format!(
            "{TEXT_DIM} s: sync \u{00b7} r: act \u{00b7} {audit_label} \u{00b7} {critical_action}q: salir{RESET}"
        ),
        width,
    ));

    RenderOutput { lines, bounds }
}

/// Renderiza el modal flotante Quick Peek usando las primitivas ANSI/Unicode seguras de `hudr2`.
pub fn render_quick_peek_modal(
    peek: &crate::state::QuickPeekState,
    width: usize,
    max_content_rows: usize,
) -> RenderOutput {
    let mut lines = Vec::new();
    let mut bounds = Vec::new();

    let title = format!(
        "{BOLD}{ACCENT_PRIMARY}Quick Peek{RESET} {TEXT_DIM}\u{00b7}{RESET} {GOLD}{}{RESET}",
        peek.title
    );
    let top_idx = lines.len();
    lines.push(draw_top(&title, false, width));
    bounds.push(CardBounds {
        id: "close_quick_peek",
        start_line: top_idx,
        end_line: top_idx,
        action_type: Some("close_quick_peek"),
        action_path: None,
    });

    let close_row_idx = lines.len();
    lines.push(draw_row(
        &format!("{ACCENT_PINK}>> [cerrar vista r\u{00e1}pida (Esc)]{RESET}"),
        &format!("{TEXT_DIM}\u{2191}/\u{2193} scroll{RESET}"),
        width,
    ));
    bounds.push(CardBounds {
        id: "close_quick_peek",
        start_line: close_row_idx,
        end_line: close_row_idx,
        action_type: Some("close_quick_peek"),
        action_path: None,
    });
    lines.push(draw_divider(width));

    let rows = max_content_rows.clamp(4, 24);
    let total = peek.lines.len();
    let start = peek.scroll.min(total.saturating_sub(1));
    for (idx, dl) in peek.lines.iter().skip(start).take(rows).enumerate() {
        let line_no = start + idx + 1;
        let colored_dl = if dl.starts_with('[') {
            format!("{AMBER}{dl}{RESET}")
        } else if dl.starts_with("@@") {
            format!("{CYAN}{dl}{RESET}")
        } else if dl.starts_with('+') {
            format!("{MINT}{dl}{RESET}")
        } else if dl.starts_with('-') {
            format!("{CORAL}{dl}{RESET}")
        } else {
            format!("{TEXT_PRIMARY}{dl}{RESET}")
        };
        let numbered = format!("{TEXT_DIM}{line_no:>3} \u{2502}{RESET} {colored_dl}");
        lines.push(draw_row(&numbered, "", width));
    }

    lines.push(draw_divider(width));
    let end_line_no = (start + rows).min(total);
    let status_left = if total == 0 {
        format!("{TEXT_DIM}Sin contenido{RESET}")
    } else {
        format!(
            "{TEXT_DIM}L\u{00ed}neas {}-{} de {}{RESET}",
            start + 1,
            end_line_no,
            total
        )
    };
    lines.push(draw_row(
        &status_left,
        &format!("{CYAN}[Espacio/Esc]{RESET}"),
        width,
    ));
    lines.push(draw_bottom(width));

    RenderOutput { lines, bounds }
}

/// Superpone el modal flotante Quick Peek centrado sobre el viewport activo sin deformar las tarjetas de fondo.
pub fn apply_quick_peek_overlay(
    base: &mut RenderOutput,
    peek: &crate::state::QuickPeekState,
    scroll_offset: usize,
    width: usize,
    viewport_rows: usize,
) {
    let content_rows = viewport_rows.saturating_sub(8).clamp(4, 18);
    let modal = render_quick_peek_modal(peek, width, content_rows);
    let modal_h = modal.lines.len();

    let needed_total = scroll_offset + viewport_rows.max(modal_h + 2);
    while base.lines.len() < needed_total {
        base.lines.push(" ".repeat(width));
    }

    let top_pad = viewport_rows.saturating_sub(modal_h) / 2;
    let insert_start = scroll_offset + top_pad;

    for (i, m_line) in modal.lines.into_iter().enumerate() {
        if insert_start + i < base.lines.len() {
            base.lines[insert_start + i] = m_line;
        } else {
            base.lines.push(m_line);
        }
    }

    base.bounds = modal
        .bounds
        .into_iter()
        .map(|mut b| {
            b.start_line += insert_start;
            b.end_line += insert_start;
            b
        })
        .collect();
}

#[cfg(test)]
mod ansi_tests {
    use super::*;

    #[test]
    fn strip_ansi_sin_escapes_devuelve_igual() {
        assert_eq!(strip_ansi("hola mundo"), "hola mundo");
    }

    #[test]
    fn strip_ansi_elimina_csi_sgr_y_truecolor() {
        assert_eq!(strip_ansi("\x1b[38;2;10;20;30mA\x1b[0mB"), "AB");
    }

    #[test]
    fn strip_ansi_elimina_csi_con_byte_final_no_alfabetico() {
        // `ESC [ 2 ~` (Insert) y `ESC [ @` usan bytes finales no alfabéticos
        assert_eq!(strip_ansi("a\x1b[2~b\x1b[@c"), "abc");
    }

    #[test]
    fn strip_ansi_elimina_osc_con_bel_y_st() {
        assert_eq!(strip_ansi("\x1b]0;titulo\x07X"), "X");
        assert_eq!(
            strip_ansi("\x1b]8;;https://x.y\x1b\\link\x1b]8;;\x1b\\"),
            "link"
        );
    }

    #[test]
    fn strip_ansi_tolera_secuencia_incompleta_y_unicode() {
        assert_eq!(strip_ansi("ñ\x1b[31"), "ñ");
        assert_eq!(strip_ansi("\x1bé"), "");
    }

    #[test]
    fn string_width_ignora_escapes() {
        assert_eq!(string_width("\x1b[31m\u{25cf} ok\x1b[0m"), 4);
    }

    #[test]
    fn truncate_preserva_escapes_y_respeta_ancho() {
        let out = truncate_to_width("\x1b[31mabcdef\x1b[0m", 3);
        assert_eq!(strip_ansi(&out), "abc");
        assert!(out.starts_with("\x1b[31m"));
        assert!(out.ends_with(RESET));
        assert_eq!(truncate_to_width("abc", 0), "");
    }
}
