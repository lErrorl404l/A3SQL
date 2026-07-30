// EXPLAIN statement tests

use super::helpers::*;

#[test]
fn explain_select() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN SELECT * FROM items", &mut db).unwrap();
    assert!(r.contains("SeqScan"), "explain select: {}", r);
    assert!(r.contains("items"), "table name: {}", r);
}

#[test]
fn explain_insert() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN INSERT INTO items VALUES ('a', 'b', 1)", &mut db).unwrap();
    assert!(r.contains("Insert"), "explain insert: {}", r);
    assert!(r.contains("items"), "table name: {}", r);
}

#[test]
fn explain_create_table() {
    let mut db = Database::new();
    let r = parse_and_exec("EXPLAIN CREATE TABLE et (id STRING PRIMARY KEY)", &mut db).unwrap();
    assert!(r.contains("CreateTable"), "explain create: {}", r);
    assert!(r.contains("et"), "table name: {}", r);
}

#[test]
fn explain_update() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN UPDATE items SET name = 'x' WHERE id = 'a'", &mut db).unwrap();
    assert!(r.contains("Update"), "explain update: {}", r);
}

#[test]
fn explain_delete() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN DELETE FROM items WHERE id = 'a'", &mut db).unwrap();
    assert!(r.contains("Delete"), "explain delete: {}", r);
}

#[test]
fn explain_with_where() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN SELECT * FROM items WHERE name = 'test'", &mut db).unwrap();
    assert!(r.contains("Filter"), "explain filter: {}", r);
    assert!(r.contains("SeqScan"), "explain scan: {}", r);
}

#[test]
fn explain_with_order_limit() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN SELECT * FROM items ORDER BY name LIMIT 5", &mut db).unwrap();
    assert!(r.contains("OrderBy"), "explain order: {}", r);
    assert!(r.contains("Limit"), "explain limit: {}", r);
}

#[test]
fn explain_analyze_rejected() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN ANALYZE SELECT * FROM items", &mut db);
    assert!(r.is_err(), "ANALYZE should be rejected");
    if let Err(e) = r {
        assert!(e.contains("ANALYZE"), "err msg: {}", e);
    }
}

#[test]
fn explain_show_tables() {
    let mut db = Database::new();
    let r = parse_and_exec("EXPLAIN SHOW TABLES", &mut db).unwrap();
    assert!(r.contains("ShowTables"), "explain show tables: {}", r);
}

#[test]
fn explain_transaction() {
    let mut db = Database::new();
    let r = parse_and_exec("EXPLAIN BEGIN", &mut db).unwrap();
    assert!(r.contains("StartTransaction"), "explain begin: {}", r);
    let r = parse_and_exec("EXPLAIN COMMIT", &mut db).unwrap();
    assert!(r.contains("Commit"), "explain commit: {}", r);
    let r = parse_and_exec("EXPLAIN ROLLBACK", &mut db).unwrap();
    assert!(r.contains("Rollback"), "explain rollback: {}", r);
}

#[test]
fn explain_create_index() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN CREATE INDEX my_idx ON items (name)", &mut db).unwrap();
    assert!(r.contains("CreateIndex"), "explain create index: {}", r);
}

#[test]
fn explain_with_indexes() {
    let mut db = make_indexed_db();
    let r = parse_and_exec("EXPLAIN SELECT * FROM idx_test WHERE v = 10", &mut db).unwrap();
    assert!(r.contains("indexes"), "should show indexes: {}", r);
    assert!(r.contains("btree_v"), "should list btree_v: {}", r);
}

#[test]
fn explain_alter_table() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN ALTER TABLE items ADD COLUMN extra INT", &mut db).unwrap();
    assert!(r.contains("AlterTable"), "explain alter: {}", r);
}

#[test]
fn explain_truncate() {
    let mut db = make_test_db();
    let r = parse_and_exec("EXPLAIN TRUNCATE items", &mut db).unwrap();
    assert!(r.contains("Truncate"), "explain truncate: {}", r);
}
