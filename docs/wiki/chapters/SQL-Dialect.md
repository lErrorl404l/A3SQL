# A3SQL SQL Dialect

This page is the SQL reference for the A3SQL database engine embedded in your
Arma 3 server. It documents **the SQL you can actually write in your mod** —
every feature listed here has been tested against the real extension binary
(the same C ABI Arma loads). If it is on this page, it runs in your mission.

```sqf
["CREATE TABLE IF NOT EXISTS weapons (id STRING PRIMARY KEY, name STRING, caliber STRING, barrelLength FLOAT)"] call a3sql_fnc_execute;
["INSERT INTO weapons VALUES ('rhs_m4a1', 'M4A1', '5.56x45mm', 368.3)"] call a3sql_fnc_execute;
_result = ["SELECT name, barrelLength FROM weapons WHERE caliber = '5.56x45mm' ORDER BY barrelLength DESC"] call a3sql_fnc_execute;
// → [0, "OK", [["name","barrelLength"],["M4A1",368.3]]]
```

## Calling SQL from SQF

### a3sql_fnc_execute

The addon registers `a3sql_fnc_execute` via CfgFunctions. Signature:
`[sql, extension, params] call a3sql_fnc_execute` — `extension` defaults to
`"a3sql"`, and `params` is optional (only needed for parameterized queries).

```sqf
["SELECT * FROM players"] call a3sql_fnc_execute;

// Parameterized — see "Parameterized queries" below
["SELECT * FROM players WHERE uid = $1", "a3sql", ["76561198000000001"]] call a3sql_fnc_execute;
```

### Raw callExtension

The same calls work directly against the extension:

```sqf
_result = "a3sql" callExtension "SELECT * FROM weapons";
_result = ["a3sql", "INSERT INTO weapons VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3)"] callExtension;
```

Multi-statement batches are accepted — separate statements with `;`:

```sqf
_result = "a3sql" callExtension "CREATE TABLE t (id STRING); INSERT INTO t VALUES ('a'); SELECT * FROM t";
```

### Response format

Every call returns `[code, status, data]`:

```
[0,  "OK",       result]
[-1, "ERR_CODE", "message"]
```

| Code | Meaning |
|---|---|
| `ERR_PARSE` | SQL parse error |
| `ERR_EXEC` | Execution error |
| `ERR_TABLE` | Table not found |
| `ERR_TYPE` | Type mismatch |
| `ERR_PK` | Primary key violation |
| `ERR_IO` | File I/O error |
| `ERR_INTERNAL` | Internal error (e.g. response over the 30KB limit — see below) |
| `ERR_AUTH` | Authentication failed (TCP login) |

For `SELECT`, `data` is the column names followed by the rows:

```json
[["id","name","caliber","barrelLength"],["rhs_m4a1","M4A1","5.56x45mm",368.3]]
```

Joined queries keep prefixed column names:

```json
[["weapons.id","weapons.name","attachments.name"],["rhs_m4a1","M4A1","M68 CCO"]]
```

## Feature overview

