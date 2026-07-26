// Window function support for SELECT execution

use std::collections::HashMap;

use sqlparser::ast::{Expr, SelectItem, WindowFrame, WindowFrameBound, WindowFrameUnits, WindowType};

use super::super::super::execute::db_value_cmp;
use super::super::super::functions::builtin::extract_func_arg;
use super::super::super::functions::eval::{eval_expr, eval_literal_expr};
use super::super::super::value::DbValue;

pub(super) fn has_window_function(projection: &[SelectItem]) -> bool {
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
pub(super) fn compute_window_functions(
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
