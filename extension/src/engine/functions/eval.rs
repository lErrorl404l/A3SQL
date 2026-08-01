// Expression evaluation and function dispatch
// ── eval_expr / eval_literal_expr evaluate AST expressions ──
// ── exec_function dispatches to exec_std_function (in builtin.rs) or plugin fns ──

//! Expression evaluator — resolves `Expr` AST nodes against a row context.
//! Handles binary ops, unary ops, CAST, BETWEEN, IN, EXISTS, subqueries,
//! and dispatches function calls to `builtin` or plugin registry.

mod cast;
mod corr;
mod expr;
mod nondet;
pub(crate) mod ops;

// Re-export all pub(crate) items from submodules so they remain accessible
// from the same paths as before the split:
//
//   crate::engine::functions::eval::{eval_expr, apply_binary_op, is_truthy, ...}
pub(crate) use expr::{eval_expr, eval_literal_expr, exec_function};
pub(crate) use ops::{apply_binary_op, is_truthy, to_float};

// Subquery correlation + nondeterminism detection, used by the executor's
// subquery cache (execute/select.rs).
pub(crate) use corr::query_has_from;
pub(crate) use nondet::query_has_nondeterministic;

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test;
