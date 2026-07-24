# A3DB — Arma 3 Database Engine

An embeddable SQL database engine for Arma 3 mods. Like **SQLite for Arma** — a
Rust `callExtension` that lets modders write SQL directly in SQF.

```sqf
private _result = ["a3db", "CREATE TABLE weapons (id STRING PRIMARY KEY, name STRING, caliber STRING, barrelLength FLOAT)"] callExtension;
private _result = ["a3db", "INSERT INTO weapons VALUES ('rhs_m4a1', 'M4A1', '5.56x45mm', 368.3)"] callExtension;
private _result = ["a3db", "SELECT * FROM weapons WHERE caliber = '5.56x45mm'"] callExtension;
```

## Features

- **Full SQL**: `CREATE TABLE`, `INSERT`, `SELECT`, `UPDATE`, `DELETE`, `DROP`
- **Fuzzy matching**: `%%` operator with trigram similarity for weapon/model name matching
- **JOINs**: `INNER JOIN`, `LEFT JOIN`, `CROSS JOIN` with `ON` clause
- **Indices**: `BTREE` for lookups, `TRIGRAM` for fuzzy search
- **Ordering & pagination**: `ORDER BY`, `LIMIT`, `OFFSET`
- **Aggregates**: `COUNT`, `SUM`, `AVG`, `MIN`, `MAX` with `GROUP BY`
- **Transactions**: `BEGIN` / `COMMIT` / `ROLLBACK` with savepoints
- **Multi-format export/import**: JSON, CSV, SQL dump, Binary
- **Persistence**: `save` / `load` to/from files via binary format
- **Multi-statement**: Run `;`-separated SQL batches
- **Multi-dialect**: Accepts PostgreSQL, MySQL/MariaDB, SQLite, DataFusion-style SQL

## SQL Dialect

```sql
-- Tables
CREATE TABLE weapons (id STRING PRIMARY KEY, name STRING, caliber STRING, barrelLength FLOAT);
CREATE TABLE attachments (id STRING PRIMARY KEY, weaponId STRING, name STRING, mass FLOAT);

-- Types: STRING, INT, FLOAT, BOOL, STRINGS[], FLOATS[]

-- CRUD
INSERT INTO weapons VALUES ('rhs_m4a1', 'M4A1', '5.56x45mm', 368.3);
SELECT * FROM weapons WHERE caliber = '5.56x45mm';
SELECT name, caliber FROM weapons WHERE barrelLength > 400.0 ORDER BY barrelLength DESC;
UPDATE weapons SET caliber = '7.62x39mm' WHERE id = 'ak74';
DELETE FROM weapons WHERE barrelLength IS NULL;
DROP TABLE weapons;

-- Fuzzy match (trigram similarity)
SELECT * FROM weapons WHERE id %% 'rhs_m4';
-- matches rhs_m4a1, rhs_m4a1_carryhandle, etc.

-- JOINS
SELECT w.name, a.name FROM weapons w INNER JOIN attachments a ON w.id = a.weaponId;
SELECT * FROM weapons w LEFT JOIN attachments a ON w.id = a.weaponId;

-- Aggregates
SELECT COUNT(*) FROM weapons;
SELECT AVG(barrelLength) FROM weapons;
SELECT caliber, COUNT(*) AS cnt FROM weapons GROUP BY caliber;

-- Ordering & limits
SELECT * FROM weapons ORDER BY name ASC LIMIT 10 OFFSET 5;

-- Transactions
BEGIN;
INSERT INTO weapons VALUES ('test', 'Test', '9x19mm', 200.0);
ROLLBACK;  -- or COMMIT

-- Savepoints
SAVEPOINT sp1;
INSERT INTO weapons VALUES ('tmp', 'Temp', '5.56x45mm', 300.0);
ROLLBACK TO sp1;
RELEASE SAVEPOINT sp1;

-- Indices
CREATE INDEX idx_caliber ON weapons (caliber) USING BTREE;
CREATE INDEX idx_name_fuzzy ON weapons (name) USING TRIGRAM;
```

## SQF API

### Initialization

