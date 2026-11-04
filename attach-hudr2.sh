#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="${SCRIPT_DIR}/target/release/hudr2"

if [ ! -f "$BIN" ]; then
    echo "[ERROR] No se encontró el binario compilado en: $BIN"
    echo "Ejecuta primero: cargo build --release"
    exit 1
fi

# Intentar acoplar a un split lateral si herdr está corriendo
if command -v herdr >/dev/null 2>&1; then
    herdr pane split --current --direction right --ratio 0.80 --no-focus 2>/dev/null || true
fi

exec "$BIN" "$@"
