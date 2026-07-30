// DML tests: CREATE TABLE, INSERT, SELECT, UPDATE, DELETE

use super::helpers::*;

#[test]
fn create_table() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, val FLOAT)", &mut db).unwrap();
    assert!(db.has_table("t"));
}

#[test]
fn insert_and_select() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items", &mut db).unwrap();
    assert!(result.contains("\"id\"") && result.contains("\"name\""));
    assert!(result.contains("alpha") && result.contains("beta"));
}

#[test]
fn select_with_where() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items WHERE value >= 20", &mut db).unwrap();
    assert!(result.contains("beta"));
    assert!(!result.contains("alpha"));
}

#[test]
fn update_rows() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    parse_and_exec("UPDATE items SET name = 'updated' WHERE id = 'a'", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items WHERE id = 'a'", &mut db).unwrap();
    assert!(result.contains("updated"));
}

#[test]
fn delete_rows() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    let result = parse_and_exec("DELETE FROM items WHERE id = 'a'", &mut db);
    eprintln!("DELETE result: {:?}", result);
    assert!(result.is_ok(), "DELETE failed: {:?}", result.err());
    assert_eq!(db.get_table("items").unwrap().rows.len(), 0);
}

#[test]
fn like_operator() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('abc123', 'test', 1)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items WHERE id LIKE '%123'", &mut db).unwrap();
    assert!(result.contains("abc123"));
}

#[test]
fn auto_increment_flag_set() {
    let mut db = Database::new();
    let _ = parse_and_exec(
        "CREATE TABLE ait (id INT PRIMARY KEY AUTO_INCREMENT, val STRING)",
        &mut db,
    );
    let table = db.get_table("ait").unwrap();
    assert!(
        table.columns[0].auto_increment,
        "id should have auto_increment=true, got false"
    );
    assert!(table.columns[0].primary_key, "id should be primary key");
}

#[test]
fn btree_index_equality_selection() {
    let mut db = make_indexed_db();
    let r = parse_and_exec("SELECT * FROM idx_test WHERE v = 10", &mut db).unwrap();
    assert!(r.contains("\"a\""), "btree index lookup: {}", r);
    assert!(!r.contains("\"b\""), "should not include b: {}", r);
}

#[test]
fn btree_index_equality_fallback() {
    let mut db = make_indexed_db();
    let r = parse_and_exec("SELECT * FROM idx_test WHERE k = 'a'", &mut db).unwrap();
    assert!(r.contains("\"a\""), "fallback lookup: {}", r);
}
