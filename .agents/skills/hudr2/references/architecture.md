# HUDR2 Architecture Reference

## Module Map

| File | Responsibility |
| :--- | :--- |
| [`src/main.rs`](../../../src/main.rs) | CLI parser (`--once`, `--pane`, `--cwd`, `--config`, `--help`), SGR button-only mouse guard (`\x1b[?1000h\x1b[?1006h`), `notify` 300ms debounced filesystem watcher, Herdr IPC subscriber hookup, and reactive `mpsc` event loop. |
| [`src/app.rs`](../../../src/app.rs) | Orchestrates `refresh_all` (parallel `spawn_blocking` tasks for session, git, engram, tree, MCP, ecosystem + `quotas_cache.json` hydration), `PartialEq` snapshot diffing, state-driven `force_refresh_with_notify` (active-first 0s, 3s staggered background + yellow-zone candidate pre-warming + live progress notices), non-blocking `sync_integrations_action_with_notify`, session release, and critical account migration. |
| [`src/models.rs`](../../../src/models.rs) | Data structures (`TelemetrySnapshot`, `AgentSession`, `AccountQuota`, `QuotaPool`, `GitTelemetry`, `GitFileChange`, `EngramTelemetry`, `HandoffSummary`, `ProjectTreeData`, `McpServer`, `EcosystemTelemetry`, `HerdrPane`). All derive `Clone, Debug, PartialEq`. |
| [`src/state.rs`](../../../src/state.rs) | `AppState` (`snapshot`, `collapsed_cards`, `hidden_cards`, `card_bounds`, `scroll_offset`, `flash_notice`, `selected_diff_file`, `needs_redraw`), atomic config persistence (`load_config` / `save_config`), and mouse hit-testing (`handle_mouse_click`). |
| [`src/ui.rs`](../../../src/ui.rs) | Pure ANSI renderer (`render_screen` / `render_screen_with_diff`), width-safe primitives (`strip_ansi`, `string_width`, `truncate_to_width`, `draw_top`, `draw_row`, `draw_divider`, `draw_bottom`), and all 9 cards + footer. |
| [`src/pi.rs`](../../../src/pi.rs) | Pi & Gentle Shell telemetry: incremental byte-offset `.jsonl` reader (`PiTail`), sibling `.jsonl` detection on `/new`, context limit resolution (`cpamc-models-cache.json` -> `models-store.json` -> `resolve_model_context_limit`), and `settings.json` default model fallback. |
| [`src/opencode/`](../../../src/opencode/mod.rs) | OpenCode SQLite (`opencode.db`) reader (`session.rs`), `time_updated` message stats cache, model normalization & context limits (`model_info.rs`), and account/pool prefix extraction (`prefix.rs`). |
| [`src/quotas/`](../../../src/quotas/mod.rs) | Direct module re-exports (`mod.rs`) and `cliproxy.rs` adapter: credential discovery, HTTP quota polling, dynamic multi-model family health (`Gemini`, `Claude`, `GPT`, `DeepSeek`, `Qwen`), `sorted_known_prefixes`, and pacing/reset formatters. |
| [`src/herdr.rs`](../../../src/herdr.rs) | `HerdrClient`: direct NDJSON socket/named-pipe IPC (`pane.list`, `pane.close`, `events.subscribe`) for Herdr and TelarDev, CLI fallback (`herdr` / `telardev` / `telar`), wire event parser (`parse_wire_event`), `metadata_tokens` (`pane.tokens`) extraction, and pane/tab correlation. |
| [`src/git.rs`](../../../src/git.rs) | Lock-free git runner (`GIT_OPTIONAL_LOCKS=0`, 3s timeout), zero-subprocess `.git/HEAD` branch & short SHA reader (`read_git_head_short` supporting loose refs, `packed-refs`, and worktree `commondir`), cross-session conflict radar (`detect_cross_session_conflicts` / `collect_cross_session_conflicts`), `porcelain=v1 -z` + `diff HEAD --numstat -z` parser, `is_sensitive_file` filter, and inline/Quick Peek diff preview (`read_file_diff_preview` / `read_file_quick_peek`). |
| [`src/engram.rs`](../../../src/engram.rs) | Read-only SQLite `engram.db` reader (`mtime` cache), raw TCP HTTP/1.0 daemon `/health` probe (`127.0.0.1:7437`), and latest `session_summary` (`HandoffSummary`) parser. |
| [`src/tree.rs`](../../../src/tree.rs) | Project tree scanner (`TREE_MAX_ENTRIES = 2000`, bounded best-entry heap, `is_real_dir` symlink guard, `TREE_SKIP_NAMES`) and deduplicated MCP server discovery (`opencode.json` + Pi `mcp.json`, maximum 1 MiB per config). |
| [`src/ecosystem.rs`](../../../src/ecosystem.rs) | One-shot ecosystem tool version detector (`pi`, `hudr2`, `gentle-shell`, `gentle-ai`, `engram`, `herdr`) with 3s subprocess timeout. |
| [`src/cleanup.rs`](../../../src/cleanup.rs) | Inactive/orphan session archiver (`d` key) and live critical-account session migrator (`x` key) with model-family-aware target selection. |
| [`src/paths.rs`](../../../src/paths.rs) | Cross-platform path resolution with environment variable overrides (`OPENCODE_DB_PATH`, `ENGRAM_DB_PATH`, `CLIPROXY_AUTHS_DIR`, `HUDR2_CONFIG_PATH`, `PI_SESSION_FILE`, `PI_AGENT_DIR`). |

