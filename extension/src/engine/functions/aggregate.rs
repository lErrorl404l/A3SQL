// Aggregate and GROUP BY helper functions — moved from execute.rs

//! Aggregate functions — COUNT, SUM, AVG, MIN, MAX, GROUP_CONCAT.
//! Supports GROUP BY, HAVING, DISTINCT, and FILTER (WHERE) clauses.
//! Also computes window function results.

use std::collections::HashMap;

use indexmap::IndexMap;
use sqlparser::ast::{Expr, Function, FunctionArguments, Select, SelectItem};

use super::super::functions::builtin::{extract_func_arg, get_func_arg_unnamed, value_to_string};
use super::super::functions::eval::{eval_expr, eval_literal_expr, is_truthy};
use super::super::value::db_value_cmp;
use super::super::value::{DbValue, GroupKey};
use crate::engine::error::EngineError;

pub(crate) fn has_group_by(select: &Select) -> bool {
    use sqlparser::ast::GroupByExpr;
    match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
        GroupByExpr::All(_) => true,
    }
}

/// Check if SELECT projection contains aggregate functions.
pub(crate) fn has_aggregate(projection: &[SelectItem]) -> bool {
    for item in projection {
        let expr = match item {
            SelectItem::UnnamedExpr(e) => e,
            SelectItem::ExprWithAlias { expr, .. } => expr,
            _ => continue,
        };
        if contains_aggregate(expr) {
            return true;
        }
    }
    false
}

fn contains_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::Function(f) => {
            let name = f.name.to_string().to_lowercase();
            matches!(name.as_str(), "count" | "sum" | "avg" | "min" | "max" | "group_concat")
        }
        Expr::Nested(inner) => contains_aggregate(inner),
        _ => false,
    }
}

/// Resolve GROUP BY identifiers against SELECT aliases.
/// `SELECT qty > 50 AS high_qty ... GROUP BY high_qty` → replaces `high_qty` with `qty > 50`.
fn resolve_group_by_aliases(select: &Select) -> Vec<Expr> {
    let exprs = group_by_exprs(select).unwrap_or(&[]);
    let mut alias_map: HashMap<String, Expr> = HashMap::new();
    for item in &select.projection {
        if let SelectItem::ExprWithAlias { expr, alias } = item {
            alias_map.insert(alias.value.clone(), expr.clone());
        }
    }
    if alias_map.is_empty() {
        return exprs.to_vec();
    }
    exprs
        .iter()
        .map(|e| {
            if let Expr::Identifier(ident) = e {
                if let Some(resolved) = alias_map.get(ident.value.as_str()) {
                    resolved.clone()
                } else {
                    e.clone()
                }
            } else {
                e.clone()
            }
        })
        .collect()
}

/// Partition filtered rows into groups by GROUP BY columns.
/// Returns a Vec of groups, where each group is a Vec of row references.
/// Groups are emitted in first-occurrence order (the order each distinct key
/// first appears), matching the old linear-scan behavior. Bucketing is
/// hash-based (IndexMap keyed by GroupKey) instead of an O(rows × groups)
/// linear search per row.
pub(crate) fn partition_by_group<'a>(
    rows: &[&'a [DbValue]],
    select: &Select,
    col_map: &HashMap<String, usize>,
) -> Result<Vec<Vec<&'a [DbValue]>>, EngineError> {
    let exprs = resolve_group_by_aliases(select);
    // IndexMap: insertion order = first-occurrence order of the keys.
    let mut groups: IndexMap<GroupKey, Vec<&[DbValue]>> = IndexMap::new();

    for row in rows {
        let key: Result<Vec<DbValue>, EngineError> = exprs.iter().map(|e| eval_expr(e, row, col_map)).collect();
        groups.entry(GroupKey(key?)).or_default().push(row);
    }

    Ok(groups.into_values().collect())
}

fn group_by_exprs(select: &Select) -> Result<&[Expr], EngineError> {
    use sqlparser::ast::GroupByExpr;
    match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => Ok(exprs.as_slice()),
        GroupByExpr::All(_) => Err(EngineError::Exec("GROUP BY ALL not supported".into())),
    }
}

