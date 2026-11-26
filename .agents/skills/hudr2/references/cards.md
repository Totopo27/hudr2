# HUDR2 Card Catalog & Tab System

`hudr2` organiza sus 13 tarjetas en un sistema de navegación por pestañas temáticas (`1..5`, `0`/`t`/`,`/`g` para Ajustes, o `Tab`/`Shift+Tab` para ciclar), renderizadas mediante la barra interactiva `draw_tab_bar_theme` y navegables también mediante clic directo con ratón. En modo legado o vertical continuo (`[ vertical ]`), soporta renderizado completo en cascada pasando `tab: None` (alternable con `v` o desde Personalizar).

## Distribución de Tarjetas por Pestaña

| Pestaña | Tecla / Clic | Orden en Pantalla | `CardBounds.id` | Título en `src/ui.rs` | Estado por Defecto |
| :--- | :---: | :---: | :--- | :--- | :--- |
| **`[ 1:Ses ]`** | `1` / `Tab` | 1<br>2<br>3 | `estado`<br>`contexto`<br>`mcp` | `Estado`<br>`Contexto`<br>`MCP` | Expanded<br>Expanded<br>Expanded |
| **`[ 2:Cód ]`** | `2` / `Tab` | 4<br>5<br>6 | `proyecto`<br>`git`<br>`tools` | `~ Proyecto`<br>`Cambios`<br>`Herramientas` | Expanded<br>Expanded<br>Expanded |
| **`[ 3:Mod ]`** | `3` / `Tab` | 7<br>8<br>9<br>10 | `integraciones`<br>`orquestadores`<br>`pools`<br>`saldos` | `Integraciones`<br>`Orquestadores`<br>`Cuota de Modelos`<br>`Saldos API` | Expanded<br>Expanded<br>Expanded<br>Expanded (Auto) |
| **`[ 4:Stk ]`** | `4` / `Tab` | 11<br>12 | `engram`<br>`ecosystem` | `Engram`<br>`Stack · Ecosistema` | Expanded<br>Expanded |
| **`[ ☼ Aju ]`** | `5` / `0` / `t` / `,` / `g` | 13 | `personalizar` | `Personalizar` | **Expanded** (siempre expandida en Ajustes) |

---

## 0. `Estado` (`id: "estado"`, index `0`)
- **Header:** `Estado · >> WORKING` (amber), `-- idle` (dim), `! bloqueado` (coral), or `+ listo` (mint).
- **Rows:**
  - `Proyecto`: shortened workspace path (`~/.../name`).
  - `Rama`: `# <branch> limpio` / `# <branch> ±<unique_dirty_count>` (using unique `git.files.len()` when available), or `sin git` when `git.has_git == false`.
  - `Modelo`: active model short name (`snap.agent_session.model_short`) + optional `[thinking_level]`.
  - Footer summary row: `mem: <engram_project_or_folder> · <enabled_MCPs> MCPs · <sorted_agent_or_provider>`.
  - Optional `Tareas` preview when `snap.agent_session.todos` is non-empty (`✓`, `◉`, `○`).

## 1. `Contexto` (`id: "contexto"`, index `1`)
- **Row 1:** `<active_turn_tokens> / <context_limit>` (`--` and `0.0%` when `context_limit <= 0`, resolved via `opencode.json` `limit.context` or Pi model caches) on the left; width-responsive status badge on the right:
  - `< 50%`: `● Optimo N%` (`MINT`)
  - `50%..80%`: `● Maduro N% [/snew]` (`AMBER`)
  - `>= 80%`: `● Saturado N% [/snew]` (`CORAL`)
- **Row 2:** Full-width inline progress bar (`━` / `─`).
- **Row 3:** `^ <tokens_input> · v <tokens_output>` and `Costo $<cost>`.
- **Row 4 (optional turns):** `<turns> turnos · prom: $<avg>/req` and `último: $<last_turn_cost>` (abbreviated on `< 46` / `< 38` widths).
- **Row 5 (when `>= 50%`):** Width-responsive `/snew` handover notice (`[aviso] Usa /snew (handover)` on `< 48`, `[aviso] Sesion madura: usa /snew (handover)` on `48..57`, full text on `>= 58`).
- **Semantics:** This card is intentionally global to the root session. Its token pressure and root-session cost are contextual telemetry, not evidence that any configured API-key account was used.

