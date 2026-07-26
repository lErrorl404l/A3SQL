// Built-in SQL functions — implementations
// ── New functions go in the appropriate module below ──

//! SQL functions — scalar, aggregate, and expression evaluation.
//! Sub-modules: `builtin` (scalar functions), `aggregate` (window/group-by), `eval` (expression evaluation).

pub(crate) mod aggregate;
pub(crate) mod builtin;
pub(crate) mod eval;
