# Building

A3DB has two build steps: the **Rust extension** (native DLL/SO) and the **Arma 3 addon** (PBO files via HEMTT).

## Prerequisites

- [Rust](https://rustup.rs/) 1.80+ (stable)
- [HEMTT](https://hemtt.dev/) 1.20+ — Arma addon build tool
- [CBA_A3](https://github.com/CBATeam/CBA_A3) — for addon compilation (HEMTT dev dependencies)

## Quick Build

```bash
# 1. Build the Rust extension
cargo build --release -p a3db

# 2. Copy to Arma 3 directory
cp target/release/liba3db.so "/path/to/Arma 3/@a3db/a3db_x64.so"
# or for Windows cross-compile:
cp target/x86_64-pc-windows-gnu/release/a3db.dll "/path/to/Arma 3/@a3db/a3db_x64.dll"

# 3. Build addon PBOs
hemtt build

# 4. Result in .hemttout/build/
```

## Cross-compilation

The extension targets 4 architectures via `rust-toolchain.toml`:

```bash
# Linux x86_64 (native)
cargo build --release --target x86_64-unknown-linux-gnu -p a3db

# Linux 32-bit
cargo build --release --target i686-unknown-linux-gnu -p a3db

# Windows x86_64 (MinGW cross)
cargo build --release --target x86_64-pc-windows-gnu -p a3db

# Windows 32-bit
cargo build --release --target i686-pc-windows-gnu -p a3db
```

## Testing

```bash
cargo test -p a3db            # 118+ tests
cargo clippy -p a3db          # lint
cargo fmt --check             # formatting
hemtt check                   # SQF + config validation
```

## CI/CD

GitHub Actions (`.github/workflows/ci.yml`) runs on push/PR to `main`:

1. **test** — cargo test + clippy + rustfmt on ubuntu-latest
2. **build-linux** — cross-compile for x86_64 and i686 Linux
3. **build-windows** — cross-compile for x86_64 and i686 Windows (MinGW)
4. **build-addon** — downloads all artifacts, runs `hemtt build`, outputs PBOs

On a release publish, it creates `a3db-release.zip` with the full addon.

## Standalone Server

The a3db-server binary is part of the same workspace:

```bash
cargo run --bin a3db-server -- --port 33307
cargo build --release --bin a3db-server
```

## Arma 3 Installation

Deploy the addon to your Arma 3 directory:

```bash
# Linux (native)
cp extension/target/release/a3db_x64.so ~/.local/share/Steam/steamapps/common/Arma\ 3/@a3db/

# Windows (cross-compiled via MinGW)
cp target/x86_64-pc-windows-gnu/release/a3db.dll "/path/to/Arma 3/@a3db/a3db_x64.dll"
```

The HEMTT-built PBOs go in the same `@a3db` directory alongside the DLLs.
