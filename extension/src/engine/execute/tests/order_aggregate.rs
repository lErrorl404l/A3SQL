// ORDER BY, LIMIT, and aggregate function tests

use super::helpers::*;

#[test]
fn order_by_desc() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items ORDER BY value DESC", &mut db).unwrap();
    let pos_20 = result.find(",beta,").unwrap_or(0);
    let pos_10 = result.find(",alpha,").unwrap_or(usize::MAX);
    assert!(
        pos_20 < pos_10,
        "beta(20) should appear before alpha(10) in DESC: {}",
        result
    );
}

#[test]
fn limit_clause() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items LIMIT 1", &mut db).unwrap();
    let count = result.matches("alpha").count() + result.matches("beta").count();
    assert_eq!(count, 1, "LIMIT 1 should return 1 row: {}", result);
}

#[test]
fn count_aggregate() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
    let result = parse_and_exec("SELECT COUNT(*) FROM items", &mut db).unwrap();
    assert!(result.contains("2"), "COUNT should be 2: {}", result);
}

#[test]
fn count_distinct() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 10)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('c', 'gamma', 20)", &mut db).unwrap();
    let result = parse_and_exec("SELECT COUNT(DISTINCT value) FROM items", &mut db).unwrap();
    assert!(result.contains("2"), "COUNT(DISTINCT value) should be 2: {}", result);
}

#[test]
fn sum_aggregate() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
    let result = parse_and_exec("SELECT SUM(value) FROM items", &mut db).unwrap();
    assert!(result.contains("30"), "SUM should be 30: {}", result);
}

#[test]
fn group_by() {
    let mut db = Database::new();
    let cols = vec![
        Column {
            name: "cat".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "val".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut table = Table::new("data".into(), cols).unwrap();
    table
        .insert(vec![DbValue::String("a".into()), DbValue::Int(10)])
        .unwrap();
    table
        .insert(vec![DbValue::String("a".into()), DbValue::Int(20)])
        .unwrap();
    table
        .insert(vec![DbValue::String("b".into()), DbValue::Int(30)])
        .unwrap();
    db.create_table("data", table).unwrap();
    let result = parse_and_exec("SELECT cat, SUM(val) FROM data GROUP BY cat", &mut db).unwrap();
    assert!(result.contains("30"), "SUM(a) = 30: {}", result);
    assert!(result.contains("30"), "SUM(b) = 30: {}", result);
}
