#!/usr/bin/env bash
# hudr2 doctor: verifies tests, clippy, release build, safety guards, and --once frame integrity.
# Usage: bash .agents/skills/hudr2/scripts/doctor.sh [--quick]
set -uo pipefail

S="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO="${HUDR2_REPO:-$(cd "$S/../../.." && pwd)}"
cd "$REPO"

fails=0
pass(){ printf 'PASS  %s\n' "$*"; }
fail(){ printf 'FAIL  %s\n' "$*"; fails=$((fails+1)); }
warn(){ printf 'WARN  %s\n' "$*"; }

[ -f "$REPO/Cargo.toml" ] || { fail "Cargo.toml not found at $REPO"; exit 1; }

if [ "${1:-}" != "--quick" ]; then
  if cargo test -- --test-threads=1 >/dev/null 2>&1; then
    pass "cargo test passed"
  else
    fail "cargo test failed"
  fi

  if cargo clippy --all-targets -- -D warnings >/dev/null 2>&1; then
    pass "cargo clippy --all-targets (0 warnings)"
  else
    fail "cargo clippy reported warnings or errors"
  fi

  if cargo build --release >/dev/null 2>&1; then
    pass "cargo build --release"
  else
    fail "cargo build --release failed"
  fi
fi

grep -q 'GIT_OPTIONAL_LOCKS' "$REPO/src/git.rs" && pass "git lock-free guard (GIT_OPTIONAL_LOCKS=0)" || fail "src/git.rs missing GIT_OPTIONAL_LOCKS=0"
grep -q 'pub fn is_sensitive_file' "$REPO/src/git.rs" && pass "git diff sensitive-file guard" || fail "src/git.rs missing is_sensitive_file"
if grep -q '?1000h\\x1b\[?1006h' "$REPO/src/main.rs" && ! grep -q 'EnableMouseCapture' "$REPO/src/main.rs"; then
  pass "button-only SGR mouse capture guard"
else
  fail "src/main.rs must use button-only SGR mouse capture"
fi
grep -q 'evaluate_account_for_model' "$REPO/src/ui.rs" && pass "dynamic multi-model quota evaluation in Integraciones" || fail "src/ui.rs missing evaluate_account_for_model"

BIN="$REPO/target/release/hudr2"
[ -f "$BIN.exe" ] && BIN="$BIN.exe"
if [ -x "$BIN" ] || [ -f "$BIN" ]; then
  ver="$("$BIN" --version 2>/dev/null || true)"
  if echo "$ver" | grep -qE '^hudr2 [0-9]+\.[0-9]+\.[0-9]+'; then
    pass "--version CLI flag ($ver)"
  else
    fail "--version CLI flag failed: $ver"
  fi
  snap="$("$BIN" --once --cwd "$REPO" 2>/dev/null)"
  if [ -n "$snap" ]; then
    missing=""
    for card in "Estado" "Contexto" "Proyecto" "Integraciones" "Cuota de Modelos" "Cambios" "Engram" "Stack" "Herramientas" "MCP"; do
      grep -qF "$card" <<<"$snap" || missing="$missing $card"
    done
    [ -z "$missing" ] && pass "--once renders all 10 catalog cards" || fail "--once missing cards:$missing"
  else
    fail "--once produced empty output"
  fi
else
  warn "binary not built yet: $BIN"
fi

[ "$fails" -eq 0 ] && { echo "doctor: OK"; exit 0; } || { echo "doctor: $fails failing check(s)"; exit 1; }
