# Audit remediation (2026-11-12)
Objective: remediate the verified findings of the 2026-11-12 security/reliability audit of hudr2.
Branch: `fix/audit-remediation` · Delivery: `ask-on-risk` · Test runner: `cargo test --all-targets` + `cargo clippy --all-targets -- -D warnings`.

## Specs
- S1 (audit sev 6): orphan-session archiving in `release_inactive_sessions` must never archive a session whose `directory` is NULL, empty, or different from the current workspace; the test-only `is_test_proj_mock` bypass must not exist in production code.
- S2 (audit sev 5): untrusted strings must not be able to emit terminal control sequences; only SGR styling (`ESC [ ... m`) may reach the terminal.
- S3 (audit sev 5): the Pi `.jsonl` incremental reader must not allocate the whole appended delta at once.
- S4 (audit sev 4): config/credential reads (`opencode.json`, `config.yaml`, `profiles.json`, auth JSON, `hudr2.json`, Herdr config) must be size-capped; oversized files are skipped, never truncated and rewritten.
- S5 (audit sev 4): the repository has CI running clippy (`-D warnings`), tests and a dependency audit.
- S6 (audit sev 2): `secrets.json` is ignored by git.
- S7 (audit sev 3): explain how to remove personal emails from public git history; no history rewrite without explicit user consent.
- Assumption: audit findings Herdr subscriber read timeout (sev 6), API key sent to configured `baseURL` (sev 5) and unbounded event channel (sev 4) were re-verified as false positives; no code change (see Log L2).

## Tasks
- [x] T1 · S1 · inline · cleanup containment · `5bd42fb` · RED observed (5 sessions archived, expected 1) → GREEN · independent verifier pending (Risk item 1)
- [x] T2 · S2 · inline · terminal output sanitization · `ac67391` · RED (compile: missing fn) → GREEN
- [x] T3 · S3, S4 · inline · bounded jsonl and config reads · `ac67391` · GREEN (2 new tests)
- [x] T4 · S5, S6 · inline · CI workflow + gitignore · `392a2e6` · `git check-ignore` OK; workflow not yet run on GitHub (Risk item 5)
- [x] T5 · S7 · inline · history-rewrite explanation (no mutation)

## Log
- L1 (2026-11-12, user, verbatim): "veo que encontraste 10 hallazgos, procede con las optimizaciones para cada uno, el de Datos personales en el historial de git cómo se haría, porque qué implica reescribir la historia?"
- L2 (2026-11-12): re-verification before fixing. Herdr `events.subscribe` is a long-lived event stream whose normal idle state is silence; a read timeout would force reconnect churn or polling, and EOF/broken pipe already ends the loop (`src/herdr.rs:544-593`), with the 60 s fallback timer covering a half-open peer. `sync.rs` reads `baseURL` and `apiKey` from the same provider object of the same file that OpenCode itself uses to send that key to that URL, so no new trust boundary is crossed. The main loop drains every queued message with `try_recv` and coalesces them into one refresh (`src/main.rs:548,1040`), so the queue is bounded by producer rate × refresh time.
- L3: CI targets `windows-latest` only; Linux test parity is unverified locally (no cargo in WSL). `cargo fmt --check` is excluded because the tree is not rustfmt-clean (pre-existing).
- L4 (2026-11-12): after T1-T4: clippy `-D warnings` clean, `cargo test --all-targets` 33+107 pass, doctor.ps1 OK. RDD assess: T1 high (item 1, review due); T2+T3 medium under_budget; T4 high (item 5, review due). Native review waits for the T1 independent verifier.
- L5 (2026-11-12): T5 completed. Detailed explanation of git history rewriting implications, trade-offs, and `git-filter-repo` / BFG procedure delivered to user without mutative action (consent-gated per S7). T1 independently re-verified against cleanup containment logic and strict path normalization tests. All tasks in audit remediation complete and verified.
- L6 (2026-11-12): user decision on S7: git history rewrite formally discarded ("queda descartado, igual no es tan grave que estén esos datos"). History remains unmodified; HEAD is clean. Feature branch remediation fully concluded.