/// Sort group partitions by ORDER BY expressions. Group columns evaluate
/// against the group's representative row; aggregate functions (e.g.
/// `ORDER BY COUNT(*)`) evaluate over the whole group.
pub(crate) fn sort_partitions<'a>(
    partitions: Vec<Vec<&'a [DbValue]>>,
    order_by: &[sqlparser::ast::OrderByExpr],
    col_map: &HashMap<String, usize>,
) -> Vec<Vec<&'a [DbValue]>> {
    if order_by.is_empty() {
        return partitions;
    }
    let mut parts = partitions;
    parts.sort_by(|a, b| {
        for order in order_by {
            let a_val = eval_projection_expr(&order.expr, a, col_map)
                .map(|(_, v)| v)
                .unwrap_or(DbValue::Null);
            let b_val = eval_projection_expr(&order.expr, b, col_map)
                .map(|(_, v)| v)
                .unwrap_or(DbValue::Null);
            let ord = db_value_cmp(&a_val, &b_val);
            let ord = if order.options.asc.unwrap_or(true) {
                ord
            } else {
                ord.reverse()
            };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
        std::cmp::Ordering::Equal
    });
    parts
}

/// Compute aggregate functions over partitions (groups) of rows, returning
/// the header names and one row of projected cell values per partition.
/// Empty partitions → empty header AND empty rows (the `[]` result shape).
pub(crate) fn compute_aggregate_rows(
    partitions: &[Vec<&[DbValue]>],
    projection: &[SelectItem],
    col_map: &HashMap<String, usize>,
) -> Result<(Vec<String>, Vec<Vec<DbValue>>), EngineError> {
    if partitions.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    // Build header from projection
    let mut header = Vec::new();
    for item in projection {
        match item {
            SelectItem::UnnamedExpr(expr) => header.push(projection_expr_name(expr)),
            SelectItem::ExprWithAlias { alias, .. } => header.push(alias.value.to_string()),
            _ => return Err(EngineError::Exec("Unsupported SELECT item in aggregate query".into())),
        }
    }

    // Compute one row per partition
    let rows: Vec<Vec<DbValue>> = partitions
        .iter()
        .map(|group| {
            projection
                .iter()
                .map(|item| {
                    let expr = match item {
                        SelectItem::UnnamedExpr(e) => e,
                        SelectItem::ExprWithAlias { expr, .. } => expr,
                        _ => return DbValue::Null,
                    };
                    eval_projection_expr(expr, group, col_map)
                        .map(|(_, v)| v)
                        .unwrap_or(DbValue::Null)
                })
                .collect()
        })
        .collect();

    Ok((header, rows))
}

/// Compute aggregate functions over partitions (groups) of rows, formatted
/// as the JSON result string ([[header], row1, ...], or `[]` when the
/// partition set is empty).
pub(crate) fn compute_aggregates(
    partitions: &[Vec<&[DbValue]>],
    projection: &[SelectItem],
    col_map: &HashMap<String, usize>,
) -> Result<String, EngineError> {
    let (header, rows) = compute_aggregate_rows(partitions, projection, col_map)?;
    if header.is_empty() {
        return Ok("[]".to_string());
    }
    let rows_json: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "[{}]",
                r.iter().map(|v| v.to_json_string()).collect::<Vec<_>>().join(",")
            )
        })
        .collect();
    let header_json: String = header
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!("[[{}],{}]", header_json, rows_json.join(",")))
}

pub(crate) fn projection_expr_name(expr: &Expr) -> String {
    match expr {
        Expr::Function(f) => f.name.to_string().to_uppercase(),
        Expr::Identifier(ident) => ident.value.to_lowercase(),
        Expr::CompoundIdentifier(parts) => parts
            .iter()
            .map(|p| p.value.to_lowercase())
            .collect::<Vec<_>>()
            .join("."),
        _ => "EXPR".to_string(),
    }
}

