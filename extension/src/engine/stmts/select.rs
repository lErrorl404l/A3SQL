// SELECT, UNION, and related execution — extracted from execute.rs

use std::collections::HashMap;

use sqlparser::ast::{
    Distinct, Expr, FunctionArguments, LimitClause, OrderByKind, Query, Select, SelectItem, SetExpr, TableFactor,
    WindowFrame, WindowFrameBound, WindowFrameUnits, WindowType,
};

use super::super::database::Database;
use super::super::execute::{
    apply_binary_op, apply_limit_offset, compute_aggregates, curdate_value, db_value_cmp, eval_expr, eval_literal_expr,
    exec_std_function, extract_func_arg, format_projected_result, get_func_arg_unnamed, has_aggregate, has_group_by,
    is_truthy, materialize_view, now_value, object_name_str, parse_expr_as_usize, partition_by_group,
    projection_expr_name, resolve_single_table, resolve_table_factor, simple_like, sort_rows, sql_val_to_db,
    try_btree_index, try_trigram_index, value_to_string, values_equal,
};
use super::super::table::Table;
use super::super::value::DbValue;

// ponytail: thread-local DB snapshot for subquery evaluation (avoids deadlock
// when exec_subquery is called inside eval_expr while DB lock is held).
thread_local! {
    pub(crate) static SUBQ_DB: std::cell::RefCell<Option<Database>> =
        const { std::cell::RefCell::new(None) };
}

fn has_window_function(projection: &[SelectItem]) -> bool {
    for item in projection {
        let func = match item {
            SelectItem::UnnamedExpr(Expr::Function(f))
            | SelectItem::ExprWithAlias {
                expr: Expr::Function(f),
                ..
            } => f,
            _ => continue,
        };
        if func.over.is_some() {
            return true;
        }
    }
    false
}

/// Compute ROWS frame bounds for a window function at a given position within a partition.
/// Returns (start, end) inclusive indices into the ordered partition.
fn frame_bounds(frame: &WindowFrame, part_len: usize, pos: usize) -> (usize, usize) {
    let eval_offset = |expr: &Expr| -> usize {
        eval_literal_expr(expr)
            .ok()
            .and_then(|v| match v {
                DbValue::Int(i) => Some(i.max(0) as usize),
                _ => None,
            })
            .unwrap_or(0)
    };
    let max_pos = part_len.saturating_sub(1);
    match frame.units {
        WindowFrameUnits::Rows => {
            let start = match &frame.start_bound {
                WindowFrameBound::Preceding(None) => 0,
                WindowFrameBound::Preceding(Some(expr)) => pos.saturating_sub(eval_offset(expr)),
                WindowFrameBound::CurrentRow => pos,
                WindowFrameBound::Following(None) => pos,
                WindowFrameBound::Following(Some(expr)) => (pos + eval_offset(expr)).min(max_pos),
            };
            let end = match &frame.end_bound {
                Some(WindowFrameBound::Preceding(None)) => 0,
                Some(WindowFrameBound::Preceding(Some(expr))) => pos.saturating_sub(eval_offset(expr)),
                Some(WindowFrameBound::CurrentRow) => pos,
                Some(WindowFrameBound::Following(None)) => max_pos,
                Some(WindowFrameBound::Following(Some(expr))) => (pos + eval_offset(expr)).min(max_pos),
                None => pos,
            };
            (start.min(end), end.max(start))
        }
        // ponytail: RANGE/GROUPS not implemented — use full partition
        _ => (0, max_pos),
    }
}

