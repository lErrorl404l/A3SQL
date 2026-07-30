use super::*;
use crate::engine::value::*;

fn make_db() -> Database {
    let mut db = Database::new();
    let cols = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::String,
            primary_key: true,
            not_null: false,
            default: None,
            auto_increment: false,
        },
        Column {
            name: "val".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        },
    ];
    let table = Table::new("items".into(), cols).unwrap();
    db.create_table("items", table).unwrap();
    db
}

#[test]
fn create_and_get() {
    let db = make_db();
    assert!(db.get_table("items").is_ok());
    assert!(db.get_table("nonexistent").is_err());
}

#[test]
fn drop_table() {
    let mut db = make_db();
    db.drop_table("items").unwrap();
    assert!(!db.has_table("items"));
}

#[test]
fn duplicate_table() {
    let mut db = make_db();
    let cols = vec![Column {
        name: "x".into(),
        dtype: ColumnType::Int,
        primary_key: false,
        not_null: false,
        default: None,
        auto_increment: false,
    }];
    let t2 = Table::new("items".into(), cols).unwrap();
    assert!(db.create_table("items", t2).is_err());
}

#[test]
fn list_tables() {
    let mut db = Database::new();
    let cols = vec![Column {
        name: "x".into(),
        dtype: ColumnType::Int,
        primary_key: false,
        not_null: false,
        default: None,
        auto_increment: false,
    }];
    db.create_table("a", Table::new("a".into(), cols.clone()).unwrap())
        .unwrap();
    db.create_table("b", Table::new("b".into(), cols).unwrap()).unwrap();
    assert_eq!(db.table_names(), vec!["a", "b"]);
}

// ── View tests ─────────────────────────────────────────────────

#[test]
fn create_and_drop_view() {
    let mut db = Database::new();
    db.create_view("myview", "SELECT * FROM t").unwrap();
    assert!(db.has_view("myview"));
    assert_eq!(db.get_view("myview"), Some(&"SELECT * FROM t".to_string()));
    db.drop_view("myview").unwrap();
    assert!(!db.has_view("myview"));
}

#[test]
fn view_duplicate_name() {
    let mut db = Database::new();
    db.create_view("v", "SELECT 1").unwrap();
    assert!(db.create_view("v", "SELECT 2").is_err());
}

#[test]
fn view_table_name_conflict() {
    let mut db = Database::new();
    let cols = vec![Column {
        name: "x".into(),
        dtype: ColumnType::Int,
        primary_key: false,
        not_null: false,
        default: None,
        auto_increment: false,
    }];
    db.create_table("t", Table::new("t".into(), cols).unwrap()).unwrap();
    assert!(db.create_view("t", "SELECT 1").is_err());
}

#[test]
fn view_rollback() {
    let mut db = Database::new();
    db.create_view("v", "SELECT 1").unwrap();
    db.begin();
    db.drop_view("v").unwrap();
    assert!(!db.has_view("v"));
    db.rollback().unwrap();
    assert!(db.has_view("v"));
}

// ── Transaction tests ─────────────────────────────────────────

#[test]
fn begin_commit() {
    let mut db = make_db();
    db.begin();
    let t = db.get_table_mut("items").unwrap();
    t.insert(vec![DbValue::String("x".into()), DbValue::Int(1)]).unwrap();
    db.commit().unwrap();
    assert_eq!(db.get_table("items").unwrap().rows.len(), 1);
}

#[test]
fn begin_rollback() {
    let mut db = make_db();
    db.begin();
    let t = db.get_table_mut("items").unwrap();
    t.insert(vec![DbValue::String("x".into()), DbValue::Int(1)]).unwrap();
    db.rollback().unwrap();
    assert_eq!(db.get_table("items").unwrap().rows.len(), 0);
}

#[test]
fn nested_commit() {
    let mut db = make_db();
    db.begin();
    db.get_table_mut("items")
        .unwrap()
        .insert(vec![DbValue::String("a".into()), DbValue::Int(1)])
        .unwrap();
    db.begin();
    db.get_table_mut("items")
        .unwrap()
        .insert(vec![DbValue::String("b".into()), DbValue::Int(2)])
        .unwrap();
    db.commit().unwrap(); // commit inner
    assert_eq!(db.get_table("items").unwrap().rows.len(), 2);
    db.rollback().unwrap(); // rollback outer
    assert_eq!(db.get_table("items").unwrap().rows.len(), 0);
}

#[test]
fn savepoint_rollback() {
    let mut db = make_db();
    db.get_table_mut("items")
        .unwrap()
        .insert(vec![DbValue::String("a".into()), DbValue::Int(1)])
        .unwrap();
    db.savepoint("sp1");
    db.get_table_mut("items")
        .unwrap()
        .insert(vec![DbValue::String("b".into()), DbValue::Int(2)])
        .unwrap();
    db.rollback_to_savepoint("sp1").unwrap();
    assert_eq!(db.get_table("items").unwrap().rows.len(), 1);
}
