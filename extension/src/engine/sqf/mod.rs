// SQF expression evaluator module
//
// Parses and evaluates SQF expressions in Rust (fast-path, no Arma callbacks).
// Used by the SQF_EVAL() SQL function for row-level expression evaluation.

pub(crate) mod commands;
pub(crate) mod database;
pub(crate) mod eval;
pub(crate) mod lexer;
pub(crate) mod parser;

use std::collections::HashMap;

use crate::engine::value::DbValue;

/// Parse and evaluate an SQF expression string.
///
/// `bindings` maps `_variable` names to DbValues for variable resolution.
///
/// # Errors
/// Returns a description of parse or evaluation failures.
pub fn eval_sqf(expression: &str, bindings: &HashMap<String, DbValue>) -> Result<DbValue, String> {
    let tokens = lexer::tokenize(expression)?;
    let expr = parser::parse(tokens)?;
    eval::eval(&expr, bindings)
}

/// Convenience: evaluate a standalone SQF expression with no variable bindings.
/// Returns DbValue::Null on error (for SQL NULL semantics).
pub fn eval_sqf_or_null(expression: &str) -> DbValue {
    match eval_sqf(expression, &HashMap::new()) {
        Ok(v) => v,
        Err(_) => DbValue::Null,
    }
}
