# A3SQL: Arma 3 Database Engine

> **Canonical documentation lives in the [wiki](wiki/) (`docs/wiki/`)**:
> Module Guide, Patch Framework, SQL Dialect, TCP Connector, Security,
> and the Production Readiness checklist. This page is the quick-start
> summary; the wiki pages are the authoritative reference.

## What is A3SQL

A3SQL is a live SQL database engine for Arma 3. It runs inside your server process, and your mission talks to it with plain SQL: create tables, insert rows, run queries, all from SQF. No external database to install, no files to parse by hand.

For a mission maker or server admin that means:

- Player stats, scores, and loadouts that survive mission restarts
- Kill, connection, and performance logs you can query with SQL
- Live patching of weapon and vehicle values from SQL rules
- A TCP listener so external tools can read and write the same data

A3SQL requires [CBA A3](https://github.com/CBATeam/CBA_A3/releases) (the latest release).

The quickest possible start:

```sqf
["CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)"] call a3sql_fnc_execute;
["INSERT INTO players VALUES ('76561198000000001', 'Scarface', 1500)"] call a3sql_fnc_execute;
_result = ["SELECT name, score FROM players WHERE score > 1000 ORDER BY score DESC"] call a3sql_fnc_execute;
// _result = [0, "OK", [["name","score"],["Scarface",1500]]]
```

## Installation

Install the mod from the Steam Workshop, or download the [latest release](https://github.com/lErrorl404l/a3sql/releases/latest) and unpack `@a3sql` into your Arma 3 directory. Launch the game or server with:

```
-mod=@cba_a3;@a3sql
```

The mod is modular: `a3sql_main` is the only required PBO. You can remove the addon PBOs for features you don't use (analytics, loadouts, persistence, progression, patching, admin); the modules table in the [root README](../README.md) lists what each one does.

## Using A3SQL in your mod

### Add the dependency

In your mod's CfgPatches:

```cpp
requiredAddons[] = {"cba_main", "a3sql_main", "a3sql_database"};
```

### The SQF API

The `a3sql_database` addon compiles one function per file under `addons/database/functions/`. The five most used also get short `a3sql_fnc_*` aliases; everything else uses the full `a3sql_database_fnc_*` name:

| Function | Description |
|---|---|
| `a3sql_fnc_execute` | Execute SQL, returns `[returnCode, status, data]`. Alias of `a3sql_database_fnc_execute`. |
| `a3sql_fnc_selectMap` | Run a query, returns an array of hashmaps keyed by column name |
| `a3sql_fnc_selectArray` | Run a query, returns rows only (header row skipped) |
| `a3sql_fnc_selectAll` | Run a query with automatic cursor pagination for large results |
| `a3sql_fnc_exportCSV` | Export a table as CSV |
| `a3sql_database_fnc_exportJSON` | Export a table as JSON |
| `a3sql_database_fnc_exportSQL` | Export the whole database as SQL statements |
| `a3sql_database_fnc_dumpSQL` | Same as `exportSQL` (command `dump_sql`) |
| `a3sql_database_fnc_loadJSON` | Import JSON (or a JSON file path) into a table |
| `a3sql_database_fnc_save` | Save the database to a binary file |
| `a3sql_database_fnc_load` | Restore the database from a binary file |
| `a3sql_database_fnc_prepare` | Prepare a statement with `$1..$N` placeholders |
| `a3sql_database_fnc_executePrepared` | Run a prepared statement with arguments |
| `a3sql_database_fnc_executeTimed` | Execute and log any query slower than 10 ms |
| `a3sql_database_fnc_sqlEscape` | Escape a string for safe inline use in SQL |
| `a3sql_database_fnc_init` | Check the version, push credentials, start the listener if needed |

Example:

```sqf
// Create and fill a table
["CREATE TABLE IF NOT EXISTS stats (uid STRING, name STRING, score INT)"] call a3sql_fnc_execute;
["INSERT INTO stats VALUES ('76561198000000001', 'Scarface', 1500)"] call a3sql_fnc_execute;

// Persist to disk, restore later
["stats.bin"] call a3sql_database_fnc_save;
["stats.bin"] call a3sql_database_fnc_load;

// Query into an array of hashmaps
private _top = ["SELECT name, score FROM stats ORDER BY score DESC LIMIT 10"] call a3sql_fnc_selectMap;

// Export
private _csv = ["players"] call a3sql_fnc_exportCSV;
private _sqlBackup = [] call a3sql_database_fnc_exportSQL;
```

### Prepared statements

Parameterized queries keep user input out of your SQL. Prepare once, then execute with arguments:

```sqf
["find_player", "SELECT * FROM players WHERE uid = $1"] call a3sql_database_fnc_prepare;
_result = ["find_player", ["76561198000000001"]] call a3sql_database_fnc_executePrepared;
```

### Control commands

Beyond SQL, the extension accepts control commands. The SQF wrappers cover the common ones; you can also call the extension directly:

| Command | Purpose |
|---|---|
| `version` | Return the current version string |
| `ping` / `reset` | Health check / wipe the database |
| `save <path>` / `load <path>` | Binary persistence (atomic, `.bak` fallback, FNV-1a checksum) |
| `dump_sql` / `export_sql` | Full database as SQL statements |
| `export <fmt> <table>` | Export a table as json/csv/binary |
| `export_to_file <fmt> [table] <path>` | Write an export to a file |
| `import <fmt> <table>` | Import json/csv/sql data (data passed as argument) |
| `listen [port]` / `stop` | Start/stop the TCP listener |
| `set_credentials <user> <pass>` | Set TCP login credentials |
| `prepare <name> <sql>` / `execute_prepared <name> [args...]` | Prepared statements |
| `cursor create <name> <query>` / `cursor fetch <name> [limit]` / `cursor drop <name>` | Paged queries |
| `plugins` / `register_function <name> <argc> <body>` / `plugin_dir <dir>` | Plugin management |
| `describe <table>` / `show create table <table>` | Schema inspection |
| `connect <host> <port>` / `disconnect` | Talk to a remote A3SQL server |
| `live_patch list` / `live_patch query <sql>` / `live_patch <target> <property> <value>` | Patch rules |

### Response format

Every command returns `[returnCode, status, data]`:

```
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
  ERR_AUTH     Authentication failure (TCP LOGIN, signed queries)
```

`result_data` for `SELECT` is a JSON array of column names plus rows:

```json
[["id","name","caliber","barrelLength"],["rhs_m4a1","M4A1","5.56x45mm",368.3]]
```

For JOINs with prefixed column names:

```json
[["weapons.id","weapons.name","attachments.name"],["rhs_m4a1","M4A1","M68 CCO"]]
```

### Raw callExtension

You can skip the wrappers and call the extension directly. The version command reports the current version:

```sqf
private _version = "a3sql" callExtension "version";
// → [0,"OK","a3sql <current version>"]
```

## Server admins

### CBA settings

Options → Addon Configuration → A3SQL:

| Setting | Type | Default | Purpose |
|---|---|---|---|
| `a3sql_listener_enabled` | CHECKBOX | true | Start the TCP listener on mission start |
| `a3sql_database_listener_port` | EDITBOX | 33306 | TCP port |
| `a3sql_database_listener_bind` | EDITBOX | 127.0.0.1 | Bind address |
| `a3sql_database_listener_user` | EDITBOX | (empty) | TCP login user; empty credentials are refused |
| `a3sql_database_listener_password` | EDITBOX | (empty) | TCP login password |
| `a3sql_auto_save` | CHECKBOX | false | Save the database when the mission ends |
| `a3sql_auto_load` | CHECKBOX | false | Load the database when a mission starts |
| `a3sql_database_auto_save_path` | EDITBOX | a3sql_autosave.bin | File path used by auto-save/auto-load |
| `a3sql_log_level` | LIST | INFO | RPT verbosity (ERROR/WARN/INFO/DEBUG) |

### TCP listener

With the listener enabled, external tools connect over TCP and must `LOGIN` first. Authentication is fail-closed: with no credentials configured, LOGIN can never succeed, and without a successful LOGIN every query is rejected with `ERR_AUTH`. Credential comparison is constant-time, so timing side channels can't leak the password.

```python
import socket
s = socket.socket()
s.connect(("127.0.0.1", 33306))
s.sendall(b"LOGIN admin mypassword\n")
print(s.recv(65536).decode())  # [0,"OK","Authenticated"]
s.sendall(b"SELECT * FROM stats ORDER BY score DESC LIMIT 5\n")
print(s.recv(65536).decode())
s.close()
```

Control the listener from SQF:

```sqf
// Set credentials, then start the listener on port 33306
["a3sql", "set_credentials", ["admin", "mypassword"]] callExtension;
["a3sql", "listen", ["33306"]] callExtension;
["a3sql", "stop"] callExtension;
```

### SQL injection safety

Pass user input as separate arguments with `$1`, `$2` placeholders instead of interpolating strings:

```sqf
// UNSAFE: string interpolation (SQL injection possible)
private _sql = format ["SELECT * FROM users WHERE name = '%1'", _userInput];
["a3sql", _sql] callExtension;

// SAFE: parameterized query (injection prevented)
["a3sql", "SELECT * FROM users WHERE name = $1", [_userInput]] callExtension;
```

## SQL dialect

### Feature summary

| Category | Features |
|---|---|
| **SQL** | CREATE/DROP TABLE/INDEX/VIEW, INSERT, SELECT, UPDATE, DELETE, REPLACE INTO, TRUNCATE, RENAME, ALTER TABLE (ADD/DROP/RENAME COLUMN, RENAME), EXPLAIN (JSON query plan), VACUUM, REINDEX |
| **Advanced SQL** | JOINs (CROSS/INNER/LEFT/FULL OUTER/NATURAL/USING), JOINs with subqueries in ON, GROUP BY/HAVING, ORDER BY/LIMIT/OFFSET, UNION/UNION ALL/EXCEPT/INTERSECT, CTE incl. WITH RECURSIVE, subqueries (scalar/IN/EXISTS), derived tables in FROM |
| **Window** | ROW_NUMBER, RANK, DENSE_RANK, LAG, LEAD, FIRST_VALUE, LAST_VALUE with OVER (PARTITION BY/ORDER BY), ROWS/RANGE frames |
| **Constraints** | PRIMARY KEY (incl. composite), NOT NULL, DEFAULT (incl. expression defaults), CHECK, FOREIGN KEY (enforced, with cascade), AUTO_INCREMENT, UNIQUE |
| **Types** | INT (BIGINT/SMALLINT/TINYINT), FLOAT (DECIMAL/NUMERIC/DOUBLE), STRING (VARCHAR/CHAR/TEXT), BOOL/BOOLEAN, DATE/TIMESTAMP, STRINGS[]/FLOATS[] |
| **Indices** | BTREE (exact + range), TRIGRAM (fuzzy, GIN-style), covering LIKE 'prefix%' and %mid% containment |
| **Functions** | UPPER/LOWER/LENGTH/SUBSTR/TRIM/CONCAT/COALESCE/IFNULL/ROUND/ABS, NOW()/CURRENT_TIMESTAMP, datetime()/strftime()/date()/time(), CAST, %% fuzzy match, LIKE, BETWEEN, IN, IS NULL, CASE WHEN, EXISTS |
| **Triggers** | CREATE/DROP TRIGGER, BEFORE/AFTER INSERT/UPDATE/DELETE, firing verified |
| **SQF Eval** | `SQF_EVAL(expr)` evaluates in-line SQF expressions |
| **Transactions** | BEGIN/COMMIT/ROLLBACK, SAVEPOINT/RELEASE, savepoint rollback |
| **Prepared statements** | `prepare <name> <sql with $1..$N>` + `execute_prepared <name> [args...]` |
| **Cursors** | `cursor create <name> <query>` + `cursor fetch <name> [limit]` + `cursor drop <name>` |
| **Persistence** | SAVE/LOAD (binary, atomic with .bak fallback + FNV-1a checksum), export/import JSON/CSV/SQL, export_to_file, auto-save/auto-load via CBA settings |
| **Security** | Parameterized queries, TCP LOGIN required by default (fail-closed), constant-time credential compare, 30KB output cap (fail-loud with cursor hint) |

### Examples

```sql
-- Tables and views
CREATE TABLE weapons (id STRING PRIMARY KEY, name STRING, caliber STRING, barrelLength FLOAT);
CREATE VIEW heavy AS SELECT * FROM weapons WHERE barrelLength > 400.0;

-- CRUD
INSERT INTO weapons VALUES ('rhs_m4a1', 'M4A1', '5.56x45mm', 368.3);
SELECT * FROM weapons WHERE caliber = '5.56x45mm';
SELECT name, caliber FROM weapons WHERE barrelLength > 400.0 ORDER BY barrelLength DESC;
UPDATE weapons SET caliber = '7.62x39mm' WHERE id = 'ak74';
DELETE FROM weapons WHERE barrelLength IS NULL;
DROP TABLE weapons;

-- Fuzzy match (trigram)
SELECT * FROM weapons WHERE id %% 'rhs_m4';
-- matches rhs_m4a1, rhs_m4a1_carryhandle, etc.

-- JOINs
SELECT w.name, a.name FROM weapons w INNER JOIN attachments a ON w.id = a.weaponId;
SELECT * FROM weapons w LEFT JOIN attachments a ON w.id = a.weaponId;
SELECT * FROM weapons w FULL OUTER JOIN attachments a ON w.id = a.weaponId;
SELECT * FROM weapons NATURAL JOIN attachments;
SELECT * FROM weapons JOIN attachments USING (id);

-- Set operations
SELECT name FROM weapons UNION SELECT name FROM legacy_weapons;
SELECT name FROM weapons UNION ALL SELECT name FROM legacy_weapons;
SELECT name FROM weapons EXCEPT SELECT name FROM retired_weapons;
SELECT name FROM weapons INTERSECT SELECT name FROM scoped_weapons;

-- Aggregates
SELECT COUNT(*) FROM weapons;
SELECT AVG(barrelLength) FROM weapons;
SELECT caliber, COUNT(*) AS cnt FROM weapons GROUP BY caliber HAVING COUNT(*) > 2;

-- Ordering and limits
SELECT * FROM weapons ORDER BY name ASC LIMIT 10 OFFSET 5;

-- Transactions and savepoints
BEGIN;
INSERT INTO weapons VALUES ('test', 'Test', '9x19mm', 200.0);
ROLLBACK;  -- or COMMIT
SAVEPOINT sp1;
INSERT INTO weapons VALUES ('tmp', 'Temp', '5.56x45mm', 300.0);
ROLLBACK TO sp1;
RELEASE SAVEPOINT sp1;

-- Indices
CREATE INDEX idx_caliber ON weapons (caliber) USING BTREE;
CREATE INDEX idx_name_fuzzy ON weapons (name) USING TRIGRAM;

-- Constraints
CREATE TABLE guilds (id STRING PRIMARY KEY, name STRING NOT NULL);
CREATE TABLE users (
    id INT PRIMARY KEY,
    name STRING NOT NULL,
    score INT DEFAULT 0 CHECK (score >= 0),
    guild STRING REFERENCES guilds(id) ON DELETE CASCADE,
    tags STRINGS[]
);

-- Triggers (body is plain SQL)
CREATE TABLE audit (action STRING);
CREATE TRIGGER log_weapon_insert AFTER INSERT ON weapons
BEGIN
    INSERT INTO audit VALUES ('weapon_inserted');
END;

-- Subqueries
SELECT * FROM weapons WHERE id IN (SELECT weaponId FROM attachments);
SELECT * FROM weapons WHERE EXISTS (SELECT 1 FROM attachments WHERE weaponId = weapons.id);
SELECT * FROM (SELECT * FROM weapons WHERE caliber = '5.56x45mm') d;

-- CTE, including recursive
WITH top AS (SELECT * FROM weapons ORDER BY name LIMIT 5) SELECT * FROM top;

-- Window functions
SELECT id, name, ROW_NUMBER() OVER (ORDER BY name) AS rn FROM weapons;
SELECT id, name, RANK() OVER (PARTITION BY caliber ORDER BY name) FROM weapons;
SELECT id, LAG(name) OVER (ORDER BY id) FROM weapons;
SELECT id, FIRST_VALUE(name) OVER (PARTITION BY caliber ORDER BY id) FROM weapons;
SELECT id, SUM(barrelLength) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM weapons;

-- Functions
SELECT NOW(), CURRENT_TIMESTAMP, datetime(), strftime('%Y-%m-%d', 'now'), date(), time();
SELECT COALESCE(barrelLength, 0.0) FROM weapons;
SELECT UPPER(name), LOWER(name), LENGTH(name), SUBSTR(name, 1, 3), TRIM(name) FROM weapons;
SELECT CONCAT(name, ' (', caliber, ')') AS combined FROM weapons;
SELECT CAST(barrelLength AS INT) FROM weapons;
SELECT CASE WHEN barrelLength > 400.0 THEN 'heavy' ELSE 'light' END FROM weapons;

-- SQF evaluated inline
SELECT SQF_EVAL('1 + 1');

-- Query plan and maintenance
EXPLAIN SELECT * FROM weapons WHERE caliber = '5.56x45mm';
VACUUM;
REINDEX;

-- ALTER TABLE
ALTER TABLE weapons ADD COLUMN mass FLOAT;
ALTER TABLE weapons DROP COLUMN barrelLength;
ALTER TABLE weapons RENAME COLUMN name TO displayName;
ALTER TABLE weapons RENAME TO armory;

-- TRUNCATE / REPLACE
TRUNCATE TABLE weapons;
REPLACE INTO weapons VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3);
```

## Large data and pagination

### The 30KB response cap

Arma's `callExtension` hands the extension a fixed output buffer (30KB in current game builds). A SELECT whose result would overflow that buffer is not silently truncated: it returns a clear error telling you how to page instead:

```
[-1,"ERR_INTERNAL","Result exceeds output buffer (30KB) — use 'cursor create <name> <query>' + 'cursor fetch <name> [limit]' to page large results"]
```

### Cursors

Cursor commands page through a large result set in slices:

```sqf
["a3sql", "cursor create", ["events_cursor", "SELECT * FROM events ORDER BY ts"]] callExtension;
_first = ["a3sql", "cursor fetch", ["events_cursor", "500"]] callExtension;
_next  = ["a3sql", "cursor fetch", ["events_cursor", "500"]] callExtension;
["a3sql", "cursor drop", ["events_cursor"]] callExtension;
```

In SQF you usually don't need cursors directly: `a3sql_fnc_selectAll` detects the oversized response and pages through it automatically.

### Data-volume envelope

A3SQL is an in-memory engine. All data lives in RAM, and each save rewrites the whole database:

| Rows | Verdict |
|---|---|
| < 10k | Trivial; saves are sub-millisecond |
| 10k-100k | Fine; roughly 1 MB RAM per 100k simple rows |
| 100k-500k | With care; watch save size and autosave frequency |
| > 500k | Not this engine; memory grows unbounded and every save rewrites the DB |

Keep tables below ~100k rows. During an autosave the engine briefly holds its write lock, so a very large save can stall new queries for a moment.

## For developers

### Build

- Rust stable, at or above the `rust-version` declared in `extension/Cargo.toml`
- [HEMTT](https://hemtt.dev/) for the addon PBOs
- MinGW-w64 for Windows cross-compilation on Linux

```bash
# Build the extension for Linux x86_64
cargo build --release --manifest-path extension/Cargo.toml

# Cross-compile for Windows
cargo build --release --target x86_64-pc-windows-gnu --manifest-path extension/Cargo.toml
cargo build --release --target i686-pc-windows-gnu --manifest-path extension/Cargo.toml

# Build the addon PBOs (output in .hemttout/build/)
hemtt build
```

### Test

```bash
# Full test suite
cargo test --manifest-path extension/Cargo.toml

# Dialect sweep: covers every feature documented in SQL-Dialect.md
python3 tools/sql_dialect_sweep.py

# SQL smoke test: runs the mod's production SQL through the real extension binary
python3 tools/sql_smoke_test.py tools/smoke_test.sql
```

Lint with `cargo fmt --check`, `cargo clippy --manifest-path extension/Cargo.toml --all-targets -- -D warnings`, and `hemtt check -p -e`.

### Integrate

The extension exposes the standard Arma C ABI (`RVExtension`), so it loads as a plain `callExtension` library. The `a3sql_database` addon wraps it for SQF; a [standalone server binary](wiki/Standalone-Server.md) exists for running without Arma.

### License

The mod is licensed under the [Arma Public License Share Alike (APL-SA)](../LICENSE), Copyright 2026 ABE Team. Note: the Rust crate's `Cargo.toml` declares `MIT OR Apache-2.0`; that is a license convention for the source code only, and does not change the mod's license. Anything you build with A3SQL is covered by APL-SA.
