// a3sql engine — in-memory database engine

//! Engine core — in-memory database, statement execution, and plugin system.
//!
//! The engine is the heart of a3sql. It manages schemas, indexes, triggers,
//! and executes SQL statements against an in-memory database. The main entry
//! point is [`execute()`].

pub(crate) mod database;
pub(crate) mod error;
pub(crate) mod execute;
pub(crate) mod functions;
pub(crate) mod index;
pub(crate) mod optimizer;
pub(crate) mod plugin;
pub(crate) mod prelude;
pub(crate) mod serialize;
pub(crate) mod stmts;
pub(crate) mod table;
#[cfg(test)]
pub(crate) mod test;
pub(crate) mod trigger;
pub(crate) mod value;

pub(crate) use database::Database;
pub(crate) use execute::execute;
