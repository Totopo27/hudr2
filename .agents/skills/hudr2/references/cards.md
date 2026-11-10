# HUDR2 Card Catalog

`hudr2` renders 11 collapsible cards (toggled via keys `1..9`, `e`, `0`/`t`, `c` for all, or clicking the card's top border) plus a top banner and a 2-line shortcut footer (`0-9: cards · e: stack · c: todo · d: lib` and `s: sync · r: act · a: auditar · [x: migrar ·] q: salir`).

| Index | Key | `CardBounds.id` | Title in `src/ui.rs` | Default State |
| :---: | :---: | :--- | :--- | :--- |
| `0` | `1` | `estado` | `Estado` | Expanded |
| `1` | `2` | `contexto` | `Contexto` | Expanded |
| `2` | `3` | `proyecto` | `~ Proyecto` | **Collapsed** |
| `3` | `4` | `integraciones` | `Integraciones` | Expanded |
| `4` | `5` | `pools` | `Cuota de Modelos` | Expanded |
| `5` | `6` | `git` | `Cambios` | Expanded |
| `6` | `7` | `engram` | `Engram` | Expanded |
| `7` | `8` | `tools` | `Herramientas` | Expanded |
| `8` | `9` | `mcp` | `MCP` | Expanded |
| `9` | `e` | `ecosystem` | `Stack · Ecosistema` | Expanded |
| `10` | `0` / `t` | `personalizar` | `Personalizar` | **Collapsed** |

---

## 0. `Estado` (`id: "estado"`, index `0`)
- **Header:** `Estado · ● trabajando` (cyan), `-- idle` (muted), `! bloqueado` (coral), or `+ listo` (mint).
- **Rows:**
  - `Proyecto`: shortened workspace path (`~/.../name`).
  - `Rama`: `# <branch> ±<dirty_count>` (or `sin git`).
  - `Modelo`: active model short name (`snap.agent_session.model_short`) + optional `[thinking_level]`.
  - Footer summary row: `mem: <project> · <N> MCPs · <agent_or_provider>`.
  - Optional `Tareas` preview when `snap.agent_session.todos` is non-empty (`✓`, `◉`, `○`).

## 1. `Contexto` (`id: "contexto"`, index `1`)
- **Row 1:** `<active_turn_tokens> / <context_limit>` on the left; status badge on the right:
  - `< 50%`: `● Optimo N%` (`MINT`)
  - `50%..80%`: `● Maduro N% [/snew]` (`AMBER`)
  - `>= 80%`: `● Saturado N% [/snew]` (`CORAL`)
- **Row 2:** Full-width inline progress bar (`━` / `─`).
- **Row 3:** `^ <tokens_input> · v <tokens_output>` and `Costo $<cost>`.
- **Row 4 (when `>= 50%`):** `[aviso] Sesion madura: usa /snew para handover limpio`.

## 2. `~ Proyecto` (`id: "proyecto"`, index `2`)
- **Header:** `~ Proyecto · <root_name> <dirs>d · <files>f`.
- **Action row (`id: "open_folder"`):** `>> [abrir en PC ->]` (never truncated even at width 36; clicking opens the project folder in the OS file explorer).
- **Tree rows:** Directories first (`+ name/`), then files (`  name`), alphabetical case-insensitive, skipped when card `2` is collapsed.

## 3. `Integraciones` (`id: "integraciones"`, index `3`)
- **Header:** `Integraciones · <N> cuentas` or `! <N> crític` (`CORAL`) when any account is critical for the **active session's model family**.
- **Account rows:** Evaluates each account via `crate::quotas::evaluate_account_for_model(acc, active_model)`:
  - Supports OAuth accounts (`auths/*.json`) and API key accounts (`config.yaml` `api-keys`, `opencode.json` `providers.<proxy>.models`, `profiles.json` orchestrator models).
  - Glyph: `>` (`ACCENT_PINK`) if `local_prefix`, `●` (`ACCENT_PRIMARY`) if active in another pane, `●` (`MINT`) if idle, `!` (`AMBER`/`CORAL`) if warning/critical, `?` (`AMBER`) if HTTP error.
  - For OAuth accounts: inline `5h` gauge + `rem_5h%` + `Wk:rem_wk%` + `en <reset_5h>` (or `[AGOTÁNDOSE]` / `[AVISO 70%]`).
  - For API key accounts: inline gauge reflecting remaining balance percentage + `$XX.XX disp` (balance remaining from $50.00 default or configured budget) + `[api-key]` (or `[SALDO AGOTADO]` when `<= 8%` / `[AVISO SALDO]` when `<= 20%`). Bypasses HTTP quota network sweeps.
- **Alert block:** Shows `flash_notice` (when active) and `! CRÍTICO: <prefix> (activa) [presioná 'x' -> migrar a <target>]` (proxy config sync `s: sync` lives exclusively in the shortcut footer to avoid UX confusion with quota refresh).

## 4. `Cuota de Modelos` (`id: "pools"`, index `4`)
- **Header:** `Cuota de Modelos · <active_acc_prefix> *` (scoped to `snap.agent_session.local_prefix`, fallback to first account).
- **Pool rows:**
  - For OAuth accounts: renders every `QuotaPool` in `target_account.pools` (`Gemini Wk`, `Gemini 5h`, `Claude Wk`, `Claude 5h`, plus dynamic `Argon`, `GPT`, `DeepSeek`, `Qwen` pools) with traffic-light thresholds (`CORAL` `<= 15%`, `AMBER` `<= 50%`, `MINT` `> 50%`).
  - For API key accounts: renders truthful financial pools (`<Provider> API` with active session cost `$X.XXX` and `<Provider> Saldo` with remaining balance `$XX.XX`) with financial traffic-light thresholds (`CORAL` `<= 8%`, `AMBER` `<= 20%`, `MINT` `> 20%`).

## 5. `Cambios` (`id: "git"`, index `5`)
- **Header:** `Cambios · <N> por agente` (or `Cambios ! <N> colisión` in `CORAL` when `cross_session_conflicts` is non-empty).
- **Rows:**
  - Optional conflict radar alert (`! CONFLICTO: <files>` + `[colisión]` in `CORAL`) when two or more active sessions/worktrees modify the same tracked file concurrently.
  - `<branch>` (+ optional `▲ahead▼behind`) and `ok limpio` / `modificado`.
  - Latest commit `* <hash> <subject>` (with zero-subprocess fallback to `* <head_short> HEAD` read from `.git/HEAD` / `refs/` / `packed-refs`).
  - `git: +<added> -<deleted>` (or `limpio` / `<N> pend`) and `ag: <N> archivos (+)`.
  - Up to 6 clickable file rows (`action_type: Some("git_file_diff")`): `> [M|A|D|?] <path>` with `+<added> -<deleted>`.
  - Clicking a file toggles an inline diff preview underneath (guarded by `is_sensitive_file`) plus an `>> [vista rápida (Espacio)]` row (`action_type: Some("open_quick_peek")`) that opens the centered floating **Quick Peek** modal (`Space` / `p` to toggle, `↑`/`↓` or wheel to scroll, `Esc` or click header to close).

## 6. `Engram` (`id: "engram"`, index `6`)
- **Header:** `Engram: <project> ^` (`^` green if daemon `:7437` is online, `v` dim if offline).
- **Rows:**
  - `Local (engram.db)` + `● En linea · <N> obs` (or `Desconectado`).
  - `Cloud: <sync_status>`.
  - Optional `Handoff #<id> (<updated_at>)` block showing extracted lines from the latest `session_summary` observation.

## 7. `Stack · Ecosistema` (`id: "ecosystem"`, index `7`)
- **Header:** `Stack · Ecosistema` (or `Stack · Ecosistema ↑<N>` in `AMBER` when updates are available).
- **Rows:** One row per tool (`pi`, `hudr2`, `gentle-shell`, `gentle-ai`, `engram`, `herdr`):
  - Up-to-date: `● <name> <version>` (`MINT`).
  - Update available: `● <name> <version> ↑<upstream>` (`AMBER`) accompanied by an interactive clickable `[actualizar]` button (or adaptive `[↑]` when `width < 44`) mapped to `action_type: Some("update_tool")`. For `hudr2` itself, renders `[asistido]` and displays non-blocking instructions in flash notice to prevent Windows executable file-locking (`os error 5`) and CPU compilation spikes.
  - In progress: Displays dynamic Braille spinner frame and real-time percent `[⠋ 45%]` alongside responsive `[actualizando]` or `[↑]` while the background worker runs the official upgrade command non-blockingly, with flash notice updates and live TUI repaints.
  - Missing/Not installed: `○ <name> -` or `○ <name> - (disp <upstream>)` (`TEXT_DIM`).
- **Upstream Cache:** Cached in `upstream_cache.json` with 3-hour TTL, queried in background via GitHub Releases API (`/repos/{owner}/{repo}/releases/latest`).

## 8. `Herramientas` & `MCP` (`id: "tools"` & `id: "mcp"`, index `8`)
- Both cards share collapse slot `collapsed[8]` (key `9`).
- **`Herramientas · <total> llamadas`:** `@ <reads> lecturas`, `@ <writes> escritu`, `mem: <engram>`, and optional `$ <bash> cmds` / `subagentes: <N>`.
- **`MCP · <N> activos`:** Lists deduplicated MCP servers (`● <name> [local|remote] en línea`).

## 10. `Personalizar` (`id: "personalizar"`, index `10`, key `0` / `t`)
- **Header:** `Personalizar` (collapsed by default).
- **Rows:**
  - One interactive checkbox row per configurable card (`Estado`, `Contexto`, `Proyecto`, `Integraciones`, `Cuota de Modelos`, `Cambios (Git)`, `Engram`, `Herramientas`, `MCP`, `Stack · Ecosistema`).
  - `[x] <Nombre>` with `visible` (`MINT`) or `[ ] <Nombre>` with `oculta` (`TEXT_DIM`). Built using ASCII checkboxes `[x]` to prevent missing glyph boxes `[?]` across non-NerdFont Windows and Linux consoles.
  - Clicking any card row toggles its visibility in real time; cards marked `oculta` are completely skipped from rendering (saving vertical space).
  - Redundant fixed row was eliminated to prevent vertical waste (`personalizar` can never hide itself).
  - **Divider & Settings section:**
    - `Nombre  <nombre>` with interactive `[editar >]` button (`action_type: Some("edit_sidebar_name")`, key `n`, or double click on `Personalizar` card). Allows in-place editing of top banner `:: <nombre> ::` with live buffer (`> <buffer>_`), saving on `Enter` and canceling on `Esc`. Entering empty or `HUDR2` restores default.
    - `Tema    <nombre_tema>` with clickable `[cambiar >]` (`action_type: Some("cycle_theme")`, key `m`), dynamically cycling between `Auto (Herdr · <id>)`, 5 native palettes, and 14 Herdr themes.
    - `Updates Check online` with clickable `[x] activo (3h)` / `[ ] apagado` (`action_type: Some("toggle_check_updates")`, key `u`), controlling online version checks.
  - State persisted atomically in `hudr2.json` (`{"collapsed": [...], "hidden": [...], "theme": "...", "sidebar_name": "...", "check_updates": true}`).
