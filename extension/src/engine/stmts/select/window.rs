// Window function support for SELECT execution

//! Window function execution — ROW_NUMBER, RANK, DENSE_RANK, NTILE,
//! aggregate functions with OVER, ROWS/RANGE/GROUPS frame bounds.

use std::collections::HashMap;

use sqlparser::ast::{Expr, OrderByExpr, SelectItem, WindowFrame, WindowFrameBound, WindowFrameUnits, WindowType};

use super::super::super::functions::builtin::extract_func_arg;
use super::super::super::functions::eval::{eval_expr, eval_literal_expr};
use super::super::super::value::DbValue;
use super::super::super::value::db_value_cmp;
use crate::engine::error::EngineError;

pub(crate) fn has_window_function(projection: &[SelectItem]) -> bool {
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

/// Get the ORDER BY value for a row within the partition (uses the first ORDER BY expr).
fn order_by_value(
    order_by: &[OrderByExpr],
    rows: &[Vec<DbValue>],
    col_map: &HashMap<String, usize>,
    idx: usize,
) -> DbValue {
    order_by
        .first()
        .and_then(|ob| eval_expr(&ob.expr, &rows[idx], col_map).ok())
        .unwrap_or(DbValue::Null)
}

/// Precompute group boundaries within an ordered partition for GROUPS frame mode.
/// Returns a list of `(start, end)` inclusive indices for each peer group.
fn compute_group_bounds(
    order_by: &[OrderByExpr],
    rows: &[Vec<DbValue>],
    col_map: &HashMap<String, usize>,
    part_indices: &[usize],
) -> Vec<(usize, usize)> {
    if order_by.is_empty() || part_indices.is_empty() {
        return vec![(0, part_indices.len().saturating_sub(1))];
    }
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    for pos in 1..=part_indices.len() {
        if pos == part_indices.len()
            || order_by_value(order_by, rows, col_map, part_indices[pos])
                != order_by_value(order_by, rows, col_map, part_indices[start])
        {
            groups.push((start, pos.saturating_sub(1)));
            start = pos;
        }
    }
    groups
}

/// Compute frame bounds for a window function at a given position within an ordered partition.
/// Returns (start, end) inclusive indices into the ordered partition (`part_indices`).
///
/// Supports three frame modes:
/// - `Rows`: physical offsets from current row.
/// - `Range`: logical offsets based on ORDER BY value differences (uses first ORDER BY expr).
/// - `Groups`: peer-group offsets (each distinct ORDER BY value set is one group).
fn frame_bounds(
    frame: &WindowFrame,
    part_indices: &[usize],
    rows: &[Vec<DbValue>],
    order_by: &[OrderByExpr],
    col_map: &HashMap<String, usize>,
    pos: usize,
) -> (usize, usize) {
    let eval_offset = |expr: &Expr| -> usize {
        eval_literal_expr(expr)
            .ok()
            .and_then(|v| match v {
                DbValue::Int(i) => Some(i.max(0) as usize),
                _ => None,
            })
            .unwrap_or(0)
    };
    let part_len = part_indices.len();
    let max_pos = part_len.saturating_sub(1);

    let compute_start_end = |start_bound: &WindowFrameBound, end_bound: Option<&WindowFrameBound>| -> (usize, usize) {
        let start = match start_bound {
            WindowFrameBound::Preceding(None) => 0,
            WindowFrameBound::Preceding(Some(expr)) => pos.saturating_sub(eval_offset(expr)),
            WindowFrameBound::CurrentRow => pos,
            WindowFrameBound::Following(None) => pos,
            WindowFrameBound::Following(Some(expr)) => (pos + eval_offset(expr)).min(max_pos),
        };
        let end = match end_bound {
            Some(WindowFrameBound::Preceding(None)) => 0,
            Some(WindowFrameBound::Preceding(Some(expr))) => pos.saturating_sub(eval_offset(expr)),
            Some(WindowFrameBound::CurrentRow) => pos,
            Some(WindowFrameBound::Following(None)) => max_pos,
            Some(WindowFrameBound::Following(Some(expr))) => (pos + eval_offset(expr)).min(max_pos),
            None => pos,
        };
        (start.min(end), end.max(start))
    };

    match frame.units {
        WindowFrameUnits::Rows => compute_start_end(&frame.start_bound, frame.end_bound.as_ref()),
        WindowFrameUnits::Range => {
            // RANGE requires ORDER BY; fall back to full partition if absent
            if order_by.is_empty() {
                return (0, max_pos);
            }
            let cur_val = order_by_value(order_by, rows, col_map, part_indices[pos]);

            // For RANGE PRECEDING/FOLLOWING, the offset is a numeric value difference,
            // not a row count. We scan for rows whose ORDER BY value is within the offset.
            let find_range_start = |offset: i64| -> usize {
                // Walk backwards from pos until value difference exceeds offset
                let mut s = pos;
                while s > 0 {
                    let v = order_by_value(order_by, rows, col_map, part_indices[s - 1]);
                    if db_value_diff_ge(&cur_val, &v, offset) {
                        break;
                    }
                    s -= 1;
                }
                s
            };
            let find_range_end = |offset: i64| -> usize {
                // Walk forwards from pos until value difference exceeds offset
                let mut e = pos;
                while e < max_pos {
                    let v = order_by_value(order_by, rows, col_map, part_indices[e + 1]);
                    if db_value_diff_ge(&v, &cur_val, offset) {
                        break;
                    }
                    e += 1;
                }
                e
            };

            let start = match &frame.start_bound {
                WindowFrameBound::Preceding(None) => 0,
                WindowFrameBound::Preceding(Some(expr)) => {
                    let offset = eval_offset(expr) as i64;
                    find_range_start(offset)
                }
                WindowFrameBound::CurrentRow => {
                    // CURRENT ROW in RANGE mode = all peers of current row
                    let mut s = pos;
                    while s > 0 {
                        let v = order_by_value(order_by, rows, col_map, part_indices[s - 1]);
                        if v != cur_val {
                            break;
                        }
                        s -= 1;
                    }
                    s
                }
                WindowFrameBound::Following(None) => pos,
                WindowFrameBound::Following(Some(expr)) => {
                    let offset = eval_offset(expr) as i64;
                    // RANGE FOLLOWING as start is unusual but valid per SQL spec
                    let mut s = pos;
                    while s < max_pos {
                        let v = order_by_value(order_by, rows, col_map, part_indices[s + 1]);
                        if db_value_diff_ge(&v, &cur_val, offset) {
                            break;
                        }
                        s += 1;
                    }
                    s
                }
            };

            let end = match &frame.end_bound {
                Some(WindowFrameBound::Preceding(None)) => 0,
                Some(WindowFrameBound::Preceding(Some(expr))) => {
                    let offset = eval_offset(expr) as i64;
                    find_range_start(offset)
                }
                Some(WindowFrameBound::CurrentRow) => {
                    // CURRENT ROW in RANGE mode = all peers of current row
                    let mut e = pos;
                    while e < max_pos {
                        let v = order_by_value(order_by, rows, col_map, part_indices[e + 1]);
                        if v != cur_val {
                            break;
                        }
                        e += 1;
                    }
                    e
                }
                Some(WindowFrameBound::Following(None)) => max_pos,
                Some(WindowFrameBound::Following(Some(expr))) => {
                    let offset = eval_offset(expr) as i64;
                    find_range_end(offset)
                }
                None => pos,
            };

            (start.min(end), end.max(start))
        }
        WindowFrameUnits::Groups => {
            // GROUPS requires ORDER BY; fall back to full partition if absent
            if order_by.is_empty() {
                return (0, max_pos);
            }
            let groups = compute_group_bounds(order_by, rows, col_map, part_indices);

            // Find which group the current position belongs to
            let group_idx = groups.iter().position(|&(s, e)| s <= pos && pos <= e).unwrap_or(0);

            let gs = match &frame.start_bound {
                WindowFrameBound::Preceding(None) => 0,
                WindowFrameBound::Preceding(Some(expr)) => {
                    let n = eval_offset(expr);
                    group_idx.saturating_sub(n)
                }
                WindowFrameBound::CurrentRow => group_idx,
                WindowFrameBound::Following(None) => group_idx,
                WindowFrameBound::Following(Some(expr)) => {
                    let n = eval_offset(expr);
                    (group_idx + n).min(groups.len().saturating_sub(1))
                }
            };
            let ge = match &frame.end_bound {
                Some(WindowFrameBound::Preceding(None)) => 0,
                Some(WindowFrameBound::Preceding(Some(expr))) => {
                    let n = eval_offset(expr);
                    group_idx.saturating_sub(n)
                }
                Some(WindowFrameBound::CurrentRow) => group_idx,
                Some(WindowFrameBound::Following(None)) => groups.len().saturating_sub(1),
                Some(WindowFrameBound::Following(Some(expr))) => {
                    let n = eval_offset(expr);
                    (group_idx + n).min(groups.len().saturating_sub(1))
                }
                None => group_idx,
            };

            let start = groups[gs.min(ge)].0;
            let end = groups[ge.max(gs)].1;
            (start, end)
        }
    }
}

/// Compare two DbValues: returns true if `val` is ≥ `ref_val` + `offset`.
/// Used for RANGE frame bounds with numeric ORDER BY columns.
fn db_value_diff_ge(val: &DbValue, ref_val: &DbValue, offset: i64) -> bool {
    match (val, ref_val) {
        (DbValue::Int(a), DbValue::Int(b)) => *a >= b.saturating_add(offset),
        (DbValue::Float(a), DbValue::Float(b)) => *a >= b + offset as f64,
        (DbValue::Int(a), DbValue::Float(b)) => *a as f64 >= b + offset as f64,
        (DbValue::Float(a), DbValue::Int(b)) => *a >= *b as f64 + offset as f64,
        _ => false,
    }
}

/// Evaluate an aggregate over a frame of rows for window functions.
fn eval_window_aggregate(
    func_name: &str,
    rows: &[&[DbValue]],
    arg: Option<&Expr>,
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
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
            let a = arg.ok_or(EngineError::Exec("SUM requires an argument".into()))?;
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
            let a = arg.ok_or(EngineError::Exec("AVG requires an argument".into()))?;
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
            let a = arg.ok_or(EngineError::Exec("MIN requires an argument".into()))?;
            rows.iter()
                .filter_map(|r| eval_expr(a, r, col_map).ok())
                .min_by(db_value_cmp)
                .ok_or_else(|| EngineError::Exec("MIN on empty frame".into()))
        }
        "max" => {
            let a = arg.ok_or(EngineError::Exec("MAX requires an argument".into()))?;
            rows.iter()
                .filter_map(|r| eval_expr(a, r, col_map).ok())
                .max_by(db_value_cmp)
                .ok_or_else(|| EngineError::Exec("MAX on empty frame".into()))
        }
        _ => Err(EngineError::Exec(format!(
            "Aggregate '{}' not supported as window function",
            func_name
        ))),
    }
}

/// Compute window function values for each row and return them as appended columns.
pub(crate) fn compute_window_functions(
    projection: &[SelectItem],
    rows: &mut [Vec<DbValue>],
    col_map: &HashMap<String, usize>,
) -> Result<(), EngineError> {
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
                            frame_bounds(f, part_indices, rows, &spec.order_by, col_map, pos)
                        } else if !spec.order_by.is_empty() {
                            // SQL default: ORDER BY without a frame clause is
                            // RANGE UNBOUNDED PRECEDING .. CURRENT ROW (running).
                            (0, pos)
                        } else {
                            (0, part_indices.len().saturating_sub(1))
                        };
                        let frame_rows: Vec<&[DbValue]> =
                            part_indices[fs..=fe].iter().map(|&p| rows[p].as_slice()).collect();
                        computed[idx] = eval_window_aggregate(func_name.as_str(), &frame_rows, arg, col_map)?;
                    }
                }
                _ => {
                    return Err(EngineError::Exec(format!(
                        "Window function '{}' not supported",
                        func_name
                    )));
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
