# A3SQL — Arma 3 Database Engine

> **Canonical documentation lives in the [wiki](wiki/) (`docs/wiki/`)** —
> Module Guide, Patch Framework, SQL Dialect, TCP Connector, Security,
> and the Production Readiness checklist. This page is the quick-start
> summary; the wiki pages are the authoritative reference.

An embeddable SQL database engine for Arma 3 mods. Like **SQLite for Arma** — a
Rust `callExtension` that lets modders write SQL directly in SQF.

```sqf
["CREATE TABLE weapons (id STRING PRIMARY KEY, name STRING)"] call a3sql_fnc_execute;
["INSERT INTO weapons VALUES ('m4a1', 'M4A1')"] call a3sql_fnc_execute;
_result = ["SELECT * FROM weapons WHERE name %% 'm4'"] call a3sql_fnc_execute;
```

## Features

| Category | Features |
|---|---|
| **SQL** | CREATE/DROP TABLE/INDEX, INSERT, SELECT, UPDATE, DELETE, REPLACE INTO, TRUNCATE, RENAME |
| **Advanced SQL** | JOINs (CROSS/INNER/LEFT), GROUP BY/HAVING, ORDER BY/LIMIT/OFFSET, UNION/UNION ALL, CTE (WITH), subqueries |
| **Expressions** | `%%` fuzzy match, LIKE, BETWEEN, IN, IS NULL, CASE WHEN, EXISTS, CAST |
| **Functions** | UPPER/LOWER, LENGTH, SUBSTR, TRIM, CONCAT, COALESCE/IFNULL, ROUND, ABS, NOW()/CURRENT_TIMESTAMP |
| **Window** | ROW_NUMBER, RANK, DENSE_RANK with OVER/PARTITION BY/ORDER BY |
| **Constraints** | PRIMARY KEY, NOT NULL, DEFAULT, CHECK, FOREIGN KEY, AUTO_INCREMENT |
| **Types** | INT (BIGINT/SMALLINT/TINYINT), FLOAT (DECIMAL/NUMERIC/DOUBLE), STRING (VARCHAR/CHAR/TEXT), BOOL, DATE/TIMESTAMP, STRINGS[]/FLOATS[] |
| **Indices** | BTREE (exact/range), TRIGRAM (fuzzy GIN-style candidate filter) |
| **Transactions** | BEGIN/COMMIT/ROLLBACK, SAVEPOINT/RELEASE |
| **Persistence** | SAVE/LOAD (binary), export/import JSON/CSV/SQL, export_to_file |
| **Security** | Parameterized queries (`$1`,`$2`), TCP LOGIN auth, CBA credential settings |
| **Network** | TCP listener (auto-start at game boot), external queries via Python/CLI |
| **Multi-statement** | Run `;`-separated SQL batches |
| **Multi-dialect** | Accepts PostgreSQL, MySQL/MariaDB, SQLite, DataFusion-style SQL |

## Quick Start

### 1. Add a3sql as a dependency

In your mod's CfgPatches:

```cpp
requiredAddons[] = {"cba_main", "a3sql_main", "a3sql_sql"};
```

### 2. Call from SQF

```sqf
// Create
["CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)"] call a3sql_fnc_execute;

// Insert
["INSERT INTO players VALUES ('76561198000000001', 'Scarface', 1500)"] call a3sql_fnc_execute;

// Query
_result = ["SELECT name, score FROM players WHERE score > 1000 ORDER BY score DESC"] call a3sql_fnc_execute;
// Returns: [0, "OK", [["name","score"],["Scarface",1500]]]
```

### 3. CBA Wrapper Functions

```sqf
// Fuzzy search
_result = ["SELECT name FROM weapons WHERE name %% 'm4'"] call a3sql_fnc_execute;

// Transactions
["BEGIN"] call a3sql_fnc_execute;
["INSERT INTO log (action) VALUES ('mission_start')"] call a3sql_fnc_execute;
["COMMIT"] call a3sql_fnc_execute;

// Save/load persistence
["data.bin"] call a3sql_fnc_save;
["data.bin"] call a3sql_fnc_load;

// Export
_table_data = ["players"] call a3sql_fnc_exportJSON;
_sql_backup = [] call a3sql_fnc_exportSQL;

// External TCP query (from Python)
// ["listen"] call a3sql_fnc_execute;  // auto-starts at game boot
```

