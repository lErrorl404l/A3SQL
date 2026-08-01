// Nondeterministic-call detection (subquery cache soundness)

// A subquery whose AST contains a clock/random call evaluates to a different
// value on every row even though the AST — and therefore the Debug-format
// cache key — is structurally identical. Caching such a result would freeze
// one value for the whole statement, so exec_subquery must skip BOTH the
// lookup and the insert for these. Detection must be a superset of the
// functions that actually read the clock (a false positive only costs a cache
// miss; a false negative freezes a value).

use sqlparser::ast::{
    Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, GroupByExpr, Query, SelectItem, SetExpr,
};

/// Is this function call nondeterministic (reads the clock or random source)?
fn func_is_nondeterministic(f: &Function) -> bool {
    let name = f.name.to_string().to_lowercase();
    let core = match name.as_str() {
        // datetime/date/time/strftime require 'now' as their base timeval
        // (datetime_from_args rejects anything else); now/curdate/current_*
        // are clock aliases; random() derives from the wall clock + pid.
        "random" | "now" | "curdate" | "current_timestamp" | "current_time" | "current_date" | "datetime" | "date"
        | "time" | "strftime" => true,
        // unix_timestamp() with no args reads the clock; with a string arg it
        // parses a fixed date → deterministic.
        "unix_timestamp" => match &f.args {
            FunctionArguments::None => true,
            FunctionArguments::List(l) => l.args.is_empty(),
            _ => false,
        },
        _ => false,
    };
    core || match &f.args {
        FunctionArguments::List(list) => list.args.iter().any(|a| match a {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => expr_has_nondeterministic(e),
            _ => false,
        }),
        FunctionArguments::Subquery(q) => query_has_nondeterministic(q),
        FunctionArguments::None => false,
    }
}

/// Does any part of a (sub)query AST contain a nondeterministic call?
pub(crate) fn query_has_nondeterministic(query: &Query) -> bool {
    set_expr_has_nondeterministic(&query.body)
        || query
            .with
            .as_ref()
            .is_some_and(|w| w.cte_tables.iter().any(|cte| query_has_nondeterministic(&cte.query)))
        || query.order_by.as_ref().is_some_and(|ob| match &ob.kind {
            sqlparser::ast::OrderByKind::Expressions(exprs) => exprs.iter().any(|e| expr_has_nondeterministic(&e.expr)),
            sqlparser::ast::OrderByKind::All(_) => false,
        })
        || query.limit_clause.as_ref().is_some_and(|lc| match lc {
            sqlparser::ast::LimitClause::LimitOffset { limit, offset, .. } => {
                limit.as_ref().is_some_and(expr_has_nondeterministic)
                    || offset.as_ref().is_some_and(|o| expr_has_nondeterministic(&o.value))
            }
            sqlparser::ast::LimitClause::OffsetCommaLimit { offset, limit } => {
                expr_has_nondeterministic(offset) || expr_has_nondeterministic(limit)
            }
        })
}

fn set_expr_has_nondeterministic(se: &SetExpr) -> bool {
    match se {
        SetExpr::Select(select) => {
            select.projection.iter().any(|item| match item {
                SelectItem::UnnamedExpr(e) | SelectItem::ExprWithAlias { expr: e, .. } => expr_has_nondeterministic(e),
                _ => false,
            }) || select.selection.as_ref().is_some_and(expr_has_nondeterministic)
                || match &select.group_by {
                    GroupByExpr::Expressions(exprs, _) => exprs.iter().any(expr_has_nondeterministic),
                    GroupByExpr::All(_) => false,
                }
                || select.having.as_ref().is_some_and(expr_has_nondeterministic)
        }
        SetExpr::SetOperation { left, right, .. } => {
            set_expr_has_nondeterministic(left) || set_expr_has_nondeterministic(right)
        }
        SetExpr::Query(q) => query_has_nondeterministic(q),
        SetExpr::Values(values) => values
            .rows
            .iter()
            .any(|r| r.content.iter().any(expr_has_nondeterministic)),
        _ => false,
    }
}

/// Walk an expression tree for nondeterministic calls (incl. nested
/// subqueries — a cached outer subquery would freeze an inner random() too).
fn expr_has_nondeterministic(e: &Expr) -> bool {
    match e {
        // Identifier forms: `SELECT current_timestamp` (eval/expr.rs:80-85)
        Expr::Identifier(id) => matches!(
            id.value.to_lowercase().as_str(),
            "current_timestamp" | "current_time" | "current_date"
        ),
        Expr::Function(f) => func_is_nondeterministic(f),
        Expr::Subquery(q) => query_has_nondeterministic(q),
        Expr::Exists { subquery, .. } => query_has_nondeterministic(subquery),
        Expr::InSubquery { subquery, .. } => query_has_nondeterministic(subquery),
        Expr::BinaryOp { left, right, .. } => expr_has_nondeterministic(left) || expr_has_nondeterministic(right),
        Expr::UnaryOp { expr, .. } => expr_has_nondeterministic(expr),
        Expr::Nested(inner) => expr_has_nondeterministic(inner),
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => expr_has_nondeterministic(inner),
        Expr::Like { expr, pattern, .. } => expr_has_nondeterministic(expr) || expr_has_nondeterministic(pattern),
        Expr::InList { expr, list, .. } => {
            expr_has_nondeterministic(expr) || list.iter().any(expr_has_nondeterministic)
        }
        Expr::Between { expr, low, high, .. } => {
            expr_has_nondeterministic(expr) || expr_has_nondeterministic(low) || expr_has_nondeterministic(high)
        }
        Expr::Case {
            operand, conditions, ..
        } => {
            operand.as_ref().is_some_and(|o| expr_has_nondeterministic(o))
                || conditions
                    .iter()
                    .any(|cw| expr_has_nondeterministic(&cw.condition) || expr_has_nondeterministic(&cw.result))
        }
        Expr::Cast { expr, .. } => expr_has_nondeterministic(expr),
        Expr::Substring { expr, .. } => expr_has_nondeterministic(expr),
        _ => false,
    }
}
