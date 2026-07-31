// a3sql table — schema + row storage + CRUD operations

//! Table implementation — schema, row storage, CRUD operations, index management.

mod row_ops;
mod schema;

use std::collections::{HashMap, HashSet};

use sqlparser::ast::Expr;

use super::index::{BTreeIndex, IndexMeta, IndexType, TrigramIndex};
use super::trigger::TriggerInfo;
use super::value::{Column, DbValue};

pub(crate) use schema::{trigrams, ForeignKeyInfo, IndexImpl};

/// An in-memory table.
#[derive(Debug, Clone)]
pub(crate) struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<DbValue>>,
    /// Maps column name → index for fast lookup.
    pub(crate) col_index: HashMap<String, usize>,
    /// Set of primary key values for uniqueness enforcement.
    pub(crate) pk_set: HashSet<String>,
    /// PK value → row index, for O(1) UPDATE/DELETE row lookup.
    pub(crate) pk_row_index: HashMap<String, usize>,
    /// Unique-column keys for UNIQUE enforcement: "col_idx|value".
    pub(crate) unique_set: HashSet<String>,
    /// Secondary indices (BTREE, TRIGRAM) created via CREATE INDEX.
    pub(crate) indices: Vec<(IndexMeta, IndexImpl)>,
    /// Next AUTO_INCREMENT counter value.
    pub(crate) next_auto_inc: i64,
    /// CHECK constraint expressions evaluated against each row on INSERT/UPDATE.
    pub(crate) check_constraints: Vec<Expr>,
    /// Foreign key definitions from CREATE TABLE.
    pub(crate) foreign_keys: Vec<ForeignKeyInfo>,
    /// Triggers defined on this table.
    pub(crate) triggers: Vec<TriggerInfo>,
}

impl Table {
    /// Create a new table with the given schema.
    pub fn new(name: String, columns: Vec<Column>) -> Result<Self, String> {
        if columns.is_empty() {
            return Err("Table must have at least one column".into());
        }
        let mut col_index = HashMap::new();
        for (i, col) in columns.iter().enumerate() {
            if col_index.contains_key(&col.name) {
                return Err(format!("Duplicate column '{}'", col.name));
            }
            col_index.insert(col.name.clone(), i);
        }
        Ok(Table {
            name,
            columns,
            rows: Vec::new(),
            col_index,
            pk_set: HashSet::new(),
            pk_row_index: HashMap::new(),
            unique_set: HashSet::new(),
            indices: Vec::new(),
            next_auto_inc: 1,
            check_constraints: Vec::new(),
            foreign_keys: Vec::new(),
            triggers: Vec::new(),
        })
    }

    /// Rebuild pk_set and secondary indices after row mutation.
    pub fn rebuild_index(&mut self) {
        self.pk_set.clear();
        self.pk_row_index.clear();
        self.unique_set.clear();
        for (idx, row) in self.rows.iter().enumerate() {
            if let Some(key) = self.pk_key(row) {
                self.pk_set.insert(key.clone());
                self.pk_row_index.insert(key, idx);
            }
            for key in Self::unique_keys(&self.columns, row) {
                self.unique_set.insert(key);
            }
        }
        self.rebuild_indices();
    }

    // ── Index management ────────────────────────────────────────────

    /// Create a secondary index on a column.
    pub fn create_index(&mut self, name: &str, column: &str, index_type: IndexType) -> Result<(), String> {
        if !self.col_index.contains_key(column) {
            return Err(format!("Column '{}' does not exist in table '{}'", column, self.name));
        }
        for (existing, _) in &self.indices {
            if existing.name == name {
                return Err(format!("Index '{}' already exists", name));
            }
        }

        let meta = IndexMeta {
            name: name.to_string(),
            table: self.name.clone(),
            column: column.to_string(),
            index_type,
        };

        let impl_ = match index_type {
            IndexType::BTree => IndexImpl::BTree(BTreeIndex::new(column)),
            IndexType::Trigram => IndexImpl::Trigram(TrigramIndex::new(column)),
        };

        // Populate with existing data
        let col_idx = self.col_index[column];
        let mut impl_ = impl_;
        for (ri, row) in self.rows.iter().enumerate() {
            match &mut impl_ {
                IndexImpl::BTree(idx) => idx.insert(ri, &row[col_idx]),
                IndexImpl::Trigram(idx) => idx.insert(ri, &row[col_idx]),
            }
        }

        self.indices.push((meta, impl_));
        Ok(())
    }

