// a3sql table schema — column/table structure definitions

//! Table schema types — [`IndexImpl`], [`ForeignKeyInfo`], trigram utilities.

use std::collections::HashSet;

use super::super::index::{BTreeIndex, TrigramIndex};
use super::super::value::{Column, ColumnType, DbValue};
use super::Table;

use crate::engine::error::EngineError;

/// A foreign key constraint referencing another table's column.
#[derive(Debug, Clone)]
pub(crate) struct ForeignKeyInfo {
    pub local_column: String,
    pub foreign_table: String,
    pub foreign_column: String,
    pub on_delete: Option<sqlparser::ast::ReferentialAction>,
    pub on_update: Option<sqlparser::ast::ReferentialAction>,
}

#[derive(Debug, Clone)]
pub(crate) enum IndexImpl {
    BTree(BTreeIndex),
    Trigram(TrigramIndex),
}

impl Table {
    /// Column count.
    pub fn col_count(&self) -> usize {
        self.columns.len()
    }

    /// Get column index by name.
    pub fn col_idx(&self, name: &str) -> Option<usize> {
        self.col_index.get(name).copied()
    }

    /// Build a string key for a row's PK columns (takes explicit columns ref).
    pub(crate) fn pk_key_static(columns: &[Column], row: &[DbValue]) -> Option<String> {
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

    /// Build "col_idx|value" keys for every UNIQUE column in the row.
    pub(crate) fn unique_keys(columns: &[Column], row: &[DbValue]) -> Vec<String> {
        columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.unique)
            .filter_map(|(i, _)| row.get(i).map(|v| format!("{}|{}", i, v)))
            .collect()
    }

    // ── Schema management ───────────────────────────────────────────

    /// Add a new column to the table. Existing rows get NULL for the new column.
    pub fn add_column(&mut self, name: String, dtype: ColumnType) -> Result<(), EngineError> {
        if self.columns.iter().any(|c| c.name == name) {
            return Err(EngineError::ColumnAlreadyExists(name));
        }
        let idx = self.columns.len();
        self.columns.push(Column {
            name: name.clone(),
            dtype,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        });
        self.col_index.insert(name, idx);
        for row in &mut self.rows {
            row.push(DbValue::Null);
        }
        Ok(())
    }

    /// Rename a column.
    pub fn rename_column(&mut self, old_name: &str, new_name: &str) -> Result<(), EngineError> {
        let col = self
            .columns
            .iter_mut()
            .find(|c| c.name == old_name)
            .ok_or_else(|| EngineError::ColumnNotFound(old_name.to_string()))?;
        col.name = new_name.to_string();
        // Update col_index map
        self.col_index.remove(old_name);
        self.col_index.insert(
            new_name.to_string(),
            self.columns
                .iter()
                .position(|c| c.name == new_name)
                .ok_or_else(|| EngineError::Internal("column index not found after rename".into()))?,
        );
        Ok(())
    }

    /// Drop a column from the table. Removes the column and all its data.
    pub fn drop_column(&mut self, name: &str) -> Result<(), EngineError> {
        let idx = self
            .columns
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| EngineError::ColumnNotFound(name.to_string()))?;
        self.columns.remove(idx);
        for row in &mut self.rows {
            row.remove(idx);
        }
        Ok(())
    }

    /// Get the header row (column names).
    #[allow(dead_code, reason = "header accessor not yet used externally")]
    pub fn header(&self) -> Vec<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    // ── Type coercion ─────────────────────────────────────────────────

    /// Try to coerce a value to match the expected column type.
    /// Returns true if coercion was applied.
    pub fn coerce_value(val: &mut DbValue, col_type: &ColumnType) -> bool {
        match col_type {
            ColumnType::Float => match val {
                DbValue::Int(n) => {
                    *val = DbValue::Float(*n as f64);
                    true
                }
                // SQLite affinity: well-formed numeric text coerces to the column type
                DbValue::String(s) => match s.parse::<f64>() {
                    Ok(f) => {
                        *val = DbValue::Float(f);
                        true
                    }
                    Err(_) => false,
                },
                _ => false,
            },
            ColumnType::Int => match val {
                DbValue::Float(f) => {
                    *val = DbValue::Int(*f as i64);
                    true
                }
                // SQLite affinity: well-formed numeric text coerces to the column type
                DbValue::String(s) => match s.parse::<i64>() {
                    Ok(n) => {
                        *val = DbValue::Int(n);
                        true
                    }
                    Err(_) => false,
                },
                _ => false,
            },
            ColumnType::String => match val {
                // SQLite affinity: numeric literals inserted into TEXT columns
                // are stored as text (e.g. Steam IDs arriving via $n substitution)
                DbValue::Int(n) => {
                    *val = DbValue::String(n.to_string());
                    true
                }
                DbValue::Float(f) => {
                    *val = DbValue::String(f.to_string());
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    pub(crate) fn type_match(col_type: &ColumnType, val: &DbValue) -> bool {
        matches!(
            (col_type, val),
            (_, DbValue::Null)
                | (ColumnType::Bool, DbValue::Bool(_))
                | (ColumnType::Int, DbValue::Int(_))
                | (ColumnType::Int, DbValue::Float(_))
                | (ColumnType::Float, DbValue::Float(_))
                | (ColumnType::Float, DbValue::Int(_))
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
