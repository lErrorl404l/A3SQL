//! SQL parse cache — memoizes text→AST so repeated identical statements skip
//! the sqlparser re-parse. Mods run the SAME queries every frame (CBA settings
//! lookups, leaderboards, ...); previously every dispatch re-parsed the text.
//!
//! Soundness: parsing produces an AST only — sqlparser resolves no tables,
//! columns, or views. The same SQL text always yields the same AST, and
//! execution re-evaluates it against the live [`Database`], so the cache never
//! needs invalidation on INSERT/UPDATE/DELETE/CREATE/DROP. The only
//! requirements are an exact-text key (no normalization), a capacity cap, and
//! clearing on `reset`/`db.clear()`.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use sqlparser::ast::Statement;
use sqlparser::parser::ParserError;

use crate::parser::parse_sql;

/// Maximum number of distinct SQL texts kept in the parse cache.
pub(crate) const DEFAULT_CAPACITY: usize = 512;

/// Bounded insertion-order (FIFO) cache: exact SQL text → parsed AST.
#[derive(Debug, Clone)]
pub(crate) struct LruSqlCache {
    entries: HashMap<String, Arc<Vec<Statement>>>,
    order: VecDeque<String>,
    capacity: usize,
    /// Number of lookups served from the cache (test instrumentation).
    hits: usize,
}

impl LruSqlCache {
    pub(crate) fn new() -> Self {
        LruSqlCache {
            entries: HashMap::new(),
            order: VecDeque::new(),
            capacity: DEFAULT_CAPACITY,
            hits: 0,
        }
    }

    /// Look up `sql`; on miss parse and store it, evicting the oldest entry
    /// when over capacity. Returns an owned `Arc` so callers can execute the
    /// statements without borrowing the cache.
    fn get_or_parse(&mut self, sql: &str) -> Result<Arc<Vec<Statement>>, ParserError> {
        if let Some(arc) = self.entries.get(sql) {
            self.hits += 1;
            return Ok(arc.clone());
        }
        let parsed = Arc::new(parse_sql(sql)?);
        self.insert(sql.to_string(), parsed.clone());
        Ok(parsed)
    }

    fn insert(&mut self, key: String, value: Arc<Vec<Statement>>) {
        if !self.entries.contains_key(&key) {
            if self.entries.len() >= self.capacity
                && let Some(oldest) = self.order.pop_front()
            {
                self.entries.remove(&oldest);
            }
            self.order.push_back(key.clone());
        }
        self.entries.insert(key, value);
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.hits = 0;
    }

    /// `(entry count, hit count)` — exposed for tests.
    #[allow(dead_code, reason = "parse-cache stats — consumed by tests")]
    pub(crate) fn stats(&self) -> (usize, usize) {
        (self.entries.len(), self.hits)
    }
}

impl super::Database {
    /// Parse `sql`, memoizing the AST so identical statements re-parse for
    /// free. The key is the exact text — no normalization.
    pub(crate) fn cached_parse(&mut self, sql: &str) -> Result<Arc<Vec<Statement>>, ParserError> {
        self.cache.get_or_parse(sql)
    }

    /// Parse-cache stats `(entries, hits)` — used by tests.
    #[allow(dead_code, reason = "parse-cache stats — consumed by tests")]
    pub(crate) fn cache_stats(&self) -> (usize, usize) {
        self.cache.stats()
    }
}
