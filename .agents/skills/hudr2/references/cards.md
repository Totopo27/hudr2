# HUDR2 Card Catalog & Tab System

`hudr2` organiza sus 13 tarjetas en un sistema de navegación por pestañas temáticas (`1..5`, `0`/`t`/`,`/`g` para Ajustes, o `Tab`/`Shift+Tab` para ciclar), renderizadas mediante la barra interactiva `draw_tab_bar_theme` y navegables también mediante clic directo con ratón. En modo legado o vertical continuo (`[ vertical ]`), soporta renderizado completo en cascada pasando `tab: None` (alternable con `v` o desde Personalizar).

## Distribución de Tarjetas por Pestaña

| Pestaña | Tecla / Clic | Orden en Pantalla | `CardBounds.id` | Título en `src/ui.rs` | Estado por Defecto |
| :--- | :---: | :---: | :--- | :--- | :--- |
| **`[ 1:Ses ]`** | `1` / `Tab` | 1<br>2<br>3 | `estado`<br>`contexto`<br>`mcp` | `Estado`<br>`Contexto`<br>`MCP` | Expanded<br>Expanded<br>Expanded |
| **`[ 2:Cód ]`** | `2` / `Tab` | 4<br>5<br>6 | `proyecto`<br>`git`<br>`tools` | `~ Proyecto`<br>`Cambios`<br>`Herramientas` | Expanded<br>Expanded<br>Expanded |
| **`[ 3:Mod ]`** | `3` / `Tab` | 7<br>8<br>9<br>10 | `integraciones`<br>`orquestadores`<br>`pools`<br>`saldos` | `Integraciones`<br>`Orquestadores`<br>`Cuota de Modelos`<br>`Saldos API` | Expanded<br>Expanded<br>Expanded<br>Expanded (Auto) |
| **`[ 4:Stk ]`** | `4` / `Tab` | 11<br>12 | `engram`<br>`ecosystem` | `Engram`<br>`Stack · Ecosistema` | Expanded<br>Expanded |
| **`[ ☼ Aju ]`** | `5` / `0` / `t` / `v` | 13 | `personalizar` | `Personalizar` | **Expanded** (vista de ajustes) |

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
- **Semantics:** This card is intentionally global to the root session. Its token pressure and root-session cost are contextual telemetry, not evidence that any configured API-key account was used.

## 2. `~ Proyecto` (`id: "proyecto"`, index `2`)
- **Header:** `~ Proyecto · <root_name> <dirs>d · <files>f`.
- **Action row (`id: "open_folder"`):** `>> [abrir en PC ->]` (never truncated even at width 36; clicking opens the project folder in the OS file explorer).
- **Tree rows:** Directories first (`+ name/`), then files (`  name`), alphabetical case-insensitive, skipped when card `2` is collapsed.

## 3. `Integraciones` (`id: "integraciones"`, index `3`)
- **Header:** `Integraciones · <N> cuentas` or `! <N> crític` (`CORAL`) when any account is critical for the **active session's model family**.
- **Account rows:** Pure inventory & availability monitor for all configured integrations (OAuth in Auth Files Management / Quota Management + API Keys in AI Providers):
  - Supports OAuth accounts (`auths/*.json`) and API key accounts (`config.yaml` `api-keys`, `opencode.json` `providers.<proxy>.models`, `profiles.json` orchestrator models).
  - Glyph: `●` (`MINT`) if available and healthy, `!` (`AMBER`/`CORAL`) if warning/critical, `?` (`AMBER`) if HTTP or credential error/expired. No active-session glyphs (`>`, colored bullets) or prefix dimming; all integrations render cleanly with uniform `TEXT_PRIMARY`.
  - For OAuth accounts: inline `5h` gauge + `rem_5h%` + `Wk:rem_wk%` + `en <reset_5h>` (or `[AGOTÁNDOSE]` / `[AVISO 70%]`).
  - For API key and subscription accounts (Claude, Codex): inline gauge reflecting remaining balance percentage or session cost + `$XX.XX disp` / `$X.XX sesión` + plan or key badge (`[plus]`, `[pro]`, `[team]`, `[codex]`, `[api-key]`) (or `[SALDO AGOTADO]` when `<= 8%` / `[AVISO SALDO]` when `<= 20%`). Bypasses HTTP quota network sweeps to Google endpoints and validates credentials locally.
- **Alert block:** Shows `flash_notice` (when active). Active-account migration alerts (`(activa) [presioná 'x' -> migrar]`) are decoupled from this card and reserved for session-scoped quota telemetry and the `Orquestadores` card.

