// Statement handlers — dispatch and implementation

//! SQL statement handlers — one module per statement type.
//! Sub-modules: `insert`, `update`, `delete`, `merge`, `explain`,
//! `transaction`, `ddl` (CREATE/ALTER/DROP), `select` (SELECT/UNION/JOIN/CTE/WINDOW).

pub(crate) mod ddl;
pub(crate) mod delete;
pub(crate) mod explain;
pub(crate) mod insert;
pub(crate) mod merge;
pub(crate) mod select;
pub(crate) mod transaction;
pub(crate) mod update;
