# A3SQL — Arma 3 Database Engine

[![CI](https://github.com/lErrorl404l/a3sql/actions/workflows/ci.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions)
[![Tests](https://github.com/lErrorl404l/a3sql/actions/workflows/test.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/test.yml)
[![Lint](https://github.com/lErrorl404l/a3sql/actions/workflows/lint.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/lint.yml)
[![Build](https://github.com/lErrorl404l/a3sql/actions/workflows/build.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/build.yml)
[![Rust](https://img.shields.io/badge/rust-stable-orange)](https://rustup.rs/)
[![HEMTT](https://img.shields.io/badge/HEMTT-1.20+-blue)](https://hemtt.dev/)
[![License](https://img.shields.io/badge/license-APL--SA-red)](LICENSE)
[![Wiki](https://img.shields.io/badge/docs-wiki-green)](https://github.com/lErrorl404l/a3sql/wiki)

An embeddable SQL database engine for Arma 3 mods. Like **SQLite for Arma** — a
Rust `callExtension` that lets modders write SQL directly in SQF.

```sqf
["CREATE TABLE players (uid STRING PRIMARY KEY, name STRING, score INT)"] call a3sql_fnc_execute;
_result = ["SELECT name, score FROM players WHERE score > 1000 ORDER BY score DESC"] call a3sql_fnc_execute;
```

## Features

| Category | What it does |
|----------|-------------|
| **SQL** | CREATE/DROP TABLE/INDEX/VIEW, INSERT, SELECT, UPDATE, DELETE, REPLACE INTO, TRUNCATE, RENAME, VACUUM, REINDEX |
| **Advanced SQL** | JOINs (CROSS/INNER/LEFT/FULL OUTER/NATURAL/USING), GROUP BY/HAVING, ORDER BY/LIMIT/OFFSET, UNION/EXCEPT/INTERSECT, CTE (WITH RECURSIVE), subqueries, window functions (ROWS BETWEEN) |
| **Expressions** | `%%` fuzzy match, LIKE, BETWEEN, IN, IS NULL, CASE WHEN, EXISTS, CAST, `fn_*()` plugin functions |
| **Functions** | COUNT(DISTINCT), SUM, AVG, MIN, MAX, UPPER/LOWER, LENGTH, SUBSTR, TRIM, CONCAT, COALESCE/IFNULL, ROUND, ABS, NOW()/CURRENT_TIMESTAMP, POW, SQRT, CEIL, FLOOR, SIGN, REPLACE |
| **SQF Eval** | `SQF_EVAL()` SQL function evaluates in-line SQF expressions — math, string ops, type checks, 55+ native commands, fallback to NULL for game-engine commands |
| **Constraints** | PRIMARY KEY, NOT NULL, DEFAULT, CHECK (enforced), FOREIGN KEY (enforced), AUTO_INCREMENT |
| **Indices** | BTREE (exact/range), TRIGRAM (fuzzy GIN-style), FTS (full-text trigram) |
| **Transactions** | BEGIN/COMMIT/ROLLBACK (no-op when idle), SAVEPOINT/RELEASE |
| **RETURNING** | `INSERT/UPDATE/DELETE ... RETURNING *` |
| **EXPLAIN** | `EXPLAIN SELECT ...` — prints query plan as JSON |
| **Persistence** | SAVE/LOAD (binary), export/import JSON/CSV/SQL, export_to_file |
| **Security** | Parameterized queries (`$1`, `$2`), TCP LOGIN auth, CBA credential settings |
| **Plugins** | Rust trait, C ABI dynamic `.so`/`.dll`, SQF `register_function` |
| **Network** | TCP listener (auto-start), standalone server (`a3sql-server`), remote connect mode |
| **Multi-dialect** | Accepts PostgreSQL, MySQL/MariaDB, SQLite, DataFusion-style SQL |

## Quick Start

### 1. Add a3sql as a dependency

```cpp
requiredAddons[] = {"cba_main", "a3sql_main", "a3sql_sql"};
```

### 2. Call from SQF

```sqf
private _result = ["SELECT * FROM players WHERE score > 1000"] call a3sql_fnc_execute;
```

### 3. Run the standalone server

```bash
cargo run --bin a3sql-server -- --port 33307
echo "SELECT * FROM players" | nc localhost 33307
```

### 4. Or connect from Python

```python
import socket
s = socket.socket()
s.connect(("127.0.0.1", 33306))
s.sendall(b"SELECT name, score FROM players ORDER BY score DESC LIMIT 5\n")
print(s.recv(65536).decode())
s.close()
```

## Documentation

- [Getting Started](https://github.com/lErrorl404l/a3sql/wiki/Getting-Started) — Full worked example
- [SQL Dialect](https://github.com/lErrorl404l/a3sql/wiki/SQL-Dialect) — Supported SQL syntax
- [CBA Settings](https://github.com/lErrorl404l/a3sql/wiki/CBA-Settings) — Addon configuration
- [Standalone Server](https://github.com/lErrorl404l/a3sql/wiki/Standalone-Server) — Run without Arma
- [Plugins](https://github.com/lErrorl404l/a3sql/wiki/Plugins) — Extend with Rust/C/SQF
- [Building](https://github.com/lErrorl404l/a3sql/wiki/Building) — Compiling from source
- [Development Setup](https://github.com/lErrorl404l/a3sql/wiki/Development-Setup) — Dev environment guide

## Building

```bash
cargo build --release -p a3sql   # extension
hemtt build                      # addon PBOs
cargo test -p a3sql                # 378+ tests
```

See the [Building page](https://github.com/lErrorl404l/a3sql/wiki/Building) for cross-compilation, code signing, and CI details.

## Project Structure

```
a3sql/
├── extension/           # Rust crate (cdylib + rlib)
│   ├── Cargo.toml
│   ├── .cargo/config.toml   # Cross-compilation linkers
│   └── src/
│       ├── lib.rs            # C ABI entry point (~80 lines)
│       ├── dispatch.rs       # Command routing + SQL execution
│       ├── server.rs         # TCP server (start_server, serve_client)
│       ├── ffi/              # C ABI extern functions + statics
│       ├── parser/           # SQL parser + dialect + preprocessor
│       ├── engine/           # Core database engine
│       │   ├── prelude.rs    # Common imports
│       │   ├── execute.rs    # Statement dispatcher + format helpers
│       │   ├── execute/      # select.rs (exec_select, exec_subquery)
│       │   ├── database/     # Database struct, persistence
│       │   ├── error.rs      # thiserror EngineError enum
│       │   ├── functions/    # SQL functions + expression eval
│       │   ├── index.rs      # BTree + Trigram indices
│       │   ├── optimizer/    # OptimizerRule trait + passes
│       │   ├── plugin.rs     # Rust/C ABI/SQF plugin system
│       │   ├── serialize/    # JSON/CSV/Binary/SQL export/import
│       │   ├── stmts/        # Statement executors (ddl/, select/, etc.)
│       │   ├── table/        # Table struct, row ops, schema
│       │   ├── test.rs       # Test helpers with fresh DB state
│       │   ├── trigger.rs    # Trigger execution + recursion guard
│       │   └── value.rs      # Column types, DbValue enum
│       ├── bin/              # Standalone a3sql-server binary
│       └── tests/            # Integration tests (abi, audit, bugs, gaps, plugins)
├── addons/{main,sql}/   # Arma 3 addon PBOs (SQF API + CBA settings)
├── include/             # CBA build-time headers + plugin.h
├── tools/               # Python dev tools (UV-managed)
├── .hemtt/              # HEMTT config + hooks
├── .github/workflows/   # CI: test, lint, build, release, wiki-sync
├── keys/                # BI signing keys
├── docs/wiki/           # GitHub Wiki source (auto-synced on push to main)
└── plugins/             # Example C ABI plugin (C source)
```

## Status

| Component | Status |
|-----------|--------|
| **Tests** | [![Tests](https://github.com/lErrorl404l/a3sql/actions/workflows/test.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/test.yml) |
| **Lint** | [![Lint](https://github.com/lErrorl404l/a3sql/actions/workflows/lint.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/lint.yml) |
| **Build** | [![Build](https://github.com/lErrorl404l/a3sql/actions/workflows/build.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/build.yml) |
| **CI** | [![CI](https://github.com/lErrorl404l/a3sql/actions/workflows/ci.yml/badge.svg)](https://github.com/lErrorl404l/a3sql/actions/workflows/ci.yml) |
| **Coverage** | SQL, JOINs, CTE, window functions, triggers, FK cascade, UPSERT |
| **Security** | Parameterized queries, TCP LOGIN |
| **Plugins** | Rust trait, C ABI dynamic, SQF registration |
| **License** | APL-SA — Arma Public License Share Alike |

## License

[Arma Public License Share Alike (APL-SA)](LICENSE) — as required by Bohemia Interactive for
Arma 3 mods. See [LICENSE](LICENSE) for full terms.