---

## Reactive Data Flow

```text
┌─────────────────────────┐   ┌────────────────────────────┐   ┌──────────────────────┐
│ Crossterm Terminal Thread│   │ notify Watcher (300ms deb.)│   │ Herdr IPC Subscriber │
│ (Keys Press, SGR Mouse)  │   │ (.git, DBs, Pi .jsonl)     │   │ (events.subscribe)   │
└────────────┬────────────┘   └─────────────┬──────────────┘   └──────────┬───────────┘
             │                              │                             │
             └──────────────────────────────┼─────────────────────────────┘
                                            ▼
                                  mpsc::unbounded_channel
                                            │
                                            ▼
                          refresh_all(&app_state) -> snapshot != prev?
                                            │ yes
                                            ▼
                                 state.needs_redraw = true
                                            │
                                            ▼
                   ui::render_screen_with_diff(...) -> stdout ANSI flush
```

---

## Multi-Agent Session Resolution (`src/pi.rs` & `src/opencode/`)

1. **Pi / Gentle Shell (`.jsonl`):**
   - Resolved from `PI_SESSION_FILE`, or Herdr pane `agent_session_id` ending in `.jsonl`, or newest `.jsonl` inside `<agent_dir>/sessions/<encoded_cwd>/`.
   - `PiTail` seeks to the last read byte offset (`SeekFrom::Start(offset)`) and parses only newly appended lines:
     - `type: "model_change"` / `"thinking_level_change"`
     - `type: "message"` (`role: "assistant"` usage tokens, cost, tool calls; `role: "toolResult"` with `toolName: "todo"` for `SessionTodo` items).
   - Context window resolved from `<agent_dir>/cpamc-models-cache.json`, then `<agent_dir>/models-store.json`, then `resolve_model_context_limit`.
2. **OpenCode (`opencode.db`):**
   - Resolves `opencode.db` by max `mtime` across `opencode.db`, `opencode.db-wal`, and `opencode.db-shm`.
   - Pairs modern Drizzle tables (`session` + `message` + `part`, OpenCode 1.19+) cleanly when coexisting with legacy `session_v2` + `session_message` + `session_part`, hydrating `agent`, `model` (`providerID` + `modelID`), `tokens_input`, `tokens_output`, and `cost` from `message.data` / `part.data` JSON when `session` lacks SQL columns.
   - Message token/tool/file-change stats are cached by `(session_id, time_updated)` to avoid rescanning JSON blobs when idle.
   - A bounded `opencode api get /api/session` read supplements the root session with direct child sessions (`parentID`). Each child contributes its own role, account/model, cost, tokens, and created/idle timestamps; configured profiles and historical prefixes are not execution evidence.

---

## Multi-Model Quotas (`src/quotas/cliproxy.rs`)

- `apply_quota_data` parses `groups` from the quota endpoint into `AccountQuota.pools`:
  - Base pools: `Gemini Wk`, `Gemini 5h`, `Claude Wk`, `Claude 5h`.
  - Dynamic extra pools when present in `groups`: `GPT Wk`, `GPT 5h`, `DeepSeek Wk`, `DeepSeek 5h`, `Qwen Wk`, `Qwen 5h`.
- `resolve_model_family(model_str)` maps the active session model to `"Claude"`, `"GPT"`, `"DeepSeek"`, `"Qwen"`, or `"Gemini"`.
- `evaluate_account_for_model(acc, active_model)` selects the `5h` and `Wk` pools matching the active model's family (with `GPT` falling back to `Claude` when the provider shares their bucket, and unknown/empty models defaulting to `Gemini`).
- API-key balances use `api_account_attributed_usage`: root-session cost only when the root belongs to that account, plus each matching child session's own cost. A child or pane without attributable cost never inherits the root session's global cost.
- Thresholds:
  - **Critical (`is_critical`):** `used >= 85%` (`remaining <= 15%`) -> `CORAL` (`[AGOTÁNDOSE]`) + migration alert (`x`).
  - **Warning (`is_warning`):** `70% <= used < 85%` (`15% < remaining <= 30%`) -> `AMBER` (`[AVISO 70%]`).
  - **Healthy:** `remaining > 30%` -> `MINT`.
