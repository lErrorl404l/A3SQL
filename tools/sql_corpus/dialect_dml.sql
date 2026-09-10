# ── Corpus: DIALECT — DML ─────────────────────────────────────────────
# Data manipulation: INSERT / SELECT / UPDATE / DELETE / REPLACE /
# UPSERT / RETURNING. One statement per line, each ends with ';'.
# Self-contained: creates and drops its own schema.

CREATE TABLE IF NOT EXISTS dialect_dml (id TEXT PRIMARY KEY, name TEXT, caliber TEXT, barrel_length FLOAT);

INSERT INTO dialect_dml VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3);

INSERT INTO dialect_dml VALUES ('ak74', 'AK74', '7.62x39mm', 370.0), ('m16a4', 'M16A4', '5.56x45mm', 370.5);

# expect contains "M4A1"
SELECT * FROM dialect_dml;

# expect contains "M4A1"
SELECT name FROM dialect_dml WHERE caliber = '5.56x45mm';

UPDATE dialect_dml SET caliber = '7.62x39mm' WHERE id = 'ak74';

DELETE FROM dialect_dml WHERE barrel_length IS NULL;

REPLACE INTO dialect_dml VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3);

INSERT INTO dialect_dml VALUES ('m4a1', 'M4A1', 'x', 1) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name;

# expect contains "T"
INSERT INTO dialect_dml VALUES ('test', 'T', '9mm', 1) RETURNING *;

# expect contains "test"
UPDATE dialect_dml SET caliber = 'T2' WHERE id = 'test' RETURNING id, caliber;

# expect contains "test"
DELETE FROM dialect_dml WHERE id = 'test' RETURNING *;

# SELECT features: ordering, aggregation, filtering, expressions
# expect contains "M4A1"
SELECT * FROM dialect_dml ORDER BY name ASC LIMIT 10 OFFSET 1;

# expect contains "OK"
SELECT caliber, COUNT(*) AS cnt FROM dialect_dml GROUP BY caliber HAVING COUNT(*) > 0;

# expect contains "OK"
SELECT * FROM dialect_dml WHERE name LIKE '%4%';

# expect contains "OK"
SELECT * FROM dialect_dml WHERE barrel_length BETWEEN 300 AND 400;

# expect contains "OK"
SELECT * FROM dialect_dml WHERE id IN ('m4a1', 'ak74');

# expect contains "OK"
SELECT CASE WHEN barrel_length > 300 THEN 'long' ELSE 'short' END FROM dialect_dml;

# expect contains "OK"
SELECT CAST(barrel_length AS INT) FROM dialect_dml;

# expect contains "OK"
SELECT id || ' (' || caliber || ')' AS combined FROM dialect_dml;

DROP TABLE dialect_dml;