| Category | Features |
|---|---|
| **Schema** | CREATE/DROP TABLE, CREATE/DROP INDEX (BTREE, TRIGRAM), CREATE/DROP VIEW, ALTER TABLE ADD/DROP/RENAME COLUMN, ALTER TABLE RENAME, TRUNCATE, VACUUM, REINDEX |
| **Types** | INT (BIGINT/SMALLINT/TINYINT/INTEGER), FLOAT (DECIMAL/NUMERIC/DOUBLE/REAL), STRING (VARCHAR/CHAR/TEXT), BOOL/BOOLEAN, DATE/TIMESTAMP, STRINGS[], FLOATS[] |
| **Constraints** | PRIMARY KEY (single + composite), NOT NULL, DEFAULT (incl. expression defaults), CHECK (enforced), FOREIGN KEY ... REFERENCES (enforced, ON DELETE CASCADE), UNIQUE, AUTO_INCREMENT |
| **DML** | INSERT (multi-row VALUES), INSERT OR REPLACE, UPSERT (`ON CONFLICT ... DO UPDATE`), UPDATE, DELETE, REPLACE INTO, RETURNING |
| **SELECT** | WHERE (`= <> < <= > >=`), AND/OR/NOT, IN, NOT IN, BETWEEN, LIKE/NOT LIKE, IS NULL/IS NOT NULL, EXISTS, CASE WHEN, CAST, `%%` fuzzy match, subqueries, derived tables, DISTINCT |
| **JOINs** | CROSS, INNER, LEFT, FULL OUTER, NATURAL, USING, ON (incl. subqueries) |
| **Aggregates** | COUNT (incl. DISTINCT and NULL-skipping), SUM, AVG, MIN, MAX, GROUP_CONCAT — with GROUP BY, HAVING, ORDER BY, LIMIT/OFFSET |
| **Window** | ROW_NUMBER, RANK, DENSE_RANK, LAG, LEAD, FIRST_VALUE, LAST_VALUE, SUM/COUNT/AVG/MIN/MAX OVER (PARTITION BY, ORDER BY, ROWS BETWEEN) |
| **Set ops** | UNION, UNION ALL, EXCEPT, INTERSECT |
| **CTE** | WITH, WITH RECURSIVE (termination enforced) |
| **Functions** | UPPER, LOWER, LENGTH, SUBSTR, TRIM, LTRIM, RTRIM, CONCAT, COALESCE, IFNULL, ROUND, ABS, INSTR, CHAR, TYPEOF, NOW(), CURRENT_TIMESTAMP/CURRENT_DATE/CURRENT_TIME, datetime()/date()/time()/strftime() with modifiers, GROUP_CONCAT |
| **Arithmetic** | `+ - * / %` (i64 wraps on overflow like SQLite, never panics; NULL propagates) |
| **Transactions** | BEGIN/COMMIT/ROLLBACK (no-op when idle), SAVEPOINT/RELEASE SAVEPOINT/ROLLBACK TO SAVEPOINT |
| **Persistence** | save/load, export/import JSON/CSV/SQL, export_to_file, cursor paging |
| **Security** | Parameterized queries (`$1`, `$2`), TCP LOGIN auth, set_credentials |

## Types

| Type | SQL spellings | Notes |
|---|---|---|
| Integer | INT, INTEGER, BIGINT, SMALLINT, TINYINT | 64-bit; arithmetic wraps like SQLite |
| Float | FLOAT, DECIMAL, NUMERIC, DOUBLE, REAL | |
| String | STRING, VARCHAR, CHAR, TEXT | |
| Boolean | BOOL, BOOLEAN | |
| Date/time | DATE, TIMESTAMP | text timestamps; work with datetime()/date()/strftime() |
| Arrays | STRINGS[], FLOATS[] | literals: `ARRAY['a','b']`, `ARRAY[1.5, 2.5]` |

Notes:

- `INTEGER PRIMARY KEY` auto-assigns a rowid when the column is omitted
  (SQLite semantics):

```sql
CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT);
INSERT INTO t (v) VALUES ('x');   -- id becomes 1, then 2, ...
```

- `AUTO_INCREMENT` columns assign the next value automatically.
- NULL is its own value; most operations involving NULL produce NULL.

## Creating and changing schema

### Tables

```sql
CREATE TABLE weapons (
    id STRING PRIMARY KEY,
    name STRING NOT NULL,
    caliber STRING,
    barrelLength FLOAT DEFAULT 0.0
);
DROP TABLE weapons;
```

`CREATE TABLE IF NOT EXISTS` and `DROP TABLE IF EXISTS` are supported.

### Indices

```sql
CREATE INDEX idx_caliber ON weapons (caliber) USING BTREE;   -- exact + range lookups
CREATE INDEX idx_name_fuzzy ON weapons (name) USING TRIGRAM; -- accelerates LIKE and %%
DROP INDEX idx_caliber;
```

