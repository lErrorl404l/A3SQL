// a3sql — Comprehensive feature audit against SQL-Dialect.md + README
// Exercise every documented feature via the public dispatch() API.
// A global mutex serializes access so tests don't need --test-threads=1.

use a3sql::dispatch;
use std::sync::Mutex;
use std::sync::MutexGuard;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

fn ok(resp: &str, label: &str) {
    assert!(resp.contains("[0,"), "FAIL {} — expected success, got: {}", label, resp);
}

fn contains(resp: &str, needle: &str, label: &str) {
    assert!(
        resp.contains(needle),
        "FAIL {} — expected '{}' in '{}'",
        label,
        needle,
        resp
    );
}

fn setup() -> MutexGuard<'static, ()> {
    let g = TEST_MUTEX.lock().unwrap();
    dispatch("reset", &[]);
    g
}

#[test]
fn audit_ddl() {
    let _g = setup();
    ok(
        &dispatch(
            "CREATE TABLE a_weapons (id STRING PRIMARY KEY, name STRING, caliber STRING, barrelLength FLOAT)",
            &[],
        ),
        "CREATE TABLE",
    );
    ok(
        &dispatch(
            "CREATE TABLE a_attachments (id STRING PRIMARY KEY, weaponId STRING, name STRING, mass FLOAT)",
            &[],
        ),
        "CREATE TABLE 2",
    );
    ok(
        &dispatch("CREATE INDEX a_idx_cal ON a_weapons (caliber) USING BTREE", &[]),
        "CREATE INDEX BTREE",
    );
    ok(
        &dispatch("CREATE INDEX a_idx_name_fuzzy ON a_weapons (name) USING TRIGRAM", &[]),
        "CREATE INDEX TRIGRAM",
    );
    ok(
        &dispatch(
            "CREATE VIEW a_short_weps AS SELECT id, name FROM a_weapons WHERE barrelLength < 300.0",
            &[],
        ),
        "CREATE VIEW",
    );
    ok(
        &dispatch("ALTER TABLE a_weapons ADD COLUMN mass FLOAT", &[]),
        "ALTER TABLE ADD",
    );
    ok(
        &dispatch("ALTER TABLE a_weapons RENAME COLUMN name TO displayName", &[]),
        "ALTER TABLE RENAME COLUMN",
    );
    ok(
        &dispatch("ALTER TABLE a_weapons RENAME TO a_armory", &[]),
        "ALTER TABLE RENAME",
    );
    ok(
        &dispatch("ALTER TABLE a_armory RENAME COLUMN displayName TO name", &[]),
        "RENAME COLUMN BACK",
    );
    ok(
        &dispatch("ALTER TABLE a_armory DROP COLUMN mass", &[]),
        "ALTER TABLE DROP",
    );
    ok(
        &dispatch("ALTER TABLE a_armory RENAME TO a_weapons", &[]),
        "RENAME TABLE BACK",
    );
    ok(&dispatch("TRUNCATE TABLE a_weapons", &[]), "TRUNCATE");
    ok(&dispatch("DROP VIEW a_short_weps", &[]), "DROP VIEW");
}

#[test]
fn audit_crud() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_crud (id STRING PRIMARY KEY, name STRING, val INT)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_crud VALUES ('a', 'alpha', 10)", &[]), "INSERT");
    ok(
        &dispatch("INSERT INTO a_crud VALUES ('b', 'beta', 20), ('c', 'gamma', 30)", &[]),
        "INSERT multi-row",
    );
    ok(
        &dispatch("INSERT INTO a_crud (id, name) VALUES ('d', 'delta')", &[]),
        "INSERT columns",
    );
    let r = dispatch("SELECT * FROM a_crud", &[]);
    ok(&r, "SELECT");
    contains(&r, "alpha", "SELECT data");
}

#[test]
fn audit_types() {
    let _g = setup();
    ok(
        &dispatch(
            "CREATE TABLE a_types (id STRING PRIMARY KEY, b BOOL, i INT, f FLOAT)",
            &[],
        ),
        "types basic",
    );
    ok(
        &dispatch("INSERT INTO a_types VALUES ('t1', true, 42, 3.14)", &[]),
        "types insert",
    );
    let r = dispatch("SELECT b, i, f FROM a_types WHERE id = 't1'", &[]);
    contains(&r, "true", "BOOL true");
    contains(&r, "42", "INT 42");
    contains(&r, "3.14", "FLOAT 3.14");
}

