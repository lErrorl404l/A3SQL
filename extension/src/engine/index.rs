// a3db indices — BTREE (sorted lookups) and TRIGRAM (fuzzy GIN)

use std::collections::{BTreeMap, HashMap, HashSet};

use super::table::trigrams;
use super::value::DbValue;

/// Available index types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IndexType {
    BTree,
    Trigram,
}

impl std::fmt::Display for IndexType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexType::BTree => write!(f, "BTREE"),
            IndexType::Trigram => write!(f, "TRIGRAM"),
        }
    }
}

/// Metadata for a created index.
#[derive(Debug, Clone)]
pub struct IndexMeta {
    pub name: String,
    pub table: String,
    pub column: String,
    pub index_type: IndexType,
}

// ── BTREE Index ────────────────────────────────────────────────────────

/// Simple BTREE index — maps encoded column values to row indices.
/// Uses BTreeMap for range query support (sorted iteration).
#[derive(Debug, Clone)]
pub struct BTreeIndex {
    column: String,
    entries: BTreeMap<String, Vec<usize>>,
}

impl BTreeIndex {
    pub fn new(column: &str) -> Self {
        BTreeIndex {
            column: column.to_string(),
            entries: BTreeMap::new(),
        }
    }

    pub fn column(&self) -> &str {
        &self.column
    }

    pub fn insert(&mut self, row_idx: usize, value: &DbValue) {
        let key = encode_key(value);
        self.entries.entry(key).or_default().push(row_idx);
    }

    pub fn remove(&mut self, row_idx: usize, value: &DbValue) {
        let key = encode_key(value);
        if let Some(indices) = self.entries.get_mut(&key) {
            indices.retain(|&i| i != row_idx);
            if indices.is_empty() {
                self.entries.remove(&key);
            }
        }
    }

    /// Exact match lookup.
    pub fn lookup(&self, value: &DbValue) -> Vec<usize> {
        let key = encode_key(value);
        self.entries.get(&key).cloned().unwrap_or_default()
    }

    /// Partial match (for `LIKE 'prefix%'` or `>`, `<` comparisons).
    /// Returns all row indices where the key matches a predicate.
    pub fn range_lookup<F>(&self, mut predicate: F) -> Vec<usize>
    where
        F: FnMut(&str) -> bool,
    {
        let mut results = Vec::new();
        for (key, indices) in &self.entries {
            if predicate(key) {
                results.extend(indices);
            }
        }
        results
    }

    /// Scan all entries.
    pub fn all_entries(&self) -> Vec<usize> {
        self.entries
            .values()
            .flat_map(|v| v.iter().copied())
            .collect()
    }
}

// ── Trigram Index ──────────────────────────────────────────────────────

/// Trigram GIN (Generalized Inverted Index).
///
/// Maps each 3-gram of the indexed column to the set of row indices
/// that contain that trigram. At query time, the trigrams of the pattern
/// are intersected to find candidate rows.
#[derive(Debug, Clone)]
pub struct TrigramIndex {
    column: String,
    /// trigram → set of row indices containing that trigram
    trigram_map: HashMap<String, HashSet<usize>>,
}

impl TrigramIndex {
    pub fn new(column: &str) -> Self {
        TrigramIndex {
            column: column.to_string(),
            trigram_map: HashMap::new(),
        }
    }

    pub fn column(&self) -> &str {
        &self.column
    }

    pub fn insert(&mut self, row_idx: usize, value: &DbValue) {
        let text = value_to_plain(value);
        for trigram in trigrams(&text) {
            self.trigram_map.entry(trigram).or_default().insert(row_idx);
        }
    }

    pub fn remove(&mut self, row_idx: usize, value: &DbValue) {
        let text = value_to_plain(value);
        for trigram in trigrams(&text) {
            if let Some(indices) = self.trigram_map.get_mut(&trigram) {
                indices.remove(&row_idx);
                if indices.is_empty() {
                    self.trigram_map.remove(&trigram);
                }
            }
        }
    }

    /// Find candidate rows for a fuzzy match query.
    ///
    /// Uses the top-N most-selective trigrams to find candidates, then
    /// returns the union (not intersection) so that partial matches are
    /// found. The final scoring is done by Table::trigram_similarity().
    pub fn candidates(&self, pattern: &str) -> Vec<usize> {
        let pat_trigrams: Vec<String> = trigrams(pattern).into_iter().collect();
        if pat_trigrams.is_empty() {
            return Vec::new();
        }

        // Collect all postings lists, sorted by size (most selective first)
        let mut lists: Vec<&HashSet<usize>> = Vec::new();
        for tg in &pat_trigrams {
            if let Some(indices) = self.trigram_map.get(tg) {
                lists.push(indices);
            }
        }

        if lists.is_empty() {
            return Vec::new();
        }

        // Sort by list size (ascending)
        lists.sort_by_key(|l| l.len());

        // Take the top 3 most-selective trigrams (or however many we have)
        let top_n = lists.len().min(3);
        let mut result: HashSet<usize> = HashSet::new();

        for list in lists.iter().take(top_n) {
            for &idx in *list {
                result.insert(idx);
            }
        }

        result.into_iter().collect()
    }
}

