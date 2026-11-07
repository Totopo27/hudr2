use crate::config::{
    ACCENT_PINK, ACCENT_PRIMARY, AMBER, BOLD, BORDER, BORDER_MUTED, CORAL, CYAN, GOLD, MAGENTA,
    MINT, RESET, TEXT_DIM, TEXT_MUTED, TEXT_PRIMARY,
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
    let icon = if collapsed { "+" } else { "-" };
    let icon_badge = format!("{TEXT_DIM}[{RESET}{TEXT_MUTED}{icon}{RESET}{TEXT_DIM}]{RESET}");
    let clean_title = if string_width(title) > width.saturating_sub(12) {
        truncate_to_width(title, width.saturating_sub(12))
    } else {
        title.to_string()
    };
    let t_len = string_width(&clean_title);
    let dash_count = width.saturating_sub(10 + t_len);
    format!(
        "{BORDER}\u{256d}\u{2500} {clean_title} {}\u{2500}{icon_badge} {BORDER}\u{256e}{RESET}",
        "\u{2500}".repeat(dash_count)
    )
}

pub fn draw_bottom(width: usize) -> String {
    format!(
        "{BORDER}\u{2570}{}\u{256f}{RESET}",
        "\u{2500}".repeat(width.saturating_sub(2))
    )
}

pub fn draw_divider(width: usize) -> String {
    format!(
        "{BORDER_MUTED}\u{251c}{}\u{2524}{RESET}",
        "\u{2500}".repeat(width.saturating_sub(2))
    )
}