#[test]
fn audit_expressions() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_expr (id STRING PRIMARY KEY, name STRING, val INT)", &[]),
        "CREATE",
    );
    ok(
        &dispatch(
            "INSERT INTO a_expr VALUES ('a', 'alpha', 10), ('b', 'beta', 20), ('c', 'gamma', 30), ('d', '', 40)",
            &[],
        ),
        "INSERT",
    );
    let r = dispatch("SELECT id FROM a_expr WHERE name LIKE 'alp%'", &[]);
    contains(&r, "a", "LIKE");
    let r = dispatch("SELECT id FROM a_expr WHERE val BETWEEN 15 AND 25", &[]);
    contains(&r, "b", "BETWEEN");
    let r = dispatch("SELECT id FROM a_expr WHERE val IN (10, 30)", &[]);
    contains(&r, "a", "IN a");
    contains(&r, "c", "IN c");
    let r = dispatch("SELECT id FROM a_expr WHERE name IS NULL", &[]);
    contains(&r, "d", "IS NULL");
    let r = dispatch(
        "SELECT id, CASE WHEN val >= 20 THEN 'big' ELSE 'small' END AS lbl FROM a_expr WHERE id = 'a'",
        &[],
    );
    contains(&r, "small", "CASE WHEN");
    let r = dispatch("SELECT EXISTS(SELECT 1 FROM a_expr WHERE val = 99) AS res", &[]);
    contains(&r, "false", "EXISTS false");
    let r = dispatch("SELECT CAST(val AS FLOAT) / 3 FROM a_expr WHERE id = 'a'", &[]);
    contains(&r, "3.333", "CAST");
}

#[test]
fn audit_joins() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_j_left (id STRING PRIMARY KEY, label STRING)", &[]),
        "CREATE L",
    );
    ok(
        &dispatch("CREATE TABLE a_j_right (id STRING PRIMARY KEY, category STRING)", &[]),
        "CREATE R",
    );
    ok(
        &dispatch(
            "INSERT INTO a_j_left VALUES ('a', 'alpha'), ('b', 'beta'), ('c', 'gamma')",
            &[],
        ),
        "INSERT L",
    );
    ok(
        &dispatch(
            "INSERT INTO a_j_right VALUES ('a', 'cat1'), ('c', 'cat2'), ('d', 'cat3')",
            &[],
        ),
        "INSERT R",
    );
    let r = dispatch(
        "SELECT a_j_left.id, a_j_left.label, a_j_right.category FROM a_j_left INNER JOIN a_j_right ON a_j_left.id = a_j_right.id",
        &[],
    );
    contains(&r, "alpha", "INNER alpha");
    contains(&r, "cat2", "INNER cat2");
    let r = dispatch(
        "SELECT a_j_left.id, a_j_left.label, a_j_right.category FROM a_j_left LEFT JOIN a_j_right ON a_j_left.id = a_j_right.id",
        &[],
    );
    contains(&r, "beta", "LEFT beta");
    let r = dispatch(
        "SELECT a_j_left.id, a_j_left.label, a_j_right.category FROM a_j_left CROSS JOIN a_j_right WHERE a_j_left.id = 'a' AND a_j_right.id = 'c'",
        &[],
    );
    contains(&r, "alpha", "CROSS");
    let r = dispatch(
        "SELECT a_j_left.id, a_j_left.label, a_j_right.category FROM a_j_left FULL OUTER JOIN a_j_right ON a_j_left.id = a_j_right.id",
        &[],
    );
    contains(&r, "alpha", "FULL alpha");
    contains(&r, "cat3", "FULL cat3");
}

