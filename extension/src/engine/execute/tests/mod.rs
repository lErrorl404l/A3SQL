// Test modules organized by SQL statement type

pub(crate) mod constraints;
pub(crate) mod dml;
pub(crate) mod edge_cases;
pub(crate) mod explain;
pub(crate) mod foreign_keys;
pub(crate) mod fts;
pub(crate) mod helpers;
pub(crate) mod joins;
pub(crate) mod order_aggregate;
pub(crate) mod parse_cache;
pub(crate) mod proptest_paths;
pub(crate) mod proptest_serialize;
pub(crate) mod transactions;
pub(crate) mod upsert;
