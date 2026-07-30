#!/bin/bash
# Copy built Rust extension binaries to the project root with Arma 3 naming.
#
# Usage:
#   ./tools/copy_ext_binaries.sh              # copy existing artifacts
#   ./tools/copy_ext_binaries.sh build         # cargo build + copy
#
# Arma 3 naming (forceRenameLib = "a3sql"):
#   a3sql_x64.dll  — Windows 64-bit (x86_64)
#   a3sql.dll      — Windows 32-bit (i686)
#   a3sql_x64.so   — Linux 64-bit  (x86_64)
#   a3sql.so       — Linux 32-bit  (i686)

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [ "${1:-}" = "build" ]; then
    echo "Building extension..."
    cargo build --release --manifest-path "$PROJECT_ROOT/extension/Cargo.toml"
fi

EXT_TARGET="$PROJECT_ROOT/extension/target"

# Linux x86_64
if [ -f "$EXT_TARGET/release/liba3sql.so" ]; then
    cp "$EXT_TARGET/release/liba3sql.so" "$PROJECT_ROOT/a3sql_x64.so"
    echo "Copied a3sql_x64.so"
fi

# Linux i686
if [ -f "$EXT_TARGET/i686-unknown-linux-gnu/release/liba3sql.so" ]; then
    cp "$EXT_TARGET/i686-unknown-linux-gnu/release/liba3sql.so" "$PROJECT_ROOT/a3sql.so"
    echo "Copied a3sql.so"
fi

# Windows x86_64
if [ -f "$EXT_TARGET/x86_64-pc-windows-gnu/release/a3sql.dll" ]; then
    cp "$EXT_TARGET/x86_64-pc-windows-gnu/release/a3sql.dll" "$PROJECT_ROOT/a3sql_x64.dll"
    echo "Copied a3sql_x64.dll"
fi

# Windows i686
if [ -f "$EXT_TARGET/i686-pc-windows-gnu/release/a3sql.dll" ]; then
    cp "$EXT_TARGET/i686-pc-windows-gnu/release/a3sql.dll" "$PROJECT_ROOT/a3sql.dll"
    echo "Copied a3sql.dll"
fi

echo "Done."
