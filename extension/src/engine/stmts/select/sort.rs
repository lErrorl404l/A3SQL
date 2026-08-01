// ORDER BY / LIMIT / OFFSET helpers — extracted from execute.rs

//! ORDER BY / LIMIT / OFFSET — sorting rows and pagination.

use std::collections::HashMap;

use sqlparser::ast::{Expr, LimitClause};

use super::super::super::functions::eval::eval_expr;
use crate::engine::error::EngineError;
use crate::engine::prelude::{DbValue, db_value_cmp};

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
            let ordering = db_value_cmp(&a_val, &b_val);
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
    if let Expr::Value(v) = expr
        && let sqlparser::ast::Value::Number(s, _) = &v.value
    {
        return s.parse::<usize>().ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::ast::{Ident, OrderByExpr, OrderByOptions};

    fn order_on_v(asc: Option<bool>) -> OrderByExpr {
        OrderByExpr {
            expr: Expr::Identifier(Ident::new("v")),
            options: OrderByOptions { asc, nulls_first: None },
            with_fill: None,
        }
    }

    fn v_rows(vals: &[DbValue]) -> (Vec<&[DbValue]>, HashMap<String, usize>) {
        let rows: Vec<&[DbValue]> = vals.iter().map(std::slice::from_ref).collect();
        let col_map = HashMap::from([("v".to_string(), 0usize)]);
        (rows, col_map)
    }

    #[test]
    fn order_by_int_asc_is_numeric_not_lexicographic() {
        // Bug T5 regression: {2,10,100} must sort numerically, not "10"<"100"<"2".
        let (rows, col_map) = v_rows(&[DbValue::Int(100), DbValue::Int(2), DbValue::Int(10)]);
        let sorted = sort_rows(rows, &[order_on_v(Some(true))], &col_map).unwrap();
        assert_eq!(
            sorted,
            vec![&[DbValue::Int(2)], &[DbValue::Int(10)], &[DbValue::Int(100)]]
        );
    }

    #[test]
    fn order_by_int_desc_reverses_numeric() {
        let (rows, col_map) = v_rows(&[DbValue::Int(2), DbValue::Int(100), DbValue::Int(10)]);
        let sorted = sort_rows(rows, &[order_on_v(Some(false))], &col_map).unwrap();
        assert_eq!(
            sorted,
            vec![&[DbValue::Int(100)], &[DbValue::Int(10)], &[DbValue::Int(2)]]
        );
    }

    #[test]
    fn order_by_mixed_int_float_is_numeric() {
        // db_value_cmp compares ints and floats on the same numeric axis.
        let (rows, col_map) = v_rows(&[
            DbValue::Float(2.5),
            DbValue::Int(10),
            DbValue::Int(2),
            DbValue::Float(100.0),
        ]);
        let sorted = sort_rows(rows, &[order_on_v(Some(true))], &col_map).unwrap();
        assert_eq!(
            sorted,
            vec![
                &[DbValue::Int(2)],
                &[DbValue::Float(2.5)],
                &[DbValue::Int(10)],
                &[DbValue::Float(100.0)]
            ]
        );
    }

    #[test]
    fn order_by_null_after_numbers_asc() {
        // db_value_cmp falls back to string compare for NULL → "NULL" > any
        // number string, so NULLs sort after numerics ascending.
        let (rows, col_map) = v_rows(&[DbValue::Null, DbValue::Int(10), DbValue::Int(2)]);
        let sorted = sort_rows(rows, &[order_on_v(Some(true))], &col_map).unwrap();
        assert_eq!(sorted, vec![&[DbValue::Int(2)], &[DbValue::Int(10)], &[DbValue::Null],]);
    }

    #[test]
    fn order_by_defaults_to_asc() {
        // OrderByExpr::from(Ident) leaves asc None → default ASC (numeric).
        let (rows, col_map) = v_rows(&[DbValue::Int(100), DbValue::Int(2)]);
        let sorted = sort_rows(rows, &[OrderByExpr::from(Ident::new("v"))], &col_map).unwrap();
        assert_eq!(sorted, vec![&[DbValue::Int(2)], &[DbValue::Int(100)]]);
    }
}