    /// Drop an index by name.
    pub fn drop_index(&mut self, name: &str) -> Result<(), String> {
        let pos = self.indices.iter().position(|(m, _)| m.name == name);
        match pos {
            Some(idx) => {
                self.indices.remove(idx);
                Ok(())
            }
            None => Err(format!("Index '{}' does not exist", name)),
        }
    }

    /// Look up row indices via BTreeIndex for an exact match on a column.
    /// Returns `Some(row_indices)` if a BTreeIndex exists on the column, `None` otherwise.
    pub fn btree_lookup(&self, column: &str, value: &DbValue) -> Option<Vec<usize>> {
        for (_, impl_) in &self.indices {
            if let IndexImpl::BTree(ref idx) = impl_ {
                if idx.column() == column {
                    return Some(idx.lookup(value));
                }
            }
        }
        None
    }

    /// Check if an index with the given name exists.
    pub fn has_index(&self, name: &str) -> bool {
        self.indices.iter().any(|(m, _)| m.name == name)
    }

    /// Get the index implementation for a column, by type.
    pub fn find_index(&self, column: &str, index_type: IndexType) -> Option<&IndexImpl> {
        self.indices
            .iter()
            .find(|(m, _)| m.column == column && m.index_type == index_type)
            .map(|(_, impl_)| impl_)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::value::*;

    fn make_test_table() -> Table {
        let cols = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::String,
                primary_key: true,
                not_null: false,
                default: None,
                default_expr: None,
                auto_increment: false,
                unique: false,
            },
            Column {
                name: "name".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                default_expr: None,
                auto_increment: false,
                unique: false,
            },
            Column {
                name: "value".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                default_expr: None,
                auto_increment: false,
                unique: false,
            },
        ];
        let mut t = Table::new("test".into(), cols).unwrap();
        t.insert(vec![
            DbValue::String("a".into()),
            DbValue::String("alpha".into()),
            DbValue::Int(10),
        ])
        .unwrap();
        t.insert(vec![
            DbValue::String("b".into()),
            DbValue::String("beta".into()),
            DbValue::Int(20),
        ])
        .unwrap();
        t
    }

    #[test]
    fn create_table() {
        let cols = vec![Column {
            name: "x".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        }];
        assert!(Table::new("t".into(), cols).is_ok());
    }

    #[test]
    fn insert_and_select() {
        let t = make_test_table();
        assert_eq!(t.rows.len(), 2);
    }

    #[test]
    fn pk_uniqueness() {
        let cols = vec![Column {
            name: "id".into(),
            dtype: ColumnType::String,
            primary_key: true,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        }];
        let mut t = Table::new("t".into(), cols).unwrap();
        t.insert(vec![DbValue::String("x".into())]).unwrap();
        let r2 = t.insert(vec![DbValue::String("x".into())]);
        assert!(r2.is_err());
    }

    #[test]
    fn delete_matching() {
        let mut t = make_test_table();
        let n = t.delete(|row| row[0] == DbValue::String("a".into()));
        assert_eq!(n, 1);
        assert_eq!(t.rows.len(), 1);
    }

    #[test]
    fn trigram_similarity_identical() {
        let sim = Table::trigram_similarity("hello", "hello");
        assert!((sim - 1.0).abs() < 0.01);
    }

    #[test]
    fn trigram_similarity_partial() {
        let sim = Table::trigram_similarity("rhs_m4a1_carryhandle", "rhs_m4");
        assert!(sim > 0.0 && sim < 1.0);
    }

    #[test]
    fn trigram_similarity_unrelated() {
        let sim = Table::trigram_similarity("abc", "xyz");
        assert!(sim < 0.1);
    }
}
