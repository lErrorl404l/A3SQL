// Correlated subquery helpers — rewrite outer references with literal values

use std::collections::HashMap;

use sqlparser::ast::{Expr, Query, SetExpr, TableFactor, Value};
use sqlparser::tokenizer::Span;

use super::super::super::value::DbValue;

// ── Correlated subquery helpers ─────────────────────────────────────

/// Extract table names used in a subquery's FROM clause.
fn subquery_table_names(query: &Query) -> Vec<String> {
    let mut names = Vec::new();
    if let SetExpr::Select(select) = &*query.body {
        for twj in &select.from {
            names.extend(table_names_and_aliases(&twj.relation));
            for j in &twj.joins {
                names.extend(table_names_and_aliases(&j.relation));
            }
        }
    }
    names
}

/// Return the table name and alias (if any) from a TableFactor, lowercased.
fn table_names_and_aliases(factor: &TableFactor) -> Vec<String> {
    match factor {
        TableFactor::Table { name, alias, .. } => {
            let mut result = Vec::new();
            let parts: Vec<String> = name
                .0
                .iter()
                .filter_map(|p| p.as_ident().map(|id| id.value.to_lowercase()))
                .collect();
            if !parts.is_empty() {
                result.push(parts.join("."));
            }
            if let Some(a) = alias {
                result.push(a.name.value.to_lowercase());
            }
            result
        }
        _ => Vec::new(),
    }
}

/// Rewrite a subquery expression by replacing outer-table column references
/// with literal values from the current row (correlated subquery decorrelation).
fn rewrite_outer_refs(expr: &Expr, subq_tables: &[String], row: &[DbValue], col_map: &HashMap<String, usize>) -> Expr {
    match expr {
        Expr::Identifier(ident) => {
            let name = ident.value.to_lowercase();
            // If the column name is NOT a column in any subquery table, it must
            // come from an outer (correlated) table — substitute its current value.
            if col_map.contains_key(&name) && !subq_tables.contains(&name) {
                if let Some(&pos) = col_map.get(&name) {
                    return dbvalue_to_literal_expr(&row[pos]);
                }
            }
            expr.clone()
        }
        Expr::CompoundIdentifier(parts) => {
            let lower: Vec<String> = parts.iter().map(|p| p.value.to_lowercase()).collect();
            if lower.len() >= 2 {
                let table = &lower[0];
                let qualified = lower.join(".");
                if !subq_tables.contains(table) {
                    // Try qualified name first (multi-table/joins), then bare
                    // column name (single-table queries where col_map only has
                    // bare names like "id" not "dept.id").
                    let col = lower[1..].join(".");
                    let pos = col_map.get(&qualified).or_else(|| col_map.get(&col));
                    if let Some(&p) = pos {
                        return dbvalue_to_literal_expr(&row[p]);
                    }
                }
            }
            expr.clone()
        }
        // Recurse into nested expressions
        Expr::Nested(inner) => Expr::Nested(Box::new(rewrite_outer_refs(inner, subq_tables, row, col_map))),
        Expr::BinaryOp { left, op, right } => Expr::BinaryOp {
            left: Box::new(rewrite_outer_refs(left, subq_tables, row, col_map)),
            op: op.clone(),
            right: Box::new(rewrite_outer_refs(right, subq_tables, row, col_map)),
        },
        Expr::UnaryOp { op, expr } => Expr::UnaryOp {
            op: *op,
            expr: Box::new(rewrite_outer_refs(expr, subq_tables, row, col_map)),
        },
        Expr::IsNull(inner) => Expr::IsNull(Box::new(rewrite_outer_refs(inner, subq_tables, row, col_map))),
        Expr::IsNotNull(inner) => Expr::IsNotNull(Box::new(rewrite_outer_refs(inner, subq_tables, row, col_map))),
        Expr::Like {
            negated,
            any,
            expr,
            pattern,
            escape_char,
        } => Expr::Like {
            negated: *negated,
            any: *any,
            expr: Box::new(rewrite_outer_refs(expr, subq_tables, row, col_map)),
            pattern: Box::new(rewrite_outer_refs(pattern, subq_tables, row, col_map)),
            escape_char: escape_char.clone(),
        },
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => Expr::Between {
            expr: Box::new(rewrite_outer_refs(expr, subq_tables, row, col_map)),
            low: Box::new(rewrite_outer_refs(low, subq_tables, row, col_map)),
            high: Box::new(rewrite_outer_refs(high, subq_tables, row, col_map)),
            negated: *negated,
        },
        Expr::InList { expr, list, negated } => Expr::InList {
            expr: Box::new(rewrite_outer_refs(expr, subq_tables, row, col_map)),
            list: list
                .iter()
                .map(|e| rewrite_outer_refs(e, subq_tables, row, col_map))
                .collect(),
            negated: *negated,
        },
        _ => expr.clone(),
    }
}

