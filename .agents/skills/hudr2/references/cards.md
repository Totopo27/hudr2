# HUDR2 Card Catalog

`hudr2` renders 9 collapsible cards (indexed `0..8`, toggled via keys `1..9`, `c` for all, or clicking the card's top border) plus a top banner and a 2-line shortcut footer.

| Index | Key | `CardBounds.id` | Title in `src/ui.rs` | Default State |
| :---: | :---: | :--- | :--- | :--- |
| `0` | `1` | `estado` | `Estado` | Expanded |
| `1` | `2` | `contexto` | `Contexto` | Expanded |
| `2` | `3` | `proyecto` | `~ Proyecto` | **Collapsed** |
| `3` | `4` | `integraciones` | `Integraciones` | Expanded |
| `4` | `5` | `pools` | `Cuota de Modelos` | Expanded |
| `5` | `6` | `git` | `Cambios` | Expanded |
| `6` | `7` | `engram` | `Engram` | Expanded |
| `7` | `8` | `ecosystem` | `Stack · Ecosistema` | Expanded |
| `8` | `9` | `tools` / `mcp` | `Herramientas` & `MCP` | Expanded |

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
- **Account rows:** Evaluates each account via `QuotaManager::evaluate_account_for_model(acc, active_model)`:
  - Glyph: `>` (`ACCENT_PINK`) if `local_prefix`, `●` (`ACCENT_PRIMARY`) if active in another pane, `●` (`MINT`) if idle, `!` (`AMBER`/`CORAL`) if warning/critical, `?` (`AMBER`) if HTTP error.
  - Inline `5h` bar + `rem_5h%` + `Wk:rem_wk%` + `en <reset_5h>` (or `[AGOTÁNDOSE]` / `[AVISO 70%]` / `[error_msg]`).
- **Action row (`id: "sync_integrations"`):** `>> [sincronizar]`.
- **Alert block:** Shows `flash_notice` (when active) and `! CRÍTICO: <prefix> (activa) [presioná 'x' -> migrar a <target>]`.

## 4. `Cuota de Modelos` (`id: "pools"`, index `4`)
- **Header:** `Cuota de Modelos · <active_acc_prefix> *` (scoped to `snap.agent_session.local_prefix`, fallback to first account).
- **Pool rows:** Renders every `QuotaPool` in `target_account.pools` (`Gemini Wk`, `Gemini 5h`, `Claude Wk`, `Claude 5h`, plus dynamic `GPT`, `DeepSeek`, `Qwen` pools when present).
- **Traffic-light colors per pool:** `CORAL` (`<= 15%`), `AMBER` (`<= 50%`), `MINT` (`> 50%`).

## 5. `Cambios` (`id: "git"`, index `5`)
- **Header:** `Cambios · <N> por agente`.
- **Rows:**
  - `<branch>` (+ optional `▲ahead▼behind`) and `ok limpio` / `modificado`.
  - Latest commit `* <hash> <subject>`.
  - `git: +<added> -<deleted>` (or `limpio` / `<N> pend`) and `ag: <N> archivos (+)`.
  - Up to 6 clickable file rows (`action_type: Some("git_file_diff")`): `> [M|A|D|?] <path>` with `+<added> -<deleted>`.
  - Clicking a file toggles an inline diff preview underneath (guarded by `is_sensitive_file`).

## 6. `Engram` (`id: "engram"`, index `6`)
- **Header:** `Engram: <project> ^` (`^` green if daemon `:7437` is online, `v` dim if offline).
- **Rows:**
  - `Local (engram.db)` + `● En linea · <N> obs` (or `Desconectado`).
  - `Cloud: <sync_status>`.
  - Optional `Handoff #<id> (<updated_at>)` block showing extracted lines from the latest `session_summary` observation.

## 7. `Stack · Ecosistema` (`id: "ecosystem"`, index `7`)
- **Header:** `Stack · Ecosistema`.
- **Rows:** One row per tool (`pi`, `hudr2`, `gentle-shell`, `gentle-ai`, `engram`, `herdr`) showing `● <name> <version>` (`MINT`) or `○ <name> no detectado`.

## 8. `Herramientas` & `MCP` (`id: "tools"` & `id: "mcp"`, index `8`)
- Both cards share collapse slot `collapsed[8]` (key `9`).
- **`Herramientas · <total> llamadas`:** `@ <reads> lecturas`, `@ <writes> escritu`, `mem: <engram>`, and optional `$ <bash> cmds` / `subagentes: <N>`.
- **`MCP · <N> activos`:** Lists deduplicated MCP servers (`● <name> [local|remote] en línea`).