### 4. CBA Addon Settings

Options → Addon Configuration → A3SQL:

| Setting | Type | Default | Purpose |
|---|---|---|---|
| Enable TCP Listener | CHECKBOX | true | Auto-start on game boot |
| Listener Port | EDIT | 33306 | TCP port |
| Listener Bind Address | EDIT | 127.0.0.1 | Bind IP |
| Listener Username | EDIT | (empty) | TCP login (empty = anonymous) |
| Listener Password | EDIT | (empty) | TCP login |
| Auto-Save | CHECKBOX | false | Save on mission end |
| Auto-Load | CHECKBOX | false | Load on mission start |
| Auto-Save Path | EDIT | a3sql_autosave.bin | File path |
| Log Level | LIST | INFO | RPT verbosity |
- **Auto-Save**: Save database when mission ends
- **Auto-Save File**: File path for auto-save

### 5. Full example

```sqf
if (isServer) then {
    // Create tables on mission start
    ["CREATE TABLE IF NOT EXISTS stats (uid STRING, name STRING, score INT)"] call a3sql_fnc_execute;

    // Restore from previous session
    ["stats_data.bin"] call a3sql_fnc_load;

    // Auto-save on mission end
    addMissionEventHandler ["Ended", {
        ["stats_data.bin"] call a3sql_fnc_save;
    }];
};

// Record event
["INSERT INTO stats VALUES ('76561198000000001', 'Scarface', 1500)"] call a3sql_fnc_execute;

// Query top scores
_result = ["SELECT name, score FROM stats ORDER BY score DESC LIMIT 10"] call a3sql_fnc_execute;
```

### 6. External query (TCP)

Enable the TCP listener in CBA settings, then connect from any tool:

```python
import socket
s = socket.socket()
s.connect(("127.0.0.1", 33306))
s.sendall(b"SELECT * FROM stats ORDER BY score DESC LIMIT 5\n")
print(s.recv(65536).decode())
s.close()
```

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

-- REPLACE / UPSERT
REPLACE INTO weapons VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3);

-- ALTER TABLE
ALTER TABLE weapons ADD COLUMN mass FLOAT;
ALTER TABLE weapons DROP COLUMN barrelLength;
ALTER TABLE weapons RENAME COLUMN name TO displayName;
ALTER TABLE weapons RENAME TO armory;

-- TRUNCATE
TRUNCATE TABLE weapons;

-- Window functions
SELECT id, name, ROW_NUMBER() OVER (ORDER BY name) AS rn FROM weapons;
SELECT id, name, RANK() OVER (PARTITION BY caliber ORDER BY name) FROM weapons;

-- Subqueries
SELECT * FROM weapons WHERE id IN (SELECT weaponId FROM attachments);
SELECT * FROM weapons WHERE EXISTS (SELECT 1 FROM attachments WHERE weaponId = weapons.id);

-- CTE
WITH top AS (SELECT * FROM weapons ORDER BY name LIMIT 5) SELECT * FROM top;

-- CAST
SELECT CAST(barrelLength AS INT) FROM weapons;
SELECT name || ' (' || caliber || ')' AS combined FROM weapons;

-- Functions
SELECT NOW(), CURRENT_TIMESTAMP;
SELECT COALESCE(barrelLength, 0.0) FROM weapons;
SELECT UPPER(name), LOWER(name), LENGTH(name), SUBSTR(name, 1, 3) FROM weapons;
```

## SQF API

### Initialization

```sqf
// In init.sqf or CfgFunctions init:
private _version = "a3sql" callExtension "version";
diag_log text format ["[A3SQL] Loading: %1", _version];
```

### SQL Execution

```sqf
// Single SQL statement (STRING callExtension STRING):
private _result = "a3sql" callExtension "SELECT * FROM weapons";