/// If the subquery is correlated (references outer-table columns), rewrite its
/// WHERE clause with literal values from the current row. Otherwise return a clone.
pub(super) fn rewrite_if_correlated(query: &Query, row: &[DbValue], col_map: &HashMap<String, usize>) -> Query {
    if !is_correlated(query, col_map) {
        return query.clone();
    }
    let subq_tables = subquery_table_names(query);
    let mut q = query.clone();
    if let SetExpr::Select(ref mut s) = &mut *q.body {
        s.selection = s
            .selection
            .as_ref()
            .map(|e| rewrite_outer_refs(e, &subq_tables, row, col_map));
    }
    q
}

/// Convert a DbValue to an sqlparser Expr::Value literal with an empty span.
fn dbvalue_to_literal_expr(v: &DbValue) -> Expr {
    let span = Span::empty();
    match v {
        DbValue::Null => Expr::Value(spanned_val(Value::Null, span)),
        DbValue::Bool(b) => Expr::Value(spanned_val(Value::Boolean(*b), span)),
        DbValue::Int(i) => Expr::Value(spanned_val(Value::Number(i.to_string(), false), span)),
        DbValue::Float(f) => Expr::Value(spanned_val(Value::Number(format!("{}", f), false), span)),
        DbValue::String(s) => Expr::Value(spanned_val(Value::SingleQuotedString(s.clone()), span)),
        DbValue::Strings(v) => {
            let json = serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string());
            Expr::Value(spanned_val(Value::SingleQuotedString(json), span))
        }
        DbValue::Floats(v) => {
            let json = serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string());
            Expr::Value(spanned_val(Value::SingleQuotedString(json), span))
        }
    }
}

fn spanned_val(v: Value, span: Span) -> sqlparser::ast::ValueWithSpan {
    sqlparser::ast::ValueWithSpan { value: v, span }
}

/// Check if a subquery is correlated (references any outer-table columns).
fn is_correlated(query: &Query, col_map: &HashMap<String, usize>) -> bool {
    let subq_tables = subquery_table_names(query);
    if let SetExpr::Select(select) = &*query.body {
        if let Some(selection) = &select.selection {
            return has_outer_refs(selection, &subq_tables, col_map);
        }
    }
    false
}

/// Walk an expression tree checking for column references that are NOT in
/// the subquery's own tables but ARE in the outer col_map.
fn has_outer_refs(expr: &Expr, subq_tables: &[String], col_map: &HashMap<String, usize>) -> bool {
    match expr {
        Expr::Identifier(ident) => {
            // Bare identifiers — any name in col_map might be outer
            let name = ident.value.to_lowercase();
            col_map.contains_key(&name)
        }
        Expr::CompoundIdentifier(parts) => {
            // A table-qualified reference whose table is NOT in the subquery's
            // FROM clause is always an outer reference — regardless of col_map
            // contents (which only has bare column names for single-table queries).
            if parts.len() >= 2 {
                let table = parts[0].value.to_lowercase();
                !subq_tables.contains(&table)
            } else {
                false
            }
        }
        Expr::Nested(inner) => has_outer_refs(inner, subq_tables, col_map),
        Expr::BinaryOp { left, right, .. } => {
            has_outer_refs(left, subq_tables, col_map) || has_outer_refs(right, subq_tables, col_map)
        }
        Expr::UnaryOp { expr, .. } => has_outer_refs(expr, subq_tables, col_map),
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => has_outer_refs(inner, subq_tables, col_map),
        Expr::Like { expr, pattern, .. } => {
            has_outer_refs(expr, subq_tables, col_map) || has_outer_refs(pattern, subq_tables, col_map)
        }
        Expr::Between { expr, low, high, .. } => {
            has_outer_refs(expr, subq_tables, col_map)
                || has_outer_refs(low, subq_tables, col_map)
                || has_outer_refs(high, subq_tables, col_map)
        }
        Expr::InList { expr, list, .. } => {
            has_outer_refs(expr, subq_tables, col_map) || list.iter().any(|e| has_outer_refs(e, subq_tables, col_map))
        }
        _ => false,
    }
}