## 4. `Orquestadores` (`id: "orquestadores"`, index `4`)
- **Header:** `Orquestadores · <N> perfiles` (or `Orquestadores · ! <N> crític` in `CORAL` when an orchestrator's sovereign model has exhausted quota).
- **Discovery & Subagent Exclusion:** Scans Gentle AI (`profiles.json`) and OpenCode (`opencode.json` `"agent"` and `"agents"`). Filters subagents via `is_known_subagent` (`subagent`, `deny`, `gentle-ai-*`, `jd-*`, `review-*`, `sdd-explore-*`, etc.) so subagents NEVER pollute this card. Supports isolated overrides via `GENTLE_AI_PROFILES_PATH` / `PI_PROFILES_PATH` and `OPENCODE_CONFIG_PATH`.
- **Friendly Display Names:** Maps technical fleet identifiers to user-sovereign names: `fleet-codex` / `gentle-orchestrator-codex` → `tavocodex`, `fleet-tavo` / `gentle-orchestrator` → `tavoclaude`, `fleet-gianni` → `gianniclaude`, `fleet-clas` → `clasclaude`, and `sdd-orchestrator-agy*` → `sdd-agy*`.
- **Compact Model Format:** Formats model strings compactly (e.g. `(5.6-sol [high])`, `(opus-5-5 [high])`, `(3.8-flash)`).
- **Status Lifecycle:**
  - `Working` (`● trabajando`, `accent_secondary` `>`): Orchestrator currently active and executing work in the local session or Herdr pane (including any of its active subagents).
  - `Idle` (`-- en espera`, `AMBER` `●`): Orchestrator paused, session active but idle with completed turns or active pane.
  - `Recently Finished` (`✓ finalizado`, `MINT` `●`): Temporary visual confirmation shown for ~4 seconds when transitioning from `Working` to `Idle`, accompanied by an audible completion alert (`FleetCompleted`) when configured.
  - `Offline` (`○ apagado`, `TEXT_DIM` `○`): Orchestrator configured but dormant / unused.
- **Sound Alerts Integration:**
  - `FleetCompleted`: Emits completion chime/bell when the orchestrator and all its subagents finish their task (`Working` -> `Idle`) for tasks >= 2 seconds.
  - `QuotaCritical`: Emits a distinctive warning tone when the active working orchestrator enters critical quota (>85% or HTTP 429), alerting the user to press `x` for hot migration before execution halts.
- **Sovereign Quota Governance:** Migration urgency is governed **strictly** by the sovereign orchestrator model (`orchestrator.model` in `profiles.json`), ignoring secondary subagent models.
- **Interactive Actions & Migration:**
  - Migration banner: When the active working orchestrator has depleted its sovereign quota (`is_critical`), renders responsive migration alert `! CRÍTICO: <name> (activo) [presioná 'x' -> migrar a <target>]` (adapting dynamically to `[x -> migrar]` or `[x: migrar]` on narrower widths).
  - Idle session release banner: When paused/idle sessions are detected, renders `>> Sesiones en espera detectadas [presioná 'd' -> liberar]`.
  - Keybindings: `5` toggles collapse, `x` triggers orchestrator migration, `d` triggers confined idle session liberation.

## 5. `Cuota de Modelos` (`id: "pools"`, index `5`)
- **Header:** `Cuota de Modelos · <active_orch_name> *` (scoped exclusively to the active orchestrator, fallback to `<active_acc_prefix> *` or first account).
- **Orchestrator Fleet Breakdown (Diseño Compacto con roles limpios y colapso rev-4r):**
  - Identifies the active orchestrator from Card 4 `Orquestadores` (`Working` -> `Idle` -> `is_default` -> first profile).
  - Inspects only the subagents and orchestrator models configured for that specific orchestrator profile (from Gentle AI `~/.pi/gentle-ai/profiles.json` and OpenCode `opencode.json`). Never mixes subagents from other orchestrators.
  - Role simplification: Shortens role identifiers to eliminate redundant prefixes (`gentle-ai-*`, `sdd-*`): `orch`, `worker`, `fix`, `explore`, `verify`, `deep-rsch`, `news-rsch`, `judge-a`, `judge-b`, `ui-design`.
  - Review consolidation: Groups the 4 review lenses (`review-risk`, `review-readability`, `review-reliability`, `review-resilience`) into a single collapsed active row: `rev-4r (sonnet-5-5)`.
  - Model abbreviation: Formats model names cleanly: `sonnet-5-5`, `opus-5-5`, `5.6-sol`, `5.6-terra`, `5.6-luna`, `codex-review`, `3.8-flash`.
   - Runtime visual activity glyphs:
     - `>` in accent color (`ACCENT_SECONDARY`) + bright bold text (`BOLD TEXT_PRIMARY`) for the active working model/agent (`Working`).
     - `●` in purple (`MAGENTA`) + white text (`TEXT_PRIMARY`) for child sessions that finished or other passive components observed at runtime.
     - `○` in gray (`TEXT_DIM`) + gray text (`TEXT_MUTED`) for inactive / unused components in this session.
     - Each observed OpenCode child session adds a compact detail row with local start/end clocks and its own attributable dollar/token consumption; a running child shows `fin activo`.
  - Quota and Balance Projections:
    - For OAuth accounts (Antigravity): evaluates the sovereign/target pool for that model with traffic-light thresholds (`CORAL` `<= 15%`, `AMBER` `<= 50%`, `MINT` `> 50%`) with inline gauge and `%` remaining (`5h` and `Wk`).
    - For API key and subscription accounts (Claude, Codex): renders session cost or balance remaining (`$XX.XX disp` / `$X.XX sesión`) with financial traffic-light thresholds (`CORAL` `<= 8%`, `AMBER` `<= 20%`, `MINT` `> 20%`).
  - Interactive balance editing: Allows in-place balance editing for active API key accounts (`[editar saldo]` / key `b`).
  - Legacy fallback: When no orchestrator profiles are discovered (e.g. tests or isolated environments), gracefully falls back to single-account pool breakdown.

## 6. `Saldos API` (`id: "saldos"`, index `6`, key `$`)
- **Header:** `Saldos API · <N> perfiles` (only renders when API key accounts exist in `snap.accounts` and the card is not hidden).
- **Accounts listing:** Displays each discovered API key account (`tavoclaude`, `gianniclaude`, `clasclaude`, etc.).
- **Interactive Balance & Budget:**
  - When budget is set: renders inline health gauge (`MINT` `> 20%`, `AMBER` `<= 20%`, `CORAL` `<= 8%`), remaining percentage, remaining dollars (`$XX.XX disp`), and clickable `[editar]` button (`action_type: Some("edit_balance")`).
  - When no budget is set: renders `○ <prefix> [definir saldo]`.
  - When editing: renders in-place terminal input buffer `> <prefix> Saldo $: [ XX.XX ]_ [Enter|Esc]`. Typing numbers/dots updates the buffer; pressing `Enter` saves atomically to `hudr2.json` under `account_budgets.<prefix>` with session baseline tracking; `Esc` cancels.
- **Dynamic Task & Quota Projection:**
  - Evaluates actual runtime telemetry: a matching working root session, a matching OpenCode child session, or a working Herdr pane with matching account/model metadata. Merely configuring an account in an orchestrator profile or retaining it in `all_active_prefixes` does not count as use.
  - When attributable consumption exists: displays row `  sesión: $<cost_since_set> · $<avg_cost>/req · ~<reqs_remaining> reqs disp` using only that account's own root/child/pane cost.
  - When no cost can be attributed, preserves the configured balance and omits session/projection metrics instead of reusing the global root-session cost.
  - Dynamically recalculates and projects remaining requests in real time as turns and tasks complete.
- **Official Billing Web Console:**
  - Renders row `  ↳ <Provider> Console [visitar ->]` (`action_type: Some("open_billing_url")`).
  - Clicking `[visitar ->]` opens the provider's official billing management page in the default system browser (Anthropic Spending Console, OpenAI Organization Overview, ChatGPT Subscription, DeepSeek Platform, etc.).

## 7. `Cambios` (`id: "git"`, index `7`, key `7`)
- **Header:** `Cambios · <N> por agente` (or `Cambios ! <N> colisión` in `CORAL` when `cross_session_conflicts` is non-empty).
- **Rows:**
  - Optional conflict radar alert (`! CONFLICTO: <files>` + `[colisión]` in `CORAL`) when two or more active sessions/worktrees modify the same tracked file concurrently.
  - `<branch>` (+ optional `▲ahead▼behind`) and `ok limpio` / `modificado`.
  - Latest commit `* <hash> <subject>` (with zero-subprocess fallback to `* <head_short> HEAD` read from `.git/HEAD` / `refs/` / `packed-refs`).
  - `git: +<added> -<deleted>` (or `limpio` / `<N> pend`) and `ag: <N> archivos (+)`.
  - Up to 6 clickable file rows (`action_type: Some("git_file_diff")`): `> [M|A|D|?] <path>` with `+<added> -<deleted>`.
  - Clicking a file toggles an inline diff preview underneath (guarded by `is_sensitive_file`) plus an `>> [vista rápida (Espacio)]` row (`action_type: Some("open_quick_peek")`) that opens the centered floating **Quick Peek** modal (`Space` / `p` to toggle, `↑`/`↓` or wheel to scroll, `Esc` or click header to close).

## 8. `Engram` (`id: "engram"`, index `8`, key `8`)
- **Header:** `Engram: <project> ^` (`^` green if daemon `:7437` is online, `v` dim if offline).
- **Rows:**
  - `Local (engram.db)` + `● En linea · <N> obs` (or `Desconectado`).
  - `Cloud: <sync_status>`.
  - Optional `Handoff #<id> (<updated_at>)` block showing extracted lines from the latest `session_summary` observation.

## 9. `Herramientas` & `MCP` (`id: "tools"` & `id: "mcp"`, index `9` & `10`)
- `Herramientas` is index `9` (key `9`).
- `MCP` is index `10` (toggleable via mouse click).
- **`Herramientas · <total> llamadas`:** `@ <reads> lecturas`, `@ <writes> escritu`, `mem: <engram>`, and optional `$ <bash> cmds` / `subagentes: <N>`.
- **`MCP · <N> activos`:** Lists deduplicated MCP servers (`● <name> [local|remote] en línea`).

## 11. `Stack · Ecosistema` (`id: "ecosystem"`, index `11`, key `e`)
- **Header:** `Stack · Ecosistema` (or `Stack · Ecosistema ↑<N>` in `AMBER` when updates are available).
- **Rows:** One row per tool (`pi`, `hudr2`, `gentle-shell`, `gentle-ai`, `engram`, `herdr`):
  - Up-to-date: `● <name> <version>` (`MINT`).
  - Update available: `● <name> <version> ↑<upstream>` (`AMBER`) accompanied by an interactive clickable `[actualizar]` button (or adaptive `[↑]` when `width < 44`) mapped to `action_type: Some("update_tool")`. For `hudr2` itself, renders `[asistido]` and displays non-blocking instructions in flash notice to prevent Windows executable file-locking (`os error 5`) and CPU compilation spikes.
  - In progress: Displays dynamic Braille spinner frame and real-time percent `[⠋ 45%]` alongside responsive `[actualizando]` or `[↑]` while the background worker runs the official upgrade command non-blockingly, with flash notice updates and live TUI repaints.
  - Missing/Not installed: `○ <name> -` or `○ <name> - (disp <upstream>)` (`TEXT_DIM`).
- **Upstream Cache:** Cached in `upstream_cache.json` with 3-hour TTL, queried in background via GitHub Releases API (`/repos/{owner}/{repo}/releases/latest`).

## 12. `Personalizar` (`id: "personalizar"`, index `12`, key `0` / `t`)
- **Header:** `Personalizar` (collapsed by default).
- **Rows:**
  - One interactive checkbox row per configurable card (`Estado`, `Contexto`, `Proyecto`, `Integraciones`, `Orquestadores`, `Cuota de Modelos`, `Saldos API`, `Cambios (Git)`, `Engram`, `Herramientas`, `MCP`, `Stack · Ecosistema`).
  - `[x] <Nombre>` with `visible` (`MINT`) or `[ ] <Nombre>` with `oculta` (`TEXT_DIM`). Built using ASCII checkboxes `[x]` to prevent missing glyph boxes `[?]` across non-NerdFont Windows and Linux consoles.
  - Clicking any card row toggles its visibility in real time; cards marked `oculta` are completely skipped from rendering (saving vertical space).
  - Redundant fixed row was eliminated to prevent vertical waste (`personalizar` can never hide itself).
  - **Divider & Settings section:**
    - `Nombre  <nombre>` with interactive `[editar >]` button (`action_type: Some("edit_sidebar_name")`, key `n`, or double click on `Personalizar` card). Allows in-place editing of top banner `:: <nombre> ::` with live buffer (`> <buffer>_`), saving on `Enter` and canceling on `Esc`. Entering empty or `HUDR2` restores default.
    - `Tema    <nombre_tema>` with clickable `[cambiar >]` (`action_type: Some("cycle_theme")`, key `m`), dynamically cycling between `Auto (Herdr · <id>)`, 5 native palettes, and 14 Herdr themes.
    - `Updates Check online` with clickable `[x] activo (3h)` / `[ ] apagado` (`action_type: Some("toggle_check_updates")`, key `u`), controlling online version checks.
    - `Avisos  Alertas sonoras` with clickable `[x] campana` / `[x] nativo` / `[ ] silencio` (`action_type: Some("cycle_sound_mode")`), controlling audio feedback for orchestrator fleet completion and critical quota warnings.
  - State persisted atomically in `hudr2.json` (`{"collapsed": [...], "hidden": [...], "theme": "...", "sidebar_name": "...", "check_updates": true, "sound_mode": "bell"}`).
