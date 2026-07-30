// Shared test helpers and re-exports

use super::super::*;
pub(crate) use crate::engine::database::Database;
pub(crate) use crate::engine::table::Table;
pub(crate) use crate::engine::value::*;

pub(crate) fn make_test_db() -> Database {
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
            name: "name".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        },
        Column {
            name: "value".into(),
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

pub(crate) fn make_indexed_db() -> Database {
    let mut db = Database::new();
    let cols = vec![
        Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: true,
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
    let t = Table::new("idx_test".into(), cols).unwrap();
    db.create_table("idx_test", t).unwrap();
    parse_and_exec("INSERT INTO idx_test VALUES ('a', 10)", &mut db).unwrap();
    parse_and_exec("INSERT INTO idx_test VALUES ('b', 20)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX btree_v ON idx_test (v) USING BTREE", &mut db).unwrap();
    parse_and_exec("CREATE INDEX trigram_k ON idx_test (k) USING TRIGRAM", &mut db).unwrap();
    db
}

pub(crate) fn parse_and_exec(sql: &str, db: &mut Database) -> Result<String, String> {
    let stmts = crate::parser::parse_sql(sql).map_err(|e| format!("{}", e))?;
    let mut result = String::new();
    for stmt in &stmts {
        result = execute(stmt, db)?;
    }
    Ok(result)
}
