// Edge case tests: empty tables, NULL handling, bulk operations

use super::helpers::*;

#[test]
fn empty_table_select() {
    let mut db = make_test_db();
    let result = parse_and_exec("SELECT * FROM items", &mut db).unwrap();
    assert_eq!(result, "[[\"id\",\"name\",\"value\"]]");
}

#[test]
fn empty_where_select() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items WHERE 1 = 0", &mut db).unwrap();
    assert_eq!(result, "[[\"id\",\"name\",\"value\"]]");
}

#[test]
fn null_insert() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('n', NULL, 99)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items WHERE id = 'n'", &mut db).unwrap();
    assert!(result.contains("null"));
}

#[test]
fn bulk_insert_500() {
    let mut db = Database::new();
    let cols = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "v".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let t = Table::new("bulk".into(), cols).unwrap();
    db.create_table("bulk", t).unwrap();
    for i in 0..500 {
        parse_and_exec(&format!("INSERT INTO bulk VALUES ({},{})", i, i * 2), &mut db).unwrap();
    }
    let r = parse_and_exec("SELECT COUNT(*) FROM bulk", &mut db).unwrap();
    assert!(r.contains("500"), "count: {}", r);
    let s = parse_and_exec("SELECT SUM(v) FROM bulk", &mut db).unwrap();
    assert!(s.contains("249500"), "sum: {}", s);
}

#[test]
fn string_with_semicolon() {
    let mut db = make_test_db();
    let sql = "INSERT INTO items VALUES ('sc', 'a;b', 1)";
    parse_and_exec(sql, &mut db).unwrap();
    let r = parse_and_exec("SELECT * FROM items WHERE id = 'sc'", &mut db).unwrap();
    assert!(r.contains("a;b"));
}

#[test]
fn order_empty_table() {
    let mut db = make_test_db();
    let r = parse_and_exec("SELECT * FROM items ORDER BY value", &mut db).unwrap();
    assert_eq!(r, "[[\"id\",\"name\",\"value\"]]");
}

#[test]
fn null_arithmetic() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('nx', 'null_test', NULL)", &mut db).unwrap();
    let r = parse_and_exec("SELECT * FROM items WHERE value IS NULL", &mut db).unwrap();
    assert!(r.contains("null_test"), "null: {}", r);
}

#[test]
fn fuzzy_fn_call_integration() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('fn_test', 'hello', 1)", &mut db).unwrap();
    let r = parse_and_exec("SELECT * FROM items WHERE id %% 'fn_t'", &mut db).unwrap();
    assert!(r.contains("fn_test"), "fuzzy fn: {}", r);
}
#[test]
#[cfg_attr(miri, ignore)] // SystemTime::now() — realtime clock blocked by miri's isolation
fn datetime_now_in_insert_values() {
    // Bug B regression: INSERT ... VALUES with datetime('now') must not fail
    // (mod SQL: INSERT INTO server_commands ... datetime('now'))
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INTEGER PRIMARY KEY, created_at TEXT)", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t VALUES (1, datetime('now'))", &mut db);
    assert!(r.is_ok(), "datetime('now') in VALUES: {:?}", r);
    let sel = parse_and_exec("SELECT created_at FROM t", &mut db).unwrap();
    // result shape: [["created_at"],["YYYY-MM-DD HH:MM:SS"]]
    let row = sel.split("],[").nth(1).expect("row present");
    let v = row.trim_matches(['[', ']', '"']);
    let parts: Vec<&str> = v.split(' ').collect();
    assert_eq!(parts.len(), 2, "expected 'date time', got: {}", v);
    assert_eq!(parts[0].len(), 10, "date part: {}", parts[0]);
    assert_eq!(parts[1].len(), 8, "time part: {}", parts[1]);
}

#[test]
#[cfg_attr(miri, ignore)] // SystemTime::now() — realtime clock blocked by miri's isolation
fn datetime_now_localtime_accepted() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INTEGER PRIMARY KEY, created_at TEXT)", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t VALUES (1, datetime('now','localtime'))", &mut db);
    assert!(r.is_ok(), "datetime('now','localtime') in VALUES: {:?}", r);
}

#[test]
fn datetime_bad_modifier_rejected() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INTEGER PRIMARY KEY, created_at TEXT)", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t VALUES (1, datetime('yesterday'))", &mut db);
    assert!(r.is_err(), "unsupported modifier must be rejected");
}

#[test]
fn select_from_view_materializes() {
    // Bug regression: SELECT on a created view reported "Table does not exist"
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE w (id TEXT, name TEXT, barrelLength FLOAT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO w VALUES ('a', 'A', 100.0)", &mut db).unwrap();
    let r = parse_and_exec(
        "CREATE VIEW v_short AS SELECT * FROM w WHERE barrelLength < 300",
        &mut db,
    );
    assert!(r.is_ok(), "create view: {:?}", r);
    let sel = parse_and_exec("SELECT * FROM v_short", &mut db);
    assert!(sel.is_ok(), "select from view: {:?}", sel);
}

#[test]
fn select_from_empty_view_resolves() {
    // Bug regression: SELECT on a view over an EMPTY table reported
    // "Table does not exist" because materialize_view skipped creating the
    // table when the result had no data rows (header only).
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE w (id TEXT, name TEXT)", &mut db).unwrap();
    parse_and_exec("CREATE VIEW v AS SELECT * FROM w", &mut db).unwrap();
    let sel = parse_and_exec("SELECT * FROM v", &mut db);
    assert!(sel.is_ok(), "select from empty view: {:?}", sel);
    // And still works once rows exist
    parse_and_exec("INSERT INTO w VALUES ('a', 'A')", &mut db).unwrap();
    let sel2 = parse_and_exec("SELECT * FROM v", &mut db);
    assert!(sel2.is_ok(), "select from populated view: {:?}", sel2);
    assert!(sel2.unwrap().contains("a"));
}

#[test]
fn array_columns_accept_array_literals() {
    // Bug regression: STRINGS[]/FLOATS[] columns were downgraded to plain
    // STRING/FLOAT by the preprocessor, so array values were rejected.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t_arr (s STRINGS[], f FLOATS[])", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t_arr VALUES (ARRAY['a','b'], ARRAY[1.5, 2.5])", &mut db);
    assert!(r.is_ok(), "array insert: {:?}", r);
    let sel = parse_and_exec("SELECT s FROM t_arr", &mut db).unwrap();
    assert!(sel.contains("a"), "array value present: {}", sel);
}
