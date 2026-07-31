#!/usr/bin/env python3
"""Dialect sweep — run every feature documented in SQL-Dialect.md against the
real extension binary and report failures. Exit 0 = documented == working."""

import ctypes
import os
import sys

lib = ctypes.CDLL(os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else "a3sql_x64.so"))
lib.RVExtension.argtypes = [ctypes.c_char_p, ctypes.c_uint32, ctypes.c_char_p]


def S(fn):
    b = ctypes.create_string_buffer(30720)
    lib.RVExtension(b, 30720, fn.encode())
    return b.value.decode()


passed, failed = 0, 0


def check(label, sql, expect_ok=True, contains=None):
    global passed, failed
    r = S(sql)
    ok = r.startswith('[0,"OK"') if expect_ok else not r.startswith('[0,"OK"')
    if ok and contains:
        ok = contains in r
    if ok:
        passed += 1
        print(f"  PASS  {label}")
    else:
        failed += 1
        print(f"  FAIL  {label}: {r[:140]}")


# ── DDL ──────────────────────────────────────────────────────────────
check(
    "CREATE TABLE",
    "CREATE TABLE w (id TEXT PRIMARY KEY, name TEXT, caliber TEXT, barrelLength FLOAT)",
)
check(
    "CREATE TABLE IF NOT EXISTS",
    "CREATE TABLE IF NOT EXISTS w (id TEXT PRIMARY KEY, name TEXT)",
)
check("DROP TABLE", "CREATE TABLE t_drop (x INT); DROP TABLE t_drop")
check("CREATE INDEX BTREE", "CREATE INDEX idx_cal ON w (caliber) USING BTREE")
check("CREATE INDEX TRIGRAM", "CREATE INDEX idx_name ON w (name) USING TRIGRAM")
check("ALTER ADD COLUMN", "ALTER TABLE w ADD COLUMN mass FLOAT")
check("ALTER DROP COLUMN", "ALTER TABLE w DROP COLUMN mass")
check("ALTER RENAME COLUMN", "ALTER TABLE w RENAME COLUMN name TO displayName")
check(
    "ALTER RENAME TABLE",
    "ALTER TABLE w RENAME TO armory; ALTER TABLE armory RENAME TO w",
)
check("TRUNCATE", "TRUNCATE TABLE w")
check("CREATE VIEW", "CREATE VIEW v_short AS SELECT * FROM w WHERE barrelLength < 300")
check("SELECT FROM VIEW", "SELECT * FROM v_short")
check("DROP VIEW", "DROP VIEW v_short")
check("CREATE TRIGGER", "CREATE TRIGGER tr_ins AFTER INSERT ON w BEGIN SELECT 1; END")
check("DROP TRIGGER", "DROP TRIGGER tr_ins")
check("VACUUM", "VACUUM w")
check("REINDEX", "REINDEX w")
check(
    "CHECK enforced",
    "CREATE TABLE prices (item TEXT PRIMARY KEY, price INT CHECK (price > 0))",
)
check("CHECK rejects bad", "INSERT INTO prices VALUES ('a', -5)", expect_ok=False)
check(
    "FK enforced",
    "CREATE TABLE orders (id TEXT PRIMARY KEY, item TEXT REFERENCES prices(item))",
)
check(
    "FK rejects bad", "INSERT INTO orders VALUES ('o1', 'nonexistent')", expect_ok=False
)
check(
    "FK CASCADE",
    "CREATE TABLE line_items (id TEXT PRIMARY KEY, order_id TEXT REFERENCES orders(id) ON DELETE CASCADE, product TEXT)",
)
check(
    "AUTOINCREMENT", "CREATE TABLE seq (id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT)"
)

# ── DML ──────────────────────────────────────────────────────────────
check("INSERT", "INSERT INTO w VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3)")
check(
    "INSERT multiple",
    "INSERT INTO w VALUES ('ak74', 'AK74', '7.62x39mm', 370.0), ('m16a4', 'M16A4', '5.56x45mm', 370.5)",
)
check("SELECT all", "SELECT * FROM w")
check("SELECT where", "SELECT name FROM w WHERE caliber = '5.56x45mm'")
check("UPDATE", "UPDATE w SET caliber = '7.62x39mm' WHERE id = 'ak74'")
check("DELETE", "DELETE FROM w WHERE barrelLength IS NULL")
check("REPLACE INTO", "REPLACE INTO w VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3)")
check(
    "UPSERT ON CONFLICT",
    "INSERT INTO w VALUES ('m4a1','M4A1','x',1) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name",
)
check("RETURNING insert", "INSERT INTO w VALUES ('test', 'T', '9mm', 1) RETURNING *")
check(
    "RETURNING update",
    "UPDATE w SET caliber = 'T2' WHERE id = 'test' RETURNING id, caliber",
)
check("RETURNING delete", "DELETE FROM w WHERE id = 'test' RETURNING *")

