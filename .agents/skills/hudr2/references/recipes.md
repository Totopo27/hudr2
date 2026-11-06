# HUDR2 Recipes (Step-by-Step Runbooks)

Every recipe ends with running the **Doctor** script (`doctor.ps1` or `doctor.sh`) and updating [`lessons.md`](./lessons.md) + Engram if anything non-obvious was discovered.

---

## 1. Cards: Add, Modify, or Restyle a Card

1. **Write a Seam Test First:**
   - In [`tests/seam_tests.rs`](../../../tests/seam_tests.rs), construct an `AppState::new()`, populate the relevant fields in `state.snapshot`, call `ui::render_screen(&state.snapshot, &state.collapsed_cards, None, 48)`, and assert on `ui::strip_ansi(&render_out.lines.join("\n"))`.
2. **Update Renderer (`src/ui.rs`):**
   - Use `draw_top(title, is_collapsed, width)`, `draw_row(left, right, width)`, `draw_divider(width)`, and `draw_bottom(width)`.
   - Push a `CardBounds` entry for the card (`start_line` to `end_line`) and any interactive sub-rows (`action_type: Some(...)`) **before** or **after** depending on hit-test priority (`handle_mouse_click` checks specific `action_type.is_some()` bounds before card header bounds).
3. **Verify Widths:**
   - Test rendering at narrow widths (`36`, `42`, `48`) so no label or button overflows the right border `│`.
4. **Update Catalog:**
   - Keep [`references/cards.md`](./cards.md) synchronized with any title or row change.

---

## 2. Session Fields: Add Data from Pi (`.jsonl`) or OpenCode (`opencode.db`)

1. **Extend Model (`src/models.rs`):**
   - Add the new field to `AgentSession` (ensure `Default` and `PartialEq` remain derived/valid).
2. **Pi / Gentle Shell (`src/pi.rs`):**
   - Parse the field incrementally inside `PiTail::ingest_line`. Never re-read the entire `.jsonl` file from byte `0` unless the file was truncated or rotated.
3. **OpenCode (`src/opencode/session.rs`):**
   - Extract the field from `session_v2` or `collect_message_stats` (preserving the `MESSAGE_STATS_CACHE` keyed by `(session_id, time_updated)`).
4. **Add Seam Test:**
   - Add assertions in `test_pi_jsonl_incremental_tail_and_context_cache` or `test_opencode_db_telemetry_scanning` in [`tests/seam_tests.rs`](../../../tests/seam_tests.rs).

---

## 3. Quotas: Add or Adjust Model Family Quota Rules

1. **Model Family Detection (`src/quotas/cliproxy.rs`):**
   - Update `resolve_model_family(model_str)` if adding a new model family keyword.
   - Update `apply_quota_data` if the quota provider returns a new `displayName` group in `groups`.
2. **Dynamic Evaluation:**
   - `evaluate_account_for_model(acc, active_model)` resolves the `5h` and `Wk` pools for the active session's model family (`Claude`, `GPT`, `DeepSeek`, `Qwen`, `Gemini`), falling back to `Gemini` when no specific model is active or when the family has no dedicated pool.
3. **Test Multi-Model Behavior:**
   - Extend `test_dynamic_model_family_quota_health_claude_gpt_deepseek_gemini` in [`tests/seam_tests.rs`](../../../tests/seam_tests.rs).

---

## 4. New Data Source (Filesystem, Subprocess, or IPC)

1. **Model (`src/models.rs`):**
   - Add the struct/field with `#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]`.
2. **Collector Module:**
   - For subprocesses: always enforce a timeout (e.g., `3s` via `try_wait` loop) and `Stdio::null()` for `stdin`. For `git`, always set `GIT_OPTIONAL_LOCKS=0`.
   - For SQLite: open with `SQLITE_OPEN_READ_ONLY` + `SQLITE_OPEN_NO_MUTEX` and cache by file `mtime` + TTL.
3. **Hook into `refresh_all` (`src/app.rs`):**
   - Run blocking I/O inside `tokio::task::spawn_blocking` alongside the existing collectors in `tokio::join!`.
   - Note that `refresh_all` automatically triggers via `MainMsg` in `--once` mode as well as the live TUI loop.
4. **Watcher Hook (`src/main.rs`):**
   - If backed by a local file, register its path in `spawn_fs_watcher` so changes wake the loop with a `300ms` debounce.
