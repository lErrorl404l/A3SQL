// Differential testing: run identical SQL against A3SQL and real SQLite
// (rusqlite) and compare results. This validates the "SQLite-compatible
// semantics" claim against an independent reference engine.
//
// Per AQAP 2210 EVV: verification against an external reference rather than
// only hand-written expectations.

use crate::engine::database::Database;
use crate::engine::execute::util::parse_and_exec;
use crate::parser::parse_sql;
use rusqlite::Connection;

/// A corpus of SQL statements covering the shared dialect surface.
const CORPUS: &[&str] = &[
    // Schema
    "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, value REAL)",
    // Insert
    "INSERT INTO t VALUES (1, 'alpha', 1.5)",
    "INSERT INTO t VALUES (2, 'beta', 2.5)",
    "INSERT INTO t VALUES (3, 'gamma', 3.5)",
    "INSERT INTO t VALUES (4, NULL, NULL)",
    // Projection + filtering
    "SELECT id, name, value FROM t",
    "SELECT id FROM t WHERE value > 2.0",
    "SELECT id FROM t WHERE name IS NULL",
    "SELECT id FROM t WHERE id IN (1, 2)",
    // Aggregates
    "SELECT COUNT(*) FROM t",
    "SELECT SUM(value) FROM t",
    "SELECT MIN(id), MAX(id) FROM t",
    // Ordering
    "SELECT id, name FROM t ORDER BY id DESC",
    // Math expressions
    "SELECT id, id * 2 AS dbl FROM t",
    "SELECT id, value + 1 AS plus FROM t WHERE id <= 2",
    "SELECT ABS(value - 1.5) FROM t WHERE id = 1",
    "SELECT ROUND(value) FROM t WHERE value IS NOT NULL",
    // Update / delete
    "UPDATE t SET value = 9.9 WHERE id = 1",
    "SELECT value FROM t WHERE id = 1",
    "DELETE FROM t WHERE id = 3",
    "SELECT id FROM t",
    // Distinct / limit
    "SELECT DISTINCT name FROM t WHERE name IS NOT NULL",
    "SELECT id FROM t LIMIT 2",
    // Subquery
    "SELECT id FROM t WHERE id IN (SELECT id FROM t WHERE value IS NOT NULL)",
    // String ops
    "SELECT UPPER(name) FROM t WHERE name IS NOT NULL",
    "SELECT LENGTH(name) FROM t WHERE name IS NOT NULL",
];

/// Normalise a query result row for order-insensitive comparison.
fn normalise(rows: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut r = rows.to_vec();
    r.sort();
    r
}

/// Extract the result rows (after the header) from A3SQL's JSON result.
fn a3sql_rows(db: &mut Database, sql: &str) -> Result<Vec<Vec<String>>, String> {
    let stmts = parse_sql(sql).map_err(|e| e.to_string())?;
    let mut all_rows = Vec::new();
    for stmt in &stmts {
        if matches!(stmt, sqlparser::ast::Statement::Query(_)) {
            let query = match stmt {
                sqlparser::ast::Statement::Query(q) => q,
                _ => unreachable!(),
            };
            let (_headers, rows) =
                crate::engine::execute::select::exec_select_rows(query, db).map_err(|e| format!("{:?}", e))?;
            all_rows.extend(
                rows.iter()
                    .map(|row| {
                        row.iter()
                            .map(|v| match v {
                                crate::engine::value::DbValue::Null => "NULL".to_string(),
                                crate::engine::value::DbValue::Bool(b) => b.to_string(),
                                crate::engine::value::DbValue::Int(n) => n.to_string(),
                                crate::engine::value::DbValue::Float(f) => format!("{}", f),
                                crate::engine::value::DbValue::String(s) => s.clone(),
                                crate::engine::value::DbValue::Strings(v) => v.join(","),
                                crate::engine::value::DbValue::Floats(v) => {
                                    v.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",")
                                }
                            })
                            .collect()
                    })
                    .collect::<Vec<_>>(),
            );
        } else {
            crate::engine::execute::execute(stmt, db).map_err(|e| e.to_string())?;
        }
    }
    Ok(all_rows)
}