- **BTREE** — exact and range lookups.
- **TRIGRAM** — accelerates `LIKE` searches, including prefix and mid/end
  containment, and `%%` fuzzy matches (below).

### Views

```sql
CREATE VIEW heavy_weapons AS SELECT id, name FROM weapons WHERE barrelLength > 400.0;
SELECT * FROM heavy_weapons;
DROP VIEW heavy_weapons;
```

### ALTER TABLE

```sql
ALTER TABLE weapons ADD COLUMN mass FLOAT;
ALTER TABLE weapons DROP COLUMN barrelLength;
ALTER TABLE weapons RENAME COLUMN name TO displayName;
ALTER TABLE weapons RENAME TO armory;
```

### Maintenance

```sql
TRUNCATE TABLE weapons;   -- remove all rows, keep the schema
VACUUM weapons;           -- rebuild storage
REINDEX weapons;          -- rebuild indices
```

## Constraints

```sql
CREATE TABLE prices (
    id STRING PRIMARY KEY,                        -- single-column PK
    price INT NOT NULL CHECK (price > 0),         -- NOT NULL + enforced CHECK
    code STRING UNIQUE,
    created STRING DEFAULT datetime('now')        -- expression default
);

-- Composite primary key
CREATE TABLE inventory (
    uid STRING,
    item STRING,
    qty INT,
    PRIMARY KEY (uid, item)
);
```

- **PRIMARY KEY** — single and composite; duplicates are rejected with `ERR_PK`.
- **NOT NULL** — rejects NULL writes.
- **DEFAULT** — constants or expressions, e.g. `datetime('now')`.
- **CHECK** — enforced on write: `INSERT INTO prices VALUES ('a', -5)` fails.
- **UNIQUE** — rejects duplicates.
- **AUTO_INCREMENT** — next value assigned automatically.
- **FOREIGN KEY ... REFERENCES** — enforced on write: referencing a missing row fails.
- **ON DELETE CASCADE** — deleting the parent deletes the child rows:

```sql
CREATE TABLE orders (id STRING PRIMARY KEY, item STRING REFERENCES prices(id));
CREATE TABLE line_items (
    id STRING PRIMARY KEY,
    order_id STRING REFERENCES orders(id) ON DELETE CASCADE
);
DELETE FROM orders WHERE id = 'o1';   -- cascades to line_items
```

## Writing data

```sql
-- Multi-row insert
INSERT INTO weapons (id, name, caliber) VALUES
    ('rhs_m4a1', 'M4A1', '5.56x45mm'),
    ('rhs_ak74', 'AK-74', '5.45x39mm');

-- Overwrite on conflict
INSERT OR REPLACE INTO weapons (id, name) VALUES ('rhs_m4a1', 'M4A1 SOPMOD');
REPLACE INTO weapons VALUES ('rhs_m4a1', 'M4A1', '5.56x45mm', 368.3);

-- UPSERT: update only the conflicting row
INSERT INTO weapons (id, name, caliber) VALUES ('rhs_m4a1', 'M4A1 SOPMOD', '5.56x45mm')
ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name;

UPDATE weapons SET barrelLength = 370.0 WHERE id = 'rhs_m4a1';
DELETE FROM weapons WHERE caliber IS NULL;
```

### RETURNING

INSERT/UPDATE/DELETE return the affected rows:

```sql
INSERT INTO weapons (id, name) VALUES ('test', 'Test') RETURNING *;
UPDATE weapons SET barrelLength = 300.0 WHERE id = 'test' RETURNING id, name;
DELETE FROM weapons WHERE id = 'test' RETURNING *;
```

```sqf
_result = ["INSERT INTO weapons (id, name) VALUES ('test', 'Test') RETURNING *"] call a3sql_fnc_execute;
// → [0, "OK", [["id","name"],["test","Test"]]]
```