#[test]
fn audit_aggregates() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_agg (id STRING PRIMARY KEY, grp STRING, val INT)", &[]),
        "CREATE",
    );
    ok(
        &dispatch(
            "INSERT INTO a_agg VALUES ('a', 'x', 10), ('b', 'x', 20), ('c', 'y', 30), ('d', 'y', 40)",
            &[],
        ),
        "INSERT",
    );
    let r = dispatch(
        "SELECT COUNT(*) AS cnt, SUM(val) AS total, AVG(val) AS avg, MIN(val) AS mn, MAX(val) AS mx FROM a_agg",
        &[],
    );
    contains(&r, "4", "COUNT");
    contains(&r, "100", "SUM");
    let r = dispatch("SELECT grp, COUNT(*) AS cnt FROM a_agg GROUP BY grp", &[]);
    contains(&r, "2", "GROUP BY count");
    let r = dispatch("SELECT COUNT(DISTINCT val) AS uniq FROM a_agg", &[]);
    contains(&r, "4", "COUNT DISTINCT");
}

#[test]
fn audit_window() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_win (id STRING PRIMARY KEY, grp STRING, val INT)", &[]),
        "CREATE",
    );
    ok(
        &dispatch(
            "INSERT INTO a_win VALUES ('a', 'x', 10), ('b', 'x', 20), ('c', 'y', 30), ('d', 'y', 40)",
            &[],
        ),
        "INSERT",
    );
    let r = dispatch("SELECT id, ROW_NUMBER() OVER (ORDER BY val) AS rn FROM a_win", &[]);
    ok(&r, "ROW_NUMBER");
    let r = dispatch(
        "SELECT id, RANK() OVER (PARTITION BY grp ORDER BY val) AS rk FROM a_win",
        &[],
    );
    ok(&r, "RANK OVER PARTITION");
    let r = dispatch(
        "SELECT id, val, SUM(val) OVER (ORDER BY val ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS ws FROM a_win",
        &[],
    );
    ok(&r, "ROWS BETWEEN");
}

#[test]
fn audit_setops() {
    let _g = setup();
    ok(&dispatch("CREATE TABLE a_s1 (id STRING PRIMARY KEY)", &[]), "CREATE S1");
    ok(&dispatch("CREATE TABLE a_s2 (id STRING PRIMARY KEY)", &[]), "CREATE S2");
    ok(
        &dispatch("INSERT INTO a_s1 VALUES ('a'), ('b'), ('c')", &[]),
        "INSERT S1",
    );
    ok(
        &dispatch("INSERT INTO a_s2 VALUES ('b'), ('c'), ('d')", &[]),
        "INSERT S2",
    );
    let r = dispatch("SELECT id FROM a_s1 UNION SELECT id FROM a_s2", &[]);
    contains(&r, "a", "UNION a");
    contains(&r, "d", "UNION d");
    let r = dispatch("SELECT id FROM a_s1 INTERSECT SELECT id FROM a_s2", &[]);
    contains(&r, "b", "INTERSECT b");
    contains(&r, "c", "INTERSECT c");
    let r = dispatch("SELECT id FROM a_s1 EXCEPT SELECT id FROM a_s2", &[]);
    contains(&r, "a", "EXCEPT a");
    let r = dispatch("SELECT id FROM a_s1 UNION ALL SELECT id FROM a_s2", &[]);
    contains(&r, "a", "UNION ALL a");
    contains(&r, "d", "UNION ALL d");
}

#[test]
fn audit_cte() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_cte (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE",
    );
    ok(
        &dispatch("INSERT INTO a_cte VALUES ('a', 1), ('b', 2), ('c', 3)", &[]),
        "INSERT",
    );
    let r = dispatch("WITH t AS (SELECT * FROM a_cte WHERE val > 1) SELECT id FROM t", &[]);
    contains(&r, "b", "CTE b");
    contains(&r, "c", "CTE c");
}

