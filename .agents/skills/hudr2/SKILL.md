---
name: hudr2
description: >-
  Trigger: hudr2, barra lateral hudr2, HUD de telemetría, tarjeta del HUD,
  agregar/quitar/modificar tarjeta, cuotas multimodelo, sesiones OpenCode o Pi/Gentle Shell,
  colores o layout del sidebar. Modificar, extender, depurar o auditar el sidecar de
  telemetría TUI en Rust `hudr2` de forma segura y registrar cada aprendizaje en Engram.
license: MIT
metadata:
  author: Totopo27
  version: "1.0"
---

# HUDR2 — Project Skill

## Activation Contract

Use this skill whenever the user asks to add, remove, reorder, restyle, optimize, debug, or explain anything in **`hudr2`** (the reactive Rust TUI telemetry sidecar for **OpenCode**, **Pi**, **Gentle Shell**, and **Herdr**).

- **Repository root:** `<REPO>` (`d:\DocumentosDiscoD\hudr2`)
- **Skill directory:** `.agents/skills/hudr2`

## Hard Rules (Non-Negotiable Invariants)

1. **Read First:** Before changing behavior, read [`references/lessons.md`](./references/lessons.md) and search Engram (`mem_search` in project `hudr2`). Past subtle bugs and platform gotchas live there.
2. **Event-Driven Only (0% Idle CPU):**
   - Never reintroduce blind polling loops (`event::poll(50ms)` or `thread::sleep` in the render path).
   - The main loop in [`src/main.rs`](../../../src/main.rs) waits on `mpsc::unbounded_channel` events (`notify` 300ms debounced filesystem watcher + Herdr NDJSON socket/pipe IPC `events.subscribe` + terminal input + background timer ticks).
   - Redraws occur **only** when `state.needs_redraw == true`, and [`refresh_all`](../../../src/app.rs) only sets `needs_redraw = true` when `state.snapshot != prev_snapshot` (`PartialEq`).
3. **Subprocess & Filesystem Safety:**
   - Every `git` command in [`src/git.rs`](../../../src/git.rs) MUST go through `run_git_cmd` with `GIT_OPTIONAL_LOCKS=0` and a `3s` timeout, executed at `git_root`. Never let the HUD create `.git/index.lock`.
   - Ecosystem CLI probes in [`src/ecosystem.rs`](../../../src/ecosystem.rs) run once per session (`OnceLock`-style cache, invalidated only on `r`) with a `3s` timeout.
   - Project tree scanning in [`src/tree.rs`](../../../src/tree.rs) enforces `TREE_MAX_ENTRIES = 2000`, skips heavy build dirs (`TREE_SKIP_NAMES`), and never follows symlinks (`is_real_dir` via `symlink_metadata`).
   - Inline git diff previews in [`src/git.rs`](../../../src/git.rs) MUST filter paths through `is_sensitive_file` (`.env*`, `*secret*`, `*token*`, `*auth*`, `*.pem`, `*.key`, `id_rsa`, etc.) before reading any content.
   - Collapsed-card persistence in [`src/state.rs`](../../../src/state.rs) writes only to `hudr2.json` via atomic `<file>.tmp` + `rename`. `AppState::new()` must stay in-memory (`config_path: None`) so unit/seam tests never touch user disk state.
4. **Dynamic Multi-Model Quota Health:**
   - `hudr2` supports **Gemini, Claude, GPT, DeepSeek, Qwen, Argon**, and other model families.
   - In [`src/quotas/cliproxy.rs`](../../../src/quotas/cliproxy.rs), `evaluate_account_for_model` and `sync_account_health_for_model` dynamically evaluate `5h` and `Wk` quotas, traffic-light colors (`CORAL` <= 15%, `AMBER` <= 50%, `MINT` > 50%), and migration alerts in `Integraciones` based on the **active model family in the session** (`snap.agent_session.model`), falling back to `Gemini` (or default account metrics) when no specific family pool matches. Para cuentas de Antigravity (`agy*`), la cuota soberana principal en `Integraciones` es siempre `GEMINI MODELS` (los pools secundarios de Argon y terciarios de Claude/GPT se desglosan en `Cuota de Modelos`).
   - `Cuota de Modelos` displays all pools available for the active workspace's account (`local_prefix`).
5. **Multi-Agent Session Telemetry (OpenCode + Pi / Gentle Shell):**
   - [`refresh_all`](../../../src/app.rs) queries [`pi::try_read_pi_session`](../../../src/pi.rs) (incremental byte-offset `.jsonl` tail reader) first and falls back to [`opencode::read_opencode_session`](../../../src/opencode/session.rs) (SQLite `opencode.db`).
6. **ANSI & Width Safety:**
   - Always build card lines with `draw_top`, `draw_row`, `draw_divider`, and `draw_bottom` in [`src/ui.rs`](../../../src/ui.rs), using `string_width` and `truncate_to_width`. Never byte-slice UTF-8 or ANSI strings directly.

## Decision Gates

| Task / Request | Reference to Read First |
| :--- | :--- |
| Add, remove, reorder, or restyle a card or clickable row | [`references/cards.md`](./references/cards.md), [`references/recipes.md`](./references/recipes.md) §Cards |
| Modify OpenCode SQLite or Pi / Gentle Shell `.jsonl` telemetry | [`references/architecture.md`](./references/architecture.md) §Sessions, [`references/recipes.md`](./references/recipes.md) §Session Fields |
| Change quota calculation, model families (Gemini, Claude, GPT, DeepSeek), or account migration | [`references/architecture.md`](./references/architecture.md) §Quotas, [`references/recipes.md`](./references/recipes.md) §Quotas |
| Touch Herdr IPC, file watcher (`notify`), or CLI flags (`--once`, `--pane`) | [`references/architecture.md`](./references/architecture.md) §Event Loop |
| Debug discrepancies in tokens, context limits, git counts, or Windows/Linux behavior | [`references/lessons.md`](./references/lessons.md) |

## Execution Steps

1. **Read Context:** Check [`references/lessons.md`](./references/lessons.md) and the relevant reference file from the Decision Gates table above.
2. **Test-First (Seam Tests):** Add or update a test in [`tests/seam_tests.rs`](../../../tests/seam_tests.rs) covering the target behavior.
3. **Implement:** Make the change following [`references/recipes.md`](./references/recipes.md). Ensure `--once` snapshot mode in [`src/main.rs`](../../../src/main.rs) reflects any new data source.
4. **Run the Doctor:**
   - On Windows (PowerShell): `powershell -ExecutionPolicy Bypass -File .agents/skills/hudr2/scripts/doctor.ps1`
   - On Linux/macOS/Bash: `bash .agents/skills/hudr2/scripts/doctor.sh`
   - All checks (`cargo test`, `cargo clippy --all-targets`, `cargo build --release`, `--once` render width/frame check, and safety guards) must pass with `0` warnings and `0` failures.
5. **Persist Learnings:**
   - If cards changed, update [`references/cards.md`](./references/cards.md).
   - Append any non-obvious root cause or platform gotcha to [`references/lessons.md`](./references/lessons.md).
   - Persist durable architectural decisions or bugfixes to Engram (`mem_save` / `mem_update`, project `hudr2`).