## Reading data

### WHERE

```sql
SELECT * FROM weapons WHERE caliber = '5.56x45mm';
SELECT * FROM weapons WHERE barrelLength > 400.0 AND caliber <> '9x19mm';
SELECT * FROM weapons WHERE caliber IN ('5.56x45mm', '7.62x39mm');
SELECT * FROM weapons WHERE caliber NOT IN ('9x19mm');
SELECT * FROM weapons WHERE barrelLength BETWEEN 300.0 AND 400.0;   -- reversed bounds also safe
SELECT * FROM weapons WHERE name LIKE 'M4%';                        -- % and _ wildcards
SELECT * FROM weapons WHERE name NOT LIKE '%carryhandle';
SELECT * FROM weapons WHERE barrelLength IS NULL;
SELECT * FROM weapons WHERE barrelLength IS NOT NULL;
```

Comparison operators: `=`, `<>` (or `!=`), `<`, `<=`, `>`, `>=`. Conditions
combine with `AND`, `OR`, `NOT`.

**LIKE** — `%` matches any run of characters, `_` matches one character. A
TRIGRAM index on the column accelerates prefix and mid/end containment
searches.

### CASE, CAST, DISTINCT

```sql
SELECT id, CASE WHEN barrelLength > 400.0 THEN 'heavy' ELSE 'light' END AS class FROM weapons;
SELECT DISTINCT caliber FROM weapons;
SELECT CAST(barrelLength AS INT) FROM weapons;
SELECT CAST('42' AS INT), CAST(1.9 AS FLOAT);
```

### Fuzzy match (`%%`)

`%%` is trigram similarity — the fuzzy-match operator for typo-tolerant
name/UID lookups:

```sql
SELECT * FROM weapons WHERE id %% 'rhs_m4';
-- matches rhs_m4a1, rhs_m4a1_carryhandle, ...
```

```sqf
_results = ["SELECT name, uid FROM players WHERE name %% 'joh'"] call a3sql_fnc_execute;
// matches "John", "Johnson", "Johansson" via trigram similarity
```

### Subqueries and derived tables

```sql
-- Scalar subquery
SELECT name, (SELECT COUNT(*) FROM attachments a WHERE a.weaponId = weapons.id) AS part_count
FROM weapons;

-- Subquery in an IN / EXISTS predicate
SELECT * FROM weapons WHERE id IN (SELECT weaponId FROM attachments);
SELECT * FROM weapons WHERE EXISTS (SELECT 1 FROM attachments WHERE weaponId = weapons.id);

-- Subquery in a JOIN ON condition
SELECT w.name FROM weapons w
JOIN attachments a ON w.id = a.weaponId
    AND (SELECT COUNT(*) FROM attachments a2 WHERE a2.weaponId = w.id) > 2;

-- Derived table
SELECT d.caliber, COUNT(*) FROM (SELECT * FROM weapons WHERE barrelLength > 300.0) d
GROUP BY d.caliber;
```

### ORDER BY, LIMIT, OFFSET

```sql
SELECT name, score FROM players ORDER BY score DESC LIMIT 10 OFFSET 5;
```

ORDER BY sorts numbers numerically, not lexically: `2 < 10`.

## Joins

```sql
-- INNER / LEFT / CROSS
SELECT w.name, a.name FROM weapons w INNER JOIN attachments a ON w.id = a.weaponId;
SELECT * FROM weapons w LEFT JOIN attachments a ON w.id = a.weaponId;
SELECT * FROM weapons CROSS JOIN attachments;

-- FULL OUTER — keeps rows unmatched on either side
SELECT * FROM weapons w FULL OUTER JOIN attachments a ON w.id = a.weaponId;

-- NATURAL — joins on columns with the same name
SELECT * FROM weapons NATURAL JOIN attachments;

-- USING — joins on the named column
SELECT * FROM weapons JOIN attachments USING (id);
```

## Aggregates