# ── SELECT features ──────────────────────────────────────────────────
check("ORDER BY", "SELECT * FROM w ORDER BY name ASC LIMIT 10 OFFSET 5")
check("JOIN INNER", "SELECT * FROM w INNER JOIN w w2 ON w.id = w2.id")
check("JOIN LEFT", "SELECT * FROM w LEFT JOIN w w2 ON w.id = w2.id")
check("JOIN FULL OUTER", "SELECT * FROM w FULL OUTER JOIN w w2 ON w.id = w2.id")
check("JOIN NATURAL", "SELECT * FROM w NATURAL JOIN w")
check("JOIN USING", "SELECT * FROM w JOIN w w2 USING (id)")
check("GROUP BY", "SELECT caliber, COUNT(*) AS cnt FROM w GROUP BY caliber")
check("HAVING", "SELECT caliber FROM w GROUP BY caliber HAVING COUNT(*) > 1")
check("UNION", "SELECT id FROM w UNION SELECT id FROM w")
check("EXCEPT", "SELECT id FROM w EXCEPT SELECT id FROM w WHERE id = 'm4a1'")
check("INTERSECT", "SELECT id FROM w INTERSECT SELECT id FROM w")
check("CTE", "WITH top AS (SELECT * FROM w LIMIT 2) SELECT * FROM top")
check(
    "WITH RECURSIVE",
    "WITH RECURSIVE nums(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 10) SELECT * FROM nums",
)
check("subquery IN", "SELECT * FROM w WHERE id IN (SELECT id FROM w)")
check(
    "subquery EXISTS",
    "SELECT * FROM w WHERE EXISTS (SELECT 1 FROM w w2 WHERE w2.id = w.id)",
)
check("LIKE", "SELECT * FROM w WHERE name LIKE '%4%'")
check("BETWEEN", "SELECT * FROM w WHERE barrelLength BETWEEN 300 AND 400")
check("IN", "SELECT * FROM w WHERE id IN ('m4a1', 'ak74')")
check("IS NULL", "SELECT * FROM w WHERE barrelLength IS NULL")
check(
    "CASE WHEN",
    "SELECT CASE WHEN barrelLength > 300 THEN 'long' ELSE 'short' END FROM w",
)
check("CAST", "SELECT CAST(barrelLength AS INT) FROM w")
check("concat ||", "SELECT id || ' (' || caliber || ')' AS combined FROM w")
check("fuzzy %%", "SELECT * FROM w WHERE id %% 'm4'")
check("window ROW_NUMBER", "SELECT id, ROW_NUMBER() OVER (ORDER BY name) AS rn FROM w")
check(
    "window RANK partition",
    "SELECT id, RANK() OVER (PARTITION BY caliber ORDER BY name) FROM w",
)
check(
    "window ROWS frame",
    "SELECT id, AVG(barrelLength) OVER (ORDER BY name ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM w",
)
check("EXPLAIN", "EXPLAIN SELECT * FROM w")

# ── Functions ────────────────────────────────────────────────────────
check("NOW", "SELECT NOW()")
check("CURRENT_TIMESTAMP", "SELECT CURRENT_TIMESTAMP")
check("datetime now", "SELECT datetime('now')")
check("COALESCE", "SELECT COALESCE(barrelLength, 0.0) FROM w")
check("UPPER", "SELECT UPPER(name) FROM w")
check("LOWER", "SELECT LOWER(name) FROM w")
check("LENGTH", "SELECT LENGTH(name) FROM w")
check("SUBSTR", "SELECT SUBSTR(name, 1, 3) FROM w")
check("TRIM", "SELECT TRIM('  x  ')")
check("CONCAT", "SELECT CONCAT('a', 'b')")
check("ROUND", "SELECT ROUND(3.14159, 2)")
check("ABS", "SELECT ABS(-5)")
check("POW", "SELECT POW(2, 3)")
check("SQRT", "SELECT SQRT(16)")
check("CEIL", "SELECT CEIL(1.2)")
check("FLOOR", "SELECT FLOOR(1.8)")
check("SIGN", "SELECT SIGN(-5)")
check("REPLACE", "SELECT REPLACE('M4A1', '4', '4A')")
check("COUNT DISTINCT", "SELECT COUNT(DISTINCT caliber) FROM w")
check(
    "aggregate SUM/AVG/MIN/MAX",
    "SELECT SUM(barrelLength), AVG(barrelLength), MIN(barrelLength), MAX(barrelLength) FROM w",
)
check("IFNULL", "SELECT IFNULL(NULL, 'dflt')")
check("SQF_EVAL sqrt", "SELECT SQF_EVAL('sqrt 25')")
check("SQF_EVAL pi", "SELECT SQF_EVAL('pi')")