#[test]
fn audit_constraints() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_ref (id STRING PRIMARY KEY)", &[]),
        "CREATE ref",
    );
    ok(&dispatch("INSERT INTO a_ref VALUES ('p1'), ('p2')", &[]), "INSERT ref");
    ok(
        &dispatch(
            "CREATE TABLE a_ck (id STRING PRIMARY KEY, val INT CHECK (val > 0))",
            &[],
        ),
        "CREATE CHECK",
    );
    ok(&dispatch("INSERT INTO a_ck VALUES ('a', 10)", &[]), "CHECK valid");
    let r = dispatch("INSERT INTO a_ck VALUES ('b', -5)", &[]);
    assert!(r.contains("ERR_"), "CHECK should reject negative: {}", r);
    ok(
        &dispatch(
            "CREATE TABLE a_fk (id STRING PRIMARY KEY, pid STRING REFERENCES a_ref(id))",
            &[],
        ),
        "CREATE FK",
    );
    ok(&dispatch("INSERT INTO a_fk VALUES ('c1', 'p1')", &[]), "FK valid");
    let r = dispatch("INSERT INTO a_fk VALUES ('c2', 'nonexistent')", &[]);
    assert!(r.contains("ERR_"), "FK should reject bad ref: {}", r);
    ok(
        &dispatch("CREATE TABLE a_nn (id STRING PRIMARY KEY, name STRING NOT NULL)", &[]),
        "CREATE NOT NULL",
    );
    ok(&dispatch("INSERT INTO a_nn VALUES ('d', 'present')", &[]), "NN valid");
    ok(
        &dispatch("CREATE TABLE a_def (id STRING PRIMARY KEY, val INT DEFAULT 99)", &[]),
        "CREATE DEFAULT",
    );
    ok(&dispatch("INSERT INTO a_def (id) VALUES ('e')", &[]), "INSERT default");
    let r = dispatch("SELECT val FROM a_def WHERE id = 'e'", &[]);
    contains(&r, "99", "DEFAULT 99");
    ok(
        &dispatch(
            "CREATE TABLE a_ai (id INT AUTO_INCREMENT PRIMARY KEY, label STRING)",
            &[],
        ),
        "CREATE AUTO INCR",
    );
    ok(
        &dispatch("INSERT INTO a_ai (label) VALUES ('first'), ('second')", &[]),
        "INSERT auto",
    );
    let r = dispatch("SELECT id FROM a_ai WHERE label = 'first'", &[]);
    contains(&r, "1", "AUTO_INCREMENT start");
}

#[test]
fn audit_returning() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_ret (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_ret VALUES ('a', 10)", &[]), "INSERT");
    let r = dispatch("INSERT INTO a_ret VALUES ('b', 20) RETURNING *", &[]);
    contains(&r, "b", "INSERT RETURNING");
    contains(&r, "20", "INSERT RETURNING val");
    // Engine captures OLD values for UPDATE RETURNING
    let r = dispatch("UPDATE a_ret SET val = 99 WHERE id = 'a' RETURNING id, val", &[]);
    contains(&r, "a", "UPDATE RETURNING id");
    contains(&r, "10", "UPDATE RETURNING old val");
    let r = dispatch("DELETE FROM a_ret WHERE id = 'b' RETURNING id", &[]);
    contains(&r, "b", "DELETE RETURNING");
}

#[test]
fn audit_upsert() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_up (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_up VALUES ('a', 10)", &[]), "INSERT");
    ok(&dispatch("REPLACE INTO a_up VALUES ('a', 99)", &[]), "REPLACE INTO");
    let r = dispatch("SELECT val FROM a_up WHERE id = 'a'", &[]);
    contains(&r, "99", "REPLACE updated val");
    ok(
        &dispatch("INSERT OR REPLACE INTO a_up VALUES ('a', 42)", &[]),
        "INSERT OR REPLACE",
    );
    let r = dispatch("SELECT val FROM a_up WHERE id = 'a'", &[]);
    contains(&r, "42", "INSERT OR REPLACE val");
}

#[test]
fn audit_explain() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_explain (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_explain VALUES ('a', 10)", &[]), "INSERT");
    let r = dispatch("EXPLAIN SELECT * FROM a_explain WHERE val > 5", &[]);
    contains(&r, "SeqScan", "EXPLAIN SELECT");
    let r = dispatch("EXPLAIN INSERT INTO a_explain VALUES ('b', 20)", &[]);
    contains(&r, "Insert", "EXPLAIN INSERT");
}