```sqf
// In init.sqf or CfgFunctions init:
private _version = "a3db" callExtension "version";
diag_log text format ["[A3DB] Loading: %1", _version];
```

### SQL Execution

```sqf
// Single SQL statement (STRING callExtension STRING):
private _result = "a3db" callExtension "SELECT * FROM weapons";

// SQL with args (STRING callExtension ARRAY):
private _result = ["a3db", "INSERT INTO weapons VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3)"] callExtension;

// Multi-statement (separate with semicolons):
private _result = "a3db" callExtension "CREATE TABLE t (id STRING); INSERT INTO t VALUES ('a'); SELECT * FROM t";
```

### Response Format

```
[returnCode, status, data]

Success: [0,"OK",result_data]
Error:   [-1,"ERR_CODE","error message"]

Error codes:
  ERR_PARSE    SQL parse error
  ERR_EXEC     Execution error
  ERR_TABLE    Table not found
  ERR_TYPE     Type mismatch
  ERR_PK       Primary key violation
  ERR_IO       File I/O error
  ERR_INTERNAL Internal error
```

`result_data` for `SELECT` is a JSON array of column names + rows:
```json
[["id","name","caliber","barrelLength"],["rhs_m4a1","M4A1","5.56x45mm",368.3]]
```

For JOINs with prefixed column names:
```json
[["weapons.id","weapons.name","attachments.name"],["rhs_m4a1","M4A1","M68 CCO"]]
```

### Commands

```sqf
// Version
private _version = "a3db" callExtension "version";
// → [0,"OK","a3db 0.1.0"]

// SQL dump
private _dump = "a3db" callExtension "dump_sql";
// → [0,"OK","CREATE TABLE weapons (...);..."]

// Export table as JSON
private _result = ["a3db", "export_json weapons"] callExtension;

// Export table as CSV
private _result = ["a3db", "export_csv weapons"] callExtension;

// Import from JSON (data in args)
private _result = ["a3db", "import_json weapons", [jsonData]] callExtension;

// Import from CSV
private _result = ["a3db", "import_csv weapons", [csvData]] callExtension;

// Full SQL dump
private _result = ["a3db", "dump_sql"] callExtension;

// Persist to file
private _result = ["a3db", "save", ["mission_db.bin"]] callExtension;

// Restore from file
private _result = ["a3db", "load", ["mission_db.bin"]] callExtension;
```

### CBA Functions

When using CBA (recommended), the addon registers these functions via `CfgFunctions`:

| Function | Description |
|---|---|
| `a3db_fnc_init` | Initialize extension, returns version string |
| `a3db_fnc_execute` | Execute SQL, returns parsed result |
| `a3db_fnc_loadJSON` | Import JSON data into a table |
| `a3db_fnc_dumpSQL` | Export full database as SQL dump |
| `a3db_fnc_exportJSON` | Export table as JSON |
| `a3db_fnc_exportCSV` | Export table as CSV |
| `a3db_fnc_exportSQL` | Export full database as SQL statements |
| `a3db_fnc_save` | Persist database to binary file |
| `a3db_fnc_load` | Restore database from binary file |

## Building

### Prerequisites