/// Evaluate an aggregate over a frame of rows for window functions.
fn eval_window_aggregate(
    func_name: &str,
    rows: &[&[DbValue]],
    arg: Option<&Expr>,
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    match func_name {
        "count" => {
            let count = if let Some(a) = arg {
                rows.iter()
                    .filter(|r| {
                        eval_expr(a, r, col_map)
                            .map(|v| !matches!(v, DbValue::Null))
                            .unwrap_or(false)
                    })
                    .count()
            } else {
                rows.len()
            };
            Ok(DbValue::Int(count as i64))
        }
        "sum" => {
            let a = arg.ok_or("SUM requires an argument")?;
            let first = rows.first().and_then(|r| eval_expr(a, r, col_map).ok());
            match first {
                Some(DbValue::Int(_)) => {
                    let sum: i64 = rows
                        .iter()
                        .filter_map(|r| eval_expr(a, r, col_map).ok())
                        .filter_map(|v| match v {
                            DbValue::Int(n) => Some(n),
                            _ => None,
                        })
                        .sum();
                    Ok(DbValue::Int(sum))
                }
                _ => {
                    let sum: f64 = rows
                        .iter()
                        .filter_map(|r| eval_expr(a, r, col_map).ok())
                        .filter_map(|v| match v {
                            DbValue::Float(f) => Some(f),
                            DbValue::Int(n) => Some(n as f64),
                            _ => None,
                        })
                        .sum();
                    Ok(DbValue::Float(sum))
                }
            }
        }
        "avg" => {
            let a = arg.ok_or("AVG requires an argument")?;
            let mut sum = 0.0f64;
            let mut count = 0usize;
            for r in rows {
                if let Ok(v) = eval_expr(a, r, col_map) {
                    match v {
                        DbValue::Int(n) => {
                            sum += n as f64;
                            count += 1;
                        }
                        DbValue::Float(f) => {
                            sum += f;
                            count += 1;
                        }
                        _ => {}
                    }
                }
            }
            if count == 0 {
                Ok(DbValue::Null)
            } else {
                Ok(DbValue::Float(sum / count as f64))
            }
        }
        "min" => {
            let a = arg.ok_or("MIN requires an argument")?;
            rows.iter()
                .filter_map(|r| eval_expr(a, r, col_map).ok())
                .min_by(db_value_cmp)
                .ok_or_else(|| "MIN on empty frame".into())
        }
        "max" => {
            let a = arg.ok_or("MAX requires an argument")?;
            rows.iter()
                .filter_map(|r| eval_expr(a, r, col_map).ok())
                .max_by(db_value_cmp)
                .ok_or_else(|| "MAX on empty frame".into())
        }
        _ => Err(format!("Aggregate '{}' not supported as window function", func_name)),
    }
}

