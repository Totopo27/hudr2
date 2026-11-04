#!/usr/bin/env bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="${SCRIPT_DIR}/target/release/hudr2"

if [ ! -f "$BIN" ]; then
    echo "[ERROR] No se encontró el binario compilado en: $BIN"
    echo "Ejecuta primero: cargo build --release"
    exit 1
fi

exec "$BIN" "$@"
