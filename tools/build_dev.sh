#!/bin/bash
# a3db dev build — build Rust DLL + HEMTT addon, sync to dev output
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "→ Building Rust DLL (Windows x86_64)..."
cargo build --release --target x86_64-pc-windows-gnu --manifest-path extension/Cargo.toml

echo "→ Copying DLL to project root..."
cp target/x86_64-pc-windows-gnu/release/a3db.dll a3db.dll

echo "→ Building addon with HEMTT..."
hemtt build

echo "→ Syncing to dev output..."
cp .hemttout/build/a3db.dll .hemttout/dev/a3db.dll 2>/dev/null || true
cp .hemttout/build/addons/*.pbo .hemttout/dev/addons/ 2>/dev/null || true

echo "✓ Dev build ready — launch with: hemtt launch"
