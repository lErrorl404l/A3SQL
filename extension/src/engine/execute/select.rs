// SELECT statement execution (single-table, no JOINs)
// Manages the thread-local DB snapshot for subquery evaluation.

//! SELECT execution — resolves tables, applies projections, handles subqueries.
//! Delegates JOINs, window functions, ORDER BY/LIMIT to their respective modules.

use std::collections::HashMap;

use indexmap::IndexSet;
use sqlparser::ast::{Distinct, Expr, OrderByKind, Query, Select, SelectItem, SetExpr, TableFactor};

use super::super::database::Database;
use super::super::functions::aggregate::projection_expr_name;
use super::super::functions::eval::{eval_expr, is_truthy, query_has_nondeterministic};
use super::super::stmts::ddl::object_name_str;
use super::super::stmts::select::sort::sort_rows;
use super::super::value::{DbValue, GroupKey};
use super::SUBQ_DB;

use crate::engine::error::EngineError;
use crate::engine::functions::eval::query_has_from;
use crate::engine::prelude::expr_has_subquery;

/// Execute a SELECT query (single table, no JOINs), returning the JSON result
/// string. A thin wrapper over [`exec_select_rows`] + JSON formatting; JOIN
/// queries are dispatched to the join executor unchanged (it owns the
/// single-wrapped `[header]` shape for empty join results).
pub(crate) fn exec_select(query: &Query, db: &mut Database) -> Result<String, EngineError> {
    let select: &Select = match &*query.body {
        SetExpr::Select(s) => s.as_ref(),
        _ => return Err(EngineError::Exec("Only SELECT statements supported".into())),
    };

    // Route to multi-table handler if FROM has JOINs OR multiple tables
    if super::super::stmts::select::joins::has_multiple_tables(select) {
        return super::super::stmts::select::joins::exec_select_joins(query, select, db);
    }

    let (header, rows) = exec_select_rows(query, db)?;
    Ok(format_rows_json(&header, &rows))
}

/// Execute a SELECT query, returning the header names and per-row projected
/// cell values — the exact cells the JSON formatters serialize. Subqueries
/// consume the rows directly, skipping the JSON serialize/re-parse round trip.
pub(crate) fn exec_select_rows(
    query: &Query,
    db: &mut Database,
) -> Result<(Vec<String>, Vec<Vec<DbValue>>), EngineError> {
    let select: &Select = match &*query.body {
        SetExpr::Select(s) => s.as_ref(),
        _ => return Err(EngineError::Exec("Only SELECT statements supported".into())),
    };

    // Route to multi-table handler if FROM has JOINs OR multiple tables
    if super::super::stmts::select::joins::has_multiple_tables(select) {
        return super::super::stmts::select::joins::exec_select_joins_rows(query, select, db);
    }

    // Handle bare SELECT without FROM clause
    if select.from.is_empty() {
        // Snapshot DB only if subqueries/EXISTS are present (clone is O(rows))
        set_subq_snapshot_if_needed(select, db);

        // Evaluate WHERE clause (SELECT 1 WHERE 1=0 should return empty)
        if let Some(where_expr) = &select.selection {
            let where_val = eval_expr(where_expr, &[], &HashMap::new()).unwrap_or(DbValue::Bool(false));
            if !is_truthy(&where_val) {
                let h: Vec<String> = select
                    .projection
                    .iter()
                    .map(|item| match item {
                        SelectItem::UnnamedExpr(e) => projection_expr_name(e),
                        SelectItem::ExprWithAlias { alias, .. } => alias.value.to_lowercase(),
                        _ => "*".into(),
                    })
                    .collect();
                return Ok((h, Vec::new()));
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
        let mut cells: Vec<DbValue> = Vec::new();
        for item in &select.projection {
            let expr = match item {
                SelectItem::UnnamedExpr(e) => e,
                SelectItem::ExprWithAlias { expr: e, .. } => e,
                _ => {
                    cells.push(DbValue::Null);
                    continue;
                }
            };
            match eval_expr(expr, row, &empty_cols) {
                Ok(v) => cells.push(v),
                Err(_) => cells.push(DbValue::Null),
            }
        }
        return Ok((header, vec![cells]));
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
    // key are the most common SELECT shape in mod workloads. Index candidates
    // are re-checked against the real predicate (verify-rescan) so coercion
    // residuals (float-vs-int, -0.0 vs +0.0, NaN) can never return a row the
    // scan would reject.
    let filtered_rows: Vec<&[DbValue]> =
        if let Some(row) = super::super::functions::builtin::try_pk_index(where_expr, table) {
            row.into_iter()
                .filter(|row| {
                    where_expr
                        .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                        .unwrap_or(true)
                })
                .collect()
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
        let result = super::super::functions::aggregate::compute_aggregate_rows(
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

    // 6. Project rows to the SELECT projection's cell values (respect only
    // chosen columns). JSON formatting happens in the caller.
    let (header, rows) = project_cells(limited_rows, &select.projection, &table.col_index, table);
    for name in &view_tables {
        let _ = db.drop_table(name);
    }
    Ok((header, rows))
}

/// Project rows to the SELECT projection's cell values — the exact cells the
/// JSON formatter serializes. Mirrors `format_projected_result` without the
/// JSON serialization, so the subquery path can consume the values directly.
fn project_cells(
    rows: Vec<&[DbValue]>,
    projection: &[SelectItem],
    col_map: &HashMap<String, usize>,
    table: &super::super::table::Table,
) -> (Vec<String>, Vec<Vec<DbValue>>) {
    let is_wildcard = projection
        .iter()
        .any(|item| matches!(item, SelectItem::Wildcard { .. }));
    if is_wildcard {
        return (
            table.columns.iter().map(|c| c.name.clone()).collect(),
            rows.iter().map(|r| r.to_vec()).collect(),
        );
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
    let projected: Vec<Vec<DbValue>> = rows
        .iter()
        .map(|row| {
            projection
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
                            let win_col = orig_cols + wf_prefix_counts[proj_idx];
                            if win_col < row.len() {
                                return row[win_col].clone();
                            }
                        }
                        eval_expr(e, row, col_map).unwrap_or(DbValue::Null)
                    } else {
                        DbValue::Null
                    }
                })
                .collect()
        })
        .collect();
    (header, projected)
}

/// Format projected SELECT rows as the JSON result string. An empty header
/// (aggregate over an empty partition set) yields `[]`; otherwise the result
/// is double-wrapped: [[header], row1, row2, ...].
fn format_rows_json(header: &[String], rows: &[Vec<DbValue>]) -> String {
    if header.is_empty() {
        return "[]".into();
    }
    let hq: Vec<String> = header.iter().map(|h| format!("\"{}\"", h)).collect();
    if rows.is_empty() {
        return format!("[[{}]]", hq.join(","));
    }
    let rj: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "[{}]",
                r.iter().map(|v| v.to_json_string()).collect::<Vec<_>>().join(",")
            )
        })
        .collect();
    format!("[[{}],{}]", hq.join(","), rj.join(","))
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

