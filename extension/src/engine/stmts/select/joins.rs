// JOIN-related functions for SELECT execution

//! JOIN execution — INNER, LEFT, RIGHT, FULL, CROSS JOINs.
//! Also handles table resolution and multi-table FROM clauses.

use std::collections::HashMap;

use sqlparser::ast::{
    Expr, FunctionArguments, LimitClause, OrderByKind, Query, Select, SelectItem, TableFactor, TableWithJoins,
};

use super::super::super::functions::builtin::{curdate_value, exec_std_function, now_value, simple_like, values_equal};
use super::sort::parse_expr_as_usize;
use crate::engine::error::EngineError;
use crate::engine::prelude::*;

/// Resolve a `TableWithJoins` reference to a simple table name.
pub(crate) fn resolve_table_from_joins(tj: &TableWithJoins) -> Result<String, EngineError> {
    match &tj.relation {
        TableFactor::Table { name, .. } => Ok(object_name_str(name)),
        _ => Err(EngineError::Exec("Only simple table references supported".into())),
    }
}

/// Check if the FROM clause has multiple tables or JOINs.
pub(crate) fn has_multiple_tables(select: &Select) -> bool {
    select.from.len() > 1 || select.from.iter().any(|t| !t.joins.is_empty())
}

/// Execute a SELECT with JOINs. Uses a flat-row column map with absolute positions.
pub(crate) fn exec_select_joins(query: &Query, select: &Select, db: &mut Database) -> Result<String, EngineError> {
    use sqlparser::ast::{JoinConstraint, JoinOperator};

    // ── Resolve all tables in FROM + JOINs ──────────────────────────
    /// Extract alias name from a TableFactor (tracks aliased self-joins).
    fn table_alias(factor: &TableFactor) -> Option<String> {
        match factor {
            TableFactor::Table { alias, .. } => alias.as_ref().map(|a| a.name.value.to_lowercase()),
            _ => None,
        }
    }

    struct Tbl {
        name: String,          // actual table name
        alias: Option<String>, // optional user-supplied alias
        cols: usize,
        start: usize,
        rows: Vec<Vec<DbValue>>,
    }

    let mut tbls: Vec<Tbl> = Vec::new();
    let mut abs: usize = 0;
    let mut view_tables: Vec<String> = Vec::new();

    for twj in &select.from {
        let (n, t) = resolve_table_factor(&twj.relation, db)?;
        let a = table_alias(&twj.relation);
        if db.has_view(&n) {
            view_tables.push(n.clone());
        }
        let r: Vec<Vec<DbValue>> = t.rows.to_vec();
        let c = t.columns.len();
        tbls.push(Tbl {
            name: n.clone(),
            alias: a,
            cols: c,
            start: abs,
            rows: r,
        });
        abs += c;
        for j in &twj.joins {
            let (jn, jt) = resolve_table_factor(&j.relation, db)?;
            let ja = table_alias(&j.relation);
            if db.has_view(&jn) {
                view_tables.push(jn.clone());
            }
            let jr: Vec<Vec<DbValue>> = jt.rows.to_vec();
            let jc = jt.columns.len();
            tbls.push(Tbl {
                name: jn.clone(),
                alias: ja,
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
        let qualifier = tbl.alias.as_deref().unwrap_or(&tbl.name);
        let tn = db
            .get_table(&tbl.name)
            .map_err(|_| EngineError::TableNotFound(tbl.name.clone()))?
            .clone();
        for (ci, col) in tn.columns.iter().enumerate() {
            let p = tbl.start + ci;
            col_map.insert(format!("{}.{}", qualifier, col.name), p);
            col_map.insert(col.name.clone(), p);
            header.push(format!("{}.{}", qualifier, col.name));
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

    let ef = |e: &Expr, r: &[DbValue]| -> Result<DbValue, EngineError> { eval_expr_on_flat_row(e, r, &col_map) };

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
                    | JoinOperator::Join(JoinConstraint::Natural)
                    | JoinOperator::Left(JoinConstraint::Natural)
                    | JoinOperator::Right(JoinConstraint::Natural)
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
                | JoinOperator::CrossJoin(c)
                | JoinOperator::Left(c)
                | JoinOperator::Right(c),
            ) => c,
            _ => &no_constraint,
        };
        let is_left = matches!(join_info, Some(JoinOperator::LeftOuter(_) | JoinOperator::Left(_)));
        let is_right = matches!(join_info, Some(JoinOperator::RightOuter(_) | JoinOperator::Right(_)));
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

    // GROUP BY / aggregates over the joined rows. The aggregate machinery
    // (partition_by_group / compute_aggregates) works on `&[DbValue]` slices
    // + a positional col_map, which is exactly the join's flat-row shape.
    if super::super::super::functions::aggregate::has_aggregate(&select.projection) {
        let flat: Vec<&[DbValue]> = rows.iter().map(|r| r.as_slice()).collect();
        let group_partitions = if super::super::super::functions::aggregate::has_group_by(select) {
            super::super::super::functions::aggregate::partition_by_group(&flat, select, &col_map)?
        } else {
            vec![flat] // single group: all rows
        };
        // HAVING — filter partitions after grouping
        let group_partitions: Vec<Vec<&[DbValue]>> = if let Some(having) = &select.having {
            group_partitions
                .into_iter()
                .filter(|group| {
                    if group.is_empty() {
                        return false;
                    }
                    ef(having, group[0]).map(|v| is_truthy(&v)).unwrap_or(false)
                })
                .collect()
        } else {
            group_partitions
        };
        // ORDER BY after GROUP BY — sort the groups
        let group_partitions = if let Some(ob) = &query.order_by {
            match &ob.kind {
                OrderByKind::Expressions(exprs) if !exprs.is_empty() => {
                    super::super::super::functions::aggregate::sort_partitions(group_partitions, exprs, &col_map)
                }
                _ => group_partitions,
            }
        } else {
            group_partitions
        };
        for name in &view_tables {
            let _ = db.drop_table(name);
        }
        return super::super::super::functions::aggregate::compute_aggregates(
            &group_partitions,
            &select.projection,
            &col_map,
        );
    }

    // ORDER BY
    if let Some(ob) = &query.order_by {
        let exs = match &ob.kind {
            OrderByKind::Expressions(e) => e,
            _ => return Err(EngineError::Exec("ORDER BY ALL not supported".into())),
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

    // Format — respect the SELECT projection (only show chosen columns),
    // and emit valid JSON even for an empty result set.
    let is_wildcard = select
        .projection
        .iter()
        .any(|item| matches!(item, SelectItem::Wildcard { .. }));
    let h: Vec<String> = if is_wildcard {
        header.iter().map(|h| format!("\"{}\"", h)).collect()
    } else {
        select
            .projection
            .iter()
            .map(|item| match item {
                SelectItem::UnnamedExpr(expr) => format!("\"{}\"", projection_expr_name(expr)),
                SelectItem::ExprWithAlias { alias, .. } => {
                    format!("\"{}\"", alias.value.to_lowercase())
                }
                SelectItem::Wildcard { .. } => unreachable!(),
                _ => format!("\"{:?}\"", item),
            })
            .collect()
    };
    let rj: Vec<String> = rows
        .iter()
        .map(|r| {
            let c: Vec<String> = if is_wildcard {
                r.iter().map(|v| v.to_json_string()).collect()
            } else {
                select
                    .projection
                    .iter()
                    .filter_map(|item| {
                        let expr = match item {
                            SelectItem::UnnamedExpr(e) => e,
                            SelectItem::ExprWithAlias { expr: e, .. } => e,
                            SelectItem::Wildcard { .. } => return None,
                            _ => return None,
                        };
                        eval_expr_on_flat_row(expr, r, &col_map)
                            .ok()
                            .map(|v| v.to_json_string())
                    })
                    .collect()
            };
            format!("[{}]", c.join(","))
        })
        .collect();
    for name in &view_tables {
        let _ = db.drop_table(name);
    }
    if rj.is_empty() {
        Ok(format!("[{}]", h.join(",")))
    } else {
        Ok(format!("[[{}],{}]", h.join(","), rj.join(",")))
    }
}

fn eval_expr_on_flat_row(
    expr: &Expr,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
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
                None => Err(EngineError::ColumnNotFound(name.clone())),
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
                    let last = parts
                        .last()
                        .ok_or_else(|| EngineError::ColumnNotFound(name.clone()))?
                        .value
                        .to_lowercase();
                    match col_map.get(&last) {
                        Some(&pos) => Ok(row[pos].clone()),
                        None => Err(EngineError::ColumnNotFound(name.clone())),
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
            // Zero-arg "functions" like USER/CURRENT_USER (sqlparser maps the
            // reserved keywords to a bare function call) may actually be a
            // column reference — check col_map before treating as a function.
            if matches!(func.args, FunctionArguments::None) {
                if let Some(&pos) = col_map.get(&name) {
                    return Ok(row[pos].clone());
                }
            }
            if name == "fuzzy_match" {
                let args = match &func.args {
                    FunctionArguments::List(list) => &list.args,
                    _ => return Err(EngineError::Exec("fuzzy_match requires args".into())),
                };
                if args.len() < 2 {
                    return Err(EngineError::Exec("fuzzy_match requires 2 args".into()));
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
        _ => Err(EngineError::Exec(format!("Unsupported expression in JOIN: {:?}", expr))),
    }
}
