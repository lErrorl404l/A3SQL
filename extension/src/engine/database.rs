// a3sql database — table container + schema management

//! Database state — table, view, config, transaction, and save/load management.
//!
//! [`Database`] is the top-level state container. It owns all tables, views,
//! triggers, sequences, and runtime configuration.

pub(crate) mod cursor;
pub(crate) mod prepared;
pub(crate) mod sql_cache;
pub(crate) mod views;

pub(crate) use cursor::CursorState;
pub(crate) use prepared::PreparedStmt;

use std::collections::HashMap;

use super::table::Table;

#[derive(Debug, Clone)]
struct Snapshot {
    name: Option<String>,
    tables: HashMap<String, Table>,
    views: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Database {
    tables: HashMap<String, Table>,
    /// Stored view definitions (view name → SQL text).
    views: HashMap<String, String>,
    /// Stack of savepoints for transaction rollback.
    savepoints: Vec<Snapshot>,
    /// Last inserted row's primary key value (set after INSERT).
    pub last_insert_rowid: Option<String>,
    /// Number of rows modified by the last statement (INSERT/UPDATE/DELETE).
    pub last_changes: usize,
    /// Session-level configuration (SET / PRAGMA key=value pairs).
    pub config: HashMap<String, String>,
    /// Active query cursors (name → cursor state).
    pub cursors: HashMap<String, CursorState>,
    /// Prepared SQL statements (name → template + arg count).
    pub prepared: HashMap<String, PreparedStmt>,
    /// LRU parse cache: exact SQL text → parsed AST (P1).
    cache: sql_cache::LruSqlCache,
}

impl Database {
    pub fn new() -> Self {
        Database {
            tables: HashMap::new(),
            views: HashMap::new(),
            savepoints: Vec::new(),
            last_insert_rowid: None,
            last_changes: 0,
            config: HashMap::new(),
            cursors: HashMap::new(),
            prepared: HashMap::new(),
            cache: sql_cache::LruSqlCache::new(),
        }
    }

    // ── Transaction support ──────────────────────────────────────────

    /// Begin a transaction (anonymous savepoint).
    pub fn begin(&mut self) {
        self.savepoints.push(Snapshot {
            name: None,
            tables: self.tables.clone(),
            views: self.views.clone(),
        });
    }

    /// Commit the active transaction — discard the snapshot.
    pub fn commit(&mut self) -> Result<(), String> {
        self.savepoints.pop().ok_or("No active transaction".to_string())?;
        Ok(())
    }

    /// Rollback the active transaction — restore the snapshot.
    /// No-op when no transaction is active (matching PostgreSQL behaviour).
    pub fn rollback(&mut self) -> Result<(), String> {
        if let Some(snap) = self.savepoints.pop() {
            self.tables = snap.tables;
            self.views = snap.views;
        }
        Ok(())
    }

    /// Create a named savepoint.
    pub fn savepoint(&mut self, name: &str) {
        self.savepoints.push(Snapshot {
            name: Some(name.to_string()),
            tables: self.tables.clone(),
            views: self.views.clone(),
        });
    }

    /// Rollback to a named savepoint (discards all savepoints after it).
    #[allow(dead_code, reason = "savepoint rollback not yet exposed")]
    pub fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), String> {
        let pos = self.savepoints.iter().rposition(|s| s.name.as_deref() == Some(name));
        match pos {
            Some(idx) => {
                let snap = self.savepoints.remove(idx);
                self.tables = snap.tables;
                self.views = snap.views;
                // Discard all savepoints added after this one
                self.savepoints.truncate(idx);
                Ok(())
            }
            None => Err(format!("Savepoint '{}' not found", name)),
        }
    }

    /// Release (forget) a named savepoint without rolling back.
    pub fn release_savepoint(&mut self, name: &str) -> Result<(), String> {
        let pos = self.savepoints.iter().rposition(|s| s.name.as_deref() == Some(name));
        match pos {
            Some(idx) => {
                self.savepoints.remove(idx);
                Ok(())
            }
            None => Err(format!("Savepoint '{}' not found", name)),
        }
    }

    /// Check if a transaction is active.
    #[allow(dead_code, reason = "transaction state query not yet used externally")]
    pub fn in_transaction(&self) -> bool {
        !self.savepoints.is_empty()
    }

    /// Create a table.
    pub fn create_table(&mut self, name: &str, table: Table) -> Result<(), String> {
        if self.tables.contains_key(name) {
            return Err(format!("Table '{}' already exists", name));
        }
        self.tables.insert(name.to_string(), table);
        Ok(())
    }

    /// Drop a table.
    /// Add a table directly (used by CTE processing).
    pub fn add_table(&mut self, name: String, table: Table) {
        self.tables.insert(name, table);
    }

    pub fn drop_table(&mut self, name: &str) -> Result<(), String> {
        if self.tables.remove(name).is_none() {
            return Err(format!("Table '{}' does not exist", name));
        }
        Ok(())
    }

    /// Get a reference to a table.
    pub fn get_table(&self, name: &str) -> Result<&Table, String> {
        self.tables
            .get(name)
            .ok_or_else(|| format!("Table '{}' does not exist", name))
    }

    /// Get a mutable reference to a table.
    pub fn get_table_mut(&mut self, name: &str) -> Result<&mut Table, String> {
        self.tables
            .get_mut(name)
            .ok_or_else(|| format!("Table '{}' does not exist", name))
    }

    /// List all table names.
    pub fn table_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tables.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Check if a table exists.
    /// Set a runtime config value.
    pub fn set_config(&mut self, key: &str, value: &str) {
        self.config.insert(key.to_lowercase(), value.to_string());
    }

    /// Get a runtime config value.
    #[allow(dead_code, reason = "runtime config getter not yet used externally")]
    pub fn get_config(&self, key: &str) -> Option<&str> {
        self.config.get(key).map(|s| s.as_str())
    }

    pub fn has_table(&self, name: &str) -> bool {
        self.tables.contains_key(name)
    }

    /// Rename a table.
    pub fn rename_table(&mut self, old_name: &str, new_name: &str) -> Result<(), String> {
        let table = self
            .tables
            .remove(old_name)
            .ok_or_else(|| format!("Table '{}' not found", old_name))?;
        self.tables.insert(new_name.to_string(), table);
        Ok(())
    }

    /// Clear all tables and views (for testing / reset).
    pub fn clear(&mut self) {
        self.tables.clear();
        self.views.clear();
        self.cursors.clear();
        self.prepared.clear();
        self.cache.clear();
    }
}

#[cfg(test)]
mod test;
