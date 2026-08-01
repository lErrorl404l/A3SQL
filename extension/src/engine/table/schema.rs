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
    ///
    /// Each PK value is encoded as a self-delimiting, length-prefixed part (see
    /// [`encode_part`]) and concatenated without a separator — the length
    /// prefix makes every part boundary unambiguous, so no two distinct rows
    /// can ever share a key, regardless of what bytes the values contain
    /// (quotes, pipes, array commas, multi-byte UTF-8). Case is preserved.
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
        Some(pk_indices.iter().map(|&i| encode_part(&row[i])).collect())
    }

    /// Build a string key for a row's PK columns.
    pub(crate) fn pk_key(&self, row: &[DbValue]) -> Option<String> {
        Self::pk_key_static(&self.columns, row)
    }

    /// Build "col_idx|encoded_value" keys for every UNIQUE column in the row.
    pub(crate) fn unique_keys(columns: &[Column], row: &[DbValue]) -> Vec<String> {
        columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.unique)
            .filter_map(|(i, _)| row.get(i).map(|v| format!("{}|{}", i, encode_part(v))))
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

/// Encode a single `DbValue` as an injective, case-preserving key part:
/// `{type_tag}{byte_len}:{raw_bytes}`.
///
/// The raw bytes are written verbatim (never escaped) — the byte-length
/// prefix is what makes the encoding unambiguous: a reader consumes exactly
/// `len` bytes, so no value can be mistaken for a longer or shorter one, and
/// concatenated parts (see [`Table::pk_key_static`]) parse back
/// deterministically without any separator. `len` is a *byte* length, so
/// multi-byte UTF-8 payloads round-trip exactly.
///
/// Tags: `n` Null, `b` Bool, `i` Int, `f` Float, `s` String, `S` Strings,
/// `F` Floats. Array tags repeat the same `{len}:{bytes}` scheme per element,
/// so `Strings(["a,b","c"])` and `Strings(["a","b,c"])` — which both Display
/// as `[a,b,c]` — get distinct keys.
pub(crate) fn encode_part(v: &DbValue) -> String {
    match v {
        DbValue::Null => "n0:".to_string(),
        DbValue::Bool(true) => "b1:1".to_string(),
        DbValue::Bool(false) => "b1:0".to_string(),
        DbValue::Int(n) => {
            let s = n.to_string();
            format!("i{}:{}", s.len(), s)
        }
        DbValue::Float(f) => {
            let s = f.to_string();
            format!("f{}:{}", s.len(), s)
        }
        DbValue::String(s) => format!("s{}:{}", s.len(), s),
        DbValue::Strings(v) => {
            let mut body = String::new();
            for e in v {
                body.push_str(&format!("{}:{}", e.len(), e));
            }
            format!("S{}:{}", body.len(), body)
        }
        DbValue::Floats(v) => {
            let mut body = String::new();
            for f in v {
                let s = f.to_string();
                body.push_str(&format!("{}:{}", s.len(), s));
            }
            format!("F{}:{}", body.len(), body)
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

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::super::value::ColumnType;
    use super::*;

    fn col(name: &str, dtype: ColumnType, primary_key: bool, unique: bool) -> Column {
        Column {
            name: name.into(),
            dtype,
            primary_key,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique,
        }
    }

    /// Two TEXT columns, both PK (composite).
    fn pair_table() -> Table {
        Table::new(
            "t".into(),
            vec![
                col("a", ColumnType::String, true, false),
                col("b", ColumnType::String, true, false),
            ],
        )
        .unwrap()
    }

    #[test]
    fn pk_key_pipe_collision_pair_distinct() {
        // Bug T2 regression — the exact triple from t2_pk_key_pipe_collision_composite.
        // Old Display+join encoding mapped rows 2 and 3 to the same "'a'|'b'|'c'".
        let t = pair_table();
        let r1 = vec![DbValue::String("a|b".into()), DbValue::String("c".into())];
        let r2 = vec![DbValue::String("a'|'b".into()), DbValue::String("c".into())];
        let r3 = vec![DbValue::String("a".into()), DbValue::String("b'|'c".into())];
        let k1 = t.pk_key(&r1).unwrap();
        let k2 = t.pk_key(&r2).unwrap();
        let k3 = t.pk_key(&r3).unwrap();
        assert_ne!(k1, k2, "row1 vs row2");
        assert_ne!(k2, k3, "the bug pair must not collide");
        assert_ne!(k1, k3, "row1 vs row3");
    }

    #[test]
    fn pk_key_case_preserved() {
        // 'AbC' and 'abc' are distinct PK values and must get distinct keys.
        let t = pair_table();
        let lower = vec![DbValue::String("abc".into()), DbValue::String("x".into())];
        let mixed = vec![DbValue::String("AbC".into()), DbValue::String("x".into())];
        assert_ne!(t.pk_key(&lower), t.pk_key(&mixed));
        // Single-column PK too.
        let t1 = Table::new("t".into(), vec![col("id", ColumnType::String, true, false)]).unwrap();
        assert_ne!(
            t1.pk_key(&[DbValue::String("AbC".into())]),
            t1.pk_key(&[DbValue::String("abc".into())])
        );
    }

    #[test]
    fn pk_key_empty_string_distinct() {
        let t = pair_table();
        let empty = vec![DbValue::String("".into()), DbValue::String("c".into())];
        let nul = vec![DbValue::Null, DbValue::String("c".into())];
        let c = vec![DbValue::String("c".into()), DbValue::String("c".into())];
        assert_ne!(t.pk_key(&empty), t.pk_key(&nul), "'' vs NULL");
        assert_ne!(t.pk_key(&empty), t.pk_key(&c), "'' vs 'c'");
        // The empty string encodes with length 0 and stays unambiguous.
        assert_eq!(t.pk_key(&empty).unwrap(), "s0:s1:c");
    }

    #[test]
    fn pk_key_special_chars_and_utf8() {
        let t = pair_table();
        let quote = vec![DbValue::String("it's".into()), DbValue::String("x".into())];
        let pipe = vec![DbValue::String("a|b|c".into()), DbValue::String("x".into())];
        // héllo—wörld is 15 bytes, 日本語 is 9 bytes.
        let utf8 = vec![DbValue::String("héllo—wörld".into()), DbValue::String("日本語".into())];
        let keys: Vec<String> = [quote, pipe, utf8.clone()]
            .iter()
            .map(|r| t.pk_key(r).unwrap())
            .collect();
        for i in 0..keys.len() {
            for j in i + 1..keys.len() {
                assert_ne!(keys[i], keys[j], "keys[{i}] vs keys[{j}]");
            }
        }
        // Length prefix is the BYTE length, so multi-byte UTF-8 round-trips.
        let k = t.pk_key(&utf8).unwrap();
        assert!(k.starts_with("s15:héllo—wörld"), "utf8 prefix: {}", k);
        assert!(k.ends_with("s9:日本語"), "utf8 suffix: {}", k);
    }

    #[test]
    fn pk_key_strings_array_commas_distinct() {
        // Display for Strings is join(",") — ["a,b","c"] and ["a","b,c"] both
        // print as [a,b,c]. The element-wise length prefix must tell them apart.
        let t = Table::new("t".into(), vec![col("s", ColumnType::Strings, true, false)]).unwrap();
        let a = vec![DbValue::Strings(vec!["a,b".into(), "c".into()])];
        let b = vec![DbValue::Strings(vec!["a".into(), "b,c".into()])];
        assert_ne!(t.pk_key(&a), t.pk_key(&b));
        // And all variants have distinct type tags.
        let single = vec![DbValue::String("[a,b,c]".into())];
        assert_ne!(t.pk_key(&a), t.pk_key(&single));
        let floats = vec![DbValue::Floats(vec![1.5, 2.5])];
        assert_ne!(t.pk_key(&floats), t.pk_key(&single));
    }

    #[test]
    fn pk_unique_ops_consistent() {
        // Insert / delete / replace_by_pk / update_cell must all agree on the
        // same encoded keys, or stale entries leak into pk_set/unique_set.
        let mut t = Table::new(
            "t".into(),
            vec![
                col("a", ColumnType::String, true, true),
                col("b", ColumnType::String, true, false),
                col("v", ColumnType::String, false, false),
            ],
        )
        .unwrap();
        let r1 = vec![
            DbValue::String("a|b".into()),
            DbValue::String("c".into()),
            DbValue::String("v1".into()),
        ];
        let r2 = vec![
            DbValue::String("a'|'b".into()),
            DbValue::String("c".into()),
            DbValue::String("v2".into()),
        ];
        let r3 = vec![
            DbValue::String("a".into()),
            DbValue::String("b'|'c".into()),
            DbValue::String("v3".into()),
        ];
        // All three distinct composite keys insert (bug T2 regression).
        t.insert(r1.clone()).unwrap();
        t.insert(r2.clone()).unwrap();
        t.insert(r3.clone()).unwrap();
        assert_eq!(t.row_count(), 3);
        // A true duplicate is still rejected.
        assert!(t.insert(r1.clone()).is_err());
        // delete removes the colliding row's key entirely…
        assert_eq!(t.delete(|row| row == r2.as_slice()), 1);
        assert_eq!(t.row_count(), 2);
        // …so it can be re-inserted.
        t.insert(r2.clone()).unwrap();
        // replace_by_pk matches via the encoded composite key and overwrites in place.
        let replaced = t.replace_by_pk(vec![
            DbValue::String("a".into()),
            DbValue::String("b'|'c".into()),
            DbValue::String("v3-new".into()),
        ]);
        assert!(matches!(replaced, Ok(true)), "replace_by_pk: {:?}", replaced);
        assert_eq!(t.row_count(), 3);
        // update_cell on a PK column keeps pk maps consistent.
        let old = t.update_cell(0, 1, DbValue::String("z".into()));
        assert_eq!(old, DbValue::String("c".into()));
        let new_key = t
            .pk_key(&[
                DbValue::String("a|b".into()),
                DbValue::String("z".into()),
                DbValue::Null,
            ])
            .unwrap();
        assert_eq!(t.pk_row_index.get(&new_key), Some(&0));
        // update_cell on the UNIQUE column frees the old unique key…
        assert_eq!(
            t.update_cell(0, 0, DbValue::String("a|b2".into())),
            DbValue::String("a|b".into())
        );
        // …so the freed unique value + a fresh PK insert cleanly.
        t.insert(vec![
            DbValue::String("a|b".into()),
            DbValue::String("w".into()),
            DbValue::String("v4".into()),
        ])
        .unwrap();
        assert_eq!(t.row_count(), 4);
    }
}