## 2. `~ Proyecto` (`id: "proyecto"`, index `2`)
- **Header:** `~ Proyecto · <root_name> <dirs>d · <files>f`.
- **Action row (`id: "open_folder"`):** `Ruta [abrir en PC ->]` + truncated path on the right (never truncated even at width 32; clicking or pressing `o` canonicalizes/verifies directory existence without verbatim Windows `\\?\` prefix and opens it in the OS explorer via `try_open_folder_in_explorer`, reporting result in `flash_notice`).
- **Tree rows:** Directories first (`> / name/ (N)` with up to 2 children `└ / sub` / `└ file`, up to 6 top-level items + `… y M elementos más` or `Sin archivos visibles`), alphabetical case-insensitive; fair depth-2 budget (`TREE_MAX_ENTRIES = 2000` reserving top-level entries first and distributing child budget across directories), symlink/junction dirs classified as dirs without descending, and lazy scanning only when visible (`is_project_tree_visible()`).

## 3. `Integraciones` (`id: "integraciones"`, index `3`)
- **Header:** `Integraciones · <N> cuentas` or `! <N> crític` (`theme.coral`) when any healthy-data (non-errored) account is critical for the **active session's model family**.
- **Account rows:** Pure inventory & availability monitor for all configured integrations (OAuth in Auth Files Management / Quota Management + API Keys in AI Providers):
  - Supports OAuth accounts (`auths/*.json`) and API key accounts (`config.yaml` `api-keys`, `opencode.json` `providers.<proxy>.models`, `profiles.json` orchestrator models).
  - Glyph & theme: `●` (`theme.mint`) if available and healthy, `!` (`theme.amber`/`theme.coral`) if warning/critical, `?` (`theme.amber`) if HTTP/credential error (`has_error` checked before `is_unknown`, painting errored `Wk` `amber`) or unfetched (`[sin consultar]`). All rows render cleanly with active theme tokens (`theme.text_primary`/`theme.text_dim`).
  - For OAuth accounts: inline `5h` gauge + `rem_5h%` + `Wk:rem_wk%` + `en <reset_5h>` (or `[AGOTÁNDOSE]` / `[AVISO 70%]`).
  - For API key and subscription accounts (Claude OAuth, Kimi, xAI, Codex): inline gauge preserving calculated balance percentage (`rem_5h`/`rem_wk`) or session cost + `$XX.XX disp` / `$X.XX sesión` + plan/key badge (`[plus]`, `[pro]`, `[team]`, `[codex]`, `[api-key]`). Non-Antigravity OAuth fails closed against `QUOTA_URL` via disk JSON `type`, ignores `zen`/`zenwarp`, excludes API keys from remote sweeps/prewarm counts, preserves Codex cache errors, and honors workspace `.gentle-ai/profiles.json` via `is_available_for(&project_dir)`.
- **Alert block:** Shows `flash_notice` (when active). Active-account migration alerts (`(activa) [presioná 'x' -> migrar]`) are decoupled from this card and reserved for session-scoped quota telemetry and the `Orquestadores` card.

## 4. `Orquestadores` (`id: "orquestadores"`, index `4`)
- **Header:** `Orquestadores · <N> perfiles` (or `Orquestadores · ! <N> crític` in `theme.coral` when an orchestrator's sovereign model has exhausted quota), themed across active `Theme` tokens (`mint`, `amber`, `coral`, `text_primary`, `text_muted`, `text_dim`).
- **Discovery & Subagent Exclusion:** Scans Gentle AI (`profiles.json`) and OpenCode (`opencode.json` `"agent"` and `"agents"`). Treats `mode: "primary"` as an orchestrator unconditionally, excludes `sdd-*` from non-SDD orchestrators, honors `permission.task` allow/deny maps, and filters subagents via `is_known_subagent` (`subagent`, `deny`, `gentle-ai-*`, `jd-*`, `review-*`, etc.). Supports `GENTLE_AI_PROFILES_PATH` / `PI_PROFILES_PATH` and `OPENCODE_CONFIG_PATH`.
- **Friendly Display Names:** Maps technical fleet identifiers to user-sovereign names: `fleet-codex` / `gentle-orchestrator-codex` → `tavocodex`, `fleet-tavo` / `gentle-orchestrator` → `tavoclaude`, `fleet-gianni` → `gianniclaude`, `fleet-clas` → `clasclaude`, and `sdd-orchestrator-agy*` → `sdd-agy*`.
- **Compact Model Format:** Formats model strings compactly (e.g. `(5.6-sol [high])`, `(opus-5-5 [high])`, `(3.8-flash)`).
- **Status Lifecycle:**
  - `Working` (`● trabajando`, `accent_secondary` `>`): Orchestrator currently active and executing work in the local session, its `cwd`-matched Herdr panes (scanning all matching panes without `Idle` short-circuit), or active `subagent_invocations`.
  - `Idle` (`-- en espera`, `AMBER` `●`): Orchestrator paused, session active but idle with completed turns or active pane.
  - `Recently Finished` (`✓ finalizado`, `theme.mint` `●`): Temporary visual confirmation shown for ~4 seconds (`RECENTLY_FINISHED_WINDOW` woken proactively via `next_orchestrator_expiry_wait` / `expire_orchestrator_finished_flags`) when transitioning from `Working` to `Idle`, accompanied by an audible completion alert (`FleetCompleted`) when configured.
  - `Offline` (`○ apagado`, `TEXT_DIM` `○`): Orchestrator configured but dormant / unused.
- **Sound Alerts Integration:**
  - `FleetCompleted`: Emits completion chime/bell when the orchestrator and all its subagents finish their task (`Working` -> `Idle`) for tasks >= 2 seconds.
  - `QuotaCritical`: Emits a distinctive warning tone (`try_emit_alert` with split per-kind 1.5s cooldown independent of `FleetCompleted`) when the active working orchestrator enters critical quota (>85% or HTTP 429 `"rate limited"`), alerting the user to press `x` for hot migration.
- **Sovereign Quota Governance:** Migration urgency is governed **strictly** by the orchestrator's sovereign model via `evaluate_account_for_sovereign_model` (reading family pools even inside AGY accounts without altering Card 3's Gemini sovereignty), ignoring secondary subagent models and unfetched Codex accounts.
- **Interactive Actions & Migration:**
  - Migration banner: Enforces same-class provider compatibility (`accounts_compatible_for_migration` / `get_best_healthy_target_prefix_for_source`, excluding unfetched Codex), renders responsive migration alert (`[presioná 'x' -> migrar a <target>]`, `[x -> migrar]`, or pre-truncated `<co.name> [x: migrar]` below `46` cols).
  - Idle session release banner & actions: Shows `>> Sesiones en espera` / `[d: liberar]` only when multiple idle orchestrators, an active working + idle orchestrator, or background idle workspace panes exist; `d` uses `release_inactive_sessions_preserving` to protect the live foreground `session_id` even outside Herdr pane metadata. Keybindings: `5` toggle collapse, `x` hot migrate, `d` confined release.
  - Keybindings: `5` toggles collapse, `x` triggers orchestrator migration, `d` triggers confined idle session liberation.

## 5. `Cuota de Modelos` (`id: "pools"`, index `5`)
- **Header:** `Cuota de Modelos · <active_orch_name> *` (scoped exclusively to the active orchestrator, fallback to `<active_acc_prefix> *` or first account), themed with active `Theme` tokens (`title_color`, `text_dim`, `text_primary`, `text_muted`, `mint`, `amber`, `coral`, `accent_secondary`; `MAGENTA` reserved for passive bullets `●`).
- **Orchestrator Fleet Breakdown (Diseño Compacto con roles limpios y colapso rev-4r):**
  - Identifies the active orchestrator from Card 4 `Orquestadores` (`Working` -> `Idle` -> `is_default` -> first profile).
  - Inspects only the subagents and orchestrator models configured for that specific orchestrator profile (from Gentle AI `~/.pi/gentle-ai/profiles.json` and OpenCode `opencode.json`). Never mixes subagents from other orchestrators.
  - Runtime-only observations (including ZenWarp actors) never create extra inventory rows; a model appears only when it belongs to the selected configured fleet.
  - Role simplification: Shortens role identifiers cleanly via `clean_subagent_role` (`orch`, `worker`, `fix`, `explore`, `verify`, `deep-rsch`, `news-rsch`, `judge-a`, `judge-b`, `ui-design`, `rev-risk`, `rev-read`, `rev-rel`, `rev-res`) padded safely via `pad_to_width` so long labels never overflow narrow panes.
  - Review consolidation: Collapses review lenses into `rev-4r` **only when homogeneous** in account prefix and compact model; heterogeneous reviewers render as separate `rev-risk`/`rev-read`/etc. rows so distinct accounts/models remain visible.
  - Model abbreviation: `compact_model_label` strips provider prefixes, thinking suffixes (`-high`, `-medium`, etc.), and `-YYYYMMDD` date stamps honestly without fabricating versions (`claude-opus-4-6` -> `opus-4-6`, `gemini-2.5-flash` -> `2.5-flash`, `codex-auto-review` -> `codex-review`).
   - Runtime visual activity glyphs:
     - `>` in accent color (`ACCENT_SECONDARY`) + bright bold text (`BOLD TEXT_PRIMARY`) for the active working model/agent (`Working`).
     - `●` in purple (`MAGENTA`) + `theme.text_primary` for finished child sessions or distinct-prefix runtime activity (subagents sharing the orchestrator's prefix require explicit invocation evidence and otherwise stay `○`; pane activity is scoped by `cwd`). Each child row formats real epoch timestamps (`>= 86_400`) in `chrono::Local` wall-clock time.
     - `○` in gray (`TEXT_DIM`) + gray text (`TEXT_MUTED`) for inactive / unused components in this session.
     - Each observed OpenCode child session adds a compact detail row with local start/end clocks and its own attributable dollar/token consumption; a running child shows `fin activo`.
  - Quota and Balance Projections:
    - For OAuth accounts (Antigravity): evaluates the subagent/orchestrator's own model family pool via `evaluate_account_for_sovereign_model` (`theme.coral` `<= 15%`, `theme.amber` `<= 50%`, `theme.mint` `> 50%`) with inline gauge and `%` remaining (`5h` and `Wk`).
    - For API key and subscription accounts (Claude, Codex): renders session cost or balance remaining (`$XX.XX disp` / `$X.XX sesión`) with financial traffic-light thresholds (`CORAL` `<= 8%`, `AMBER` `<= 20%`, `MINT` `> 20%`).
  - Interactive balance editing: Key `b` is gated by `can_edit_balance_in_current_view()` (only Tab `Modelos` / un-tabbed view when `saldos` is visible), uses `resolve_balance_edit_target()` to target an API-key account, and expands Card 6 (`collapsed_cards[6] = false`).
  - Legacy fallback: When no orchestrator profiles are discovered (e.g. tests or isolated environments), gracefully falls back to single-account pool breakdown.

## 6. `Saldos API` (`id: "saldos"`, index `6`, key `$`)
- **Header:** `Saldos API · <N> perfiles` (only renders when API key accounts exist in `snap.accounts` and the card is not hidden).
- **Accounts listing:** Displays each discovered API key account (`tavoclaude`, `gianniclaude`, `clasclaude`, etc.).
- **Interactive Balance & Budget:**
  - When budget is set: renders inline health gauge using active theme tokens (`mint` `> 20%`, `amber` `<= 20%`, `coral` `<= 8%`, `text_primary`/`text_dim`/`text_muted`), remaining percentage, remaining dollars (`$XX.XX disp`), and clickable `[editar]` button (`action_type: Some("edit_balance")`).
  - When no budget is set: renders `○ <prefix> [definir saldo]`.
  - When editing: renders in-place terminal input buffer (`> <prefix> Saldo $: <buf>_ [Enter|Esc]`, compacting prefix/prompt at `< 42` cols so `[Enter|Esc]` stays visible at `36` cols). Typing numbers/dots updates the buffer; pressing `Enter` saves atomically to the active `--config` path (`TelemetrySnapshot::config_path` / `resolve_hudr2_config_path()`) under `account_budgets` with case-insensitive key canonicalization and cross-session / Pi (`session_id: None`, reset on `session_cost < baseline`) tracking; `Esc` cancels.
- **Dynamic Task & Quota Projection:**
  - Evaluates actual runtime telemetry via `is_api_account_active_in_dir` / `api_account_attributed_usage_in_dir`: combines a matching root session and its distinct matching child `subagent_invocations`, or falls back to Herdr panes matching the normalized `project_dir` (`cwd`). Merely configuring an account in an orchestrator profile, foreign-workspace panes, or retaining a prefix in `all_active_prefixes` does not count as use.
  - When attributable consumption exists: displays row `  sesión: $<cost_since_set> · $<avg_cost>/req · ~<reqs_remaining> reqs disp` using only that account's own root/child/pane cost.
  - When no cost can be attributed, preserves the configured balance and omits session/projection metrics instead of reusing the global root-session cost.
  - Dynamically recalculates and projects remaining requests in real time as turns and tasks complete.
- **Official Billing Web Console:**
  - Resolves `account_billing_info(acc)` (`acc.prefix` first, falling back to `acc.pools` provider labels when using custom key aliases) and renders row `  ↳ <Provider> Console [visitar ->]` (`action_type: Some("open_billing_url")`).
  - Clicking `[visitar ->]` opens the provider's official billing management page in the default system browser (Anthropic Spending Console, OpenAI Organization Overview, ChatGPT Subscription, DeepSeek Platform, etc.).

## 7. `Cambios` (`id: "git"`, index `7`, key `7`)
- **Header:** `Cambios · <N> por agente` (or `Cambios ! 1 colisión` / `! <N> colisiones` in `coral` when `cross_session_conflicts` is non-empty).
- **Rows:**
  - Optional conflict radar alert (`! CONFLICTO: <files>` + `[colisión]` in `coral`, wrapping file list to a second row when narrow) scoped to distinct worktrees of the same `commondir` repository that touch `primary_dir` tracked files concurrently.
  - When git context is absent: `sin repositorio git` / `--` and `git: --` + `ag: <N> archivos (+)`; otherwise `<branch>` (+ optional `▲ahead▼behind`) and `ok limpio` / `modificado`.
  - Latest commit `* <hash> <subject>` (with zero-subprocess fallback to `* <head_short> HEAD` read from `.git/HEAD` / `refs/` / `packed-refs`).
  - `git: +<added> -<deleted>` (or `limpio` / `<unique_files> pend`) and `ag: <N> archivos (+)` (with normalized relative paths from both OpenCode and Pi).
  - Up to 6 clickable file rows (`action_type: Some("git_file_diff")`): `> [M|A|D|?] <path>` with `+<added> -<deleted>`.
  - Clicking a file toggles an inline 12-line + trailer diff preview underneath (guarded by hardened `is_sensitive_file`; never dumps tracked files as untracked, and refreshes with `GIT_CACHE` 2s working-tree TTL) plus an `>> [vista rápida (Espacio)]` row (`action_type: Some("open_quick_peek")`) opening the themed **Quick Peek** modal (clears `tab_bounds`, captures non-modal shortcuts, and keeps viewport rows filled at bottom scroll).

## 8. `Engram` (`id: "engram"`, index `8`, key `8`)
- **Header:** `Engram: <snap.engram.project> ^` (`^` theme `mint` if daemon `:7437` is online, `v` theme `text_dim` if offline; resolves `.engram/config.json`, canonical cwd basename even for `.`/`..`, git `origin` `owner/repo` + repo name, and case-insensitive SQLite project matches).
- **Rows:**
  - `Local (engram.db)` + `● En linea · <N> obs` (`mint` when DB opens) or `○ Offline · <N> obs` (`coral` if DB absent; `ENGRAM_DB_PATH` honors configured non-existent files and `:7437` daemon probe runs even without a local DB).
  - `Cloud: proyecto inscrito para sync` / `Cloud: solo local o no configurado` (`sync inscrito` / `solo local` at `< 40`) plus always-visible `Servicio: daemon ok` / `daemon off` (`+ sync_status` relative time refreshed on cache hits), styled with active `Theme` tokens.
  - Optional `Handoff #<id> (<updated_at>)` block supporting ISO/RFC3339 and Unix integer/ms `created_at` timestamps and skipping bare `#` Markdown headings when body lines exist.

## 9. `Herramientas` & `MCP` (`id: "tools"` & `id: "mcp"`, index `9` & `10`)
- `Herramientas` is index `9` (key `9`).
- `MCP` is index `10` (toggleable via mouse click).
- **`Herramientas · <total> llamadas`:** Unified OpenCode/Pi tool classification (`classify_tool_name`, `message`/`part` dedup, non-tool JSON exclusion); renders `@ <reads> lecturas` / `lect`, `@ <writes> escritu` / `escr`, width-protected `mem: <engram>`, and optional Row 2 with `$ <bash> cmds`, `<N> otros`, and `subagentes: <N>` (`subag: <N>` at `< 38`).
- **`MCP · <N> configurados` (· `<M> deshabilitado(s)` cuando aplica; `conf`/`off` en anchos `< 52`):** Lists deduplicated MCP servers (project `opencode.json` before `OPENCODE_CONFIG_DIR`; V2 `mcp.servers` before V1 legacy) with their real state: `● <name> [<tipo>·<scope>]` where `scope` is `proj` (archivo del proyecto), `global` (config de usuario) or `agente` (Pi / Gentle Shell), or `Sin servidores MCP configurados` when empty. The row preserves the right control (`[apagar]`/`[encender]`, `[off]`/`[on]` at `< 36`, `sin interruptor`/`fijo` for `AgentHost`). A row click emits `toggle_mcp`, edits only the effective (last) control field preserving CRLF/LF and key order, and displays `flash_notice` globally when `Integraciones` is off-tab; existing MCP configs are watched for external edits.

## 11. `Stack · Ecosistema` (`id: "ecosystem"`, index `11`, key `e`)
- **Header:** `Stack · Ecosistema` (or `Stack · Ecosistema ↑<N>` in `AMBER` when updates are available).
- **Rows:** One row per tool (`pi`, `hudr2`, `gentle-shell`, `gentle-ai`, `engram`, `herdr`):
  - Up-to-date: `● <name> <version>` (`MINT`).
  - Update available: `● <name> <version> ↑<upstream>` (`amber`, compacting/truncating `<version>` first on narrow widths so `[actualizar]`/`[asistido]`/`[↑]` is never clipped) mapped to `action_type: Some("update_tool")` (passing target upstream version so Windows `gentle-ai` resolves dynamic `/vN/` Go module paths). For `hudr2`, displays `[asistido]` and wraps the global pre-footer `flash_notice` (up to 3 lines on Tab 4+) so full `cargo install` / `EBUSY` instructions remain readable.
  - In progress / Completed: Retains in-flight state across periodic/event-driven refreshes (`preserve_in_flight_update_state`). Shows an ASCII spinner, stage and elapsed time, e.g. `[| instalando 32s]`; narrow panes abbreviate the stage and omit the installed version. No synthetic percentage or retry-looking button appears while running. Success requires a fresh installed-version probe. A FIFO completion event releases the row before inventory refresh and displays the result in a flash notice, without a fixed completion delay. Installation, output collection and version verification share a 120s deadline; Windows installer descendants are contained in a private job. `EBUSY` advises closing affected sessions and updating from another terminal, without automatic retries or closing user sessions.
  - Missing/Not installed: `○ <name> -` or `○ <name> - (disp <upstream>)` (`TEXT_DIM`).
- **Upstream Cache & Comparison:** Cached in `upstream_cache.json` with 3-hour TTL (only persisted when at least one release tag succeeds; all-`None` offline caches are treated as expired, governed by a 5-minute automatic retry backoff while manual `r`/`u` forces re-check), refreshed on startup, timer ticks, and `r`, using SemVer 2.0.0 comparison (`is_newer_version`, rejecting malformed cores and ordering prereleases) and `version` keyword stripping in `clean_version_output`.

## 12. `Personalizar` (`id: "personalizar"`, index `12`, key `0` / `t`)
- **Header:** `Personalizar` (expanded by default via `DEFAULT_COLLAPSED`, always expanded on Tab 5 `Ajustes` and immune to single-click collapse there; hiding all cards on Tabs 1–4 renders an empty-tab hint `Sin tarjetas visibles`).
- **Rows:**
  - One interactive checkbox row per configurable card (`Estado`, `Contexto`, `Proyecto`, `Integraciones`, `Orquestadores`, `Cuota de Modelos`, `Saldos API`, `Cambios (Git)`, `Engram`, `Herramientas`, `MCP`, `Stack · Ecosistema`).
  - `[x] <Nombre>` with `visible` (`mint`, `text_primary`) or `[ ] <Nombre>` with `oculta` (`text_dim`, `text_muted`), using active `Theme` tokens across Card 12, gauges, tab bar (switching to full labels at `width >= 63`), and footers. Built using ASCII checkboxes `[x]` to prevent missing glyph boxes `[?]` across non-NerdFont Windows and Linux consoles.
  - Clicking any card row toggles its visibility in real time; cards marked `oculta` are completely skipped from rendering (saving vertical space).
  - Redundant fixed row was eliminated to prevent vertical waste (`personalizar` can never hide itself).
  - **Divider & Settings section:**
    - `Nombre  <nombre>` with interactive `[editar >]` button (`action_type: Some("edit_sidebar_name")`, key `n`, or double click on `Personalizar` card). Auto-switches to Tab 5 and expands Card 12; truncates value to protect `[editar >]`; saves on `Enter` (`Esc` cancels, and switching tabs cancels active editing). Clearing to empty/`HUDR2`/`defecto` removes `sidebar_name`/`name`/`title` from `hudr2.json` (while `--once --name` stays in-memory only).
    - `Tema    <nombre_tema>` with clickable `[cambiar >]` (`action_type: Some("cycle_theme")`, key `m`), dynamically cycling between `Auto (<id>)` (live-synced every 30s even without quotas), 5 native palettes, and 14 Herdr themes; truncates label on narrow widths to keep `[cambiar >]` intact.
    - `Vista   Diseño` with clickable `[ pestañas ]` / `[ vertical ]` (`action_type: Some("toggle_tabbed_view")`, key `v`), resetting `scroll_offset` on toggle/tab switch (footers advertise `t: pers` in vertical mode and `Enter: guardar` during both name and balance editing).
    - `Avisos  Alertas sonoras` with clickable `[x] campana` / `[x] nativo` / `[ ] silencio` (`action_type: Some("cycle_sound_mode")`), using locked stdout writes for BEL output.
    - `Updates Check online` with clickable `[x] activo (3h)` / `[ ] apagado` (`action_type: Some("toggle_check_updates")`, key `u`; `[x] 3h`/`[ ] off` at `< 36` cols), controlling online version checks.
  - State persisted atomically in `hudr2.json` via capped `read_config_bytes` (`{"collapsed": [...], "hidden": [...], "theme": "...", "sidebar_name": "...", "check_updates": true, "sound_mode": "bell", "tabbed": true}`, removing cleared `theme`/`sidebar_name`/`name`/`title` keys and loading object `account_budgets` cleanly).
