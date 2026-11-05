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
    collapsed: &[bool; 9],
    flash_notice: Option<&str>,
    width: usize,
) -> RenderOutput {
    let mut lines = Vec::new();
    let mut bounds = Vec::new();

    let folder = std::path::Path::new(&snap.project_dir)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("proyecto");

    // 1. Header Linear (Línea 0 fija siempre)
    let banner_raw = ":: HUDR2 \u{00b7} GENTLEMAN ::";
    let banner_pad = width.saturating_sub(string_width(banner_raw)) / 2;
    lines.push(format!("{}{ACCENT_PRIMARY}:: HUDR2{RESET} {BORDER}\u{00b7} {GOLD}GENTLEMAN {ACCENT_PRIMARY}::{RESET}", " ".repeat(banner_pad)));

    // 2. Estado Card (0)
    let estado_start = lines.len();
    let is_working = snap.opencode.agent_status == "working";
    let petal = if is_working {
        format!("{BOLD}{AMBER}>> WORKING{RESET}")
    } else {
        format!("{TEXT_DIM}-- idle{RESET}")
    };
    let estado_title = format!("{BOLD}{GOLD}Estado{RESET} {TEXT_DIM}\u{00b7}{RESET} {petal}");
    lines.push(draw_top(&estado_title, collapsed[0], width));
    if !collapsed[0] {
        let short_proj = if folder.len() > 18 {
            format!("{}…", &folder[..16])
        } else {
            folder.to_string()
        };
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
        lines.push(draw_row(
            &format!("{GOLD}Modelo{RESET}"),
            &format!("{ACCENT_PRIMARY}{}{RESET}", snap.opencode.model_short),
            width,
        ));

        let active_prof_labels = if let Some(ref lp) = snap.opencode.local_prefix {
            format!("{ACCENT_PINK}*{RESET} {MINT}{lp}{RESET} {ACCENT_PINK}*{RESET}")
        } else if snap.opencode.all_active_prefixes.is_empty() {
            format!("{TEXT_MUTED}default{RESET}")
        } else {
            snap.opencode
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

    // 3. Contexto Card (1)
    let ctx_start = lines.len();
    lines.push(draw_top(
        &format!("{BOLD}{GOLD}Contexto{RESET}"),
        collapsed[1],
        width,
    ));
    if !collapsed[1] {
        let cur_tokens = snap.opencode.active_turn_tokens;
        let limit = snap.opencode.context_limit.max(1);
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

        let in_t = snap.opencode.tokens_input;
        let out_t = snap.opencode.tokens_output;
        let left_io = format!(
            "{TEXT_DIM}^{RESET} {TEXT_PRIMARY}{}{RESET} {TEXT_DIM}\u{00b7}{RESET} {TEXT_DIM}v{RESET} {TEXT_PRIMARY}{}{RESET}",
            format_tokens(in_t),
            format_tokens(out_t)
        );
        let right_cost = format!(
            "{GOLD}Costo{RESET} {TEXT_PRIMARY}${:.3}{RESET}",
            snap.opencode.cost
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

    // 4. Proyecto Card (Árbol 2 niveles) (2)
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
        let normalized_path = snap.tree.root_path.replace('\\', "/");
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
            action_path: Some(snap.tree.root_path.clone()),
        });

        for item in snap.tree.tree.iter().take(4) {
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
    }
    lines.push(draw_bottom(width));
    bounds.push(CardBounds {
        id: "proyecto",
        start_line: proj_start,
        end_line: lines.len() - 1,
        action_type: None,
        action_path: None,
    });

    // 5. Integraciones Card (3)
    let int_start = lines.len();
    let count_str = format!("{TEXT_DIM}{} cuentas{RESET}", snap.accounts.len());
    let critical_count = snap.accounts.iter().filter(|a| a.is_critical).count();
    let int_title = if critical_count > 0 {
        format!(
            "{GOLD}Integraciones{RESET} {TEXT_DIM}\u{00b7}{RESET} {BOLD}{CORAL}! {critical_count} cr\u{00ed}tic{RESET}"
        )
    } else {
        format!("{GOLD}Integraciones{RESET} {TEXT_DIM}\u{00b7}{RESET} {count_str}")
    };
    lines.push(draw_top(&int_title, collapsed[3], width));
    if !collapsed[3] {
        for acc in &snap.accounts {
            let is_local = snap
                .opencode
                .local_prefix
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case(&acc.prefix))
                .unwrap_or(false);
            let is_any_active = snap
                .opencode
                .all_active_prefixes
                .contains(&acc.prefix.to_lowercase());

            let (status_glyph, pct_color, right_text) = if acc.has_error {
                (
                    format!("{AMBER}?{RESET}"),
                    AMBER,
                    format!("{AMBER}[{}]{RESET}", acc.error_msg),
                )
            } else if acc.is_critical {
                (
                    format!("{BOLD}{CORAL}!{RESET}"),
                    CORAL,
                    format!("{BOLD}{CORAL}[AGOT\u{00c1}NDOSE]{RESET}"),
                )
            } else if acc.is_warning {
                (
                    format!("{BOLD}{AMBER}!{RESET}"),
                    AMBER,
                    format!("{AMBER}[AVISO 70%]{RESET}"),
                )
            } else if is_local {
                (
                    format!("{ACCENT_PINK}>{RESET}"),
                    MINT,
                    format!("{TEXT_DIM}en {}{RESET}", acc.reset_5h),
                )
            } else if is_any_active {
                (
                    format!("{ACCENT_PRIMARY}\u{25cf}{RESET}"),
                    MINT,
                    format!("{TEXT_DIM}en {}{RESET}", acc.reset_5h),
                )
            } else {
                (
                    format!("{MINT}\u{25cf}{RESET}"),
                    MINT,
                    format!("{TEXT_DIM}en {}{RESET}", acc.reset_5h),
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
            let bar = render_gauge_inline(acc.rem_5h, 3, pct_color);
            let pct_5h = format!("{BOLD}{pct_color}{:>3}%{RESET}", acc.rem_5h);
            let wk_color = if acc.is_critical {
                CORAL
            } else if acc.is_warning {
                AMBER
            } else {
                MINT
            };
            let pct_wk = format!(
                "{TEXT_DIM}Wk:{RESET}{BOLD}{wk_color}{:>3}%{RESET}",
                acc.rem_wk
            );

            let left = format!("{status_glyph} {pfx_disp} {bar} {pct_5h} {pct_wk}");
            lines.push(draw_row(&left, &right_text, width));
        }
        lines.push(draw_divider(width));
        let sync_line_idx = lines.len();
        lines.push(draw_row(
            &format!("{CYAN}>> [sincronizar]{RESET}"),
            "",
            width,
        ));
        bounds.push(CardBounds {
            id: "sync_integrations",
            start_line: sync_line_idx,
            end_line: sync_line_idx,
            action_type: Some("sync_integrations"),
            action_path: None,
        });
    }

    // Alertas de cuentas críticas activas + Flash notice dentro de Integraciones
    let target_healthy = crate::cleanup::get_best_healthy_target_prefix(
        &snap.accounts,
        &snap.opencode.all_active_prefixes,
    );
    let active_critical_accs: Vec<&crate::models::AccountQuota> = snap
        .accounts
        .iter()
        .filter(|a| {
            (a.is_critical || (100u8.saturating_sub(a.rem_5h) >= 85 && !a.has_error))
                && snap
                    .opencode
                    .all_active_prefixes
                    .contains(&a.prefix.to_lowercase())
        })
        .collect();

    if !active_critical_accs.is_empty() || flash_notice.is_some() {
        lines.push(draw_divider(width));
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

    // 6. Pool de Modelos / Cuotas Breakdown Card (4)
    let pools_start = lines.len();
    let default_prefix = snap
        .accounts
        .first()
        .map(|a| a.prefix.as_str())
        .unwrap_or("local");
    let active_acc_prefix = snap
        .opencode
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
    if !collapsed[4]
        && let Some(acc) = target_account
    {
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
    lines.push(draw_bottom(width));
    bounds.push(CardBounds {
        id: "pools",
        start_line: pools_start,
        end_line: lines.len() - 1,
        action_type: None,
        action_path: None,
    });

    // 7. Cambios / Git Card (5)
    let git_start = lines.len();
    let ag_changes = format!(
        "{TEXT_DIM}\u{00b7} {} por agente{RESET}",
        snap.opencode.attributed_changes.files_count
    );
    lines.push(draw_top(
        &format!("{BOLD}{GOLD}Cambios{RESET} {ag_changes}"),
        collapsed[5],
        width,
    ));
    if !collapsed[5] {
        let clean_st = if snap.git.is_clean {
            format!("{MINT}ok limpio{RESET}")
        } else {
            format!("{AMBER}modificado{RESET}")
        };
        lines.push(draw_row(
            &format!("{CYAN}{}{RESET}", snap.git.branch),
            &clean_st,
            width,
        ));
        lines.push(draw_row(
            &format!("{TEXT_DIM}* (sin commits recientes){RESET}"),
            "",
            width,
        ));
        let git_part = format!("{TEXT_DIM}git:{RESET} {MINT}limpio{RESET}");
        let ag_part = format!(
            "{ACCENT_PRIMARY}ag:{RESET} {MINT}{} archivos (+){RESET}",
            snap.opencode.attributed_changes.files_count
        );
        lines.push(draw_row(&git_part, &ag_part, width));
    }
    lines.push(draw_bottom(width));
    bounds.push(CardBounds {
        id: "git",
        start_line: git_start,
        end_line: lines.len() - 1,
        action_type: None,
        action_path: None,
    });

    // 8. Engram Card (6)
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
        let cloud_row = match (&snap.engram.cloud_host, snap.engram.enrolled) {
            (Some(host), true) => format!("Cloud: {host} (inscrito)"),
            (Some(host), false) => format!("Cloud: {host} (no inscrito)"),
            (None, true) => "Cloud: proyecto inscrito para sync".to_string(),
            (None, false) => "Cloud: solo local o no configurado".to_string(),
        };
        lines.push(draw_row(&cloud_row, "", width));
    }
    lines.push(draw_bottom(width));
    bounds.push(CardBounds {
        id: "engram",
        start_line: engram_start,
        end_line: lines.len() - 1,
        action_type: None,
        action_path: None,
    });

    // 9. Herramientas Card (7)
    let tools_start = lines.len();
    let tool_title = format!(
        "{BOLD}{GOLD}Herramientas{RESET} {TEXT_DIM}\u{00b7} {} llamadas{RESET}",
        snap.opencode.tool_total
    );
    lines.push(draw_top(&tool_title, collapsed[7], width));
    if !collapsed[7] {
        let p_read = format!("{MAGENTA}@ {} lecturas{RESET}", snap.opencode.tool_reads);
        let p_write = format!("{CYAN}@ {} escritu{RESET}", snap.opencode.tool_writes);
        let p_mem = format!("{CORAL}mem: {}{RESET}", snap.opencode.tool_engram);
        lines.push(draw_row(&format!("{p_read}  {p_write}"), &p_mem, width));
    }
    lines.push(draw_bottom(width));
    bounds.push(CardBounds {
        id: "tools",
        start_line: tools_start,
        end_line: lines.len() - 1,
        action_type: None,
        action_path: None,
    });

    // 10. MCP Card (8)
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

    // Footer de atajos
    lines.push(format!(
        "{TEXT_DIM} 1-9: colap \u{00b7} c: todo \u{00b7} d: liberar \u{00b7} o: abrir{RESET}"
    ));
    let has_active_critical = snap.accounts.iter().any(|a| {
        (a.is_critical || (100u8.saturating_sub(a.rem_5h) >= 85 && !a.has_error))
            && snap
                .opencode
                .all_active_prefixes
                .contains(&a.prefix.to_lowercase())
    });
    let critical_action = if has_active_critical {
        format!("{CORAL}x: migrar ({target_healthy}){TEXT_DIM} \u{00b7} {RESET}")
    } else {
        String::new()
    };
    lines.push(format!(
        "{TEXT_DIM} s: sync \u{00b7} r: act \u{00b7} {critical_action}q: salir{RESET}"
    ));

    RenderOutput { lines, bounds }
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