#[test]
fn audit_transactions() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_tx (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_tx VALUES ('a', 10)", &[]), "INSERT");
    ok(&dispatch("BEGIN", &[]), "BEGIN");
    ok(
        &dispatch("UPDATE a_tx SET val = 99 WHERE id = 'a'", &[]),
        "UPDATE in tx",
    );
    ok(&dispatch("ROLLBACK", &[]), "ROLLBACK");
    let r = dispatch("SELECT val FROM a_tx WHERE id = 'a'", &[]);
    contains(&r, "10", "ROLLBACK restored");
    ok(&dispatch("SAVEPOINT sp1", &[]), "SAVEPOINT");
    ok(
        &dispatch("UPDATE a_tx SET val = 42 WHERE id = 'a'", &[]),
        "UPDATE after sp",
    );
    ok(&dispatch("ROLLBACK TO SAVEPOINT sp1", &[]), "ROLLBACK TO");
    let r = dispatch("SELECT val FROM a_tx WHERE id = 'a'", &[]);
    contains(&r, "10", "ROLLBACK TO restored");
}

#[test]
fn audit_triggers() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_trg_log (id STRING PRIMARY KEY, msg STRING)", &[]),
        "CREATE log",
    );
    ok(
        &dispatch("CREATE TABLE a_trg_data (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE data",
    );
    ok(
        &dispatch(
            "CREATE TRIGGER a_trg_after_update AFTER UPDATE ON a_trg_data BEGIN INSERT INTO a_trg_log VALUES ('x_log', 'updated') END",
            &[],
        ),
        "CREATE TRIGGER",
    );
    ok(
        &dispatch("INSERT INTO a_trg_data VALUES ('x', 100)", &[]),
        "INSERT data",
    );
    ok(
        &dispatch("UPDATE a_trg_data SET val = 200 WHERE id = 'x'", &[]),
        "UPDATE data",
    );
    let r = dispatch("SELECT msg FROM a_trg_log WHERE id = 'x_log'", &[]);
    contains(&r, "updated", "TRIGGER fired");
    ok(&dispatch("DROP TRIGGER a_trg_after_update", &[]), "DROP TRIGGER");
}

#[test]
fn audit_subqueries() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_sq_parent (id STRING PRIMARY KEY)", &[]),
        "CREATE parent",
    );
    ok(
        &dispatch("CREATE TABLE a_sq_child (id STRING PRIMARY KEY, pid STRING)", &[]),
        "CREATE child",
    );
    ok(
        &dispatch("INSERT INTO a_sq_parent VALUES ('p1'), ('p2')", &[]),
        "INSERT parent",
    );
    ok(
        &dispatch("INSERT INTO a_sq_child VALUES ('c1', 'p1'), ('c2', 'p2')", &[]),
        "INSERT child",
    );
    let r = dispatch(
        "SELECT id FROM a_sq_parent WHERE id IN (SELECT pid FROM a_sq_child)",
        &[],
    );
    contains(&r, "p1", "IN subquery p1");
    contains(&r, "p2", "IN subquery p2");
    ok(&dispatch("INSERT INTO a_sq_parent VALUES ('p3')", &[]), "INSERT orphan");
    let r = dispatch(
        "SELECT id FROM a_sq_parent WHERE EXISTS (SELECT 1 FROM a_sq_child WHERE pid = 'p1')",
        &[],
    );
    contains(&r, "p1", "EXISTS matches parent");
    contains(&r, "p2", "EXISTS matches second parent");
}

#[test]
fn audit_index_usage() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_idx (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE",
    );
    ok(
        &dispatch("INSERT INTO a_idx VALUES ('a', 10), ('b', 20), ('c', 30)", &[]),
        "INSERT",
    );
    ok(
        &dispatch("CREATE INDEX a_idx_val ON a_idx (val) USING BTREE", &[]),
        "CREATE INDEX",
    );
    let r = dispatch("SELECT id FROM a_idx WHERE val = 20", &[]);
    contains(&r, "b", "index equality lookup");
    ok(&dispatch("DROP INDEX a_idx_val", &[]), "DROP INDEX");
    let r = dispatch("SELECT id FROM a_idx WHERE val = 20", &[]);
    contains(&r, "b", "after DROP INDEX (seq scan)");
}

