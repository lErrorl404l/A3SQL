// SELECT statement execution (single-table, no JOINs)
// Manages the thread-local DB snapshot for subquery evaluation.

//! SELECT execution — resolves tables, applies projections, handles subqueries.
//! Delegates JOINs, window functions, ORDER BY/LIMIT to their respective modules.

use std::collections::HashMap;

use sqlparser::ast::{Distinct, Expr, OrderByKind, Query, Select, SelectItem, SetExpr, TableFactor};

use super::super::database::Database;
use super::super::functions::aggregate::projection_expr_name;
use super::super::functions::eval::{eval_expr, is_truthy};
use super::super::stmts::ddl::object_name_str;
use super::super::stmts::select::sort::sort_rows;
use super::super::value::{DbValue, json_val_to_dbvalue};
use super::SUBQ_DB;
use super::format_projected_result;

use crate::engine::error::EngineError;
use crate::engine::prelude::expr_has_subquery;

/// Execute a SELECT query (single table, no JOINs).
pub(crate) fn exec_select(query: &Query, db: &mut Database) -> Result<String, EngineError> {
    let select: &Select = match &*query.body {
        SetExpr::Select(s) => s.as_ref(),
        _ => return Err(EngineError::Exec("Only SELECT statements supported".into())),
    };

    // Route to multi-table handler if FROM has JOINs OR multiple tables
    if super::super::stmts::select::joins::has_multiple_tables(select) {
        return super::super::stmts::select::joins::exec_select_joins(query, select, db);
    }

    // Handle bare SELECT without FROM clause
    if select.from.is_empty() {
        // Snapshot DB only if subqueries/EXISTS are present (clone is O(rows))
        set_subq_snapshot_if_needed(select, db);

        // Evaluate WHERE clause (SELECT 1 WHERE 1=0 should return empty)
        if let Some(where_expr) = &select.selection {
            let where_val = eval_expr(where_expr, &[], &HashMap::new()).unwrap_or(DbValue::Bool(false));
            if !is_truthy(&where_val) {
                let h = select
                    .projection
                    .iter()
                    .map(|item| match item {
                        SelectItem::UnnamedExpr(e) => format!("\"{}\"", projection_expr_name(e)),
                        SelectItem::ExprWithAlias { alias, .. } => format!("\"{}\"", alias.value.to_lowercase()),
                        _ => "\"*\"".into(),
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                return Ok(format!("[[{}]]", h));
            }
        }

        let row: &[DbValue] = &[];
        let empty_cols: HashMap<String, usize> = HashMap::new();
        let header: Vec<String> = select
            .projection
            .iter()
            .map(|item| match item {
                SelectItem::UnnamedExpr(e) => projection_expr_name(e),
                SelectItem::ExprWithAlias { alias, .. } => alias.value.to_lowercase(),
                SelectItem::Wildcard { .. } => "*".into(),
                _ => format!("{:?}", item),
            })
            .collect();
        let mut cells: Vec<String> = Vec::new();
        for item in &select.projection {
            let expr = match item {
                SelectItem::UnnamedExpr(e) => e,
                SelectItem::ExprWithAlias { expr: e, .. } => e,
                _ => {
                    cells.push("null".into());
                    continue;
                }
            };
            match eval_expr(expr, row, &empty_cols) {
                Ok(v) => cells.push(v.to_json_string()),
                Err(_) => cells.push("null".into()),
            }
        }
        let h = header
            .iter()
            .map(|h| format!("\"{}\"", h))
            .collect::<Vec<_>>()
            .join(",");
        let c = cells.join(",");
        return Ok(format!("[[{}],[{}]]", h, c));
    }

    // ── View resolution — materialise views referenced in FROM ──
    let view_tables: Vec<String> = {
        let tf = select
            .from
            .first()
            .ok_or_else(|| EngineError::Exec("No FROM clause".into()))?;
        match &tf.relation {
            TableFactor::Table { name, .. } => {
                let tname = object_name_str(name);
                if !db.has_table(&tname) && db.has_view(&tname) {
                    super::super::functions::builtin::materialize_view(&tname, db)?;
                    vec![tname]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    };

    // Resolve table (single-table)
    let table = super::super::functions::builtin::resolve_single_table(&select.from, db)?;
    let where_expr = select.selection.as_ref();

    // 1. Filter rows by WHERE — index-assisted when possible
    // Snapshot DB only if subqueries/EXISTS are present (clone is O(rows))
    set_subq_snapshot_if_needed(select, db);

    // PK fast path first (O(1) via pk_row_index) — point lookups by primary
    // key are the most common SELECT shape in mod workloads.
    let filtered_rows: Vec<&[DbValue]> =
        if let Some(row) = super::super::functions::builtin::try_pk_index(where_expr, table) {
            row
        } else if let Some(candidates) = super::super::functions::builtin::try_trigram_index(where_expr, table) {
            candidates
                .into_iter()
                .filter(|row| {
                    where_expr
                        .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                        .unwrap_or(true)
                })
                .collect()
        } else if let Some(rows) = super::super::functions::builtin::try_btree_index(where_expr, table) {
            rows
        } else {
            table
                .rows
                .iter()
                .filter(|row| {
                    where_expr
                        .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                        .unwrap_or(true)
                })
                .map(|r| r.as_slice())
                .collect()
        };

    // 2. If aggregates are present, handle them (with or without GROUP BY)
    if super::super::functions::aggregate::has_aggregate(&select.projection) {
        let group_partitions = if super::super::functions::aggregate::has_group_by(select) {
            super::super::functions::aggregate::partition_by_group(&filtered_rows, select, &table.col_index)?
        } else {
            vec![filtered_rows] // single group: all rows
        };
        // HAVING — filter partitions after grouping
        let group_partitions = if let Some(having) = &select.having {
            let flattened: Vec<Vec<&[DbValue]>> = group_partitions
                .into_iter()
                .filter(|group| {
                    if group.is_empty() {
                        return false;
                    }
                    is_truthy(&eval_expr(having, group[0], &table.col_index).unwrap_or(DbValue::Bool(false)))
                })
                .collect();
            flattened
        } else {
            group_partitions
        };
        // ORDER BY after GROUP BY — sort the groups (mod shape: ORDER BY <group-col>)
        let group_partitions = if let Some(order_by) = &query.order_by {
            match &order_by.kind {
                sqlparser::ast::OrderByKind::Expressions(exprs) if !exprs.is_empty() => {
                    super::super::functions::aggregate::sort_partitions(group_partitions, exprs, &table.col_index)
                }
                _ => group_partitions,
            }
        } else {
            group_partitions
        };
        let result = super::super::functions::aggregate::compute_aggregates(
            &group_partitions,
            &select.projection,
            &table.col_index,
        );
        for name in &view_tables {
            let _ = db.drop_table(name);
        }
        return result;
    }

    // 3. GROUP BY without aggregates — simple dedup
    let grouped_rows = if super::super::functions::aggregate::has_group_by(select) {
        let partitions =
            super::super::functions::aggregate::partition_by_group(&filtered_rows, select, &table.col_index)?;
        // HAVING — filter after grouping
        let partitions: Vec<Vec<&[DbValue]>> = if let Some(having) = &select.having {
            partitions
                .into_iter()
                .filter(|group| {
                    if group.is_empty() {
                        return false;
                    }
                    is_truthy(&eval_expr(having, group[0], &table.col_index).unwrap_or(DbValue::Bool(false)))
                })
                .collect()
        } else {
            partitions
        };
        partitions.into_iter().map(|p| p[0]).collect()
    } else {
        filtered_rows
    };

    // 3.5 DISTINCT — dedup by comparing projected values (or DISTINCT ON expressions)
    let deduped_rows: Vec<&[DbValue]> = if select.distinct.is_some() {
        let mut seen: Vec<Vec<DbValue>> = Vec::new();
        let distinct_on_exprs: Option<Vec<Expr>> = match &select.distinct {
            Some(Distinct::On(exprs)) => Some(exprs.clone()),
            _ => None,
        };
        grouped_rows
            .into_iter()
            .filter(|row| {
                let proj: Vec<DbValue> = if let Some(on_exprs) = &distinct_on_exprs {
                    // DISTINCT ON (expr1, expr2) — only compare these expressions
                    on_exprs
                        .iter()
                        .filter_map(|e| eval_expr(e, row, &table.col_index).ok())
                        .collect()
                } else {
                    select
                        .projection
                        .iter()
                        .filter_map(|item| {
                            if let SelectItem::UnnamedExpr(e) = item {
                                eval_expr(e, row, &table.col_index).ok()
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                if seen.contains(&proj) {
                    false
                } else {
                    seen.push(proj);
                    true
                }
            })
            .collect()
    } else {
        grouped_rows
    };

    // 3.75 Window functions — compute OVER expressions before ORDER BY
    let mut owned_rows: Vec<Vec<DbValue>> = deduped_rows.iter().map(|r| r.to_vec()).collect();
    if super::super::stmts::select::window::has_window_function(&select.projection) {
        super::super::stmts::select::window::compute_window_functions(
            &select.projection,
            &mut owned_rows,
            &table.col_index,
        )?;
    }
    let post_wf_rows: Vec<&[DbValue]> = owned_rows.iter().map(|r| r.as_slice()).collect();

    // 4. ORDER BY
    let sorted_rows = if let Some(order_by) = &query.order_by {
        let exprs = match &order_by.kind {
            OrderByKind::Expressions(exprs) => exprs,
            _ => return Err(EngineError::Exec("ORDER BY ALL not supported".into())),
        };
        if !exprs.is_empty() {
            sort_rows(post_wf_rows, exprs, &table.col_index)?
        } else {
            post_wf_rows
        }
    } else {
        post_wf_rows
    };

    // 5. LIMIT / OFFSET
    let limited_rows = super::super::stmts::select::sort::apply_limit_offset(sorted_rows, &query.limit_clause)?;

    // 6. Format result — respect SELECT projection (only show chosen columns)
    let result = format_projected_result(limited_rows, &select.projection, &table.col_index, table);
    for name in &view_tables {
        let _ = db.drop_table(name);
    }
    Ok(result)
}

/// Set the thread-local DB snapshot only when the query needs it (contains a
/// subquery). Cloning the whole DB per SELECT is O(total rows) — the snapshot
/// exists only so nested SELECTs can read a consistent view.
fn set_subq_snapshot_if_needed(select: &Select, db: &Database) {
    let needs_snapshot = select.projection.iter().any(|item| match item {
        SelectItem::UnnamedExpr(e) | SelectItem::ExprWithAlias { expr: e, .. } => expr_has_subquery(e),
        _ => false,
    }) || select.selection.as_ref().is_some_and(expr_has_subquery)
        || match &select.group_by {
            sqlparser::ast::GroupByExpr::Expressions(exprs, _) => exprs.iter().any(expr_has_subquery),
            sqlparser::ast::GroupByExpr::All(_) => false,
        }
        || select.having.as_ref().is_some_and(expr_has_subquery);
    if needs_snapshot {
        SUBQ_DB.with(|snap| *snap.borrow_mut() = Some(db.clone()));
        super::clear_subq_cache();
    }
}

/// Execute a subquery (SELECT) and return the first column of each row.
/// Uses the thread-local DB snapshot set by exec_select (avoids deadlock).
///
/// Results are memoized per statement: eval_expr re-evaluates the subquery
/// for every outer row, and cloning the snapshot is O(total rows) — without
/// the cache, `WHERE n=(SELECT 1)` over 100k rows is O(n²). The cache key is
/// the (correlation-rewritten) query string, so correlated subqueries —
/// which inline distinct literals per row — never hit a stale entry.
pub(crate) fn exec_subquery(query: &Query) -> Result<Vec<DbValue>, EngineError> {
    let key = format!("{:?}", query);
    if let Some(hit) = super::SUBQ_CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return Ok(hit);
    }
    let db_snapshot = super::SUBQ_DB.with(|snap| {
        snap.borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| EngineError::Exec("Subquery not supported in this context".to_string()))
    })?;
    let mut db_copy = db_snapshot;
    let result_str = exec_select(query, &mut db_copy)?;

    // Parse the JSON result (format: [[header], [row1], [row2], ...])
    // exec_select returns raw data, NOT [code, msg, data] wrapped format
    let mut values = Vec::new();
    match serde_json::from_str::<Vec<serde_json::Value>>(&result_str) {
        Ok(rows) => {
            for row in rows.iter().skip(1) {
                if let Some(arr) = row.as_array()
                    && let Some(first) = arr.first()
                {
                    values.push(json_val_to_dbvalue(first));
                }
            }
        }
        Err(_) => {
            // Fallback: parse raw string
            if let Some(start) = result_str.find("[[")
                && let Some(end) = result_str.rfind("]]")
            {
                let inner = &result_str[start + 1..end];
                for row_str in inner.split("],[") {
                    let cleaned = row_str.trim_matches('[').trim_matches(']').trim();
                    if !cleaned.is_empty() {
                        let val = cleaned.trim_matches('"');
                        if let Ok(n) = val.parse::<i64>() {
                            values.push(DbValue::Int(n));
                        } else if let Ok(f) = val.parse::<f64>() {
                            values.push(DbValue::Float(f));
                        } else {
                            values.push(DbValue::String(val.to_string()));
                        }
                    }
                }
                if values.len() > 1 {
                    values.remove(0);
                }
            }
        }
    }
    // Store in per-statement cache (key = rewritten query string). Cap growth:
    // a pathological number of DISTINCT correlated rewrites per statement is
    // bounded, and the cache is cleared on every statement anyway.
    super::SUBQ_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.len() < 10_000 {
            cache.insert(key, values.clone());
        }
    });
    Ok(values)
}

/// Apply ORDER BY and LIMIT from a Query to a parsed JSON result string.
#[allow(dead_code, reason = "ORDER BY/LIMIT on UNION results not yet implemented")]
pub(crate) fn apply_order_limit(json_str: &str, query: &Query) -> Result<String, EngineError> {
    if query.order_by.is_none() && query.limit_clause.is_none() {
        return Ok(json_str.to_string());
    }
    // ponytail: ORDER BY/LIMIT on UNION results is complex — pass through raw
    Ok(json_str.to_string())
}