/// Extract result rows from real SQLite via rusqlite.
fn sqlite_rows(conn: &Connection, sql: &str) -> Result<Vec<Vec<String>>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let col_count = stmt.column_count();
    let mut rows = Vec::new();
    let mut q = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = q.next().map_err(|e| e.to_string())? {
        let mut cells = Vec::new();
        for i in 0..col_count {
            let v: rusqlite::types::Value = row.get(i).map_err(|e| e.to_string())?;
            cells.push(match v {
                rusqlite::types::Value::Null => "NULL".to_string(),
                rusqlite::types::Value::Integer(n) => n.to_string(),
                rusqlite::types::Value::Real(f) => format!("{}", f),
                rusqlite::types::Value::Text(s) => s,
                rusqlite::types::Value::Blob(b) => String::from_utf8_lossy(&b).to_string(),
            });
        }
        rows.push(cells);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the corpus on both engines and assert identical results
    /// (order-insensitive for unordered queries).
    #[test]
    fn differential_against_sqlite() {
        let mut a3 = Database::new();
        let sqlite = Connection::open_in_memory().expect("sqlite in-memory");

        for sql in CORPUS {
            // Execute exactly once on each engine; capture the result rows.
            let a3 = a3sql_rows(&mut a3, sql);
            let sq = sqlite_rows(&sqlite, sql);

            match (&a3, &sq) {
                (Ok(_), Ok(_)) => {}
                (Err(a3e), Ok(_)) => {
                    panic!("A3SQL failed but SQLite succeeded for: {sql}\n  a3sql: {a3e}")
                }
                (Ok(_), Err(sqe)) => {
                    panic!("SQLite failed but A3SQL succeeded for: {sql}\n  sqlite: {sqe}")
                }
                (Err(_), Err(_)) => continue, // both reject. Not comparable
            }

            let a3 = a3.unwrap();
            let sq = sq.unwrap();
            // Column count must match; rows compared as sets.
            if !a3.is_empty() || !sq.is_empty() {
                let a3_cols = a3.first().map(|r| r.len()).unwrap_or(0);
                let sq_cols = sq.first().map(|r| r.len()).unwrap_or(0);
                assert_eq!(
                    a3_cols, sq_cols,
                    "column count mismatch for: {sql}\n  a3sql: {:?}\n  sqlite: {:?}",
                    a3, sq
                );
            }
            let a3_norm = normalise(&a3);
            let sq_norm = normalise(&sq);
            assert_eq!(
                a3_norm, sq_norm,
                "result mismatch for: {sql}\n  a3sql: {a3:?}\n  sqlite: {sq:?}"
            );
        }
    }

    /// Verify a statement that both engines accept produces matching counts.
    #[test]
    fn differential_insert_count() {
        let mut a3 = Database::new();
        let sqlite = Connection::open_in_memory().expect("sqlite in-memory");
        parse_and_exec("CREATE TABLE c (id INTEGER, n INTEGER)", &mut a3).unwrap();
        sqlite.execute_batch("CREATE TABLE c (id INTEGER, n INTEGER)").unwrap();

        for i in 0..10 {
            let sql = format!("INSERT INTO c VALUES ({i}, {i})");
            parse_and_exec(&sql, &mut a3).unwrap();
            sqlite.execute(&sql, []).unwrap();
        }

        let a3_rows = a3sql_rows(&mut a3, "SELECT COUNT(*) FROM c").unwrap();
        let sq_rows = sqlite_rows(&sqlite, "SELECT COUNT(*) FROM c").unwrap();
        assert_eq!(a3_rows, sq_rows, "COUNT mismatch after bulk insert");
    }
}
