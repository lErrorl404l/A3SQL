// a3sql — Engine prelude: commonly imported types and functions
//
// Use `use crate::engine::prelude::*;` in engine submodules instead of
// deep `super::super::super::` import chains.

pub(crate) use sqlparser::ast::Expr;

pub(crate) use super::database::Database;
pub(crate) use super::value::{db_value_cmp, json_val_to_dbvalue, Column, ColumnType, DbValue};

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

// DDL helpers
pub(crate) use super::stmts::ddl::object_name_str;
