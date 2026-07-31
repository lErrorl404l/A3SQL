// a3sql — Test helpers: fresh Database instances for each test.
//
// Use `use crate::engine::test::*;` in tests to get isolated state
// instead of the shared global `DB` static.

#![cfg(test)]

use crate::engine::database::Database;
use crate::engine::execute::execute;
use crate::engine::table::Table;
use crate::engine::value::{Column, ColumnType};

/// Parse and execute SQL statements against a fresh Database.
/// Returns the database for further inspection.
pub(crate) fn setup(sql: &[&str]) -> Database {
    let mut db = Database::new();
    for stmt_str in sql {
        let stmts = crate::parser::parse_sql(stmt_str).unwrap();
        for stmt in &stmts {
            execute(stmt, &mut db).unwrap();
        }
    }
    db
}

/// Create a simple table with the given column name/type pairs.
pub(crate) fn create_table(name: &str, columns: &[(&str, ColumnType, bool)]) -> Database {
    let mut db = Database::new();
    let cols: Vec<Column> = columns
        .iter()
        .map(|(name, dtype, pk)| Column {
            name: name.to_string(),
            dtype: dtype.clone(),
            primary_key: *pk,
            not_null: false,
            default: None,
            auto_increment: false,
            unique: false,
        })
        .collect();
    db.create_table(name, Table::new(name.to_string(), cols).unwrap())
        .unwrap();
    db
}

/// Execute a single SQL statement and return the result.
pub(crate) fn exec_sql(db: &mut Database, sql: &str) -> String {
    let stmts = crate::parser::parse_sql(sql).unwrap();
    let mut result = String::new();
    for stmt in &stmts {
        result = execute(stmt, db).unwrap();
    }
    result
}

/// Shorthand for a string primary key column.
pub(crate) fn pk(col: &str) -> (&str, ColumnType, bool) {
    (col, ColumnType::String, true)
}

/// Shorthand for an int column.
pub(crate) fn int(col: &str) -> (&str, ColumnType, bool) {
    (col, ColumnType::Int, false)
}

/// Shorthand for a string column.
#[allow(dead_code, reason = "test helper not used by all test modules")]
pub(crate) fn str_col(col: &str) -> (&str, ColumnType, bool) {
    (col, ColumnType::String, false)
}

#[test]
fn test_helper_create_and_select() {
    let mut db = create_table("t", &[pk("id"), int("v")]);
    exec_sql(&mut db, "INSERT INTO t VALUES ('a', 10)");
    let r = exec_sql(&mut db, "SELECT v FROM t WHERE id = 'a'");
    assert!(r.contains("10"), "select after insert: {}", r);
}

#[test]
fn test_helper_setup_multi() {
    let db = setup(&[
        "CREATE TABLE t (id STRING PRIMARY KEY, v INT)",
        "INSERT INTO t VALUES ('x', 42)",
    ]);
    let sql = "SELECT v FROM t WHERE id = 'x'";
    let stmts = crate::parser::parse_sql(sql).unwrap();
    let mut result = String::new();
    let mut db = db; // Take ownership
    for stmt in &stmts {
        result = crate::engine::execute::execute(stmt, &mut db).unwrap();
    }
    assert!(result.contains("42"), "setup query: {}", result);
}
