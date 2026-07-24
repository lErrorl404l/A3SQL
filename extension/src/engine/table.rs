// a3db table — schema + row storage + CRUD operations

use std::collections::{HashMap, HashSet};

use sqlparser::ast::Expr;

use super::index::{BTreeIndex, IndexMeta, IndexType, TrigramIndex};
use super::value::{Column, ColumnType, DbValue};

/// A foreign key constraint referencing another table's column.
#[derive(Debug, Clone)]
pub struct ForeignKeyInfo {
    pub local_column: String,
    pub foreign_table: String,
    pub foreign_column: String,
    pub on_delete: Option<sqlparser::ast::ReferentialAction>,
    pub on_update: Option<sqlparser::ast::ReferentialAction>,
}

/// Runtime index implementation.
#[derive(Debug, Clone)]
pub enum IndexImpl {
    BTree(BTreeIndex),
    Trigram(TrigramIndex),
}

/// An in-memory table.
#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<DbValue>>,
    /// Maps column name → index for fast lookup.
    pub(crate) col_index: HashMap<String, usize>,
    /// Set of primary key values for uniqueness enforcement.
    pub(crate) pk_set: HashSet<String>,
    /// Secondary indices (BTREE, TRIGRAM) created via CREATE INDEX.
    pub(crate) indices: Vec<(IndexMeta, IndexImpl)>,
    /// Next AUTO_INCREMENT counter value.
    pub(crate) next_auto_inc: i64,
    /// CHECK constraint expressions evaluated against each row on INSERT/UPDATE.
    pub(crate) check_constraints: Vec<Expr>,
    /// FOREIGN KEY constraints referencing other tables.
    pub(crate) foreign_keys: Vec<ForeignKeyInfo>,
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
            indices: Vec::new(),
            next_auto_inc: 1,
            check_constraints: Vec::new(),
            foreign_keys: Vec::new(),
        })
    }

    /// Column count.
    pub fn col_count(&self) -> usize {
        self.columns.len()
    }

    /// Get column index by name.
    pub fn col_idx(&self, name: &str) -> Option<usize> {
        self.col_index.get(name).copied()
    }

    /// Check if a column is the primary key.
    fn is_pk_col(&self, idx: usize) -> bool {
        self.columns.get(idx).map(|c| c.primary_key).unwrap_or(false)
    }

    /// Build a string key for a row's PK columns (takes explicit columns ref).
    fn pk_key_static(columns: &[Column], row: &[DbValue]) -> Option<String> {
        let pk_indices: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.primary_key)
            .map(|(i, _)| i)
            .collect();
        if pk_indices.is_empty() {
            return None;
        }
        let parts: Vec<String> = pk_indices.iter().map(|&i| format!("{}", row[i])).collect();
        Some(parts.join("|"))
    }

    /// Build a string key for a row's PK columns.
    pub(crate) fn pk_key(&self, row: &[DbValue]) -> Option<String> {
        Self::pk_key_static(&self.columns, row)
    }

    // ── Row operations ───────────────────────────────────────────────

    /// Insert a row. Returns error if PK constraint violated or type mismatch.
    pub fn insert(&mut self, mut row: Vec<DbValue>) -> Result<(), String> {
        if row.len() != self.columns.len() {
            return Err(format!(
                "Insert: expected {} columns, got {}",
                self.columns.len(),
                row.len()
            ));
        }

        // Coerce, apply defaults, check NOT NULL, check types
        for (i, val) in row.iter_mut().enumerate() {
            // Apply default if value is NULL and a default exists
            if matches!(val, DbValue::Null) {
                if let Some(ref def) = self.columns[i].default {
                    *val = def.clone();
                }
            }

            Self::coerce_value(val, &self.columns[i].dtype);
            if !Self::type_match(&self.columns[i].dtype, val) {
                return Err(format!(
                    "Column '{}' expected {:?}, got {:?}",
                    self.columns[i].name, self.columns[i].dtype, val
                ));
            }

            // NOT NULL check after default + coercion
            if self.columns[i].not_null && matches!(val, DbValue::Null) {
                return Err(format!("Column '{}' cannot be NULL", self.columns[i].name));
            }
        }

        // Check PK uniqueness
        if let Some(key) = self.pk_key(&row) {
            if self.pk_set.contains(&key) {
                return Err(format!("Duplicate primary key value '{}'", key));
            }
            self.pk_set.insert(key);
        }

        let row_idx = self.rows.len();
        self.rows.push(row);

        // Update indices
        self.update_indices_insert(row_idx);

        Ok(())
    }

    /// Update all indices for a newly inserted row.
    fn update_indices_insert(&mut self, row_idx: usize) {
        for (meta, impl_) in &mut self.indices {
            if let Some(col_idx) = self.col_index.get(&meta.column) {
                let val = &self.rows[row_idx][*col_idx];
                match impl_ {
                    IndexImpl::BTree(idx) => idx.insert(row_idx, val),
                    IndexImpl::Trigram(idx) => idx.insert(row_idx, val),
                }
            }
        }
    }

    /// Remove a row from all indices (used before row is deleted).
    fn remove_from_indices(&mut self, row_idx: usize, row: &[DbValue]) {
        for (meta, impl_) in &mut self.indices {
            if let Some(col_idx) = self.col_index.get(&meta.column) {
                if let Some(val) = row.get(*col_idx) {
                    match impl_ {
                        IndexImpl::BTree(idx) => idx.remove(row_idx, val),
                        IndexImpl::Trigram(idx) => idx.remove(row_idx, val),
                    }
                }
            }
        }
    }

    /// Rebuild all indices from scratch (after bulk deletes that shift row indices).
    fn rebuild_indices(&mut self) {
        for (meta, impl_) in &mut self.indices {
            // Clear and rebuild
            let col_idx_opt = self.col_index.get(&meta.column).copied();
            match impl_ {
                IndexImpl::BTree(ref mut idx) => {
                    *idx = BTreeIndex::new(&meta.column);
                    if let Some(ci) = col_idx_opt {
                        for (ri, row) in self.rows.iter().enumerate() {
                            idx.insert(ri, &row[ci]);
                        }
                    }
                }
                IndexImpl::Trigram(ref mut idx) => {
                    *idx = TrigramIndex::new(&meta.column);
                    if let Some(ci) = col_idx_opt {
                        for (ri, row) in self.rows.iter().enumerate() {
                            idx.insert(ri, &row[ci]);
                        }
                    }
                }
            }
        }
    }

    /// Delete a row by primary key value. Returns true if a row was removed.
    pub fn delete_by_pk(&mut self, pk_val: &DbValue) -> bool {
        if let Some(pk_col) = self.columns.iter().position(|c| c.primary_key) {
            let before = self.rows.len();
            self.delete(|row| &row[pk_col] == pk_val);
            before > self.rows.len()
        } else {
            false
        }
    }

    /// Delete rows matching a predicate. Returns count of deleted rows.
    pub fn delete<F>(&mut self, mut predicate: F) -> usize
    where
        F: FnMut(&[DbValue]) -> bool,
    {
        let old_rows = std::mem::take(&mut self.rows);
        let mut deleted = 0usize;

        for (old_idx, row) in old_rows.into_iter().enumerate() {
            if predicate(&row) {
                // Remove PK key
                if let Some(key) = Self::pk_key_static(&self.columns, &row) {
                    self.pk_set.remove(&key);
                }
                // Remove from indices
                self.remove_from_indices(old_idx, &row);
                deleted += 1;
            } else {
                self.rows.push(row);
            }
        }

        // Rebuild indices since row indices have shifted
        // (simplest approach: clear and rebuild)
        self.rebuild_indices();

        deleted
    }

    /// Update rows matching a predicate. `setter` receives a mutable row reference.
    /// Returns count of updated rows.
    pub fn update<F>(&mut self, mut predicate: F, mut setter: impl FnMut(&mut [DbValue])) -> usize
    where
        F: FnMut(&[DbValue]) -> bool,
    {
        let mut count = 0usize;
        for row in &mut self.rows {
            if predicate(row) {
                setter(row);
                count += 1;
            }
        }
        count
    }

    /// Select rows matching a predicate. Returns references to matching rows.
    pub fn select<F>(&self, mut predicate: F) -> Vec<&[DbValue]>
    where
        F: FnMut(&[DbValue]) -> bool,
    {
        let mut result = Vec::new();
        for row in &self.rows {
            if predicate(row.as_slice()) {
                result.push(row.as_slice());
            }
        }
        result
    }

    /// Update a single cell, maintaining indices on the changed column.
    /// Returns the old value.
    pub fn update_cell(&mut self, row_idx: usize, col_idx: usize, mut new_value: DbValue) -> DbValue {
        let col_name = &self.columns[col_idx].name;
        Self::coerce_value(&mut new_value, &self.columns[col_idx].dtype);
        let old_value = std::mem::replace(&mut self.rows[row_idx][col_idx], new_value);

        // Update indices that track this column
        for (meta, impl_) in &mut self.indices {
            if &meta.column == col_name {
                match impl_ {
                    IndexImpl::BTree(idx) => {
                        idx.remove(row_idx, &old_value);
                        idx.insert(row_idx, &self.rows[row_idx][col_idx]);
                    }
                    IndexImpl::Trigram(idx) => {
                        idx.remove(row_idx, &old_value);
                        idx.insert(row_idx, &self.rows[row_idx][col_idx]);
                    }
                }
            }
        }

        old_value
    }

    // ── Schema management ───────────────────────────────────────────

    /// Add a new column to the table. Existing rows get NULL for the new column.
    pub fn add_column(&mut self, name: String, dtype: ColumnType) -> Result<(), String> {
        if self.columns.iter().any(|c| c.name == name) {
            return Err(format!("Column '{}' already exists", name));
        }
        let idx = self.columns.len();
        self.columns.push(Column {
            name: name.clone(),
            dtype,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        });
        self.col_index.insert(name, idx);
        for row in &mut self.rows {
            row.push(DbValue::Null);
        }
        Ok(())
    }

    /// Drop a column from the table. Removes the column and all its data.
    /// Rename a column.
    pub fn rename_column(&mut self, old_name: &str, new_name: &str) -> Result<(), String> {
        let col = self
            .columns
            .iter_mut()
            .find(|c| c.name == old_name)
            .ok_or_else(|| format!("Column '{}' not found", old_name))?;
        col.name = new_name.to_string();
        // Update col_index map
        self.col_index.remove(old_name);
        self.col_index.insert(
            new_name.to_string(),
            self.columns.iter().position(|c| c.name == new_name).unwrap(),
        );
        Ok(())
    }

    /// Truncate — remove all rows.
    pub fn truncate(&mut self) -> Result<(), String> {
        self.rows.clear();
        Ok(())
    }

    pub fn drop_column(&mut self, name: &str) -> Result<(), String> {
        let idx = self
            .columns
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| format!("Column '{}' not found", name))?;
        self.columns.remove(idx);
        for row in &mut self.rows {
            row.remove(idx);
        }
        Ok(())
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

    /// Get the header row (column names).
    pub fn header(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    /// All rows (references).
    pub fn all_rows(&self) -> Vec<&[DbValue]> {
        self.rows.iter().map(|r| r.as_slice()).collect()
    }

    /// Format a result set as a JSON array string: [[header], [row1], [row2], ...]
    pub fn format_result(&self, rows: Vec<&[DbValue]>) -> String {
        let header: Vec<String> = self.columns.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let mut parts = vec![format!("[{}]", header.join(","))];

        for row in rows {
            let cells: Vec<String> = row.iter().map(|v| v.to_json_string()).collect();
            parts.push(format!("[{}]", cells.join(",")));
        }

        format!("[{}]", parts.join(","))
    }

    /// Type-check: can `val` be stored in a column of type `col_type`?
    /// Try to coerce a value to match the expected column type.
    /// Try to coerce a value to match the expected column type.
    /// Returns true if coercion was applied.
    pub fn coerce_value(val: &mut DbValue, col_type: &ColumnType) -> bool {
        match col_type {
            ColumnType::Float => {
                if let DbValue::Int(n) = val {
                    *val = DbValue::Float(*n as f64);
                    true
                } else {
                    false
                }
            }
            ColumnType::Int => {
                if let DbValue::Float(f) = val {
                    *val = DbValue::Int(*f as i64);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn type_match(col_type: &ColumnType, val: &DbValue) -> bool {
        matches!(
            (col_type, val),
            (_, DbValue::Null)
                | (ColumnType::Bool, DbValue::Bool(_))
                | (ColumnType::Int, DbValue::Int(_))
                | (ColumnType::Int, DbValue::Float(_))  // truncation coerced
                | (ColumnType::Float, DbValue::Float(_))
                | (ColumnType::Float, DbValue::Int(_))   // widened to Float
                | (ColumnType::String, DbValue::String(_))
                | (ColumnType::Strings, DbValue::Strings(_))
                | (ColumnType::Floats, DbValue::Floats(_))
        )
    }

    // ── Trigram fuzzy matching ───────────────────────────────────────

    /// Compute trigram Jaccard similarity between two strings.
    /// Used by the fuzzy_match() function.
    pub fn trigram_similarity(a: &str, b: &str) -> f64 {
        let a_tri = trigrams(a);
        let b_tri = trigrams(b);

        if a_tri.is_empty() && b_tri.is_empty() {
            return 1.0;
        }

        let intersection = a_tri.intersection(&b_tri).count();
        let union = a_tri.union(&b_tri).count();
        if union == 0 {
            1.0
        } else {
            intersection as f64 / union as f64
        }
    }
}

/// Generate trigrams from text (shared by Table and TrigramIndex).
pub(crate) fn trigrams(s: &str) -> HashSet<String> {
    let padded = format!("  {}  ", s.to_lowercase());
    let bytes = padded.as_bytes();
    if bytes.len() < 3 {
        let mut set = HashSet::new();
        set.insert(padded);
        return set;
    }
    bytes
        .windows(3)
        .map(|w| String::from_utf8_lossy(w).to_string())
        .collect()
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
                auto_increment: false,
            },
            Column {
                name: "name".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "value".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
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
            auto_increment: false,
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
            auto_increment: false,
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
