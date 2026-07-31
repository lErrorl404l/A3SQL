// a3sql table row operations — insert, update, delete

//! Row-level operations — insert, update, delete, and their trigger/integrity checks.

use super::super::index::{BTreeIndex, TrigramIndex};
use super::super::value::DbValue;
use super::{IndexImpl, Table};

use crate::engine::error::EngineError;

impl Table {
    /// Number of rows in this table.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    // ── Row operations ───────────────────────────────────────────────

    /// Insert a row. Returns error if PK constraint violated or type mismatch.
    pub fn insert(&mut self, mut row: Vec<DbValue>) -> Result<(), EngineError> {
        if row.len() != self.columns.len() {
            return Err(EngineError::Exec(format!(
                "Insert: expected {} columns, got {}",
                self.columns.len(),
                row.len()
            )));
        }

        // Coerce, apply defaults, check NOT NULL, check types
        for (i, val) in row.iter_mut().enumerate() {
            // Apply default if value is NULL and a default exists. A
            // non-literal default (DEFAULT datetime('now')) is evaluated at
            // INSERT time; a literal default is cloned directly.
            if matches!(val, DbValue::Null) {
                if let Some(expr) = &self.columns[i].default_expr {
                    *val = crate::engine::functions::eval::eval_literal_expr(expr)?;
                } else if let Some(ref def) = self.columns[i].default {
                    *val = def.clone();
                }
            }

            Self::coerce_value(val, &self.columns[i].dtype);
            if !Self::type_match(&self.columns[i].dtype, val) {
                return Err(EngineError::TypeError {
                    expected: format!("{:?}", self.columns[i].dtype),
                    actual: format!("{:?}", val),
                });
            }

            // NOT NULL check after default + coercion
            if self.columns[i].not_null && matches!(val, DbValue::Null) {
                return Err(EngineError::Exec(format!(
                    "Column '{}' cannot be NULL",
                    self.columns[i].name
                )));
            }
        }

        // Check PK uniqueness
        let pk = self.pk_key(&row);
        if let Some(ref key) = pk {
            if self.pk_set.contains(key) {
                return Err(EngineError::DuplicateKey(key.clone()));
            }
        }

        // Check UNIQUE columns (before mutating state — a rejected UNIQUE
        // insert must not leave the PK registered)
        let uniq = Self::unique_keys(&self.columns, &row);
        for key in &uniq {
            if self.unique_set.contains(key) {
                return Err(EngineError::DuplicateKey(key.clone()));
            }
        }

        // All constraints pass — commit the keys
        if let Some(key) = pk {
            self.pk_set.insert(key.clone());
            self.pk_row_index.insert(key, self.rows.len());
        }
        for key in uniq {
            self.unique_set.insert(key);
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
    pub(super) fn rebuild_indices(&mut self) {
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

    /// Replace a row in place by primary key (INSERT OR REPLACE semantics).
    /// O(1) via pk_row_index — no Vec::remove shifting. Returns true if the
    /// row existed and was replaced, false if it did not exist.
    pub fn replace_by_pk(&mut self, full_row: Vec<DbValue>) -> Result<bool, EngineError> {
        let Some(pk_col) = self.columns.iter().position(|c| c.primary_key) else {
            return Err(EngineError::Exec("REPLACE requires a primary key column".into()));
        };
        let pk_val = &full_row[pk_col];
        let mut key_row: Vec<DbValue> = (0..self.columns.len()).map(|_| DbValue::Null).collect();
        key_row[pk_col] = pk_val.clone();
        if let Some(key) = self.pk_key(&key_row) {
            if let Some(&idx) = self.pk_row_index.get(&key) {
                // Remove old UNIQUE keys for the replaced row, then re-add
                // for the new values (PK is unchanged so pk maps stay valid)
                for ukey in Self::unique_keys(&self.columns, &self.rows[idx]) {
                    self.unique_set.remove(&ukey);
                }
                self.rows[idx] = full_row;
                self.unique_set
                    .extend(Self::unique_keys(&self.columns, &self.rows[idx]));
                return Ok(true);
            }
        }
        // Row didn't exist — normal insert (validates + maintains indexes)
        self.insert(full_row)?;
        Ok(false)
    }

    /// Find the row index for a primary key value (O(1) via pk_row_index).
    pub fn find_by_pk(&self, pk_val: &DbValue) -> Option<usize> {
        if let Some(pk_col) = self.columns.iter().position(|c| c.primary_key) {
            let mut key_row: Vec<DbValue> = (0..self.columns.len()).map(|_| DbValue::Null).collect();
            key_row[pk_col] = pk_val.clone();
            if let Some(key) = self.pk_key(&key_row) {
                return self.pk_row_index.get(&key).copied();
            }
        }
        None
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
                    self.pk_row_index.remove(&key);
                }
                // Remove UNIQUE keys
                for key in Self::unique_keys(&self.columns, &row) {
                    self.unique_set.remove(&key);
                }
                // Remove from indices
                self.remove_from_indices(old_idx, &row);
                deleted += 1;
            } else {
                self.rows.push(row);
            }
        }

        // Rebuild indices since row indices have shifted
        // (simplest approach: clear and rebuild — also refreshes pk_row_index)
        self.rebuild_index();

        deleted
    }

    /// Update rows matching a predicate. `setter` receives a mutable row reference.
    /// Returns count of updated rows.
    #[allow(dead_code, reason = "bulk row update not yet wired into executor")]
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
    #[allow(dead_code, reason = "predicate-based row select not yet wired into executor")]
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
    /// Also updates pk_set if the column is the primary key.
    /// Returns the old value.
    pub fn update_cell(&mut self, row_idx: usize, col_idx: usize, mut new_value: DbValue) -> DbValue {
        let col_name = &self.columns[col_idx].name;
        Self::coerce_value(&mut new_value, &self.columns[col_idx].dtype);

        // If this is a PK column, get the old PK key before swapping
        let old_key = if self.columns[col_idx].primary_key {
            self.pk_key(&self.rows[row_idx])
        } else {
            None
        };

        let old_value = std::mem::replace(&mut self.rows[row_idx][col_idx], new_value);

        // Update pk_set if this is a primary key column
        if self.columns[col_idx].primary_key {
            if let Some(ref ok) = old_key {
                self.pk_set.remove(ok);
                self.pk_row_index.remove(ok);
            }
            if let Some(new_k) = self.pk_key(&self.rows[row_idx]) {
                self.pk_set.insert(new_k.clone());
                self.pk_row_index.insert(new_k, row_idx);
            }
        }

        // Update unique_set if this is a UNIQUE column
        if self.columns[col_idx].unique {
            self.unique_set.remove(&format!("{}|{}", col_idx, old_value));
            self.unique_set
                .insert(format!("{}|{}", col_idx, self.rows[row_idx][col_idx]));
        }

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

    /// Truncate — remove all rows.
    pub fn truncate(&mut self) -> Result<(), EngineError> {
        self.rows.clear();
        self.pk_set.clear();
        self.indices.clear();
        Ok(())
    }

    /// All rows (references).
    #[allow(dead_code, reason = "full row scan not yet wired into executor")]
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
}
