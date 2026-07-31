// CHECK constraint tests

use super::helpers::*;

#[test]
fn unique_column_coexists_with_primary_key() {
    // Bug A regression: UNIQUE must not be treated as a second primary key
    // (mod SQL: CREATE TABLE patch_presets (id INTEGER PRIMARY KEY,
    //  name TEXT UNIQUE NOT NULL, ...))
    let mut db = Database::new();
    let r = parse_and_exec(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL)",
        &mut db,
    );
    assert!(r.is_ok(), "UNIQUE + PK create: {:?}", r);
    assert!(db.has_table("t"));
}

#[test]
fn unique_column_rejects_duplicate_insert() {
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT UNIQUE NOT NULL)",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 'alpha')", &mut db).unwrap();
    // same name, different PK -> UNIQUE violation
    let r = parse_and_exec("INSERT INTO t VALUES (2, 'alpha')", &mut db);
    assert!(r.is_err(), "duplicate UNIQUE value must be rejected");
    if let Err(e) = r {
        assert!(e.contains("Duplicate"), "msg: {}", e);
    }
    // different name, same PK -> PK violation
    let r = parse_and_exec("INSERT INTO t VALUES (1, 'beta')", &mut db);
    assert!(r.is_err(), "duplicate PK value must be rejected");
    // and the table is still consistent afterwards
    let ok = parse_and_exec("INSERT INTO t VALUES (2, 'beta')", &mut db);
    assert!(ok.is_ok(), "fresh row: {:?}", ok);
}

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