```sql
SELECT COUNT(*) FROM weapons;
SELECT COUNT(DISTINCT caliber) FROM weapons;      -- distinct values only
SELECT COUNT(barrelLength) FROM weapons;          -- skips NULLs
SELECT SUM(barrelLength), AVG(barrelLength), MIN(barrelLength), MAX(barrelLength) FROM weapons;
SELECT GROUP_CONCAT(name) FROM weapons;           -- comma-separated list

SELECT caliber, COUNT(*) AS cnt FROM weapons GROUP BY caliber;

-- HAVING filters on aggregate results
SELECT caliber, AVG(barrelLength) AS avg_len FROM weapons
GROUP BY caliber HAVING AVG(barrelLength) > 350.0;
```

## Window functions

```sql
SELECT id, ROW_NUMBER() OVER (ORDER BY name) AS rn FROM weapons;
SELECT id, RANK()       OVER (PARTITION BY caliber ORDER BY name) AS r FROM weapons;
SELECT id, DENSE_RANK() OVER (ORDER BY score DESC) AS dr FROM players;

SELECT id, name,
       LAG(barrelLength)  OVER (ORDER BY name) AS prev,
       LEAD(barrelLength) OVER (ORDER BY name) AS next,
       FIRST_VALUE(name)  OVER (ORDER BY name) AS first_name,
       LAST_VALUE(name)   OVER (ORDER BY name) AS last_name
FROM weapons;

-- Aggregate over a window
SELECT id, SUM(score) OVER (PARTITION BY faction ORDER BY ts) AS running_total FROM kills;

-- ROWS BETWEEN frames (running sums, moving averages)
SELECT id, name, AVG(barrelLength) OVER (
    ORDER BY name ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING
) AS moving_avg FROM weapons;
```

## Set operations

```sql
SELECT id FROM weapons UNION        SELECT weaponId FROM attachments;
SELECT id FROM weapons UNION ALL    SELECT weaponId FROM attachments;   -- keeps duplicates
SELECT id FROM weapons EXCEPT       SELECT weaponId FROM attachments;
SELECT id FROM weapons INTERSECT    SELECT weaponId FROM attachments;
```

## Common table expressions

```sql
WITH top AS (SELECT * FROM weapons ORDER BY name LIMIT 5) SELECT * FROM top;

-- Recursive — grows a row set until it stops changing. A recursion that does
-- not terminate within 100 iterations fails with an error instead of running
-- forever.
WITH RECURSIVE nums(n) AS (
    SELECT 1
    UNION ALL
    SELECT n + 1 FROM nums WHERE n < 10
) SELECT * FROM nums;
```

## Functions

### Scalar

```sql
SELECT UPPER(name), LOWER(name), LENGTH(name), SUBSTR(name, 1, 3) FROM weapons;
SELECT TRIM(name), LTRIM(name), RTRIM(name) FROM weapons;
SELECT CONCAT(id, ' / ', caliber) FROM weapons;
SELECT COALESCE(barrelLength, 0.0), IFNULL(barrelLength, 0.0) FROM weapons;
SELECT ROUND(barrelLength, 1), ABS(barrelLength) FROM weapons;
SELECT INSTR(name, 'M4'), CHAR(65), TYPEOF(barrelLength) FROM weapons;
```

- `SUBSTR` — 1-indexed, 2 or 3 args.
- `INSTR` — 1-based position, 0 when the substring is absent.
- `TYPEOF` — returns `null`, `integer`, `real`, `text`, or `array`.

### Date/time

```sql
SELECT NOW(), CURRENT_TIMESTAMP, CURRENT_DATE, CURRENT_TIME;
SELECT datetime('now'), date('now'), time('now');
SELECT datetime('now', '+1 day'), datetime('now', '-30 days'), datetime('now', '+3 hours');
SELECT strftime('%Y-%m-%d', 'now');
```

