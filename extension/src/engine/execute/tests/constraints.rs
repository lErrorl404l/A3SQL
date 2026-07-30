// CHECK constraint tests

use super::helpers::*;

#[test]
fn check_table_level_constraint_create() {
    let mut db = Database::new();
    let r = parse_and_exec(
        "CREATE TABLE t (id STRING PRIMARY KEY, val INT, CHECK (val > 0))",
        &mut db,
    );
    assert!(r.is_ok(), "create with CHECK: {:?}", r);
    assert!(db.has_table("t"));
    let t = db.get_table("t").unwrap();
    assert_eq!(t.check_constraints.len(), 1, "should have 1 CHECK constraint");
}

#[test]
fn check_column_level_constraint() {
    let mut db = Database::new();
    let r = parse_and_exec(
        "CREATE TABLE t (id STRING PRIMARY KEY, val INT CHECK (val > 0))",
        &mut db,
    );
    assert!(r.is_ok(), "col-level CHECK: {:?}", r);
}

#[test]
fn check_constraint_enforced_on_insert() {
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (id STRING PRIMARY KEY, val INT CHECK (val > 0))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES ('a', 10)", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t VALUES ('b', -5)", &mut db);
    assert!(r.is_err(), "should reject negative val");
    if let Err(e) = r {
        assert!(e.contains("CHECK"), "msg: {}", e);
    }
}

#[test]
fn check_constraint_enforced_on_update() {
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (id STRING PRIMARY KEY, val INT CHECK (val > 0))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES ('a', 10)", &mut db).unwrap();
    parse_and_exec("UPDATE t SET val = 20 WHERE id = 'a'", &mut db).unwrap();
    let r = parse_and_exec("UPDATE t SET val = -1 WHERE id = 'a'", &mut db);
    assert!(r.is_err(), "should reject UPDATE with CHECK violation");
}
