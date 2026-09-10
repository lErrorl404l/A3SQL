# ── Corpus: DIALECT — ADVANCED ────────────────────────────────────────
# CTEs, recursive CTEs, joins, set operations, subqueries, aggregates,
# window functions, MERGE. One statement per line, each ends with ';'.
# Self-contained: creates and drops its own schema.

CREATE TABLE IF NOT EXISTS dialect_adv (id TEXT PRIMARY KEY, name TEXT, caliber TEXT, barrel_length FLOAT);

CREATE TABLE IF NOT EXISTS dialect_adv_ref (id TEXT PRIMARY KEY, note TEXT);

CREATE TABLE IF NOT EXISTS dialect_adv_src (id TEXT PRIMARY KEY, name TEXT, caliber TEXT, barrel_length FLOAT);

INSERT INTO dialect_adv VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3), ('ak74', 'AK74', '7.62x39mm', 370.0), ('m16a4', 'M16A4', '5.56x45mm', 370.5);

INSERT INTO dialect_adv_ref VALUES ('m4a1', 'rifle'), ('ak74', 'rifle');

INSERT INTO dialect_adv_src VALUES ('m4a1', 'M4A1', '5.56x45mm', 368.3), ('m249', 'M249', '5.56x45mm', 400.0);

# expect contains "OK"
WITH top AS (SELECT * FROM dialect_adv LIMIT 2) SELECT * FROM top;

# expect contains "OK"
WITH RECURSIVE nums(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 10) SELECT * FROM nums;

# expect contains "OK"
SELECT * FROM dialect_adv INNER JOIN dialect_adv_ref r ON dialect_adv.id = r.id;

# expect contains "OK"
SELECT * FROM dialect_adv LEFT JOIN dialect_adv_ref r ON dialect_adv.id = r.id;

# expect contains "OK"
SELECT * FROM dialect_adv JOIN dialect_adv_ref r USING (id);

# expect contains "OK"
SELECT caliber, COUNT(*) AS cnt FROM dialect_adv GROUP BY caliber;

# expect contains "OK"
SELECT caliber FROM dialect_adv GROUP BY caliber HAVING COUNT(*) > 1;

# expect contains "OK"
SELECT id FROM dialect_adv UNION SELECT id FROM dialect_adv;

# expect contains "OK"
SELECT id FROM dialect_adv EXCEPT SELECT id FROM dialect_adv WHERE id = 'm4a1';

# expect contains "OK"
SELECT id FROM dialect_adv INTERSECT SELECT id FROM dialect_adv;

# expect contains "OK"
SELECT * FROM dialect_adv WHERE id IN (SELECT id FROM dialect_adv);

# expect contains "OK"
SELECT * FROM dialect_adv WHERE EXISTS (SELECT 1 FROM dialect_adv_ref r WHERE r.id = dialect_adv.id);

# expect contains "OK"
SELECT id, ROW_NUMBER() OVER (ORDER BY name) AS rn FROM dialect_adv;

# expect contains "OK"
SELECT id, RANK() OVER (PARTITION BY caliber ORDER BY name) FROM dialect_adv;

# expect contains "OK"
SELECT id, AVG(barrel_length) OVER (ORDER BY name ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM dialect_adv;

# expect contains "OK"
SELECT id, LAG(name) OVER (ORDER BY name) FROM dialect_adv;

# expect contains "OK"
SELECT id, LEAD(name) OVER (ORDER BY name) FROM dialect_adv;

# expect contains "OK"
SELECT id, SUM(barrel_length) OVER (ORDER BY name) FROM dialect_adv;

# expect contains "OK"
SELECT id, COUNT(*) OVER () FROM dialect_adv;

# MERGE upsert from a source table
# expect contains "OK"
MERGE INTO dialect_adv AS t USING dialect_adv_src AS s ON t.id = s.id WHEN MATCHED THEN UPDATE SET name = s.name WHEN NOT MATCHED THEN INSERT (id, name, caliber, barrel_length) VALUES (s.id, s.name, s.caliber, s.barrel_length);

# expect contains "OK"
SELECT * FROM dialect_adv;

DROP TABLE dialect_adv_src;

DROP TABLE dialect_adv_ref;

DROP TABLE dialect_adv;
