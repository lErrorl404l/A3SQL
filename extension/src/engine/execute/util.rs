// Utility functions and thread-local storage for statement execution.

//! Thread-local state and shared helpers used across statement executors.

use std::collections::HashMap;

use sqlparser::ast::{Expr, SelectItem};
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;

use super::super::database::Database;
use super::super::functions::aggregate::projection_expr_name;
use super::super::functions::eval::eval_expr;
use super::super::table::Table;
use super::super::value::DbValue;

use crate::engine::error::EngineError;

// ponytail: thread-local DB snapshot for subquery evaluation (avoids deadlock
// when exec_subquery is called inside eval_expr while DB lock is held).
thread_local! {
    pub(crate) static SUBQ_DB: std::cell::RefCell<Option<Database>> =
        const { std::cell::RefCell::new(None) };
}

// ponytail: global tracking for last_insert_rowid / changes (no db ref in eval path)
thread_local! {
    pub(crate) static LAST_INSERT_ROWID: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
    pub(crate) static LAST_CHANGES: std::cell::RefCell<usize> =
        const { std::cell::RefCell::new(0) };
}

// ponytail: thread-local buffer for COPY FROM stdin data
thread_local! {
    pub(crate) static COPY_STDIN: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// Format SELECT results, projecting to only the requested columns.
pub(crate) fn format_projected_result(
    rows: Vec<&[DbValue]>,
    projection: &[SelectItem],
    col_map: &HashMap<String, usize>,
    table: &Table,
) -> String {
    let is_wildcard = projection
        .iter()
        .any(|item| matches!(item, SelectItem::Wildcard { .. }));
    if is_wildcard {
        return table.format_result(rows);
    }

    // Build header from projection
    let header: Vec<String> = projection
        .iter()
        .map(|item| match item {
            SelectItem::UnnamedExpr(expr) => projection_expr_name(expr),
            SelectItem::ExprWithAlias { alias, .. } => alias.value.to_lowercase(),
            SelectItem::Wildcard { .. } => unreachable!(),
            _ => format!("{:?}", item),
        })
        .collect();

    let header_json = format!(
        "[{}]",
        header
            .iter()
            .map(|h| format!("\"{}\"", h))
            .collect::<Vec<_>>()
            .join(",")
    );

    // Pre-count window functions in projection for correct column offset
    let wf_prefix_counts: Vec<usize> = projection
        .iter()
        .scan(0, |count, item| {
            let expr = match item {
                SelectItem::UnnamedExpr(e) => Some(e),
                SelectItem::ExprWithAlias { expr: e, .. } => Some(e),
                _ => None,
            };
            let is_win = expr.is_some_and(|e| matches!(e, Expr::Function(f) if f.over.is_some()));
            let idx = *count;
            if is_win {
                *count += 1;
            }
            Some(idx)
        })
        .collect();
    let orig_cols = col_map.len();
    let row_jsons: Vec<String> = rows
        .iter()
        .map(|row| {
            let cells: Vec<String> = projection
                .iter()
                .enumerate()
                .map(|(proj_idx, item)| {
                    let expr = match item {
                        SelectItem::UnnamedExpr(e) => Some(e),
                        SelectItem::ExprWithAlias { expr: e, .. } => Some(e),
                        SelectItem::Wildcard { .. } => None,
                        _ => None,
                    };
                    if let Some(e) = expr {
                        let is_window = matches!(e, Expr::Function(f) if f.over.is_some());
                        if is_window {
                            let win_idx = wf_prefix_counts[proj_idx];
                            let win_col = orig_cols + win_idx;
                            if win_col < row.len() {
                                return row[win_col].to_json_string();
                            }
                        }
                        match eval_expr(e, row, col_map) {
                            Ok(v) => v.to_json_string(),
                            Err(_) => "null".to_string(),
                        }
                    } else {
                        "null".to_string()
                    }
                })
                .collect();
            format!("[{}]", cells.join(","))
        })
        .collect();

    let mut parts: Vec<String> = vec![header_json];
    parts.extend(row_jsons);
    format!("[{}]", parts.join(","))
}

// ── CREATE INDEX handling ──────────────────────────────────────────────

pub(super) fn drop_index_by_name(db: &mut Database, name: &str) -> bool {
    let table_names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    for tname in table_names {
        if let Ok(table) = db.get_table_mut(&tname) {
            if table.drop_index(name).is_ok() {
                return true;
            }
        }
    }
    false
}

// ── TRIGGERS ─────────────────────────────────────────────────────────────

/// Parse and execute a raw SQL statement string within the DB context.
pub(crate) fn parse_and_exec(sql: &str, db: &mut Database) -> Result<String, EngineError> {
    // Try SQLite dialect first (handles CREATE TRIGGER with BEGIN...END body)
    let sqlite = SQLiteDialect {};
    if let Ok(stmts) = Parser::parse_sql(&sqlite, sql) {
        let mut results = Vec::new();
        for stmt in stmts {
            results.push(super::execute(&stmt, db)?);
        }
        return Ok(results.join("\n"));
    }
    let meta = sqlparser::dialect::GenericDialect {};
    let stmts =
        Parser::parse_sql(&meta, sql).map_err(|e| EngineError::Exec(format!("Parse error in trigger body: {}", e)))?;
    let mut results = Vec::new();
    for stmt in stmts {
        results.push(super::execute(&stmt, db)?);
    }
    Ok(results.join("\n"))
}