// SQL with args (STRING callExtension ARRAY):
private _result = ["a3sql", "INSERT INTO weapons VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3)"] callExtension;

// Multi-statement (separate with semicolons):
private _result = "a3sql" callExtension "CREATE TABLE t (id STRING); INSERT INTO t VALUES ('a'); SELECT * FROM t";
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
private _version = "a3sql" callExtension "version";
// → [0,"OK","a3sql 0.1.0"]

// SQL dump
private _dump = "a3sql" callExtension "dump_sql";
// → [0,"OK","CREATE TABLE weapons (...);..."]

// Query with parameterized args (SQL injection safe)
private _result = ["a3sql", "SELECT * FROM weapons WHERE id = $1", ["m4a1"]] callExtension;

// TCP listener (auto-starts on game boot). Manual control:
private _result = ["a3sql", "listen", ["33306"]] callExtension;
private _result = ["a3sql", "stop"] callExtension;
```

### CBA Functions

When using CBA (recommended), the addon registers these functions via `CfgFunctions`:

| Function | Description |
|---|---|
| `a3sql_fnc_init` | Initialize extension, returns version string |
| `a3sql_fnc_execute` | Execute SQL, returns parsed result |
| `a3sql_fnc_loadJSON` | Import JSON data into a table |
| `a3sql_fnc_dumpSQL` | Export full database as SQL dump |
| `a3sql_fnc_exportJSON` | Export table as JSON |
| `a3sql_fnc_exportCSV` | Export table as CSV |
| `a3sql_fnc_exportSQL` | Export full database as SQL statements |
| `a3sql_fnc_save` | Persist database to binary file |
| `a3sql_fnc_load` | Restore database from binary file |
| `a3sql_fnc_init` | Initialize extension |
| `a3sql_fnc_settings` | Register CBA settings (auto-called via PreInit) |
| `a3sql_fnc_postInit` | Post-mission init (auto-save/load hooks) |

## Security

### Parameterized Queries

Prevent SQL injection by passing user input as separate args with `$1`, `$2` placeholders:

```sqf
// UNSAFE — string interpolation (SQL injection possible)
private _sql = format ["SELECT * FROM users WHERE name = '%1'", _userInput];
["a3sql", _sql] callExtension;

// SAFE — parameterized query (injection prevented)
["a3sql", "SELECT * FROM users WHERE name = $1", [_userInput]] callExtension;
```

### TCP Authentication

Set a username and password in CBA Settings (Options → Addon Configuration → A3SQL).
When credentials are non-empty, clients must `LOGIN` before querying:

```python
import socket
s = socket.socket()
s.connect(("127.0.0.1", 33306))
s.sendall(b"LOGIN admin mypassword\n")
print(s.recv(65536).decode())  # [0,"OK","Authenticated"]
s.sendall(b"SELECT * FROM weapons\n")
print(s.recv(65536).decode())
s.close()
```

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
cargo build --release --manifest-path extension/Cargo.toml

# Build for Windows (cross-compile from Linux)
cargo build --release --target x86_64-pc-windows-gnu --manifest-path extension/Cargo.toml
cargo build --release --target i686-pc-windows-gnu --manifest-path extension/Cargo.toml
```

### Build the addon

```bash
hemtt build
```

Output goes to `.hemttout/build/`.

### Run tests

```bash
# All 524 tests
cargo test --manifest-path extension/Cargo.toml
```

### Linting & validation

```bash
# Rust
cargo fmt --check
cargo clippy --manifest-path extension/Cargo.toml --all-targets -- -D warnings

# SQF + config
python3 tools/sqf_validator.py addons/
python3 tools/config_style_checker.py

# Arma addon structure
hemtt check -p -e

# SQL smoke test — runs the mod's production SQL through the real extension
# binary (the same C ABI Arma uses). Fails on any misbehaving statement:
python3 tools/sql_smoke_test.py tools/smoke_test.sql
```

## CI/CD

The project includes a GitHub Actions workflow (`.github/workflows/ci.yml`) that:

1. Runs `cargo test`
2. Builds for 4 targets: `x86_64-linux`, `i686-linux`, `x86_64-windows`, `i686-windows`
3. Runs the SQL smoke test against the real release binary
4. Runs `hemtt build` to produce the addon PBOs
5. On release: creates a `a3sql-<tag>.zip` with the complete mod

Test locally with [ACT](https://github.com/nektos/act):

```bash
act -j test          # Run test job
act --list           # List all jobs
```

## Project Structure

```
a3sql/
├── extension/                  # Rust extension workspace (cdylib + rlib)
│   ├── Cargo.toml
│   ├── .cargo/config.toml      # Cross-compilation linkers
│   └── src/
│       ├── lib.rs              # Library root + FFI module layout
│       ├── ffi/                # C ABI (RVExtension, RVExtensionArgs, RVExtensionVersion,
│       │                       #   RVExtensionRegisterCallback)
│       ├── dispatch/           # Command routing (SQL + control commands: save/load,
│       │   │                   #   cursor*, prepare, listen, set_credentials, exports)
│       │   ├── commands.rs     # Control-command handlers
│       │   └── sql.rs          # SQL splitting + $1/$n parameter substitution
│       ├── parser/             # SQL parser (sqlparser-rs + custom A3sqlDialect)
│       ├── engine/             # In-memory database engine
│       │   ├── database.rs     # Table storage + transaction snapshots
│       │   ├── table/          # Row/column storage, PK/UNIQUE sets, triggers
│       │   ├── stmts/          # DDL/DML statement execution
│       │   ├── functions/      # Scalar + aggregate functions, expression evaluator
│       │   ├── serialize/      # JSON, CSV, SQL dump, Binary formats
│       │   ├── execute.rs      # Statement executor + JOINs
│       │   ├── index.rs        # BTreeIndex + TrigramIndex (GIN-style)
│       │   ├── error.rs        # Structured error codes (ERR_*)
│       │   └── value.rs        # ColumnType, Column, DbValue enums
│       ├── server.rs           # TCP listener (loopback, LOGIN auth, panic barrier)
│       ├── config.rs           # Config (A3SQL_CONFIG env / a3sql.toml)
│       └── bin/                # Standalone a3sql-server binary
├── addons/
│   ├── main/                   # Main addon (CBA macro includes + CfgPatches)
│   ├── database/               # SQL API (fnc_execute, fnc_save/load, exports, prepared)
│   ├── admin/                  # Admin command execution (ban/kick/whitelist)
│   ├── analytics/              # Kills/fired-event analytics snapshots
│   ├── loadouts/               # Loadout templates persistence
│   ├── persistence/            # Player persistence
│   ├── progression/            # Progression tracking
│   ├── patch_core/             # Dynamic live-patching rule engine
│   ├── patch_editor/           # In-game rule/preset editor
│   └── patch_operators/        # Patch operator definitions
├── include/
│   └── x/cba/addons/           # CBA header stubs for build-time resolution
├── .hemtt/
│   └── project.toml            # HEMTT v1 build config
├── .github/workflows/ci.yml    # GitHub Actions CI/CD
├── mod.cpp                     # Mod definition (name, logo, etc.)
├── meta.cpp                    # Steam Workshop metadata (publishedid)
├── tools/                      # Development utility scripts
│   ├── sql_smoke_test.py       # Production SQL gate (runs against real binary)
│   ├── smoke_test.sql          # The mod's own SQL as regression suite
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
| **Prefix** | `prefix = "a3sql"`, `mainprefix = "z"` |
| **PBO path** | `z\a3sql\addons\{addon_name}` |
| **Include path** | `\z\a3sql\addons\main\script_mod.hpp` |
| **CBA dependency** | CBA_A3 required (`cba_main`, `cba_xeh`) |
| **Build system** | HEMTT v1 (`.hemtt/project.toml`) |
| **Rust workspace** | `extension/` (own `Cargo.toml`; target under `extension/target/`) |
| **Release profile** | `opt-level = "z"`, `lto = true`, `strip = true` |

## License

MIT — use freely in your Arma 3 mods.