Modifiers: `'now'`, `'+1 day'`, `'-30 days'`, `'+3 hours'`, and combinations —
handy for TTL-style cleanup:

```sql
DELETE FROM sessions WHERE last_seen < datetime('now', '-7 days');
```

### Aggregate

COUNT, SUM, AVG, MIN, MAX, GROUP_CONCAT — see [Aggregates](#aggregates).

## Arithmetic

```sql
SELECT score + 1, score - 1, score * 2, score / 2, score % 10 FROM players;
```

- Integer math is 64-bit and **wraps on overflow like SQLite** — it never
  panics (e.g. `MIN_INT - 1` wraps to `MAX_INT`).
- **NULL propagates** — any arithmetic with NULL yields NULL.

## Transactions

```sql
BEGIN;
INSERT INTO weapons VALUES ('test', 'Test', '9x19mm', 200.0);
COMMIT;   -- or ROLLBACK

-- ROLLBACK with no open transaction is a no-op (like PostgreSQL)
ROLLBACK;

-- Savepoints
SAVEPOINT sp1;
INSERT INTO weapons VALUES ('tmp', 'Temp', '5.56x45mm', 300.0);
ROLLBACK TO sp1;
RELEASE SAVEPOINT sp1;
```

```sqf
["BEGIN"] call a3sql_fnc_execute;
["INSERT INTO log (event, time) VALUES ('mission_start', datetime('now'))"] call a3sql_fnc_execute;
["COMMIT"] call a3sql_fnc_execute;
```

## Control commands

Control commands are not SQL — they manage the engine itself (persistence,
prepared statements, cursors, the TCP listener). They run through the same
`a3sql_fnc_execute` / `callExtension` path.

```sqf
// Save / load the whole database
["data.bin"] call a3sql_fnc_save;
["data.bin"] call a3sql_fnc_load;

// Prepared statements — parse once, run many times with different args
["prepare add_kill INSERT INTO kills (uid, weapon) VALUES ($1, $2)"] call a3sql_fnc_execute;
["execute_prepared add_kill 76561198000000001 rhs_m4a1"] call a3sql_fnc_execute;

// Cursors — page large results (see "Large results" below)
["cursor create c SELECT * FROM kill_events"] call a3sql_fnc_execute;
["cursor fetch c 100"] call a3sql_fnc_execute;
["cursor drop c"] call a3sql_fnc_execute;

// Export / import
["export json players"] call a3sql_fnc_execute;
["export csv players"] call a3sql_fnc_execute;
["export sql"] call a3sql_fnc_execute;                          // whole database
_result = ["a3sql", "import json players", [_jsonData]] callExtension;

// Write an export to a file on disk
["export_to_file csv players player_stats.csv"] call a3sql_fnc_execute;

// TCP listener (auto-starts on game boot per CBA settings)
["listen"] call a3sql_fnc_execute;         // default port 33306
["listen 33307"] call a3sql_fnc_execute;   // custom port
["stop"] call a3sql_fnc_execute;

// TCP login credentials — after this, clients must LOGIN before querying
["set_credentials admin mypassword"] call a3sql_fnc_execute;

// Plugins
["plugins"] call a3sql_fnc_execute;        // list loaded plugins

// Execution plan (runs as a SQL statement)
["EXPLAIN SELECT * FROM weapons WHERE caliber = '5.56x45mm'"] call a3sql_fnc_execute;

// Current build
["version"] call a3sql_fnc_execute;
// → [0, "OK", "a3sql <current build>"]  — check it against your installed release
```

| Command | What it does |
|---|---|
| `save` / `save <path>` | Persist the database to a binary file |
| `load` / `load <path>` | Restore the database from a file |
| `prepare <name> <sql>` | Store a parameterized statement |
| `execute_prepared <name> [args...]` | Run a prepared statement |
| `cursor create <name> <query>` | Open a cursor over a query |
| `cursor fetch <name> [limit]` | Fetch the next page (default 100 rows) |
| `cursor drop <name>` | Close a cursor |
| `export <format> [table]` | Export `json` / `csv` / `sql` (`sql` = whole DB) |
| `import <format> <table>` | Import `json` / `csv` data |
| `export_to_file <format> [table] <path>` | Write an export to a file |
| `listen [port]` / `stop` | Start / stop the TCP listener |
| `set_credentials <user> <pass>` | Set TCP login credentials |
| `plugins` | List loaded plugins |
| `EXPLAIN <sql>` | Show the execution plan |
| `version` | Report the current build |

The `version` command returns the current build; it is derived from the
extension itself, so do not assume a specific number — compare it against the
release you installed. (`help` is not a command in the in-game extension; the
standalone server's REPL accepts `:help`.)

## Large results — the 30KB limit

Arma 3's `callExtension` has a 30KB output ceiling. Any single response larger
than that **fails loudly** — it does not silently truncate, and it does not
suggest LIMIT/OFFSET:

```
[-1, "ERR_INTERNAL", "Result exceeds output buffer (30KB) — use 'cursor create <name> <query>' + 'cursor fetch <name> [limit]' to page large results"]
```

A `SELECT *` over thousands of rows will hit this. Do not page with
LIMIT/OFFSET — use a cursor instead:

```sqf
// Too big to return in one response:
// _result = ["SELECT * FROM kill_events"] call a3sql_fnc_execute;
// → [-1, "ERR_INTERNAL", "Result exceeds output buffer (30KB) — ..."]

// Open a cursor, fetch in pages, drop it when done
["cursor create big_kills SELECT * FROM kill_events"] call a3sql_fnc_execute;

_page = ["cursor fetch big_kills 100"] call a3sql_fnc_execute;
// → [0, "OK", [["id","uid","weapon","ts"], [first 100 rows...]]]

_page = ["cursor fetch big_kills 100"] call a3sql_fnc_execute;
// → next 100 rows — the cursor tracks the offset

["cursor drop big_kills"] call a3sql_fnc_execute;
```

`cursor fetch` without an explicit limit returns up to 100 rows. Keep fetching
until a page comes back with no rows, then drop the cursor.

## Parameterized queries

Never build SQL by string-concatenating player input — use `$1`, `$2`
placeholders:

```sqf
// UNSAFE — string interpolation (SQL injection possible)
private _sql = format ["SELECT * FROM users WHERE name = '%1'", _userInput];
["a3sql", _sql] callExtension;

// SAFE — parameterized query (injection prevented)
_result = ["SELECT * FROM users WHERE name = $1", "a3sql", [_userInput]] call a3sql_fnc_execute;
```

## CBA wrapper functions

| Function | Description |
|---|---|
| `a3sql_fnc_init` | Initialize the extension |
| `a3sql_fnc_execute` | Execute SQL (params via `$1`, `$2`) |
| `a3sql_fnc_save` | Persist the database to a binary file |
| `a3sql_fnc_load` | Restore the database from a file |
| `a3sql_fnc_loadJSON` | Import JSON data into a table |
| `a3sql_fnc_exportJSON` | Export a table as JSON |
| `a3sql_fnc_exportCSV` | Export a table as CSV |
| `a3sql_fnc_exportSQL` | Export the whole database as SQL statements |
| `a3sql_fnc_dumpSQL` | Same as exportSQL |
| `a3sql_fnc_settings` | Register CBA settings (auto-called via PreInit) |
| `a3sql_fnc_postInit` | Post-mission init (auto-save/load hooks) |

## See also

- [TCP-Connector](TCP-Connector) and [Security](Security) — the TCP listener,
  LOGIN auth, and credentials.
- [SQL-Compatibility-Report](SQL-Compatibility-Report) — how the dialect is
  verified and how to test your own mod's SQL.
- [Building](Building) and [Development-Setup](Development-Setup) — building
  the extension and running the test suite.

## License

APL-SA (Arma Public License Share Alike).
