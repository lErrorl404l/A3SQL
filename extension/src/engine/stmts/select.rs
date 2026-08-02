// SELECT, UNION, and related execution — extracted from execute.rs

//! SELECT execution — FROM, WHERE, GROUP BY, HAVING, DISTINCT, wildcards.
//! Delegates to sub-modules for JOINs, CTEs, UNION, ORDER BY/LIMIT, window functions.

use sqlparser::ast::Query;

use super::super::database::Database;
use crate::engine::error::EngineError;

pub(crate) mod cte;
pub(crate) mod derived;
pub(crate) mod joins;
pub(crate) mod sort;
pub(crate) mod union;
pub(crate) mod window;

/// Execute a SELECT query. Thin delegation: the single source of truth is the
/// executor's `execute::select::exec_select`. This module previously carried a
/// near-duplicate implementation (FROM-less WHERE evaluation, projection
/// formatting, subquery snapshotting) that had diverged from the main path —
/// UNION branches (union.rs), CTAS (ddl/create.rs), and INSERT...SELECT
/// (insert.rs) route through here and now inherit the main path's behavior,
/// including `SELECT 1 WHERE 1=0` returning zero rows.
pub(crate) fn exec_select(query: &Query, db: &mut Database) -> Result<String, EngineError> {
    super::super::execute::select::exec_select(query, db)
}

pub(crate) use self::union::exec_union;
