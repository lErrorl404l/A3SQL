# Development Setup

Setting up your environment to build and test A3SQL locally, following ACE3/CBA conventions.

## Prerequisites

- [Rust](https://rustup.rs/) 1.80+
- [HEMTT](https://hemtt.dev/) 1.20+
- [UV](https://docs.astral.sh/uv/) (recommended for Python tools)
- Arma 3 (for in-game testing)
- CBA_A3 (workshop or source)
- Wine / Proton (Linux) — optional, for code signing

## Quick Start

```bash
# 1. Clone
git clone https://github.com/lErrorl404l/a3sql.git
cd a3sql

# 2. Build the Rust DLL (Linux native)
cargo build --release -p a3sql

# 3. Build the addon PBOs
hemtt build

# 4. Setup file patching
uv run python3 tools/setup.py

# 5. Launch (builds + copies + starts Arma 3)
bash tools/launch.sh
# Or: hemtt launch
```

## Python Tooling (UV)

All development tools use UV for dependency management:

```bash
# Run any tool
uv run python3 tools/sqf_validator.py addons/

# Environment report
uv run python3 tools/setup.py --report

# Generate signing keys (requires Arma 3 Tools via Wine)
uv run python3 tools/setup.py --keys
```

## File Patching

File patching allows SQF changes to take effect by simply restarting the mission (no PBO rebuild needed):

```bash
# Setup symlink (auto-detects Arma 3 via Steam VDF)
uv run python3 tools/setup.py

# Launch with file patching
hemtt launch
```

CBA function caching must be disabled for file patching to work. This is already configured in `script_component.hpp`:

```cpp
#define DISABLE_COMPILE_CACHE
```

## Code Signing

Release builds should be signed with a BI key. Keys are generated using Arma 3 Tools' DSSignFile running through Wine/Proton:

```bash
uv run python3 tools/setup.py --keys
# Creates keys/a3sql.bikey + keys/a3sql.biprivatekey
```

HEMTT signs automatically during `hemtt release`. Config is in `.hemtt/project.toml`.

## Testing

### Rust tests (CI — fastest feedback)

```bash
cargo test --lib -p a3sql        # 162+ tests
cargo clippy -p a3sql            # lint
cargo fmt --check               # formatting
```

### SQF validation (static analysis, no game needed)

```bash
uv run python3 tools/sqf_validator.py addons/
uv run python3 tools/config_style_checker.py
```

### In-game smoke test

```sqf
// Run from Arma 3 debug console (server only)
execVM "tests/a3sql_smoke_test.sqf";
// Check RPT log for "A3SQL Smoke Test" results
```

## Project Structure

```
a3sql/
├── extension/           # Rust DLL (cdylib + rlib)
│   └── src/
│       ├── lib.rs       # C ABI (RVExtension, dispatcher)
│       ├── engine/      # In-memory database engine
│       ├── parser/      # SQL parser (sqlparser-rs dialect)
│       └── bin/         # a3sql-server (standalone TCP)
├── addons/
│   ├── main/            # Core addon (CfgPatches, macros)
│   └── sql/             # SQL engine SQF API (CfgFunctions)
├── include/             # CBA build-time includes
├── tools/               # Python dev tools (UV-managed)
├── .hemtt/              # HEMTT build config + hooks
├── keys/                # BI code signing keys
├── .editorconfig        # Editor consistency (tab=SQF, space=Rust)
├── .gitattributes       # linguist-language=SQF
└── pyproject.toml        # UV configuration
```