# ── Transactions ─────────────────────────────────────────────────────
check("BEGIN", "BEGIN")
check("COMMIT", "COMMIT")
check("ROLLBACK idle", "ROLLBACK")
check("SAVEPOINT", "SAVEPOINT sp1; RELEASE SAVEPOINT sp1")
check("ROLLBACK TO", "BEGIN; SAVEPOINT sp1; ROLLBACK TO sp1; COMMIT")

# ── Types ────────────────────────────────────────────────────────────
check("BOOL", "CREATE TABLE t_bool (b BOOL); INSERT INTO t_bool VALUES (true)")
check(
    "STRINGS[]",
    "CREATE TABLE t_arr (s STRINGS[]); INSERT INTO t_arr VALUES (ARRAY['a','b'])",
)
check(
    "FLOATS[]",
    "CREATE TABLE t_farr (f FLOATS[]); INSERT INTO t_farr VALUES (ARRAY[1.5, 2.5])",
)
check("DATE type", "CREATE TABLE t_date (d DATE)")
check("TIMESTAMP type", "CREATE TABLE t_ts (t TIMESTAMP)")

# ── Path B additions: SQLite compat layer ────────────────────────────
check(
    "INSERT OR REPLACE",
    "CREATE TABLE t_ir (id INTEGER PRIMARY KEY, v TEXT); INSERT OR REPLACE INTO t_ir VALUES (1, 'x')",
)
check("INSERT OR IGNORE dup", "INSERT OR IGNORE INTO t_ir VALUES (1, 'y')")
check("INSERT OR IGNORE new", "INSERT OR IGNORE INTO t_ir VALUES (2, 'z')")
check("datetime +1 day", "SELECT datetime('now', '+1 day')")
check("datetime -30 days", "SELECT datetime('now', '-30 days')")
check("datetime +3 hours", "SELECT datetime('now', '+3 hours')")
check("date()", "SELECT date('now')")
check("time()", "SELECT time('now')")
check("strftime", "SELECT strftime('%Y-%m-%d %H:%M:%S', 'now')")
check("instr", "SELECT instr('hello', 'll')")
check("ltrim", "SELECT ltrim('  x')")
check("rtrim", "SELECT rtrim('x  ')")
check("typeof", "SELECT typeof(42)")
check("char", "SELECT char(65, 66)")
check("lower/upper", "SELECT lower('ABC'), upper('abc')")

print(f"\nDIALECT SWEEP: {passed} passed, {failed} failed")
sys.exit(1 if failed else 0)

# ── Path B additions: SQLite compat layer ────────────────────────────
check(
    "INSERT OR REPLACE",
    "CREATE TABLE t_ir (id INTEGER PRIMARY KEY, v TEXT); INSERT OR REPLACE INTO t_ir VALUES (1, 'x')",
)
check("INSERT OR IGNORE dup", "INSERT OR IGNORE INTO t_ir VALUES (1, 'y')")
check("INSERT OR IGNORE new", "INSERT OR IGNORE INTO t_ir VALUES (2, 'z')")
check("datetime +1 day", "SELECT datetime('now', '+1 day')")
check("datetime -30 days", "SELECT datetime('now', '-30 days')")
check("datetime +3 hours", "SELECT datetime('now', '+3 hours')")
check("date()", "SELECT date('now')")
check("time()", "SELECT time('now')")
check("strftime", "SELECT strftime('%Y-%m-%d %H:%M:%S', 'now')")
check("instr", "SELECT instr('hello', 'll')")
check("ltrim", "SELECT ltrim('  x')")
check("rtrim", "SELECT rtrim('x  ')")
check("typeof", "SELECT typeof(42)")
check("char", "SELECT char(65, 66)")
check("lower/upper", "SELECT lower('ABC'), upper('abc')")
