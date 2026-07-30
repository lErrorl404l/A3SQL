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
        },
        Column {
            name: "v".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
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