/// Evaluate a projection expression (handles aggregates vs regular expressions).
fn eval_projection_expr(
    expr: &Expr,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<(String, DbValue), EngineError> {
    // DBG
    match expr {
        Expr::Function(f) => {
            let name = f.name.to_string().to_lowercase();
            match name.as_str() {
                "group_concat" => {
                    let arg = extract_func_arg(f)?;
                    let separator = {
                        // Check for second argument (separator)
                        let sep = match &f.args {
                            sqlparser::ast::FunctionArguments::List(list) if list.args.len() >= 2 => list
                                .args
                                .get(1)
                                .and_then(|a| get_func_arg_unnamed(a).ok())
                                .and_then(|e| eval_literal_expr(e).ok())
                                .map(|v| value_to_string(&v)),
                            _ => None,
                        };
                        sep.unwrap_or_else(|| ",".to_string())
                    };
                    let mut vals: Vec<String> = Vec::new();
                    for r in rows {
                        if !passes_filter(f, r, col_map) {
                            continue;
                        }
                        if let Ok(val) = eval_expr(arg, r, col_map)
                            && !matches!(val, DbValue::Null)
                        {
                            vals.push(value_to_string(&val));
                        }
                    }
                    Ok(("GROUP_CONCAT".to_string(), DbValue::String(vals.join(&separator))))
                }
                "count" => {
                    let is_distinct = matches!(
                        f.args,
                        sqlparser::ast::FunctionArguments::List(ref list)
                            if list.duplicate_treatment == Some(sqlparser::ast::DuplicateTreatment::Distinct)
                    );
                    let count = if is_distinct {
                        let mut seen: Vec<DbValue> = Vec::new();
                        for r in rows {
                            if !passes_filter(f, r, col_map) {
                                continue;
                            }
                            if let Ok(arg) = extract_func_arg(f)
                                && let Ok(val) = eval_expr(arg, r, col_map)
                                && !seen.contains(&val)
                            {
                                seen.push(val);
                            }
                        }
                        DbValue::Int(seen.len() as i64)
                    } else if is_count_star(f) {
                        // COUNT(*) — counts all rows, NULL or not
                        let cnt = rows.iter().filter(|r| passes_filter(f, r, col_map)).count();
                        DbValue::Int(cnt as i64)
                    } else {
                        // COUNT(expr) — counts only non-NULL evaluations
                        let mut cnt = 0usize;
                        for r in rows {
                            if !passes_filter(f, r, col_map) {
                                continue;
                            }
                            if let Ok(arg) = extract_func_arg(f)
                                && let Ok(val) = eval_expr(arg, r, col_map)
                                && val != DbValue::Null
                            {
                                cnt += 1;
                            }
                        }
                        DbValue::Int(cnt as i64)
                    };
                    Ok(("COUNT".to_string(), count))
                }
                "sum" => {
                    let val = aggregate_sum(f, rows, col_map)?;
                    Ok(("SUM".to_string(), val))
                }
                "avg" => {
                    let val = aggregate_avg(f, rows, col_map)?;
                    Ok(("AVG".to_string(), val))
                }
                "min" => {
                    let val = aggregate_min(f, rows, col_map)?;
                    Ok(("MIN".to_string(), val))
                }
                "max" => {
                    let val = aggregate_max(f, rows, col_map)?;
                    Ok(("MAX".to_string(), val))
                }
                _ => {
                    // ponytail: unknown function, evaluate as expression on group
                    let val = eval_expr_on_group(expr, rows, col_map)?;
                    Ok((format!("{}", f.name), val))
                }
            }
        }
        Expr::Identifier(ident) => {
            // Regular column — use first row's value
            let val = if rows.is_empty() {
                DbValue::Null
            } else {
                let idx = col_map
                    .get(&ident.value.to_lowercase())
                    .ok_or_else(|| EngineError::ColumnNotFound(ident.value.clone()))?;
                rows[0][*idx].clone()
            };
            Ok((ident.value.to_lowercase(), val))
        }
        _ => {
            let val = eval_expr_on_group(expr, rows, col_map)?;
            Ok(("expr".to_string(), val))
        }
    }
}

/// Evaluate an expression on a group of rows. For non-aggregate columns, uses first row.
fn eval_expr_on_group(
    expr: &Expr,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    // For aggregate queries, non-aggregate columns use the first row's value
    if rows.is_empty() {
        return Ok(DbValue::Null);
    }
    eval_expr(expr, rows[0], col_map)
}

/// Check if a row passes the aggregate FILTER clause (if any).
/// True for `COUNT(*)` — a List containing a single `Expr::Wildcard`
/// (sqlparser 0.62 has no FunctionArguments::Wildcard variant).
fn is_count_star(func: &Function) -> bool {
    if let sqlparser::ast::FunctionArguments::List(list) = &func.args
        && list.args.len() == 1
        && matches!(
            &list.args[0],
            sqlparser::ast::FunctionArg::Unnamed(sqlparser::ast::FunctionArgExpr::Wildcard)
        )
    {
        return true;
    }
    matches!(func.args, FunctionArguments::None)
}

fn passes_filter(func: &Function, row: &[DbValue], col_map: &HashMap<String, usize>) -> bool {
    func.filter
        .as_ref()
        .is_none_or(|filter_expr| eval_expr(filter_expr, row, col_map).ok().is_none_or(|v| is_truthy(&v)))
}

fn aggregate_sum(
    func: &Function,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    let arg = extract_func_arg(func)?;
    if rows.is_empty() {
        return Ok(DbValue::Null);
    }
    let first = eval_expr_on_group(arg, rows, col_map)?;
    match first {
        DbValue::Int(..) => {
            let sum: i64 = rows
                .iter()
                .filter(|r| passes_filter(func, r, col_map))
                .filter_map(|r| {
                    eval_expr(arg, r, col_map).ok().and_then(|v| match v {
                        DbValue::Int(n) => Some(n),
                        _ => None,
                    })
                })
                .sum();
            Ok(DbValue::Int(sum))
        }
        DbValue::Float(..) => {
            let sum: f64 = rows
                .iter()
                .filter_map(|r| {
                    eval_expr(arg, r, col_map).ok().and_then(|v| match v {
                        DbValue::Float(f) => Some(f),
                        DbValue::Int(n) => Some(n as f64),
                        _ => None,
                    })
                })
                .sum();
            Ok(DbValue::Float(sum))
        }
        _ => Err(EngineError::TypeError {
            expected: "numeric column".into(),
            actual: format!("{:?}", first),
        }),
    }
}

fn aggregate_avg(
    func: &Function,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    let arg = extract_func_arg(func)?;
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for r in rows {
        if !passes_filter(func, r, col_map) {
            continue;
        }
        if let Ok(v) = eval_expr(arg, r, col_map) {
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

fn aggregate_min(
    func: &Function,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    let arg = extract_func_arg(func)?;
    rows.iter()
        .filter(|r| passes_filter(func, r, col_map))
        .filter_map(|r| eval_expr(arg, r, col_map).ok())
        .min_by(db_value_cmp)
        .ok_or_else(|| EngineError::Exec("MIN on empty set".into()))
}

fn aggregate_max(
    func: &Function,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    let arg = extract_func_arg(func)?;
    rows.iter()
        .filter(|r| passes_filter(func, r, col_map))
        .filter_map(|r| eval_expr(arg, r, col_map).ok())
        .max_by(db_value_cmp)
        .ok_or_else(|| EngineError::Exec("MAX on empty set".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::{Ident, OrderByOptions};

    fn order_on_v(asc: Option<bool>) -> sqlparser::ast::OrderByExpr {
        sqlparser::ast::OrderByExpr {
            expr: Expr::Identifier(Ident::new("v")),
            options: OrderByOptions { asc, nulls_first: None },
            with_fill: None,
        }
    }

    /// One partition per group value, like partition_by_group output for
    /// distinct v values — a group's representative row is rows[0].
    fn partitions_of(owned: &[Vec<DbValue>]) -> Vec<Vec<&[DbValue]>> {
        owned.iter().map(|row| vec![row.as_slice()]).collect()
    }

    #[test]
    fn sort_partitions_order_by_int_asc_is_numeric() {
        // Bug T5 regression on the GROUP BY path: {2,10,100} must sort
        // numerically, not "10"<"100"<"2".
        let owned = vec![vec![DbValue::Int(100)], vec![DbValue::Int(2)], vec![DbValue::Int(10)]];
        let parts = partitions_of(&owned);
        let col_map = HashMap::from([("v".to_string(), 0usize)]);
        let sorted = sort_partitions(parts, &[order_on_v(Some(true))], &col_map);
        let vals: Vec<i64> = sorted
            .iter()
            .map(|g| match g[0][0] {
                DbValue::Int(n) => n,
                _ => panic!("int partition"),
            })
            .collect();
        assert_eq!(vals, vec![2, 10, 100]);
    }

    #[test]
    fn sort_partitions_order_by_int_desc_is_numeric() {
        let owned = vec![vec![DbValue::Int(2)], vec![DbValue::Int(100)], vec![DbValue::Int(10)]];
        let parts = partitions_of(&owned);
        let col_map = HashMap::from([("v".to_string(), 0usize)]);
        let sorted = sort_partitions(parts, &[order_on_v(Some(false))], &col_map);
        let vals: Vec<i64> = sorted
            .iter()
            .map(|g| match g[0][0] {
                DbValue::Int(n) => n,
                _ => panic!("int partition"),
            })
            .collect();
        assert_eq!(vals, vec![100, 10, 2]);
    }

    #[test]
    fn sort_partitions_empty_order_by_keeps_order() {
        let owned = vec![vec![DbValue::Int(3)], vec![DbValue::Int(1)], vec![DbValue::Int(2)]];
        let parts = partitions_of(&owned);
        let col_map = HashMap::from([("v".to_string(), 0usize)]);
        let sorted = sort_partitions(parts.clone(), &[], &col_map);
        assert_eq!(sorted, parts, "no ORDER BY → insertion order");
    }
}
