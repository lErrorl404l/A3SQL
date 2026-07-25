# A3SQL — Arma 3 Database Engine

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
| **Functions** | COUNT(DISTINCT), SUM, AVG, MIN, MAX, UPPER/LOWER, LENGTH, SUBSTR, TRIM, CONCAT, COALESCE/IFNULL, ROUND, ABS, NOW()/CURRENT_TIMESTAMP |
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

- [Getting Started](https://github.com/lErrorl404l/db_a3/wiki/Getting-Started) — Full worked example
- [SQL Dialect](https://github.com/lErrorl404l/db_a3/wiki/SQL-Dialect) — Supported SQL syntax
- [CBA Settings](https://github.com/lErrorl404l/db_a3/wiki/CBA-Settings) — Addon configuration
- [Standalone Server](https://github.com/lErrorl404l/db_a3/wiki/Standalone-Server) — Run without Arma
- [Plugins](https://github.com/lErrorl404l/db_a3/wiki/Plugins) — Extend with Rust/C/SQF
- [Building](https://github.com/lErrorl404l/db_a3/wiki/Building) — Compiling from source
- [Development Setup](https://github.com/lErrorl404l/db_a3/wiki/Development-Setup) — Dev environment guide

## Building

```bash
cargo build --release -p a3sql   # extension
hemtt build                      # addon PBOs
cargo test --lib -p a3sql         # 162+ tests
```

See the [Building page](https://github.com/lErrorl404l/db_a3/wiki/Building) for cross-compilation, code signing, and CI details.

## Project Structure

```
a3sql/
├── extension/           # Rust crate (cdylib + rlib)
│   └── src/             # lib.rs (C ABI), engine/, parser/, bin/
├── addons/{main,sql}/   # Arma 3 addon PBOs
├── include/             # CBA build-time headers
├── tools/               # Python dev tools (UV-managed)
├── .hemtt/              # HEMTT config + hooks
├── keys/                # BI signing keys
└── plugins/             # Example C ABI plugin
```

## Status

| Component | Status |
|-----------|--------|
| SQL engine | 162 tests passing |
| Linting | Clippy clean, SQF validated |
| CI/CD | GitHub Actions (test + 4-platform build + release) |
| Cross-platform | Linux x86_64/i686, Windows x86_64/i686 |
| Signing | DSSignFile via Wine/Proton (Linux CI) |
| License | APL-SA — Arma Public License Share Alike |

## License

[Arma Public License Share Alike (APL-SA)](LICENSE) — as required by Bohemia Interactive for
Arma 3 mods. See [LICENSE](LICENSE) for full terms.