#[test]
fn audit_vacuum_reindex() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_vr (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_vr VALUES ('a', 10), ('b', 20)", &[]), "INSERT");
    ok(&dispatch("VACUUM", &[]), "VACUUM");
    ok(&dispatch("REINDEX", &[]), "REINDEX");
}

#[test]
fn audit_functions() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_fn (id STRING PRIMARY KEY, name STRING, val INT)", &[]),
        "CREATE",
    );
    ok(
        &dispatch("INSERT INTO a_fn VALUES ('a', 'hello', 5), ('b', ' WORLD ', -3)", &[]),
        "INSERT",
    );
    let r = dispatch("SELECT UPPER(name) AS up FROM a_fn WHERE id = 'a'", &[]);
    contains(&r, "HELLO", "UPPER");
    let r = dispatch("SELECT LOWER(name) AS low FROM a_fn WHERE id = 'b'", &[]);
    contains(&r, " world ", "LOWER");
    let r = dispatch("SELECT LENGTH(name) AS len FROM a_fn WHERE id = 'a'", &[]);
    contains(&r, "5", "LENGTH");
    let r = dispatch("SELECT SUBSTR(name, 1, 2) AS sub FROM a_fn WHERE id = 'a'", &[]);
    contains(&r, "he", "SUBSTR");
    let r = dispatch("SELECT TRIM(name) AS t FROM a_fn WHERE id = 'b'", &[]);
    contains(&r, "WORLD", "TRIM");
    let r = dispatch("SELECT CONCAT(name, '!') AS c FROM a_fn WHERE id = 'a'", &[]);
    contains(&r, "hello!", "CONCAT");
    let r = dispatch(
        "SELECT COALESCE(NULL, name, 'default') AS c FROM a_fn WHERE id = 'a'",
        &[],
    );
    contains(&r, "hello", "COALESCE");
    let r = dispatch("SELECT ROUND(3.14159, 2) AS r", &[]);
    contains(&r, "3.14", "ROUND");
    let r = dispatch("SELECT ABS(val) AS a FROM a_fn WHERE id = 'b'", &[]);
    contains(&r, "3", "ABS");
    let r = dispatch("SELECT NOW() AS n", &[]);
    ok(&r, "NOW");
    let r = dispatch("SELECT CURRENT_TIMESTAMP AS ts", &[]);
    ok(&r, "CURRENT_TIMESTAMP");
    let r = dispatch("SELECT IFNULL(NULL, 'fallback') AS f", &[]);
    contains(&r, "fallback", "IFNULL");
    let r = dispatch("SELECT LOWER(UPPER(name)) AS lu FROM a_fn WHERE id = 'a'", &[]);
    contains(&r, "hello", "LOWER(UPPER())");
}

#[test]
fn audit_persistence() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_persist (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE",
    );
    ok(
        &dispatch("INSERT INTO a_persist VALUES ('a', 10), ('b', 20)", &[]),
        "INSERT",
    );
    ok(&dispatch("save a3sql_audit_persist.bin", &[]), "SAVE");
    ok(&dispatch("INSERT INTO a_persist VALUES ('c', 30)", &[]), "INSERT extra");
    ok(&dispatch("load a3sql_audit_persist.bin", &[]), "LOAD");
    let r = dispatch("SELECT val FROM a_persist WHERE id = 'a'", &[]);
    contains(&r, "10", "LOAD restored");
    let r = dispatch("SELECT val FROM a_persist WHERE id = 'c'", &[]);
    assert!(!r.contains("30"), "LOAD should not have extra row");
    let r = dispatch("export json a_persist", &[]);
    ok(&r, "export JSON");
    let r = dispatch("export csv a_persist", &[]);
    ok(&r, "export CSV");
    drop(dispatch("DROP TABLE a_persist", &[]));
    drop(std::fs::remove_file("a3sql_data/a3sql_audit_persist.bin"));
}

