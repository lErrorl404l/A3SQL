// UNION / INTERSECT / EXCEPT — extracted from execute.rs

//! Set operations — UNION, INTERSECT, EXCEPT (ALL).

use sqlparser::ast::{Query, SetExpr};

use super::exec_select;
use crate::engine::error::EngineError;
use crate::engine::prelude::*;

/// Execute a SetOperation (UNION/INTERSECT/EXCEPT) by executing both branches
/// and combining results.
pub(crate) fn exec_union(so: &SetExpr, _query: &Query, db: &mut Database) -> Result<String, EngineError> {
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
        _ => return Err(EngineError::Exec("Expected SetOperation".into())),
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
