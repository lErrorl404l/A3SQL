#!/bin/bash
# a3db dev launch — builds and copies to game-directory prefix path, then launches
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

GAME_DIR="/ext/SteamLibrary/steamapps/common/Arma 3"

echo "→ Building Rust DLL (Windows x86_64)..."
cargo build --release --target x86_64-pc-windows-gnu --manifest-path extension/Cargo.toml

echo "→ Copying DLL to project root..."
cp target/x86_64-pc-windows-gnu/release/a3db.dll a3db.dll
cp a3db.dll a3db_x64.dll

echo "→ Building addon with HEMTT..."
hemtt build

echo "→ Copying to ${GAME_DIR}/z/a3db (CBA convention: -mod=z/a3db)..."
rm -rf "${GAME_DIR}/z/a3db"
mkdir -p "${GAME_DIR}/z/a3db/addons"
cp .hemttout/build/a3db.dll "${GAME_DIR}/z/a3db/"
cp .hemttout/build/a3db_x64.dll "${GAME_DIR}/z/a3db/"
cp .hemttout/build/addons/*.pbo "${GAME_DIR}/z/a3db/addons/"
cp .hemttout/build/mod.cpp "${GAME_DIR}/z/a3db/"
cp .hemttout/build/meta.cpp "${GAME_DIR}/z/a3db/"
# Also sync dev output (HEMTT launch may reference this path)
cp .hemttout/build/*.dll .hemttout/dev/ 2>/dev/null || true
cp .hemttout/build/*.so .hemttout/dev/ 2>/dev/null || true
cp .hemttout/build/addons/*.pbo .hemttout/dev/addons/ 2>/dev/null || true

echo "→ Launching through Steam..."
# HEMTT adds the dev mod path automatically; we also add the real dir in the game dir via passthrough
hemtt launch -Q -- -mod=z\\a3db