/// Compute window function values for each row and return them as appended columns.
fn compute_window_functions(
    projection: &[SelectItem],
    rows: &mut [Vec<DbValue>],
    col_map: &HashMap<String, usize>,
) -> Result<(), String> {
    let total = rows.len();
    if total == 0 {
        return Ok(());
    }

    for item in projection {
        let (func, _alias) = match item {
            SelectItem::UnnamedExpr(Expr::Function(f)) => (f, None),
            SelectItem::ExprWithAlias {
                expr: Expr::Function(f),
                alias,
            } => (f, Some(alias.value.to_lowercase())),
            _ => continue,
        };
        let Some(WindowType::WindowSpec(spec)) = &func.over else {
            continue;
        };

        let mut computed = vec![DbValue::Null; total];
        let func_name = func.name.to_string().to_lowercase();

        // Build partition index groups
        let mut partitions: Vec<Vec<usize>> = if spec.partition_by.is_empty() {
            vec![(0..total).collect()]
        } else {
            let mut groups: Vec<(Vec<DbValue>, Vec<usize>)> = Vec::new();
            for (i, row) in rows.iter().enumerate() {
                let key: Vec<DbValue> = spec
                    .partition_by
                    .iter()
                    .filter_map(|pe| eval_expr(pe, row, col_map).ok())
                    .collect();
                if let Some(pos) = groups.iter().position(|(k, _)| *k == key) {
                    groups[pos].1.push(i);
                } else {
                    groups.push((key, vec![i]));
                }
            }
            groups.into_iter().map(|(_, indices)| indices).collect()
        };

        for part_indices in &mut partitions {
            // Sort indices within partition by ORDER BY
            if !spec.order_by.is_empty() {
                part_indices.sort_by(|&a, &b| {
                    for ob in &spec.order_by {
                        let va = eval_expr(&ob.expr, &rows[a], col_map).unwrap_or(DbValue::Null);
                        let vb = eval_expr(&ob.expr, &rows[b], col_map).unwrap_or(DbValue::Null);
                        let cmp = db_value_cmp(&va, &vb);
                        let order = match ob.options.asc {
                            Some(false) => cmp.reverse(),
                            _ => cmp,
                        };
                        if order != std::cmp::Ordering::Equal {
                            return order;
                        }
                    }
                    std::cmp::Ordering::Equal
                });
            }

            // Apply the window function
            match func_name.as_str() {
                "row_number" => {
                    for (pos, &idx) in part_indices.iter().enumerate() {
                        computed[idx] = DbValue::Int(pos as i64 + 1);
                    }
                }
                "rank" => {
                    let mut rank = 1i64;
                    for pos in 0..part_indices.len() {
                        if pos > 0 {
                            let cur = &rows[part_indices[pos]];
                            let prev = &rows[part_indices[pos - 1]];
                            let mut equal = true;
                            for ob in &spec.order_by {
                                let vc = eval_expr(&ob.expr, cur, col_map).unwrap_or(DbValue::Null);
                                let vp = eval_expr(&ob.expr, prev, col_map).unwrap_or(DbValue::Null);
                                if vc != vp {
                                    equal = false;
                                    break;
                                }
                            }
                            if !equal {
                                rank = pos as i64 + 1;
                            }
                        }
                        computed[part_indices[pos]] = DbValue::Int(rank);
                    }
                }
                "dense_rank" => {
                    let mut dense_rank = 1i64;
                    for pos in 0..part_indices.len() {
                        if pos > 0 {
                            let cur = &rows[part_indices[pos]];
                            let prev = &rows[part_indices[pos - 1]];
                            let mut equal = true;
                            for ob in &spec.order_by {
                                let vc = eval_expr(&ob.expr, cur, col_map).unwrap_or(DbValue::Null);
                                let vp = eval_expr(&ob.expr, prev, col_map).unwrap_or(DbValue::Null);
                                if vc != vp {
                                    equal = false;
                                    break;
                                }
                            }
                            if equal {
                                computed[part_indices[pos]] = DbValue::Int(dense_rank);
                            } else {
                                computed[part_indices[pos]] = DbValue::Int(dense_rank);
                                // still same as dense rank — use pos+1
                            }
                            if !equal {
                                dense_rank += 1;
                            }
                        } else {
                            computed[part_indices[pos]] = DbValue::Int(dense_rank);
                        }
                    }
                    // Recompute: assign dense ranks properly
                    let mut rank = 1i64;
                    for pos in 0..part_indices.len() {
                        if pos > 0 {
                            let cur = &rows[part_indices[pos]];
                            let prev = &rows[part_indices[pos - 1]];
                            let mut equal = true;
                            for ob in &spec.order_by {
                                let vc = eval_expr(&ob.expr, cur, col_map).unwrap_or(DbValue::Null);
                                let vp = eval_expr(&ob.expr, prev, col_map).unwrap_or(DbValue::Null);
                                if vc != vp {
                                    equal = false;
                                    break;
                                }
                            }
                            if !equal {
                                rank += 1;
                            }
                        }
                        computed[part_indices[pos]] = DbValue::Int(rank);
                    }
                }
                "count" | "sum" | "avg" | "min" | "max" => {
                    let arg = extract_func_arg(func).ok();
                    for (pos, &idx) in part_indices.iter().enumerate() {
                        let (fs, fe) = if let Some(ref f) = spec.window_frame {
                            frame_bounds(f, part_indices.len(), pos)
                        } else {
                            (0, part_indices.len().saturating_sub(1))
                        };
                        let frame_rows: Vec<&[DbValue]> =
                            part_indices[fs..=fe].iter().map(|&p| rows[p].as_slice()).collect();
                        computed[idx] = eval_window_aggregate(func_name.as_str(), &frame_rows, arg, col_map)?;
                    }
                }
                _ => {
                    return Err(format!("Window function '{}' not supported", func_name));
                }
            }
        }

        // Append computed column to each row
        for (i, val) in computed.into_iter().enumerate() {
            rows[i].push(val);
        }
    }
    Ok(())
}

// ── SELECT ──────────────────────────────────────────────────────────────

