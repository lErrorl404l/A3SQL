// SELECT, UNION, and related execution — extracted from execute.rs

//! SELECT execution — FROM, WHERE, GROUP BY, HAVING, DISTINCT, wildcards.
//! Delegates to sub-modules for JOINs, CTEs, UNION, ORDER BY/LIMIT, window functions.

use std::collections::HashMap;

use indexmap::IndexSet;
use sqlparser::ast::{Distinct, Expr, OrderByKind, Query, Select, SelectItem, SetExpr, TableFactor};

use super::super::functions::aggregate::{
    compute_aggregates, has_aggregate, has_group_by, partition_by_group, sort_partitions,
};
use super::ddl::object_name_str;
use crate::engine::error::EngineError;
use crate::engine::prelude::*;
use crate::engine::value::GroupKey;

pub(crate) mod cte;
pub(crate) mod derived;
pub(crate) mod joins;
pub(crate) mod sort;
pub(crate) mod union;
pub(crate) mod window;

use self::joins::{exec_select_joins, has_multiple_tables};
use self::sort::{apply_limit_offset, sort_rows};
use self::window::{compute_window_functions, has_window_function};

/// Set the thread-local DB snapshot only when the query needs it (contains a
/// subquery). Cloning the whole DB per SELECT is O(total rows) — the snapshot
/// exists only so nested SELECTs can read a consistent view. Written to the
/// executor's SUBQ_DB (the one exec_subquery reads — a per-module twin here
/// would be written but never read).
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
        super::super::execute::SUBQ_DB.with(|snap| *snap.borrow_mut() = Some(db.clone()));
        super::super::execute::clear_subq_cache();
    }
}

pub(crate) fn exec_select(query: &Query, db: &mut Database) -> Result<String, EngineError> {
    let select: &Select = match &*query.body {
        SetExpr::Select(s) => s.as_ref(),
        _ => return Err(EngineError::Exec("Only SELECT statements supported".into())),
    };

    // Route to multi-table handler if FROM has JOINs OR multiple tables
    if has_multiple_tables(select) {
        return exec_select_joins(query, select, db);
    }

    // Handle bare SELECT without FROM clause
    if select.from.is_empty() {
        // Snapshot DB only if subqueries/EXISTS are present (clone is O(rows))
        set_subq_snapshot_if_needed(select, db);

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
        let tf = select.from.first().ok_or(EngineError::Exec("No FROM clause".into()))?;
        match &tf.relation {
            TableFactor::Table { name, .. } => {
                let tname = object_name_str(name);
                if !db.has_table(&tname) && db.has_view(&tname) {
                    materialize_view(&tname, db)?;
                    vec![tname]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    };

    // Resolve table (single-table)
    let table = resolve_single_table(&select.from, db)?;
    let where_expr = select.selection.as_ref();

    // 1. Filter rows by WHERE — index-assisted when possible
    // Snapshot DB only if subqueries/EXISTS are present (clone is O(rows))
    set_subq_snapshot_if_needed(select, db);

    // PK fast path first (O(1) via pk_row_index) — point lookups by primary
    // key are the most common SELECT shape in mod workloads (armaos file
    // reads, LAMBS unit lookups, ACRE2 preset fetches). Index candidates are
    // re-checked against the real predicate (verify-rescan) so coercion
    // residuals (float-vs-int, -0.0 vs +0.0, NaN) can never return a row the
    // scan would reject.
    let filtered_rows: Vec<&[DbValue]> = if let Some(row) = try_pk_index(where_expr, table) {
        row.into_iter()
            .filter(|row| {
                where_expr
                    .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                    .unwrap_or(true)
            })
            .collect()
    } else if let Some(candidates) = try_trigram_index(where_expr, table) {
        candidates
            .into_iter()
            .filter(|row| {
                where_expr
                    .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                    .unwrap_or(true)
            })
            .collect()
    } else if let Some(rows) = try_btree_index(where_expr, table) {
        rows.into_iter()
            .filter(|row| {
                where_expr
                    .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                    .unwrap_or(true)
            })
            .collect()
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
    if has_aggregate(&select.projection) {
        let group_partitions = if has_group_by(select) {
            partition_by_group(&filtered_rows, select, &table.col_index)?
        } else {
            vec![filtered_rows] // single group: all rows
        };
        // HAVING — filter partitions after grouping (aggregates over the group)
        let group_partitions = if let Some(having) = &select.having {
            let flattened: Vec<Vec<&[DbValue]>> = group_partitions
                .into_iter()
                .filter(|group| {
                    if group.is_empty() {
                        return false;
                    }
                    super::super::functions::aggregate::eval_projection_expr(having, group, &table.col_index)
                        .map(|(_, v)| is_truthy(&v))
                        .unwrap_or(false)
                })
                .collect();
            flattened
        } else {
            group_partitions
        };
        // ORDER BY after GROUP BY — sort the groups (mod shape: ORDER BY <group-col>)
        let group_partitions = if let Some(order_by) = &query.order_by {
            match &order_by.kind {
                OrderByKind::Expressions(exprs) if !exprs.is_empty() => {
                    sort_partitions(group_partitions, exprs, &table.col_index)
                }
                _ => group_partitions,
            }
        } else {
            group_partitions
        };
        let result = compute_aggregates(&group_partitions, &select.projection, &table.col_index);
        for name in &view_tables {
            let _ = db.drop_table(name);
        }
        return result;
    }
    // 3. GROUP BY without aggregates — simple dedup
    let grouped_rows = if has_group_by(select) {
        let partitions = partition_by_group(&filtered_rows, select, &table.col_index)?;
        // HAVING — filter after grouping (aggregates over the group)
        let partitions: Vec<Vec<&[DbValue]>> = if let Some(having) = &select.having {
            partitions
                .into_iter()
                .filter(|group| {
                    if group.is_empty() {
                        return false;
                    }
                    super::super::functions::aggregate::eval_projection_expr(having, group, &table.col_index)
                        .map(|(_, v)| is_truthy(&v))
                        .unwrap_or(false)
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
        let mut seen: IndexSet<GroupKey> = IndexSet::new();
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
                seen.insert(GroupKey(proj))
            })
            .collect()
    } else {
        grouped_rows
    };

    // 3.75 Window functions — compute OVER expressions before ORDER BY
    let mut owned_rows: Vec<Vec<DbValue>> = deduped_rows.iter().map(|r| r.to_vec()).collect();
    if has_window_function(&select.projection) {
        compute_window_functions(&select.projection, &mut owned_rows, &table.col_index)?;
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
    let limited_rows = apply_limit_offset(sorted_rows, &query.limit_clause)?;

    // 6. Format result — respect SELECT projection (only show chosen columns)
    let result = format_projected_result(limited_rows, &select.projection, &table.col_index, table);
    for name in &view_tables {
        let _ = db.drop_table(name);
    }
    Ok(result)
}

pub(crate) use self::union::exec_union;
