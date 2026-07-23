// a3db database — table container + schema management

use std::collections::HashMap;

use super::table::Table;

/// In-memory database instance with transaction replay capability.
#[derive(Debug, Clone)]
struct Snapshot {
    name: Option<String>,
    tables: HashMap<String, Table>,
}

#[derive(Debug)]
pub struct Database {
    tables: HashMap<String, Table>,
    /// Stack of savepoints for transaction rollback.
    savepoints: Vec<Snapshot>,
}

impl Database {
    pub fn new() -> Self {
        Database {
            tables: HashMap::new(),
            savepoints: Vec::new(),
        }
    }

    // ── Transaction support ──────────────────────────────────────────

    /// Begin a transaction (anonymous savepoint).
    pub fn begin(&mut self) {
        self.savepoints.push(Snapshot {
            name: None,
            tables: self.tables.clone(),
        });
    }

    /// Commit the active transaction — discard the snapshot.
    pub fn commit(&mut self) -> Result<(), String> {
        self.savepoints
            .pop()
            .ok_or("No active transaction".to_string())?;
        Ok(())
    }

    /// Rollback the active transaction — restore the snapshot.
    pub fn rollback(&mut self) -> Result<(), String> {
        let snap = self
            .savepoints
            .pop()
            .ok_or("No active transaction".to_string())?;
        self.tables = snap.tables;
        Ok(())
    }

    /// Create a named savepoint.
    pub fn savepoint(&mut self, name: &str) {
        self.savepoints.push(Snapshot {
            name: Some(name.to_string()),
            tables: self.tables.clone(),
        });
    }

    /// Rollback to a named savepoint (discards all savepoints after it).
    pub fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), String> {
        let pos = self
            .savepoints
            .iter()
            .rposition(|s| s.name.as_deref() == Some(name));
        match pos {
            Some(idx) => {
                let snap = self.savepoints.remove(idx);
                self.tables = snap.tables;
                // Discard all savepoints added after this one
                self.savepoints.truncate(idx);
                Ok(())
            }
            None => Err(format!("Savepoint '{}' not found", name)),
        }
    }

    /// Release (forget) a named savepoint without rolling back.
    pub fn release_savepoint(&mut self, name: &str) -> Result<(), String> {
        let pos = self
            .savepoints
            .iter()
            .rposition(|s| s.name.as_deref() == Some(name));
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
    pub fn has_table(&self, name: &str) -> bool {
        self.tables.contains_key(name)
    }

    /// Clear all tables (for testing / reset).
    pub fn clear(&mut self) {
        self.tables.clear();
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
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::Int,
                primary_key: false,
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
        }];
        db.create_table("a", Table::new("a".into(), cols.clone()).unwrap())
            .unwrap();
        db.create_table("b", Table::new("b".into(), cols).unwrap())
            .unwrap();
        assert_eq!(db.table_names(), vec!["a", "b"]);
    }

    // ── Transaction tests ─────────────────────────────────────────

    #[test]
    fn begin_commit() {
        let mut db = make_db();
        db.begin();
        let t = db.get_table_mut("items").unwrap();
        t.insert(vec![DbValue::String("x".into()), DbValue::Int(1)])
            .unwrap();
        db.commit().unwrap();
        assert_eq!(db.get_table("items").unwrap().rows.len(), 1);
    }

    #[test]
    fn begin_rollback() {
        let mut db = make_db();
        db.begin();
        let t = db.get_table_mut("items").unwrap();
        t.insert(vec![DbValue::String("x".into()), DbValue::Int(1)])
            .unwrap();
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
