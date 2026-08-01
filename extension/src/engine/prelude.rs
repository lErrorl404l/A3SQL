// a3sql — Engine prelude: commonly imported types and functions
//
// Use `use crate::engine::prelude::*;` in engine submodules instead of
// deep `super::super::super::` import chains.

pub(crate) use sqlparser::ast::{Expr, Query, SetExpr, Statement};

pub(crate) use super::database::Database;
pub(crate) use super::value::{Column, ColumnType, DbValue, db_value_cmp, json_val_to_dbvalue};

pub(crate) use super::table::Table;

// Re-export key execution utilities
pub(crate) use super::execute::format_projected_result;

// Common helper functions
pub(crate) use super::functions::aggregate::projection_expr_name;
pub(crate) use super::functions::builtin::{
    get_func_arg_unnamed, materialize_view, resolve_single_table, resolve_table_factor, sql_val_to_db, try_btree_index,
    try_pk_index, try_trigram_index, value_to_string,
};
pub(crate) use super::functions::eval::{apply_binary_op, eval_expr, is_truthy};

/// Recursively check an expression tree for subqueries (Expr::Subquery /
/// Expr::Exists). The thread-local DB snapshot is only needed when one is
/// present — cloning the whole database on every SELECT is O(total rows).
pub(crate) fn expr_has_subquery(e: &Expr) -> bool {
    match e {
        Expr::Subquery(_) | Expr::Exists { .. } | Expr::InSubquery { .. } => true,
        Expr::BinaryOp { left, right, .. } => expr_has_subquery(left) || expr_has_subquery(right),
        Expr::UnaryOp { expr, .. } => expr_has_subquery(expr),
        Expr::Nested(e) => expr_has_subquery(e),
        Expr::Function(f) => match &f.args {
            sqlparser::ast::FunctionArguments::List(list) => list.args.iter().any(|a| match a {
                sqlparser::ast::FunctionArg::Unnamed(sqlparser::ast::FunctionArgExpr::Expr(e)) => expr_has_subquery(e),
                _ => false,
            }),
            sqlparser::ast::FunctionArguments::Subquery(_) => true,
            sqlparser::ast::FunctionArguments::None => false,
        },
        Expr::InList { expr, list, .. } => expr_has_subquery(expr) || list.iter().any(expr_has_subquery),
        Expr::Between { expr, low, high, .. } => {
            expr_has_subquery(expr) || expr_has_subquery(low) || expr_has_subquery(high)
        }
        Expr::Case {
            operand, conditions, ..
        } => {
            operand.as_ref().is_some_and(|o| expr_has_subquery(o))
                || conditions
                    .iter()
                    .any(|cw| expr_has_subquery(&cw.condition) || expr_has_subquery(&cw.result))
        }
        Expr::Cast { expr, .. } => expr_has_subquery(expr),
        Expr::Substring { expr, .. } => expr_has_subquery(expr),
        _ => false,
    }
}

/// Does a Query AST contain any subquery (incl. CTE bodies, set operations,
/// JOIN ON conditions, ORDER BY / LIMIT expressions)?
pub(crate) fn query_has_subquery(query: &Query) -> bool {
    set_expr_has_subquery(&query.body)
        || query
            .with
            .as_ref()
            .is_some_and(|w| w.cte_tables.iter().any(|cte| query_has_subquery(&cte.query)))
        || query.order_by.as_ref().is_some_and(|ob| match &ob.kind {
            sqlparser::ast::OrderByKind::Expressions(exprs) => exprs.iter().any(|e| expr_has_subquery(&e.expr)),
            sqlparser::ast::OrderByKind::All(_) => false,
        })
        || query.limit_clause.as_ref().is_some_and(|lc| match lc {
            sqlparser::ast::LimitClause::LimitOffset { limit, offset, .. } => {
                limit.as_ref().is_some_and(expr_has_subquery)
                    || offset.as_ref().is_some_and(|o| expr_has_subquery(&o.value))
            }
            sqlparser::ast::LimitClause::OffsetCommaLimit { offset, limit } => {
                expr_has_subquery(offset) || expr_has_subquery(limit)
            }
        })
}

fn set_expr_has_subquery(se: &SetExpr) -> bool {
    match se {
        SetExpr::Select(select) => {
            select.projection.iter().any(|item| match item {
                sqlparser::ast::SelectItem::UnnamedExpr(e)
                | sqlparser::ast::SelectItem::ExprWithAlias { expr: e, .. } => expr_has_subquery(e),
                _ => false,
            }) || select.selection.as_ref().is_some_and(expr_has_subquery)
                || match &select.group_by {
                    sqlparser::ast::GroupByExpr::Expressions(exprs, _) => exprs.iter().any(expr_has_subquery),
                    sqlparser::ast::GroupByExpr::All(_) => false,
                }
                || select.having.as_ref().is_some_and(expr_has_subquery)
                || select.from.iter().any(|twj| {
                    twj.joins.iter().any(|j| match &j.join_operator {
                        sqlparser::ast::JoinOperator::Join(c)
                        | sqlparser::ast::JoinOperator::Inner(c)
                        | sqlparser::ast::JoinOperator::Left(c)
                        | sqlparser::ast::JoinOperator::LeftOuter(c)
                        | sqlparser::ast::JoinOperator::Right(c)
                        | sqlparser::ast::JoinOperator::RightOuter(c) => match c {
                            sqlparser::ast::JoinConstraint::On(e) => expr_has_subquery(e),
                            _ => false,
                        },
                        _ => false,
                    })
                })
        }
        SetExpr::SetOperation { left, right, .. } => set_expr_has_subquery(left) || set_expr_has_subquery(right),
        SetExpr::Query(q) => query_has_subquery(q),
        SetExpr::Values(values) => values.rows.iter().any(|r| r.content.iter().any(expr_has_subquery)),
        _ => false,
    }
}

/// Does a non-Query Statement contain any subquery? Used by the executor to
/// decide whether a fresh SUBQ_DB snapshot must be taken before a DML/DDL
/// statement runs (subqueries in INSERT/UPDATE/DELETE/MERGE values and
/// predicates otherwise read a stale — or missing — snapshot).
pub(crate) fn stmt_has_subquery(stmt: &Statement) -> bool {
    match stmt {
        Statement::Insert(ins) => ins.source.as_ref().is_some_and(|q| query_has_subquery(q)),
        Statement::Update(upd) => {
            upd.assignments.iter().any(|a| expr_has_subquery(&a.value))
                || upd.selection.as_ref().is_some_and(expr_has_subquery)
        }
        Statement::Delete(del) => del.selection.as_ref().is_some_and(expr_has_subquery),
        Statement::Merge(m) => expr_has_subquery(&m.on),
        Statement::CreateView(cv) => query_has_subquery(&cv.query),
        Statement::CreateTable(def) => def.query.as_ref().is_some_and(|q| query_has_subquery(q)),
        Statement::Explain { statement: inner, .. } => stmt_has_subquery(inner),
        _ => false,
    }
}

// DDL helpers
pub(crate) use super::stmts::ddl::object_name_str;
