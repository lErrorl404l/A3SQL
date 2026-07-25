# Building

A3DB has two build steps: the **Rust extension** (native DLL/SO) and the **Arma 3 addon** (PBO files via HEMTT).

## Prerequisites

- [Rust](https://rustup.rs/) 1.80+ (stable)
- [HEMTT](https://hemtt.dev/) 1.20+ — Arma addon build tool
- [CBA_A3](https://github.com/CBATeam/CBA_A3) — for addon compilation (HEMTT dev dependencies)
- [UV](https://docs.astral.sh/uv/) (recommended) — Python tooling runner
- Wine or Proton (optional) — for code signing on Linux (see Code Signing below)

## Quick Build

```bash
# 1. Build the Rust extension
cargo build --release -p a3sql

# 2. Copy to Arma 3 directory
cp target/release/liba3sql.so "/path/to/Arma 3/@a3sql/a3sql_x64.so"
# or for Windows cross-compile:
cp target/x86_64-pc-windows-gnu/release/a3sql.dll "/path/to/Arma 3/@a3sql/a3sql_x64.dll"

# 3. Build addon PBOs
hemtt build

# 4. Result in .hemttout/build/
```

## Cross-compilation

The extension targets 4 architectures via `rust-toolchain.toml`:

```bash
# Linux x86_64 (native)
cargo build --release --target x86_64-unknown-linux-gnu -p a3sql

# Linux 32-bit
cargo build --release --target i686-unknown-linux-gnu -p a3sql

# Windows x86_64 (MinGW cross)
cargo build --release --target x86_64-pc-windows-gnu -p a3sql

# Windows 32-bit
cargo build --release --target i686-pc-windows-gnu -p a3sql
```

## Testing

```bash
cargo test -p a3sql            # 118+ tests
cargo clippy -p a3sql          # lint
cargo fmt --check             # formatting
hemtt check                   # SQF + config validation
```

## Developer Tooling

### Python tools (UV)

All development Python scripts are in `tools/`. Run with UV:

```bash
# Environment report
uv run python3 tools/setup.py --report

# SQF validation
uv run python3 tools/sqf_validator.py addons/

# Config style check
uv run python3 tools/config_style_checker.py

# Generate signing keys (requires Arma 3 Tools)
uv run python3 tools/setup.py --keys
```

### Code Signing (BIKey)

A3DB supports code signing via `DSSignFile.exe` (from Arma 3 Tools) running under Wine/Proton on Linux:

```bash
# Generate signing keys
uv run python3 tools/setup.py --keys
# Creates keys/a3sql.bikey + keys/a3sql.biprivatekey

# HEMTT signs automatically during `hemtt release`
# Config in .hemtt/project.toml → [hemtt.signing]
```

The signing key is configured in `.hemtt/project.toml`. On CI, use GitHub Secrets or an encrypted key file.

### Steam Library Discovery

`tools/proton.py` automatically finds Arma 3 via Steam VDF (`libraryfolders.vdf`), including Wine prefix and workshop paths. Used by `tools/setup.py` for file patching symlink setup.

## CI/CD

GitHub Actions (`.github/workflows/ci.yml`) runs on push/PR to `main`:

1. **validate** — SQF syntax validation + config style check + HEMTT check + BOM check
2. **test** — cargo test + clippy + rustfmt on ubuntu-latest
3. **build-linux** — cross-compile for x86_64 and i686 Linux
4. **build-windows** — cross-compile for x86_64 and i686 Windows (MinGW)
5. **build-addon** — downloads all artifacts, runs `hemtt build`, outputs PBOs
6. **workshop** — publish to Steam Workshop (on release)

On a release publish, it creates `a3sql-release.zip` with the full addon.

## Standalone Server

The a3sql-server binary is part of the same workspace:

```bash
cargo run --bin a3sql-server -- --port 33307
cargo build --release --bin a3sql-server
```

## Arma 3 Installation

Deploy the addon to your Arma 3 directory:

```bash
# Linux (native)
cp extension/target/release/a3sql_x64.so ~/.local/share/Steam/steamapps/common/Arma\ 3/@a3sql/

# Windows (cross-compiled via MinGW)
cp target/x86_64-pc-windows-gnu/release/a3sql.dll "/path/to/Arma 3/@a3sql/a3sql_x64.dll"
```

The HEMTT-built PBOs go in the same `@a3sql` directory alongside the DLLs.
