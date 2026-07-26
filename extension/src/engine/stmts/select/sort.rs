// ORDER BY / LIMIT / OFFSET helpers — extracted from execute.rs

//! ORDER BY / LIMIT / OFFSET — sorting rows and pagination.

use std::collections::HashMap;

use sqlparser::ast::{Expr, LimitClause};

use super::super::super::functions::builtin::value_to_string;
use super::super::super::functions::eval::eval_expr;
use crate::engine::error::EngineError;
use crate::engine::prelude::DbValue;

/// ORDER BY sorting
pub(crate) fn sort_rows<'a>(
    mut rows: Vec<&'a [DbValue]>,
    order_by: &[sqlparser::ast::OrderByExpr],
    col_map: &HashMap<String, usize>,
) -> Result<Vec<&'a [DbValue]>, EngineError> {
    if order_by.is_empty() {
        return Ok(rows);
    }

    rows.sort_by(|a, b| {
        for order in order_by {
            let a_val = eval_expr(&order.expr, a, col_map).unwrap_or(DbValue::Null);
            let b_val = eval_expr(&order.expr, b, col_map).unwrap_or(DbValue::Null);
            let ordering = value_to_string(&a_val).cmp(&value_to_string(&b_val));
            let is_asc = order.options.asc.unwrap_or(true);
            let ordering = if is_asc { ordering } else { ordering.reverse() };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        std::cmp::Ordering::Equal
    });

    Ok(rows)
}

/// Apply LIMIT and OFFSET.
pub(crate) fn apply_limit_offset<'a>(
    rows: Vec<&'a [DbValue]>,
    limit_clause: &Option<LimitClause>,
) -> Result<Vec<&'a [DbValue]>, EngineError> {
    let (offset_val, limit_val) = match limit_clause {
        Some(LimitClause::LimitOffset { limit, offset, .. }) => {
            let off = parse_expr_as_usize(offset.as_ref().map(|o| &o.value)).unwrap_or(0);
            let lim = limit.as_ref().and_then(|e| parse_expr_as_usize(Some(e)));
            (off, lim)
        }
        Some(LimitClause::OffsetCommaLimit { offset, limit }) => {
            let off = parse_expr_as_usize(Some(offset)).unwrap_or(0);
            let lim = parse_expr_as_usize(Some(limit));
            (off, lim)
        }
        None => (0, None),
    };

    let start = offset_val.min(rows.len());
    let end = match limit_val {
        Some(l) => (start + l).min(rows.len()),
        None => rows.len(),
    };

    Ok(rows[start..end].to_vec())
}

pub(crate) fn parse_expr_as_usize(expr: Option<&Expr>) -> Option<usize> {
    let expr = expr?;
    if let Expr::Value(v) = expr {
        if let sqlparser::ast::Value::Number(s, _) = &v.value {
            return s.parse::<usize>().ok();
        }
    }
    None
}
