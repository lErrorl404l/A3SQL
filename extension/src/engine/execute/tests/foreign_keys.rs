// Foreign key constraint and PK set maintenance tests

use super::helpers::*;

#[test]
fn fk_update_local_column_validated() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p2')", &mut db).unwrap();
    parse_and_exec(
        "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
    parse_and_exec("UPDATE child SET pid = 'p2' WHERE id = 'c1'", &mut db).unwrap();
    let r = parse_and_exec("UPDATE child SET pid = 'nonexistent' WHERE id = 'c1'", &mut db);
    assert!(r.is_err(), "should reject FK update to nonexistent ref");
    if let Err(e) = r {
        assert!(e.contains("FOREIGN KEY"), "msg: {}", e);
    }
}

#[test]
fn fk_update_referenced_pk_restrict() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
    parse_and_exec(
        "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
    let r = parse_and_exec("UPDATE parent SET id = 'p2' WHERE id = 'p1'", &mut db);
    assert!(r.is_err(), "should reject PK update with FK ref");
    if let Err(e) = r {
        assert!(e.contains("FOREIGN KEY"), "msg: {}", e);
    }
}

#[test]
fn fk_update_referenced_pk_allowed_when_no_refs() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p2')", &mut db).unwrap();
    parse_and_exec(
        "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
    let r = parse_and_exec("UPDATE parent SET id = 'p3' WHERE id = 'p2'", &mut db);
    assert!(r.is_ok(), "should allow PK update with no FK refs: {:?}", r);
}

#[test]
fn fk_delete_restrict() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
    parse_and_exec(
        "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
    let r = parse_and_exec("DELETE FROM parent WHERE id = 'p1'", &mut db);
    assert!(r.is_err(), "should reject DELETE with FK ref");
    if let Err(e) = r {
        assert!(e.contains("FOREIGN KEY"), "msg: {}", e);
    }
}

#[test]
fn fk_delete_allowed_when_no_references() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p2')", &mut db).unwrap();
    parse_and_exec(
        "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
    let r = parse_and_exec("DELETE FROM parent WHERE id = 'p2'", &mut db);
    assert!(r.is_ok(), "should allow DELETE with no FK refs: {:?}", r);
}

#[test]
fn pk_update_works_and_maintains_pk_set() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, val INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('a', 1)", &mut db).unwrap();
    let t = db.get_table("t").unwrap();
    assert!(t.pk_set.contains("'a'"), "PK set should have 'a', got: {:?}", t.pk_set);
    parse_and_exec("UPDATE t SET id = 'b' WHERE id = 'a'", &mut db).unwrap();
    let t = db.get_table("t").unwrap();
    assert!(!t.pk_set.contains("'a'"), "PK set should no longer have 'a'");
    assert!(t.pk_set.contains("'b'"), "PK set should have 'b'");
    parse_and_exec("INSERT INTO t VALUES ('a', 2)", &mut db).unwrap();
}

#[test]
fn fk_delete_cascade() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p2')", &mut db).unwrap();
    parse_and_exec(
        "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id) ON DELETE CASCADE)",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c2', 'p1')", &mut db).unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c3', 'p2')", &mut db).unwrap();
    assert_eq!(db.get_table("child").unwrap().row_count(), 3);
    parse_and_exec("DELETE FROM parent WHERE id = 'p1'", &mut db).unwrap();
    assert_eq!(db.get_table("child").unwrap().row_count(), 1, "c3 should remain");
    let child_rows = &db.get_table("child").unwrap().rows;
    assert_eq!(child_rows[0][0].to_string(), "'c3'", "only c3 remains");
}

#[test]
fn fk_delete_set_null() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
    parse_and_exec(
        "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id) ON DELETE SET NULL)",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
    parse_and_exec("DELETE FROM parent WHERE id = 'p1'", &mut db).unwrap();
    let child = db.get_table("child").unwrap();
    assert_eq!(child.row_count(), 1, "child row should remain");
    assert_eq!(child.rows[0][1], DbValue::Null, "pid should be NULL");
}

#[test]
fn fk_update_cascade() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO parent VALUES ('old_pk')", &mut db).unwrap();
    parse_and_exec(
        "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id) ON UPDATE CASCADE)",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO child VALUES ('c1', 'old_pk')", &mut db).unwrap();
    parse_and_exec("UPDATE parent SET id = 'new_pk' WHERE id = 'old_pk'", &mut db).unwrap();
    let child = db.get_table("child").unwrap();
    assert_eq!(child.rows[0][1].to_string(), "'new_pk'", "child FK should be updated");
}