pub(crate) fn exec_select(query: &Query, db: &mut Database) -> Result<String, String> {
    let select: &Select = match &*query.body {
        SetExpr::Select(s) => s.as_ref(),
        _ => return Err("Only SELECT statements supported".into()),
    };

    // Route to multi-table handler if FROM has JOINs OR multiple tables
    if has_multiple_tables(select) {
        return exec_select_joins(query, select, db);
    }

    // Handle bare SELECT without FROM clause
    if select.from.is_empty() {
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
        let tf = select.from.first().ok_or("No FROM clause")?;
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
    // Set thread-local DB snapshot for subquery evaluation
    SUBQ_DB.with(|snap| *snap.borrow_mut() = Some(db.clone()));

    // Try trigram index first (fuzzy_match candidates); still re-eval WHERE for accuracy
    let filtered_rows: Vec<&[DbValue]> = if let Some(candidates) = try_trigram_index(where_expr, table) {
        candidates
            .into_iter()
            .filter(|row| {
                where_expr
                    .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                    .unwrap_or(true)
            })
            .collect()
    } else if let Some(rows) = try_btree_index(where_expr, table) {
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
    if has_aggregate(&select.projection) {
        let group_partitions = if has_group_by(select) {
            partition_by_group(&filtered_rows, select, &table.col_index)?
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
        let result = compute_aggregates(&group_partitions, &select.projection, &table.col_index);
        for name in &view_tables {
            let _ = db.drop_table(name);
        }
        return result;
    }

    // 3. GROUP BY without aggregates — simple dedup
    let grouped_rows = if has_group_by(select) {
        let partitions = partition_by_group(&filtered_rows, select, &table.col_index)?;
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
    if has_window_function(&select.projection) {
        compute_window_functions(&select.projection, &mut owned_rows, &table.col_index)?;
    }
    let post_wf_rows: Vec<&[DbValue]> = owned_rows.iter().map(|r| r.as_slice()).collect();

    // 4. ORDER BY
    let sorted_rows = if let Some(order_by) = &query.order_by {
        let exprs = match &order_by.kind {
            OrderByKind::Expressions(exprs) => exprs,
            _ => return Err("ORDER BY ALL not supported".into()),
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

/// Check if the FROM clause has multiple tables or JOINs.
fn has_multiple_tables(select: &Select) -> bool {
    select.from.len() > 1 || select.from.iter().any(|t| !t.joins.is_empty())
}

/// Execute a SELECT with JOINs. Uses a flat-row column map with absolute positions.
fn exec_select_joins(query: &Query, select: &Select, db: &mut Database) -> Result<String, String> {
    use sqlparser::ast::{JoinConstraint, JoinOperator};

    // ── Resolve all tables in FROM + JOINs ──────────────────────────
    struct Tbl {
        name: String,
        cols: usize,
        start: usize,
        rows: Vec<Vec<DbValue>>,
    }

    let mut tbls: Vec<Tbl> = Vec::new();
    let mut abs: usize = 0;
    let mut view_tables: Vec<String> = Vec::new();

    for twj in &select.from {
        let (n, t) = resolve_table_factor(&twj.relation, db)?;
        if db.has_view(&n) {
            view_tables.push(n.clone());
        }
        let r: Vec<Vec<DbValue>> = t.rows.to_vec();
        let c = t.columns.len();
        tbls.push(Tbl {
            name: n.clone(),
            cols: c,
            start: abs,
            rows: r,
        });
        abs += c;
        for j in &twj.joins {
            let (jn, jt) = resolve_table_factor(&j.relation, db)?;
            if db.has_view(&jn) {
                view_tables.push(jn.clone());
            }
            let jr: Vec<Vec<DbValue>> = jt.rows.to_vec();
            let jc = jt.columns.len();
            tbls.push(Tbl {
                name: jn.clone(),
                cols: jc,
                start: abs,
                rows: jr,
            });
            abs += jc;
        }
    }

    // ── Build flat column map ───────────────────────────────────────
    let mut col_map: HashMap<String, usize> = HashMap::new();
    let mut header: Vec<String> = Vec::new();
    for tbl in &tbls {
        let tn = db.get_table(&tbl.name).map_err(|e| format!("JOIN: {}", e))?.clone();
        for (ci, col) in tn.columns.iter().enumerate() {
            let p = tbl.start + ci;
            col_map.insert(format!("{}.{}", tbl.name, col.name), p);
            col_map.insert(col.name.clone(), p);
            header.push(format!("{}.{}", tbl.name, col.name));
        }
    }

    let total = abs;

    // Helper: build flat row from table-row indices
    let bf = |idxs: &[usize]| -> Vec<DbValue> {
        let mut v = Vec::with_capacity(total);
        for (ti, &ri) in idxs.iter().enumerate() {
            if ri == usize::MAX {
                v.resize(v.len() + tbls[ti].cols, DbValue::Null);
            } else {
                v.extend_from_slice(&tbls[ti].rows[ri]);
            }
        }
        v
    };

    let ef = |e: &Expr, r: &[DbValue]| -> Result<DbValue, String> { eval_expr_on_flat_row(e, r, &col_map) };

    // ── Generate combined rows ──────────────────────────────────────
    let mut cidx: Vec<Vec<usize>> = (0..tbls[0].rows.len()).map(|i| vec![i]).collect();
    let no_constraint = JoinConstraint::None;
    let joins = &select.from[0].joins;

    // Precompute common column names for NATURAL joins
    let natural_common: Vec<Vec<(String, usize, usize)>> = joins
        .iter()
        .enumerate()
        .map(|(i, j)| {
            if matches!(
                &j.join_operator,
                JoinOperator::Inner(JoinConstraint::Natural)
                    | JoinOperator::LeftOuter(JoinConstraint::Natural)
                    | JoinOperator::RightOuter(JoinConstraint::Natural)
                    | JoinOperator::FullOuter(JoinConstraint::Natural)
            ) {
                // Right table is at tbls index i+1 (left accumulated = tbls[0..=i])
                let right_ti = i + 1;
                if right_ti < tbls.len() {
                    let right_name = &tbls[right_ti].name;
                    if let Ok(rt) = db.get_table(right_name) {
                        // For each right column, find if any left table has the same name
                        let mut common = Vec::new();
                        for right_col in &rt.columns {
                            for left_tbl in &tbls[0..right_ti] {
                                if let Ok(lt) = db.get_table(&left_tbl.name) {
                                    if lt.columns.iter().any(|c| c.name == right_col.name) {
                                        // Store (col_name, left_table_idx, right_start_in_flat_row + col_idx)
                                        if let Some(&lp) = col_map.get(&format!("{}.{}", left_tbl.name, right_col.name))
                                        {
                                            if let Some(&rp) =
                                                col_map.get(&format!("{}.{}", right_name, right_col.name))
                                            {
                                                common.push((right_col.name.clone(), lp, rp));
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return common;
                    }
                }
            }
            Vec::new()
        })
        .collect();

    for (ti, tbl) in tbls.iter().enumerate().skip(1) {
        // Determine join type and constraint
        let join_info = if ti <= joins.len() {
            let join = &joins[ti - 1];
            Some(&join.join_operator)
        } else {
            None
        };
        let con: &JoinConstraint = match join_info {
            Some(
                JoinOperator::Inner(c)
                | JoinOperator::LeftOuter(c)
                | JoinOperator::RightOuter(c)
                | JoinOperator::FullOuter(c)
                | JoinOperator::Join(c)
                | JoinOperator::CrossJoin(c),
            ) => c,
            _ => &no_constraint,
        };
        let is_left = matches!(join_info, Some(JoinOperator::LeftOuter(_)));
        let is_right = matches!(join_info, Some(JoinOperator::RightOuter(_)));
        let is_full = matches!(join_info, Some(JoinOperator::FullOuter(_)));
        let preserve_left = is_left || is_full;
        let preserve_right = is_right || is_full;

        let mut right_matched = vec![false; tbl.rows.len()];
        let mut next = Vec::new();

        // Precompute USING column positions if applicable
        let using_cols: Vec<(usize, usize)> = match con {
            JoinConstraint::Using(cols) => {
                let mut pairs = Vec::new();
                for obj in cols {
                    let cname = obj.to_string().to_lowercase();
                    // Left side: look up bare name in col_map (ambiguous but standard SQL uses qualified)
                    // Try qualified: find which left table has this column
                    for left_tbl in &tbls[0..ti] {
                        if let Ok(lt) = db.get_table(&left_tbl.name) {
                            if lt.columns.iter().any(|c| c.name == cname) {
                                if let Some(&lp) = col_map.get(&format!("{}.{}", left_tbl.name, cname)) {
                                    if let Some(&rp) = col_map.get(&format!("{}.{}", tbl.name, cname)) {
                                        pairs.push((lp, rp));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                pairs
            }
            _ => Vec::new(),
        };

        // Precompute NATURAL positions if applicable
        let natural_pairs: &[(String, usize, usize)] = if ti >= 1 && ti - 1 < natural_common.len() {
            &natural_common[ti - 1]
        } else {
            &[]
        };

        for ls in &cidx {
            let mut hit = false;
            for (ri, rm) in right_matched.iter_mut().enumerate() {
                let mut cs = ls.clone();
                cs.push(ri);
                let f = bf(&cs);
                let ok = match con {
                    JoinConstraint::On(ex) => ef(ex, &f).map(|v| is_truthy(&v)).unwrap_or(false),
                    JoinConstraint::Using(_) => using_cols.iter().all(|&(lp, rp)| {
                        if lp < f.len() && rp < f.len() {
                            values_equal(&f[lp], &f[rp])
                        } else {
                            false
                        }
                    }),
                    JoinConstraint::Natural => natural_pairs.iter().all(|(_, lp, rp)| {
                        if *lp < f.len() && *rp < f.len() {
                            values_equal(&f[*lp], &f[*rp])
                        } else {
                            false
                        }
                    }),
                    _ => true,
                };
                if ok {
                    next.push(cs);
                    hit = true;
                    *rm = true;
                }
            }
            if preserve_left && !hit {
                let mut ns = ls.clone();
                ns.push(usize::MAX);
                next.push(ns);
            }
        }

        // Add unmatched right rows for RIGHT / FULL OUTER join
        if preserve_right {
            let all_max: Vec<usize> = (0..ti).map(|_| usize::MAX).collect();
            for (ri, matched) in right_matched.iter().enumerate() {
                if !matched {
                    let mut cs = all_max.clone();
                    cs.push(ri);
                    next.push(cs);
                }
            }
        }

        cidx = next;
    }

    // ── Materialize ─────────────────────────────────────────────────
    let mut rows: Vec<Vec<DbValue>> = cidx.iter().map(|ix| bf(ix)).collect();

    // WHERE
    if let Some(ex) = select.selection.as_ref() {
        rows.retain(|r| ef(ex, r).map(|v| is_truthy(&v)).unwrap_or(false));
    }

    // ORDER BY
    if let Some(ob) = &query.order_by {
        let exs = match &ob.kind {
            OrderByKind::Expressions(e) => e,
            _ => return Err("ORDER BY ALL not supported".into()),
        };
        if !exs.is_empty() {
            rows.sort_by(|a, b| {
                for o in exs {
                    let av = ef(&o.expr, a).unwrap_or(DbValue::Null);
                    let bv = ef(&o.expr, b).unwrap_or(DbValue::Null);
                    let c = value_to_string(&av).cmp(&value_to_string(&bv));
                    let c = if o.options.asc.unwrap_or(true) { c } else { c.reverse() };
                    if c != std::cmp::Ordering::Equal {
                        return c;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
    }

    // LIMIT / OFFSET
    let (off, lim) = match &query.limit_clause {
        Some(LimitClause::LimitOffset { limit, offset, .. }) => (
            parse_expr_as_usize(offset.as_ref().map(|o| &o.value)).unwrap_or(0),
            limit.as_ref().and_then(|e| parse_expr_as_usize(Some(e))),
        ),
        Some(LimitClause::OffsetCommaLimit { offset, limit }) => (
            parse_expr_as_usize(Some(offset)).unwrap_or(0),
            parse_expr_as_usize(Some(limit)),
        ),
        None => (0, None),
    };
    let s = off.min(rows.len());
    let e = match lim {
        Some(l) => (s + l).min(rows.len()),
        None => rows.len(),
    };
    rows = rows[s..e].to_vec();

    // Format
    let h = header
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect::<Vec<_>>()
        .join(",");
    let rj: Vec<String> = rows
        .iter()
        .map(|r| {
            let c: Vec<String> = r.iter().map(|v| v.to_json_string()).collect();
            format!("[{}]", c.join(","))
        })
        .collect();
    for name in &view_tables {
        let _ = db.drop_table(name);
    }
    Ok(format!("[[{}],{}]", h, rj.join(",")))
}

fn eval_expr_on_flat_row(expr: &Expr, row: &[DbValue], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    match expr {
        Expr::Identifier(ident) => {
            let name = ident.value.to_lowercase();
            if name == "current_timestamp" || name == "current_time" {
                return Ok(now_value());
            }
            if name == "current_date" {
                return Ok(curdate_value());
            }
            match col_map.get(&name) {
                Some(&pos) => Ok(row[pos].clone()),
                None => Err(format!("Unknown column '{}'", name)),
            }
        }
        Expr::CompoundIdentifier(parts) => {
            // e.g. a.id → "a.id"
            let name = parts
                .iter()
                .map(|p| p.value.to_lowercase())
                .collect::<Vec<_>>()
                .join(".");
            match col_map.get(&name) {
                Some(&pos) => Ok(row[pos].clone()),
                None => {
                    // Try just the last part
                    let last = parts.last().unwrap().value.to_lowercase();
                    match col_map.get(&last) {
                        Some(&pos) => Ok(row[pos].clone()),
                        None => Err(format!("Unknown column '{}'", name)),
                    }
                }
            }
        }
        Expr::Value(v) => Ok(sql_val_to_db(&v.value)),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr_on_flat_row(left, row, col_map)?;
            let r = eval_expr_on_flat_row(right, row, col_map)?;
            apply_binary_op(&l, op, &r)
        }
        Expr::Nested(inner) => eval_expr_on_flat_row(inner, row, col_map),
        Expr::Function(func) => {
            let name = func.name.to_string().to_lowercase();
            if name == "fuzzy_match" {
                let args = match &func.args {
                    FunctionArguments::List(list) => &list.args,
                    _ => return Err("fuzzy_match requires args".into()),
                };
                if args.len() < 2 {
                    return Err("fuzzy_match requires 2 args".into());
                }
                let a1 = eval_expr_on_flat_row(get_func_arg_unnamed(&args[0])?, row, col_map)?;
                let a2 = eval_expr_on_flat_row(get_func_arg_unnamed(&args[1])?, row, col_map)?;
                let sim = Table::trigram_similarity(&value_to_string(&a1), &value_to_string(&a2));
                Ok(DbValue::Bool(sim >= 0.3))
            } else {
                exec_std_function(func, name, row, col_map)
            }
        }
        Expr::IsNull(expr) => {
            let val = eval_expr_on_flat_row(expr, row, col_map)?;
            Ok(DbValue::Bool(matches!(val, DbValue::Null)))
        }
        Expr::IsNotNull(expr) => {
            let val = eval_expr_on_flat_row(expr, row, col_map)?;
            Ok(DbValue::Bool(!matches!(val, DbValue::Null)))
        }
        Expr::Like {
            negated, expr, pattern, ..
        } => {
            let val = eval_expr_on_flat_row(expr, row, col_map)?;
            let pat = eval_expr_on_flat_row(pattern, row, col_map)?;
            let matched = simple_like(&value_to_string(&val), &value_to_string(&pat));
            Ok(DbValue::Bool(if *negated { !matched } else { matched }))
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_val = operand
                .as_ref()
                .map(|o| eval_expr_on_flat_row(o, row, col_map))
                .transpose()?;
            for cw in conditions.iter() {
                let matched = match &operand_val {
                    Some(ref op_val) => *op_val == eval_expr_on_flat_row(&cw.condition, row, col_map)?,
                    None => is_truthy(&eval_expr_on_flat_row(&cw.condition, row, col_map)?),
                };
                if matched {
                    return eval_expr_on_flat_row(&cw.result, row, col_map);
                }
            }
            match else_result {
                Some(expr) => eval_expr_on_flat_row(expr, row, col_map),
                None => Ok(DbValue::Null),
            }
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let val = eval_expr_on_flat_row(expr, row, col_map)?;
            let l = eval_expr_on_flat_row(low, row, col_map)?;
            let h = eval_expr_on_flat_row(high, row, col_map)?;
            use std::cmp::Ordering;
            let ge = db_value_cmp(&val, &l) != Ordering::Less;
            let le = db_value_cmp(&val, &h) != Ordering::Greater;
            Ok(DbValue::Bool(if *negated { !(ge && le) } else { ge && le }))
        }
        _ => Err(format!("Unsupported expression in JOIN: {:?}", expr)),
    }
}

// ── UNION / Set operations ──────────────────────────────────────────────

pub(crate) fn exec_union(so: &SetExpr, _query: &Query, db: &mut Database) -> Result<String, String> {
    use sqlparser::ast::{SetOperator, SetQuantifier};
    let (left, right, op, is_all) = match so {
        SetExpr::SetOperation {
            left,
            op,
            set_quantifier,
            right,
        } => {
            let is_all = matches!(set_quantifier, SetQuantifier::All);
            (left, right, op, is_all)
        }
        _ => return Err("Expected SetOperation".into()),
    };

    let lq = wrap_setexpr(left);
    let rq = wrap_setexpr(right);
    let lj = exec_select(&lq, db)?;
    let rj = exec_select(&rq, db)?;

    let parse = |s: &str| -> Vec<Vec<serde_json::Value>> { serde_json::from_str(s).unwrap_or_default() };
    let l_rows = parse(&lj);
    let r_rows = parse(&rj);

    // Helper to count row multiplicities for ALL variants
    let row_counts = |rows: &[Vec<serde_json::Value>]| -> Vec<(Vec<serde_json::Value>, usize)> {
        let mut counts: Vec<(Vec<serde_json::Value>, usize)> = Vec::new();
        for row in rows {
            if let Some(pos) = counts.iter().position(|(r, _)| r == row) {
                counts[pos].1 += 1;
            } else {
                counts.push((row.clone(), 1));
            }
        }
        counts
    };

    let all = match op {
        SetOperator::Union if is_all => {
            // UNION ALL — concatenate, no dedup
            let mut rows = l_rows.clone();
            if !rows.is_empty() && !r_rows.is_empty() {
                rows.extend(r_rows[1..].iter().cloned());
            } else {
                rows.extend(r_rows);
            }
            rows
        }
        SetOperator::Union => {
            // UNION DISTINCT — deduplicate across both branches
            let mut rows = Vec::new();
            if !l_rows.is_empty() {
                rows.push(l_rows[0].clone()); // header from left
                for row in &l_rows[1..] {
                    if !rows[1..].contains(row) {
                        rows.push(row.clone());
                    }
                }
                for row in &r_rows[1..] {
                    if !rows[1..].contains(row) {
                        rows.push(row.clone());
                    }
                }
            }
            rows
        }
        SetOperator::Except if is_all => {
            // EXCEPT ALL — remove multiplicities from left
            if l_rows.len() > 1 && r_rows.len() > 1 {
                let header = l_rows[0].clone();
                let mut data: Vec<Vec<serde_json::Value>> = l_rows[1..].to_vec();
                for r_row in &r_rows[1..] {
                    if let Some(pos) = data.iter().position(|d| d == r_row) {
                        data.remove(pos);
                    }
                }
                let mut result = vec![header];
                result.extend(data);
                result
            } else {
                l_rows.clone()
            }
        }
        SetOperator::Except | SetOperator::Minus => {
            // EXCEPT / EXCEPT DISTINCT — rows in left but not in right
            let mut rows = Vec::new();
            if !l_rows.is_empty() {
                rows.push(l_rows[0].clone()); // header from left
                for row in &l_rows[1..] {
                    if !r_rows[1..].contains(row) {
                        rows.push(row.clone());
                    }
                }
            }
            rows
        }
        SetOperator::Intersect if is_all => {
            // INTERSECT ALL — min multiplicity
            let mut rows = Vec::new();
            if !l_rows.is_empty() && r_rows.len() > 1 {
                rows.push(l_rows[0].clone()); // header
                let l_counts = row_counts(&l_rows[1..]);
                let r_counts = row_counts(&r_rows[1..]);
                for (l_row, l_cnt) in &l_counts {
                    if let Some((_, r_cnt)) = r_counts.iter().find(|(r, _)| r == l_row) {
                        let take = (*l_cnt).min(*r_cnt);
                        for _ in 0..take {
                            rows.push(l_row.clone());
                        }
                    }
                }
            }
            rows
        }
        SetOperator::Intersect => {
            // INTERSECT / INTERSECT DISTINCT — rows in both
            let mut rows = Vec::new();
            if !l_rows.is_empty() {
                rows.push(l_rows[0].clone()); // header from left
                for row in &l_rows[1..] {
                    if r_rows[1..].contains(row) && !rows[1..].contains(row) {
                        rows.push(row.clone());
                    }
                }
            }
            rows
        }
    };

    Ok(serde_json::to_string(&all).unwrap_or_else(|_| "[]".into()))
}

/// Wrap a SetExpr into a minimal Query for exec_select.
fn wrap_setexpr(expr: &SetExpr) -> Query {
    Query {
        with: None,
        body: Box::new(expr.clone()),
        order_by: None,
        limit_clause: None,
        fetch: None,
        locks: vec![],
        for_clause: None,
        settings: None,
        format_clause: None,
        pipe_operators: vec![],
    }
}
