// a3sql — Engine prelude: commonly imported types and functions
//
// Use `use crate::engine::prelude::*;` in engine submodules instead of
// deep `super::super::super::` import chains.

pub(crate) use super::database::Database;
pub(crate) use super::value::{db_value_cmp, json_val_to_dbvalue, Column, ColumnType, DbValue};

pub(crate) use super::table::Table;

// Re-export key execution utilities
pub(crate) use super::execute::format_projected_result;

// Common helper functions
pub(crate) use super::functions::aggregate::projection_expr_name;
pub(crate) use super::functions::builtin::{
    get_func_arg_unnamed, materialize_view, resolve_single_table, resolve_table_factor, sql_val_to_db, try_btree_index,
    try_trigram_index, value_to_string,
};
pub(crate) use super::functions::eval::{apply_binary_op, eval_expr, is_truthy};

// DDL helpers
pub(crate) use super::stmts::ddl::object_name_str;