pub fn draw_row(left: &str, right: &str, width: usize) -> String {
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
    format!("{BORDER}\u{2502}{RESET} {clean_left}{pad}{clean_right} {BORDER}\u{2502}{RESET}")
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

pub fn render_screen(
    snap: &TelemetrySnapshot,
    collapsed: &[bool; 11],
    flash_notice: Option<&str>,
    width: usize,
) -> RenderOutput {
    let empty = std::collections::HashSet::new();
    render_screen_with_diff(snap, collapsed, flash_notice, None, &[], &empty, width)
}

pub fn render_screen_with_diff(
    snap: &TelemetrySnapshot,
    collapsed: &[bool; 11],
    flash_notice: Option<&str>,
    selected_diff_file: Option<&str>,
    selected_diff_lines: &[String],
    hidden_cards: &std::collections::HashSet<String>,
    width: usize,
) -> RenderOutput {
    let mut lines = Vec::new();
    let mut bounds = Vec::new();

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
    let banner_raw = ":: HUDR2 ::";
    let banner_pad = width.saturating_sub(string_width(banner_raw)) / 2;
    lines.push(format!("{}{ACCENT_PRIMARY}:: HUDR2 ::{RESET}", " ".repeat(banner_pad)));

    // 2. Estado Card (0)
    if !hidden_cards.contains("estado") {
        let estado_start = lines.len();
        let is_working = snap.agent_session.agent_status == "working";
        let petal = if is_working {
            format!("{BOLD}{AMBER}>> WORKING{RESET}")
        } else {
            format!("{TEXT_DIM}-- idle{RESET}")
        };
        let estado_title = format!("{BOLD}{GOLD}Estado{RESET} {TEXT_DIM}\u{00b7}{RESET} {petal}");
        lines.push(draw_top(&estado_title, collapsed[0], width));
        if !collapsed[0] {
            let short_proj = truncate_plain(folder, 18);
            lines.push(draw_row(
                &format!("{GOLD}Proyecto{RESET}"),
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
                &format!("{GOLD}Rama{RESET}"),
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
                    "{ACCENT_PRIMARY}{}{RESET} {TEXT_MUTED}[{lvl}]{RESET}",
                    snap.agent_session.model_short
                ),
                None => format!("{ACCENT_PRIMARY}{}{RESET}", snap.agent_session.model_short),
            };
            lines.push(draw_row(&format!("{GOLD}Modelo{RESET}"), &model_disp, width));

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
                lines.push(draw_row(&format!("{GOLD}Tareas{RESET}"), &right_todo, width));
            }

            let active_prof_labels = if let Some(ref lp) = snap.agent_session.local_prefix {
                format!("{ACCENT_PINK}*{RESET} {MINT}{lp}{RESET} {ACCENT_PINK}*{RESET}")
            } else if snap.agent_session.all_active_prefixes.is_empty() {
                format!("{TEXT_MUTED}default{RESET}")
            } else {
                snap.agent_session
                    .all_active_prefixes
                    .iter()
                    .map(|p| format!("{ACCENT_PINK}*{RESET} {MINT}{p}{RESET} {ACCENT_PINK}*{RESET}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let sub_vitals = format!(
                "{ACCENT_PRIMARY}mem:{RESET} {TEXT_PRIMARY}{folder}{RESET} {TEXT_DIM}\u{00b7}{RESET} {TEXT_MUTED}{} MCPs{RESET} {TEXT_DIM}\u{00b7}{RESET} {active_prof_labels}",
                snap.mcps.len()
            );
            lines.push(draw_row(
                &sub_vitals,
                &format!("{ACCENT_PINK}>{RESET}"),
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
            &format!("{BOLD}{GOLD}Contexto{RESET}"),
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
                "{GOLD}Costo{RESET} {TEXT_PRIMARY}${:.3}{RESET}",
                snap.agent_session.cost
            );
            lines.push(draw_row(&left_io, &right_cost, width));

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
            "{ACCENT_PRIMARY}~{RESET} {BOLD}{GOLD}Proyecto{RESET} {TEXT_DIM}\u{00b7}{RESET} {CYAN}{folder}{RESET} {stats_badge}"
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
            let btn_label = format!("{GOLD}Ruta{RESET} {MINT}[abrir en PC ->]{RESET}");
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
            "{GOLD}Integraciones{RESET} {TEXT_DIM}\u{00b7}{RESET} {BOLD}{CORAL}! {critical_count} cr\u{00ed}tic{RESET}"
        )
    } else {
        format!("{GOLD}Integraciones{RESET} {TEXT_DIM}\u{00b7}{RESET} {count_str}")
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

                let (status_glyph, pct_color, right_text) = if acc.has_error {
                    (
                        format!("{AMBER}?{RESET}"),
                        AMBER,
                        format!("{AMBER}[{}]{RESET}", acc.error_msg),
                    )
                } else if eff.is_critical {
                    (
                        format!("{BOLD}{CORAL}!{RESET}"),
                        CORAL,
                        format!("{BOLD}{CORAL}[AGOT\u{00c1}NDOSE]{RESET}"),
                    )
                } else if eff.is_warning {
                    (
                        format!("{BOLD}{AMBER}!{RESET}"),
                        AMBER,
                        format!("{AMBER}[AVISO 70%]{RESET}"),
                    )
                } else if is_local {
                    (
                        format!("{ACCENT_PINK}>{RESET}"),
                        MINT,
                        format!("{TEXT_DIM}en {}{RESET}", eff.reset_5h),
                    )
                } else if is_any_active {
                    (
                        format!("{ACCENT_PRIMARY}\u{25cf}{RESET}"),
                        MINT,
                        format!("{TEXT_DIM}en {}{RESET}", eff.reset_5h),
                    )
                } else {
                    (
                        format!("{MINT}\u{25cf}{RESET}"),
                        MINT,
                        format!("{TEXT_DIM}en {}{RESET}", eff.reset_5h),
                    )
                };

                let pfx_clean = format!("{:<7}", acc.prefix);
                let pfx_disp = if is_local {
                    format!("{BOLD}{TEXT_PRIMARY}{pfx_clean}{RESET}")
                } else if is_any_active {
                    format!("{TEXT_PRIMARY}{pfx_clean}{RESET}")
                } else {
                    format!("{TEXT_MUTED}{pfx_clean}{RESET}")
                };
                let bar = render_gauge_inline(eff.rem_5h, 3, pct_color);
                let pct_5h = format!("{BOLD}{pct_color}{:>3}%{RESET}", eff.rem_5h);
                let wk_color = if eff.is_critical {
                    CORAL
                } else if eff.is_warning {
                    AMBER
                } else {
                    MINT
                };
                let pct_wk = format!(
                    "{TEXT_DIM}Wk:{RESET}{BOLD}{wk_color}{:>3}%{RESET}",
                    eff.rem_wk
                );

                let left = format!("{status_glyph} {pfx_disp} {bar} {pct_5h} {pct_wk}");
                lines.push(draw_row(&left, &right_text, width));
            }
        }
    }

    // Alertas de cuentas críticas activas + Flash notice dentro de Integraciones
    let active_critical_accs: Vec<&crate::models::AccountQuota> = snap
        .accounts
        .iter()
        .filter(|a| {
            let eff = crate::quotas::evaluate_account_for_model(a, active_model);
            (eff.is_critical || (100u8.saturating_sub(eff.rem_5h) >= 85 && !a.has_error))
                && snap
                    .agent_session
                    .all_active_prefixes
                    .contains(&a.prefix.to_lowercase())
        })
        .collect();

    if !active_critical_accs.is_empty() || flash_notice.is_some() {
        if !collapsed[3] {
            lines.push(draw_divider(width));
        }
        if let Some(notice) = flash_notice {
            lines.push(draw_row(&format!("{BOLD}{MINT}{notice}{RESET}"), "", width));
        }
        for ca in active_critical_accs {
            let action_label = format!("[presion\u{00e1} 'x' -> migrar a {target_healthy}]");
            lines.push(draw_row(
                &format!(
                    "{BOLD}{CORAL}! CR\u{00cd}TICO: {} (activa) {action_label}{RESET}",
                    ca.prefix
                ),
                "",
                width,
            ));
        }
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

    // 6. Pool de Modelos / Cuotas Breakdown Card (4)
    if !hidden_cards.contains("pools") {
        let pools_start = lines.len();
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

    // Cuenta activa contextual para este espacio de trabajo específico
    let target_account = snap
        .accounts
        .iter()
        .find(|a| a.prefix.to_lowercase() == active_acc_prefix.to_lowercase())
        .or_else(|| snap.accounts.first());

    let pool_title = format!(
        "{GOLD}Cuota de Modelos{RESET} {TEXT_DIM}\u{00b7}{RESET} {MINT}{active_acc_prefix} *{RESET}"
    );

    lines.push(draw_top(&pool_title, collapsed[4], width));
    if !collapsed[4] {
        if snap.accounts.is_empty() {
            lines.push(draw_row(
                &format!("{TEXT_DIM}Sin proveedor de cuotas activo{RESET}"),
                &format!("{TEXT_DIM}[inactivo]{RESET}"),
                width,
            ));
        } else if let Some(acc) = target_account {
            for p in &acc.pools {
                let pool_color = if p.percent <= 15 {
                    CORAL
                } else if p.percent <= 50 {
                    AMBER
                } else {
                    MINT
                };

                let bar = render_gauge_inline(p.percent, 6, pool_color);
                let pct_fmt = format!("{BOLD}{pool_color}{:>3}%{RESET}", p.percent);
                let p_label = format!("{:<9}", p.label);
                let left = format!(
                    "{pool_color}\u{25cf}{RESET} {TEXT_PRIMARY}{p_label}{RESET} {bar} {pct_fmt}"
                );
                let right = format!(
                    "{pool_color}{}{RESET} {TEXT_DIM}en {}{RESET}",
                    p.pace, p.reset
                );
                lines.push(draw_row(&left, &right, width));
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
        &format!("{BOLD}{GOLD}Cambios{RESET} {ag_changes}"),
        collapsed[5],
        width,
    ));
    if !collapsed[5] {
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

    // 8. Engram Card (6)
    if !hidden_cards.contains("engram") {
        let engram_start = lines.len();
    lines.push(draw_top(
        &format!("{ACCENT_PRIMARY}Engram:{RESET} {CYAN}{folder}{RESET} {TEXT_DIM}^{RESET}"),
        collapsed[6],
        width,
    ));
    if !collapsed[6] {
        let online = if snap.engram.online {
            format!("{MINT}\u{25cf} En linea{RESET}")
        } else {
            format!("{CORAL}\u{25cb} Offline{RESET}")
        };
        // El estado "en línea" se deriva de poder abrir engram.db (SQLite local),
        // no de un puerto HTTP, así que la etiqueta refleja la fuente real.
        lines.push(draw_row(
            "Local (engram.db)",
            &format!(
                "{online} {TEXT_DIM}\u{00b7}{RESET} {CYAN}{} obs{RESET}",
                snap.engram.obs_count
            ),
            width,
        ));
        let cloud_row = if snap.engram.enrolled {
            "Cloud: proyecto inscrito para sync"
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
                &format!("{GOLD}Handoff{RESET} {TEXT_DIM}#{} {RESET}", handoff.id),
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
            "{BOLD}{GOLD}Herramientas{RESET} {TEXT_DIM}\u{00b7} {} llamadas{RESET}",
            snap.agent_session.tool_total
        );
        lines.push(draw_top(&tool_title, collapsed[7], width));
        if !collapsed[7] {
            let p_read = format!("{MAGENTA}@ {} lecturas{RESET}", snap.agent_session.tool_reads);
            let p_write = format!("{CYAN}@ {} escritu{RESET}", snap.agent_session.tool_writes);
            let p_mem = format!("{CORAL}mem: {}{RESET}", snap.agent_session.tool_engram);
            lines.push(draw_row(&format!("{p_read}  {p_write}"), &p_mem, width));
            if snap.agent_session.tool_bash > 0 {
                let p_bash = format!("{GOLD}$ {} cmds{RESET}", snap.agent_session.tool_bash);
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

    // 9. MCP Card (idx 8)
    if !hidden_cards.contains("mcp") {
        let mcp_start = lines.len();
        lines.push(draw_top(
            &format!(
                "{BOLD}{GOLD}MCP{RESET} {TEXT_DIM}\u{00b7} {} activos{RESET}",
                snap.mcps.len()
            ),
            collapsed[8],
            width,
        ));
        if !collapsed[8] {
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
        let eco_title =
            format!("{BOLD}{GOLD}Stack{RESET} {TEXT_DIM}\u{00b7}{RESET} {GOLD}Ecosistema{RESET}");
        lines.push(draw_top(&eco_title, collapsed[9], width));
        if !collapsed[9] {
            if snap.ecosystem.tools.is_empty() {
                lines.push(draw_row(
                    &format!("{TEXT_DIM}escaneando ecosistema...{RESET}"),
                    "",
                    width,
                ));
            } else {
                for tool in &snap.ecosystem.tools {
                    let left = if tool.is_installed {
                        let ver_str = tool.version.as_deref().unwrap_or("");
                        format!(
                            "{MINT}\u{25cf}{RESET} {TEXT_PRIMARY}{}{RESET} {TEXT_MUTED}{}{RESET}",
                            tool.name, ver_str
                        )
                    } else {
                        format!("{TEXT_DIM}\u{25cb} {} -{RESET}", tool.name)
                    };
                    lines.push(draw_row(&left, "", width));
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

    // 11. Tarjetas Card (10) — Selector interactivo de visibilidad
    let tarjetas_start = lines.len();
    let tarjetas_title = format!("{BOLD}{GOLD}Tarjetas{RESET}");
    lines.push(draw_top(&tarjetas_title, collapsed[10], width));
    if !collapsed[10] {
        let cards_info: [(&str, &str); 10] = [
            ("estado", "Estado"),
            ("contexto", "Contexto"),
            ("proyecto", "Proyecto"),
            ("integraciones", "Integraciones"),
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
                    format!("{MINT}[\u{2713}]{RESET} {TEXT_PRIMARY}{label}{RESET}"),
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

        // Fila fija para la card de Tarjetas misma (no puede ocultarse)
        let fija_left = format!("{MINT}[\u{2713}]{RESET} {TEXT_PRIMARY}Tarjetas{RESET}");
        let fija_right = format!("{TEXT_DIM}fija{RESET}");
        lines.push(draw_row(&fija_left, &fija_right, width));
    }
    lines.push(draw_bottom(width));
    bounds.push(CardBounds {
        id: "tarjetas",
        start_line: tarjetas_start,
        end_line: lines.len() - 1,
        action_type: None,
        action_path: None,
    });

    // Footer de atajos
    lines.push(truncate_to_width(
        &format!(
            "{TEXT_DIM} 0-9: colap \u{00b7} t: tarjetas \u{00b7} c: todo \u{00b7} d: liberar \u{00b7} o: abrir{RESET}"
        ),
        width,
    ));
    let has_active_critical = snap.accounts.iter().any(|a| {
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
