// a3sql database — table container + schema management

//! Database state — table, view, config, transaction, and save/load management.
//!
//! [`Database`] is the top-level state container. It owns all tables, views,
//! triggers, sequences, and runtime configuration.

pub(crate) mod save;

use std::collections::HashMap;

use super::table::Table;

/// Active query cursor for iterative fetching (used by cursor commands).
#[derive(Debug, Clone)]
pub(crate) struct CursorState {
    /// The original SQL the cursor was created from.
    pub sql: String,
    /// Current offset (row position).
    pub offset: usize,
    /// Number of rows per fetch.
    pub page_size: usize,
}

/// A stored SQL template with a parameter count (used by prepared statements).
#[derive(Debug, Clone)]
pub(crate) struct PreparedStmt {
    /// SQL template with $1, $2, ... placeholders.
    pub sql: String,
    /// Expected number of arguments.
    pub arg_count: usize,
}

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
    }

    // ── Cursor support ──────────────────────────────────────────────────

    /// Create a new cursor for iterative query fetching.
    pub fn create_cursor(&mut self, name: &str, sql: &str, page_size: usize) {
        self.cursors.insert(
            name.to_lowercase(),
            CursorState {
                sql: sql.to_string(),
                offset: 0,
                page_size,
            },
        );
    }

    /// Drop a cursor by name.
    pub fn drop_cursor(&mut self, name: &str) -> Result<(), String> {
        self.cursors
            .remove(&name.to_lowercase())
            .map(|_| ())
            .ok_or_else(|| format!("Cursor '{}' not found", name))
    }

    // ── Prepared statement support ──────────────────────────────────────

    /// Store a prepared SQL template.
    pub fn prepare(&mut self, name: &str, sql: &str, arg_count: usize) {
        self.prepared.insert(
            name.to_lowercase(),
            PreparedStmt {
                sql: sql.to_string(),
                arg_count,
            },
        );
    }

    /// Remove a prepared statement.
    pub fn drop_prepared(&mut self, name: &str) -> Result<(), String> {
        self.prepared
            .remove(&name.to_lowercase())
            .map(|_| ())
            .ok_or_else(|| format!("Prepared statement '{}' not found", name))
    }

    // ── View support ────────────────────────────────────────────────────

    /// Store a view definition.
    pub fn create_view(&mut self, name: &str, sql: &str) -> Result<(), String> {
        if self.has_table(name) {
            return Err(format!("'{}' is a table name", name));
        }
        if self.views.contains_key(name) {
            return Err(format!("View '{}' already exists", name));
        }
        self.views.insert(name.to_string(), sql.to_string());
        Ok(())
    }

    /// Remove a view definition.
    pub fn drop_view(&mut self, name: &str) -> Result<(), String> {
        if self.views.remove(name).is_none() {
            return Err(format!("View '{}' does not exist", name));
        }
        Ok(())
    }

    /// Get a view's SQL text.
    pub fn get_view(&self, name: &str) -> Option<&String> {
        self.views.get(name)
    }

    /// Check if a view exists.
    pub fn has_view(&self, name: &str) -> bool {
        self.views.contains_key(name)
    }

    /// List all view names.
    pub fn view_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.views.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::value::*;

    fn make_db() -> Database {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::String,
                primary_key: true,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let table = Table::new("items".into(), cols).unwrap();
        db.create_table("items", table).unwrap();
        db
    }

    #[test]
    fn create_and_get() {
        let db = make_db();
        assert!(db.get_table("items").is_ok());
        assert!(db.get_table("nonexistent").is_err());
    }

    #[test]
    fn drop_table() {
        let mut db = make_db();
        db.drop_table("items").unwrap();
        assert!(!db.has_table("items"));
    }

    #[test]
    fn duplicate_table() {
        let mut db = make_db();
        let cols = vec![Column {
            name: "x".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let t2 = Table::new("items".into(), cols).unwrap();
        assert!(db.create_table("items", t2).is_err());
    }

    #[test]
    fn list_tables() {
        let mut db = Database::new();
        let cols = vec![Column {
            name: "x".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        db.create_table("a", Table::new("a".into(), cols.clone()).unwrap())
            .unwrap();
        db.create_table("b", Table::new("b".into(), cols).unwrap()).unwrap();
        assert_eq!(db.table_names(), vec!["a", "b"]);
    }

    // ── View tests ─────────────────────────────────────────────────

    #[test]
    fn create_and_drop_view() {
        let mut db = Database::new();
        db.create_view("myview", "SELECT * FROM t").unwrap();
        assert!(db.has_view("myview"));
        assert_eq!(db.get_view("myview"), Some(&"SELECT * FROM t".to_string()));
        db.drop_view("myview").unwrap();
        assert!(!db.has_view("myview"));
    }

    #[test]
    fn view_duplicate_name() {
        let mut db = Database::new();
        db.create_view("v", "SELECT 1").unwrap();
        assert!(db.create_view("v", "SELECT 2").is_err());
    }

    #[test]
    fn view_table_name_conflict() {
        let mut db = Database::new();
        let cols = vec![Column {
            name: "x".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        db.create_table("t", Table::new("t".into(), cols).unwrap()).unwrap();
        assert!(db.create_view("t", "SELECT 1").is_err());
    }

    #[test]
    fn view_rollback() {
        let mut db = Database::new();
        db.create_view("v", "SELECT 1").unwrap();
        db.begin();
        db.drop_view("v").unwrap();
        assert!(!db.has_view("v"));
        db.rollback().unwrap();
        assert!(db.has_view("v"));
    }

    // ── Transaction tests ─────────────────────────────────────────

    #[test]
    fn begin_commit() {
        let mut db = make_db();
        db.begin();
        let t = db.get_table_mut("items").unwrap();
        t.insert(vec![DbValue::String("x".into()), DbValue::Int(1)]).unwrap();
        db.commit().unwrap();
        assert_eq!(db.get_table("items").unwrap().rows.len(), 1);
    }

    #[test]
    fn begin_rollback() {
        let mut db = make_db();
        db.begin();
        let t = db.get_table_mut("items").unwrap();
        t.insert(vec![DbValue::String("x".into()), DbValue::Int(1)]).unwrap();
        db.rollback().unwrap();
        assert_eq!(db.get_table("items").unwrap().rows.len(), 0);
    }

    #[test]
    fn nested_commit() {
        let mut db = make_db();
        db.begin();
        db.get_table_mut("items")
            .unwrap()
            .insert(vec![DbValue::String("a".into()), DbValue::Int(1)])
            .unwrap();
        db.begin();
        db.get_table_mut("items")
            .unwrap()
            .insert(vec![DbValue::String("b".into()), DbValue::Int(2)])
            .unwrap();
        db.commit().unwrap(); // commit inner
        assert_eq!(db.get_table("items").unwrap().rows.len(), 2);
        db.rollback().unwrap(); // rollback outer
        assert_eq!(db.get_table("items").unwrap().rows.len(), 0);
    }

    #[test]
    fn savepoint_rollback() {
        let mut db = make_db();
        db.get_table_mut("items")
            .unwrap()
            .insert(vec![DbValue::String("a".into()), DbValue::Int(1)])
            .unwrap();
        db.savepoint("sp1");
        db.get_table_mut("items")
            .unwrap()
            .insert(vec![DbValue::String("b".into()), DbValue::Int(2)])
            .unwrap();
        db.rollback_to_savepoint("sp1").unwrap();
        assert_eq!(db.get_table("items").unwrap().rows.len(), 1);
    }
}