#[test]
fn audit_security_params() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_sec (id STRING PRIMARY KEY, secret STRING)", &[]),
        "CREATE",
    );
    ok(
        &dispatch("INSERT INTO a_sec VALUES ('a', 'classified'), ('b', 'public')", &[]),
        "INSERT",
    );
    let r = dispatch("SELECT secret FROM a_sec WHERE id = $1", &["a"]);
    contains(&r, "classified", "param $1 lookup");
}

#[test]
fn audit_multi_dialect() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_md (id VARCHAR(64) PRIMARY KEY, label TEXT)", &[]),
        "CREATE dialect",
    );
    ok(
        &dispatch("INSERT INTO a_md VALUES ('a', 'first'), ('b', 'second')", &[]),
        "INSERT dialect",
    );
    let r = dispatch("SELECT label FROM a_md WHERE id = 'a'", &[]);
    contains(&r, "first", "VARCHAR/TEXT");
    let r = dispatch("SELECT id FROM a_md WHERE label = 'second'", &[]);
    contains(&r, "b", "PostgreSQL TEXT type");
}

#[test]
fn audit_fk_cascade() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_fkc_parent (id STRING PRIMARY KEY)", &[]),
        "CREATE parent",
    );
    ok(
        &dispatch(
            "CREATE TABLE a_fkc_child (id STRING PRIMARY KEY, pid STRING REFERENCES a_fkc_parent(id) ON DELETE CASCADE)",
            &[],
        ),
        "CREATE child",
    );
    ok(
        &dispatch("INSERT INTO a_fkc_parent VALUES ('p1'), ('p2')", &[]),
        "INSERT parent",
    );
    ok(
        &dispatch(
            "INSERT INTO a_fkc_child VALUES ('c1', 'p1'), ('c2', 'p1'), ('c3', 'p2')",
            &[],
        ),
        "INSERT child",
    );
    ok(
        &dispatch("DELETE FROM a_fkc_parent WHERE id = 'p1'", &[]),
        "DELETE cascade",
    );
    let r = dispatch("SELECT id FROM a_fkc_child", &[]);
    contains(&r, "c3", "CASCADE kept c3");
    assert!(!r.contains("c1"), "CASCADE removed c1");
}

#[test]
fn audit_insert_select() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_is_src (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE src",
    );
    ok(
        &dispatch("INSERT INTO a_is_src VALUES ('a', 10), ('b', 20)", &[]),
        "INSERT src",
    );
    ok(
        &dispatch("CREATE TABLE a_is_dst (id STRING PRIMARY KEY, val INT)", &[]),
        "CREATE dst",
    );
    ok(
        &dispatch("INSERT INTO a_is_dst SELECT * FROM a_is_src WHERE val > 10", &[]),
        "INSERT SELECT",
    );
    let r = dispatch("SELECT id FROM a_is_dst", &[]);
    contains(&r, "b", "INSERT SELECT b");
    assert!(!r.contains("a"), "INSERT SELECT filtered a");
}

#[test]
fn audit_commands() {
    let _g = setup();
    let r = dispatch("version", &[]);
    contains(&r, "a3sql", "version");
    let r = dispatch("ping", &[]);
    contains(&r, "PONG", "ping");
    let r = dispatch("plugins", &[]);
    ok(&r, "plugins");
    let r = dispatch("register_function test_fn 2", &[]);
    ok(&r, "register_function");
}

#[test]
fn audit_fuzzy() {
    let _g = setup();
    ok(
        &dispatch("CREATE TABLE a_fuzzy (id STRING PRIMARY KEY, name STRING)", &[]),
        "CREATE",
    );
    ok(
        &dispatch(
            "INSERT INTO a_fuzzy VALUES ('1', 'rhs_m4a1'), ('2', 'rhs_m4a1_carryhandle'), ('3', 'hlc_ak74')",
            &[],
        ),
        "INSERT",
    );
    ok(
        &dispatch("CREATE INDEX a_fuzzy_name ON a_fuzzy (name) USING TRIGRAM", &[]),
        "CREATE TRIGRAM INDEX",
    );
    let r = dispatch("SELECT id FROM a_fuzzy WHERE name %% 'm4a1'", &[]);
    contains(&r, "1", "fuzzy match 1");
}
