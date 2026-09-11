# SQL Compatibility Report

A3SQL is an embedded SQL engine, not a drop-in for SQLite or PostgreSQL. There
is no external "compatibility spec" to consult — the engine itself is the
spec. The only reliable way to know whether your mod's SQL will run is to run
it against the real extension binary, which is exactly what this page is about.

## Why test your own SQL against the engine

- **The dialect is its own.** A3SQL implements a SQL subset with its own
  behavior (types like `STRINGS[]`, the `%%` fuzzy operator, cursor paging,
  wrapping integer math). Queries that work in SQLite or MySQL are not
  guaranteed to work here, and vice versa.
- **A feature list is not a guarantee.** The dialect reference
  ([SQL-Dialect](SQL-Dialect)) covers every verified feature, but it cannot
  predict whether *your* query shape — compound expressions, nested subqueries,
  odd NULL handling, a specific JOIN — is accepted. The only authority is the
  engine.
- **The binary is the ground truth.** The same C ABI Arma loads is what the
  verification tooling runs against. If a statement passes there, it passes in
  your mission.

## How the engine is verified

Everything documented in the dialect reference is checked against the real
extension binary in three layers:

1. **Dialect sweep** — `tools/sql_dialect_sweep.py` runs checks across
   DDL, DML, SELECT, JOINs, window functions, set operations, CTEs, functions,
   transactions, and types:

   ```bash
   python3 tools/sql_dialect_sweep.py
   ```

2. **Compatibility corpus** — `tools/sql_corpus/` holds realistic SQL
   workloads modeled on what real Arma mods do. Current result: **71/71
   statements pass** across all four domains:

   ```
   tools/sql_corpus/
   ├── 01_stats.sql            # kill tracking, leaderboards, aggregates, TTL cleanup
   ├── 02_admin.sql            # ban/kick queues, whitelists, audit trails
   ├── 03_loadouts.sql         # template CRUD, fuzzy search, progression, FK cascade
   └── 04_sqf_persistence.sql  # profileNamespace-style key-value, player blobs,
                               #   stat counters, event feeds, per-player state maps
   ```

   ```bash
   python3 tools/sql_compat_report.py            # auto-detects the built binary
   python3 tools/sql_compat_report.py --bin a3sql_x64.so
   ```

3. **Rust test suite** — the engine's unit and integration suites
   (`cargo test`), plus the mod's own production SQL in `tools/smoke_test.sql`
   (run through the real binary by `tools/sql_smoke_test.py`).

## Testing YOUR mod's SQL

Drop your own queries into a file and run it through the same runner:

```bash
python3 tools/sql_compat_report.py my_mod.sql
```

File format — plain SQL, one statement per line (may span lines, end with `;`):

```sql
# comments are ignored
CREATE TABLE my_table (id INTEGER PRIMARY KEY, name TEXT);

# assert the next statement returns OK containing a value
# expect contains "PlayerOne"
SELECT name FROM my_table;

# assert the next statement FAILS (e.g. a UNIQUE violation)
# expect error
INSERT INTO my_table VALUES (1, 'dup');
```

No SQL in your mod? The SQF-persistence corpus (`04_sqf_persistence.sql`)
models what mods actually do today — `profileNamespace` key-value stores,
ACE-Arsenal-style loadout blobs, stat counters, missionNamespace event feeds,
per-player state maps. Run it to see that the a3sql equivalent of each pattern
works before you migrate.

## What the corpus found (and fixed)

The corpus is not a formality — it has already caught real engine gaps:

| Gap | Query shape | Status |
|---|---|---|
| Self-referential UPDATE | `UPDATE t SET col = col + 1` | **Fixed** — SET now evaluates with row context |
| Composite primary keys | `PRIMARY KEY (uid, state_key)` | **Fixed** — table-level constraints now mark all columns; duplicates rejected |
| Auto rowid PK | `INSERT INTO t (v) VALUES ('x')` | **Fixed** — INTEGER PRIMARY KEY auto-assigns |
| Date arithmetic | `WHERE ts < datetime('now', '-7 days')` | **Fixed** — SQLite modifiers supported |
| SQLite functions | `instr/ltrim/rtrim/typeof/char/strftime` | **Fixed** |

## How it differs from the smoke test

- **Smoke test** (`sql_smoke_test.py` + `tools/smoke_test.sql`): the mod's
  *own* production SQL — a regression gate for this repo's schema.
- **Compatibility report** (this): realistic *third-party* workload shapes —
  a dialect-coverage gate that grows as mods surface new query patterns.

Both run in CI on every push. If your mod hits something neither covers, add it
to the corpus — it protects every future user.