/// Execute a subquery and return the first column of each result row, in
/// result order. A thin wrapper over [`exec_subquery_rows`] flattening each
/// row to its first cell — the exact flatten the old JSON serialize/re-parse
/// produced (scalar cells: Int/Float/Bool/String/Null all survive a JSON
/// round trip unchanged).
pub(crate) fn exec_subquery(query: &Query, correlated: bool) -> Result<Vec<DbValue>, EngineError> {
    Ok(exec_subquery_rows(query, correlated)?
        .into_iter()
        .filter_map(|row| row.into_iter().next())
        .collect())
}

/// Execute a subquery and return its projected rows directly (no JSON round
/// trip). Uses the thread-local DB snapshot set by exec_select (avoids
/// deadlock).
///
/// Results are memoized per statement: eval_expr re-evaluates the subquery
/// for every outer row, and cloning the snapshot is O(total rows) — without
/// the cache, `WHERE n=(SELECT 1)` over 100k rows is O(n²). The cache key is
/// the (correlation-rewritten) query string, so correlated subqueries —
/// which inline distinct literals per row — never hit a stale entry.
///
/// Soundness skips (no lookup, no insert):
///   - `nondeterministic`: random()/datetime('now')/current_* have a
///     row-invariant AST — and therefore key — but a row-varying value;
///     caching them freezes one value for the whole statement.
///   - `correlated`: the rewritten key is distinct per outer-row value combo,
///     so the cache would miss anyway; skipping it avoids the per-row
///     Debug-format key and the 10k-cap insert churn.
pub(crate) fn exec_subquery_rows(query: &Query, correlated: bool) -> Result<Vec<Vec<DbValue>>, EngineError> {
    let nondet = query_has_nondeterministic(query);
    if !correlated && !nondet {
        let key = format!("{:?}", query);
        if let Some(hit) = super::SUBQ_CACHE.with(|c| c.borrow().get(&key).cloned()) {
            return Ok(hit);
        }
        let mut db_copy = subq_db_snapshot(query)?;
        let (_, rows) = exec_select_rows(query, &mut db_copy)?;
        // Store in per-statement cache (key = rewritten query string). Cap growth:
        // a pathological number of DISTINCT correlated rewrites per statement is
        // bounded, and the cache is cleared on every statement anyway.
        super::SUBQ_CACHE.with(|c| {
            let mut cache = c.borrow_mut();
            if cache.len() < 10_000 {
                cache.insert(key, rows.clone());
            }
        });
        return Ok(rows);
    }
    // Uncacheable path — evaluate fresh against a per-call DB copy.
    let mut db_copy = subq_db_snapshot(query)?;
    let (_, rows) = exec_select_rows(query, &mut db_copy)?;
    Ok(rows)
}

/// Clone the database a subquery executes against. A FROM-less subquery
/// (`SELECT random()`, `SELECT 1`) reads no table data, so a fresh empty
/// Database is equivalent — and avoids cloning the whole snapshot (O(rows))
/// on every row of a per-row-evaluated subquery.
fn subq_db_snapshot(query: &Query) -> Result<Database, EngineError> {
    if !query_has_from(query) && !crate::engine::prelude::query_has_subquery(query) {
        return Ok(Database::new());
    }
    super::SUBQ_DB.with(|snap| {
        snap.borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| EngineError::Exec("Subquery not supported in this context".to_string()))
    })
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