- [Rust](https://rustup.rs/) 1.80+ (for `std::sync::LazyLock`)
- [HEMTT](https://hemtt.dev/) 1.20+ — Arma 3 addon build tool
- C++ build tools (for Rust cross-compilation to Windows)
- MinGW-w64 (for Windows cross-compilation on Linux):
  ```bash
  sudo apt-get install mingw-w64 gcc-multilib
  ```

### Build the extension

```bash
# Build for Linux x86_64
cargo build --release

# Build for Windows (cross-compile from Linux)
cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target i686-pc-windows-gnu
```

### Build the addon

```bash
hemtt build
```

Output goes to `.hemttout/release/a3db/`.

### Run tests

```bash
# From workspace root
cargo test --lib

# Or from extension directory
cargo test --manifest-path extension/Cargo.toml
```

### Linting & validation

```bash
# Rust
cargo fmt --check
cargo clippy --all-targets

# SQF
sqflint addons/a3db/*.sqf
sqfvm --parse-only -i addons/a3db/fn_init.sqf

# Arma addon structure
hemtt check
```

## CI/CD

The project includes a GitHub Actions workflow (`.github/workflows/ci.yml`) that:

1. Runs `cargo test`
2. Builds for 4 targets: `x86_64-linux`, `i686-linux`, `x86_64-windows`, `i686-windows`
3. Runs `hemtt build` to produce the addon PBOs
4. On release: creates a `a3db-<tag>.zip` with the complete mod

Test locally with [ACT](https://github.com/nektos/act):

```bash
act -j test          # Run test job
act --list           # List all jobs
```

## Project Structure

```
a3db/
├── Cargo.toml                  # Workspace → extension/
├── extension/                  # Rust extension crate (cdylib + rlib)
│   ├── Cargo.toml
│   ├── .cargo/config.toml      # Cross-compilation linkers
│   └── src/
│       ├── lib.rs              # C ABI (RVExtension, RVExtensionArgs, RVExtensionVersion)
│       ├── parser/             # SQL parser (sqlparser-rs + custom A3DbDialect)
│       │   ├── dialect.rs      # A3DbDialect (GenericDialect-based, multi-dialect)
│       │   ├── preprocessor.rs # %% → fuzzy_match(), string-literal-aware
│       │   └── mod.rs          # parse_sql() entry point
│       └── engine/             # In-memory database engine
│           ├── database.rs     # Table storage + transaction snapshots
│           ├── table.rs        # Row/column storage, CRUD, trigram similarity
│           ├── value.rs        # ColumnType, Column, DbValue enums
│           ├── execute.rs      # Statement executor + expression evaluator + JOINs
│           ├── index.rs        # BTreeIndex + TrigramIndex (GIN-style)
│           ├── serialize.rs    # JSON, CSV, SQL dump, Binary formats
│           └── error.rs        # Structured error codes (ERR_*)
├── addons/
│   ├── main/                   # Main addon (CBA macro includes + CfgPatches)
│   │   ├── config.cpp
│   │   ├── script_mod.hpp
│   │   └── $PBOPREFIX$
│   └── sql/                    # SQL engine addon (CfgFunctions + SQF API)
│       ├── config.cpp
│       ├── script_component.hpp
│   ├── fn_init.sqf
│   ├── fn_execute.sqf
│   ├── fn_loadJSON.sqf
│   ├── fn_dumpSQL.sqf
│   ├── fn_exportJSON.sqf
│   ├── fn_exportCSV.sqf
│   ├── fn_exportSQL.sqf
│   ├── fn_save.sqf
│   ├── fn_load.sqf
│   └── $PBOPREFIX$
├── include/
│   └── x/cba/addons/           # CBA header stubs for build-time resolution
│       ├── main/
│       │   ├── script_mod.hpp
│       │   ├── script_macros.hpp
│       │   └── script_macros_common.hpp
│       └── xeh/
│           └── script_xeh.hpp
├── .hemtt/
│   └── project.toml            # HEMTT v1 build config
├── .github/workflows/ci.yml    # GitHub Actions CI/CD
├── mod.cpp                     # Mod definition (name, logo, etc.)
├── meta.cpp                    # Steam Workshop metadata (publishedid)
├── tools/                      # Development utility scripts
│   ├── build_current_addon.py
│   ├── config_style_checker.py
│   ├── getExtensionHash.py
│   ├── search_privates.py
│   ├── search_unused_privates.py
│   └── sqfvmChecker.py
└── README.md
```

## Development

Built following the same conventions as ACE3 and CBA_A3:

| Aspect | Convention |
|---|---|
| **Prefix** | `prefix = "a3db"`, `mainprefix = "z"` |
| **PBO path** | `z\a3db\addons\{addon_name}` |
| **Include path** | `\z\a3db\addons\main\script_mod.hpp` |
| **CBA dependency** | CBA_A3 required (`cba_main`, `cba_xeh`) |
| **Build system** | HEMTT v1 (`.hemtt/project.toml`) |
| **Rust workspace** | Workspace at root, crate in `extension/` |
| **Release profile** | `opt-level = "z"`, `lto = true`, `strip = true` |

## License

MIT — use freely in your Arma 3 mods.
