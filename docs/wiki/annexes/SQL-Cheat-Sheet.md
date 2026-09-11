# SQL Cheat Sheet

Copy-paste patterns for the most common A3SQL operations. For full details
see [SQL Dialect](SQL-Dialect).

## Schema

```sql
-- Create table
CREATE TABLE players (
    uid   STRING PRIMARY KEY,
    name  STRING NOT NULL,
    score INT DEFAULT 0
);

-- Add column
ALTER TABLE players ADD COLUMN kills INT;

-- Index (BTREE for exact/range, TRIGRAM for fuzzy)
CREATE INDEX idx_score ON players (score) USING BTREE;
CREATE INDEX idx_name  ON players (name)  USING TRIGRAM;

-- Drop
DROP TABLE IF EXISTS players;
DROP INDEX IF EXISTS idx_score;
```

## Write

```sql
-- Insert
INSERT INTO players (uid, name) VALUES ('76561198000000001', 'Ghost');

-- Multi-row insert
INSERT INTO players (uid, name) VALUES
    ('76561198000000001', 'Ghost'),
    ('76561198000000002', 'Scarface');

-- Upsert (insert or update on conflict)
INSERT INTO players (uid, name, score) VALUES ($1, $2, 1)
ON CONFLICT (uid) DO UPDATE SET score = score + 1, name = $2;

-- Update
UPDATE players SET score = 42 WHERE uid = '76561198000000001';

-- Delete
DELETE FROM players WHERE score < 0;
```

## Read

```sql
-- Basic select
SELECT * FROM players WHERE uid = '76561198000000001';

-- Filter
SELECT * FROM players WHERE score > 10 AND name LIKE 'S%';

-- Top 10 leaderboard
SELECT name, score FROM players ORDER BY score DESC LIMIT 10;

-- Distinct values
SELECT DISTINCT name FROM players;

-- Count
SELECT COUNT(*) FROM players;
SELECT COUNT(DISTINCT uid) FROM players;
```

## Joins

```sql
-- Inner join
SELECT p.name, s.kills
FROM players p
INNER JOIN stats s ON p.uid = s.uid;

-- Left join (keep all players even without stats)
SELECT p.name, COALESCE(s.kills, 0) AS kills
FROM players p
LEFT JOIN stats s ON p.uid = s.uid;
```

## Aggregates

```sql
-- Group by
SELECT name, SUM(kills) AS total_kills
FROM stats
GROUP BY name;

-- Having (filter on aggregate)
SELECT name, SUM(kills) AS total_kills
FROM stats
GROUP BY name
HAVING SUM(kills) > 10;
```

## Window Functions

```sql
-- Rank players within each faction
SELECT uid, name, faction,
    RANK() OVER (PARTITION BY faction ORDER BY score DESC) AS rank
FROM players;

-- Running total
SELECT uid, score,
    SUM(score) OVER (ORDER BY ts) AS running_total
FROM kills;

-- Row number
SELECT uid, ROW_NUMBER() OVER (ORDER BY score DESC) AS rn
FROM players;
```

## CTEs

```sql
-- With
WITH top5 AS (SELECT * FROM players ORDER BY score DESC LIMIT 5)
SELECT * FROM top5 WHERE score > 100;

-- Recursive (max 100 iterations)
WITH RECURSIVE tree AS (
    SELECT id, parent_id, name FROM units WHERE id = 1
    UNION ALL
    SELECT u.id, u.parent_id, u.name
    FROM units u JOIN tree t ON u.parent_id = t.id
) SELECT * FROM tree;
```

## Transactions

```sql
BEGIN;
INSERT INTO log (event, time) VALUES ('mission_start', datetime('now'));
COMMIT;

-- Rollback on error
BEGIN;
UPDATE accounts SET balance = balance - 100 WHERE id = 'a';
-- something went wrong
ROLLBACK;

-- Savepoint
SAVEPOINT sp1;
INSERT INTO tmp VALUES ('test');
ROLLBACK TO sp1;
```

## Date/Time

```sql
-- Current time
SELECT NOW(), CURRENT_TIMESTAMP, CURRENT_DATE;

-- Relative time
SELECT datetime('now', '+1 day');
SELECT datetime('now', '-7 days');

-- Cleanup old sessions
DELETE FROM sessions WHERE last_seen < datetime('now', '-7 days');

-- Format
SELECT strftime('%Y-%m-%d', 'now');
```

## Fuzzy Match

```sql
-- Trigram similarity (typo-tolerant)
SELECT * FROM players WHERE name %% 'joh';
-- matches "John", "Johnson", "Johansson"
```

## Cursors (Large Results)

```sql
-- Open cursor for queries that exceed 30KB
["cursor create c SELECT * FROM kill_events"] call a3sql_fnc_execute;

-- Fetch pages
["cursor fetch c 100"] call a3sql_fnc_execute;

-- Done
["cursor drop c"] call a3sql_fnc_execute;
```

## Parameterized Queries (Security)

```sqf
-- SAFE: placeholders ($1, $2)
_result = ["SELECT * FROM players WHERE uid = $1", "a3sql", [_uid]] call a3sql_fnc_execute;

-- UNSAFE: never do this
_result = [format ["SELECT * FROM players WHERE uid = '%1'", _uid]] call a3sql_fnc_execute;
```

## Control Commands

```sqf
-- Save / load
["data.bin"] call a3sql_fnc_save;
["data.bin"] call a3sql_fnc_load;

-- Prepared statements
["prepare add_kill INSERT INTO kills (uid, weapon) VALUES ($1, $2)"] call a3sql_fnc_execute;
["execute_prepared add_kill 76561198000000001 rhs_m4a1"] call a3sql_fnc_execute;

-- Export
["export json players"] call a3sql_fnc_execute;
["export csv players"] call a3sql_fnc_execute;
["export_to_file csv players stats.csv"] call a3sql_fnc_execute;
```

## Types

| Type | Aliases | Notes |
|---|---|---|
| INT | INTEGER, BIGINT, SMALLINT, TINYINT | 64-bit, wraps on overflow |
| FLOAT | DECIMAL, NUMERIC, DOUBLE, REAL | |
| STRING | VARCHAR, CHAR, TEXT | |
| BOOL | BOOLEAN | |
| DATE | TIMESTAMP | Text timestamps |
| STRINGS[] | | `ARRAY['a','b']` |
| FLOATS[] | | `ARRAY[1.5, 2.5]` |

## See Also

- [SQL Dialect](SQL-Dialect) — full reference
- [Module Guide](Module-Guide) — integration patterns
- [Use Cases](Use-Cases) — practical recipes
