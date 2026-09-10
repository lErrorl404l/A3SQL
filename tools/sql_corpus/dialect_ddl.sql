# ── Corpus: DIALECT — DDL ─────────────────────────────────────────────
# Schema definition language: CREATE / ALTER / DROP / INDEX / VIEW /
# TRIGGER / constraints. One statement per line, each ends with ';'.
# Self-contained: creates and drops its own schema.

CREATE TABLE IF NOT EXISTS dialect_ddl (id TEXT PRIMARY KEY, name TEXT, caliber TEXT, barrel_length FLOAT);

CREATE INDEX IF NOT EXISTS idx_ddl_caliber ON dialect_ddl (caliber) USING BTREE;

CREATE INDEX IF NOT EXISTS idx_ddl_name ON dialect_ddl (name) USING TRIGRAM;

ALTER TABLE dialect_ddl ADD COLUMN mass FLOAT;

ALTER TABLE dialect_ddl RENAME COLUMN name TO display_name;

ALTER TABLE dialect_ddl DROP COLUMN mass;

CREATE VIEW IF NOT EXISTS v_ddl_short AS SELECT * FROM dialect_ddl WHERE barrel_length < 300;

DROP VIEW v_ddl_short;

CREATE TRIGGER tr_ddl AFTER INSERT ON dialect_ddl BEGIN SELECT 1; END;

DROP TRIGGER tr_ddl;

CREATE TABLE IF NOT EXISTS dialect_checks (item TEXT PRIMARY KEY, price INT CHECK (price > 0));

# CHECK constraint rejects a non-positive price
# expect error
INSERT INTO dialect_checks VALUES ('bad', -5);

CREATE TABLE IF NOT EXISTS dialect_parent (id TEXT PRIMARY KEY, item TEXT REFERENCES dialect_checks(item));

# Foreign key rejects a missing parent row
# expect error
INSERT INTO dialect_parent VALUES ('p1', 'nonexistent');

CREATE TABLE IF NOT EXISTS dialect_child (id TEXT PRIMARY KEY, parent_id TEXT REFERENCES dialect_parent(id) ON DELETE CASCADE, product TEXT);

CREATE TABLE IF NOT EXISTS dialect_seq (id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT);

CREATE TABLE IF NOT EXISTS dialect_drop (x INT);

DROP TABLE dialect_drop;

ALTER TABLE dialect_ddl RENAME TO dialect_ddl_renamed;

ALTER TABLE dialect_ddl_renamed RENAME TO dialect_ddl;

TRUNCATE TABLE dialect_ddl;

VACUUM dialect_ddl;

REINDEX dialect_ddl;

DROP TABLE dialect_child;

DROP TABLE dialect_parent;

DROP TABLE dialect_checks;

DROP TABLE dialect_seq;

DROP TABLE dialect_ddl;