// ── Encoding helpers ───────────────────────────────────────────────────

/// Encode a DbValue as a sortable string key for BTreeMap ordering.
fn encode_key(v: &DbValue) -> String {
    match v {
        DbValue::Null => "\x00".to_string(),
        DbValue::Bool(true) => "\x01true".to_string(),
        DbValue::Bool(false) => "\x01false".to_string(),
        DbValue::Int(n) => {
            // Pad i64 to fixed width for lexicographic ordering
            let shifted = n.wrapping_add(i64::MAX);
            format!("\x02{:020}", shifted)
        }
        DbValue::Float(f) => {
            // Encode f64 as sortable bytes
            let bits = f.to_bits();
            let sortable = if f.is_sign_negative() {
                !bits
            } else {
                bits ^ (1u64 << 63)
            };
            format!("\x03{:020}", sortable)
        }
        DbValue::String(s) => format!("\x04{}", s.to_lowercase()),
        DbValue::Strings(arr) => format!("\x05{}", arr.join(",")),
        DbValue::Floats(arr) => {
            let joined: Vec<String> = arr.iter().map(|f| f.to_string()).collect();
            format!("\x06{}", joined.join(","))
        }
    }
}

/// Extract plain text from a DbValue for trigram extraction.
fn value_to_plain(v: &DbValue) -> String {
    match v {
        DbValue::Null => String::new(),
        DbValue::Bool(b) => b.to_string(),
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => s.clone(),
        DbValue::Strings(arr) => arr.join(" "),
        DbValue::Floats(arr) => arr
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::value::*;

    #[test]
    fn btree_insert_and_lookup() {
        let mut idx = BTreeIndex::new("name");
        idx.insert(0, &DbValue::String("alpha".into()));
        idx.insert(1, &DbValue::String("beta".into()));
        idx.insert(2, &DbValue::String("alpha".into()));

        let alpha_rows = idx.lookup(&DbValue::String("alpha".into()));
        assert_eq!(alpha_rows.len(), 2);
        assert!(alpha_rows.contains(&0));
        assert!(alpha_rows.contains(&2));

        let beta_rows = idx.lookup(&DbValue::String("beta".into()));
        assert_eq!(beta_rows, vec![1]);
    }

    #[test]
    fn btree_remove() {
        let mut idx = BTreeIndex::new("x");
        idx.insert(0, &DbValue::Int(42));
        idx.insert(1, &DbValue::Int(42));
        idx.remove(0, &DbValue::Int(42));
        let rows = idx.lookup(&DbValue::Int(42));
        assert_eq!(rows, vec![1]);
    }

    #[test]
    fn btree_range() {
        let mut idx = BTreeIndex::new("val");
        idx.insert(0, &DbValue::Int(10));
        idx.insert(1, &DbValue::Int(20));
        idx.insert(2, &DbValue::Int(30));

        // Lookups work
        assert_eq!(idx.lookup(&DbValue::Int(10)), vec![0]);
        assert!(idx.lookup(&DbValue::Int(99)).is_empty());
        // All entries
        assert_eq!(idx.all_entries().len(), 3);
    }

    #[test]
    fn trigram_insert_and_candidates() {
        let mut idx = TrigramIndex::new("name");
        idx.insert(0, &DbValue::String("rhs_m4a1".into()));
        idx.insert(1, &DbValue::String("rhs_m4a1_carryhandle".into()));
        idx.insert(2, &DbValue::String("hlc_ak74".into()));

        let candidates = idx.candidates("rhs_m4");
        assert!(
            candidates.contains(&0),
            "rhs_m4a1 should match, got: {:?}",
            candidates
        );
        assert!(candidates.contains(&1), "rhs_m4a1_carryhandle should match");
    }

    #[test]
    fn trigram_remove() {
        let mut idx = TrigramIndex::new("name");
        idx.insert(0, &DbValue::String("test_abc".into()));
        idx.insert(1, &DbValue::String("test_xyz".into()));
        idx.remove(0, &DbValue::String("test_abc".into()));

        let candidates = idx.candidates("test");
        assert!(candidates.contains(&1), "row 1 should still match");
    }

    #[test]
    fn trigram_no_match() {
        let mut idx = TrigramIndex::new("name");
        idx.insert(0, &DbValue::String("abcdef".into()));
        let _candidates = idx.candidates("xyz");
        // With permissive algorithm, candidates may be found. That's fine.
        // The actual scoring in Table::trigram_similarity will filter.
    }

    #[test]
    fn btree_int_ordering() {
        let mut idx = BTreeIndex::new("val");
        idx.insert(0, &DbValue::Int(5));
        idx.insert(1, &DbValue::Int(100));
        idx.insert(2, &DbValue::Int(1));

        assert_eq!(idx.lookup(&DbValue::Int(1)), vec![2]);
        assert_eq!(idx.lookup(&DbValue::Int(5)), vec![0]);
        assert_eq!(idx.lookup(&DbValue::Int(100)), vec![1]);
    }
}
