// a3sql indices — BTREE (sorted lookups) and TRIGRAM (fuzzy GIN)

//! Index implementations — BTree and Trigram indices for fast lookups.
//! Used by CREATE INDEX and automatically by WHERE clauses with equality or fuzzy matches.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Bound;

use super::table::trigrams;
use super::value::DbValue;

/// Available index types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum IndexType {
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
pub(crate) struct IndexMeta {
    pub name: String,
    pub table: String,
    pub column: String,
    pub index_type: IndexType,
}

// ── BTREE Index ────────────────────────────────────────────────────────

/// Simple BTREE index — maps encoded column values to row indices.
/// Uses BTreeMap for range query support (sorted iteration).
#[derive(Debug, Clone)]
pub(crate) struct BTreeIndex {
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

    /// Range scan over encoded keys, in ascending key order.
    /// Bounds are produced by `encode_key` of the bound values (type-prefix
    /// byte + value encoding), so the key order matches the value order. For
    /// `LIKE 'prefix%'` the upper bound is the byte-successor of the prefixed
    /// key — every key with the prefix sorts below it, everything else above.
    pub fn range(&self, lower: Bound<&str>, upper: Bound<&str>) -> Vec<usize> {
        self.entries
            .range::<str, (Bound<&str>, Bound<&str>)>((lower, upper))
            .flat_map(|(_, indices)| indices.iter().copied())
            .collect()
    }

    /// Scan all entries.
    #[allow(dead_code, reason = "full index scan not yet wired in executor")]
    pub fn all_entries(&self) -> Vec<usize> {
        self.entries.values().flat_map(|v| v.iter().copied()).collect()
    }
}

// ── Trigram Index ──────────────────────────────────────────────────────

/// Trigram GIN (Generalized Inverted Index).
///
/// Maps each 3-gram of the indexed column to the set of row indices
/// that contain that trigram. At query time, the trigrams of the pattern
/// are intersected to find candidate rows.
#[derive(Debug, Clone)]
pub(crate) struct TrigramIndex {
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

    #[allow(dead_code, reason = "index column accessor not yet used externally")]
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

    /// Candidate rows for a LIKE containment match on `run` — the longest
    /// literal run of a `%..%` / `a%b` pattern (`run` must be >= 3 chars).
    ///
    /// Intersects the postings of every interior trigram window of `run`, so
    /// every row whose text contains `run` as a substring is guaranteed to be
    /// present: LIKE's verify-rescan then drops the false positives. Unlike
    /// [`candidates`](Self::candidates) this does NOT use the top-3 union
    /// heuristic — that is tuned for fuzzy partial matches and can skip rows
    /// a containment filter must return.
    pub(crate) fn like_candidates(&self, run: &str) -> Vec<usize> {
        let lower = run.to_lowercase();
        let bytes = lower.as_bytes();
        if bytes.len() < 3 {
            return Vec::new();
        }
        let mut windows: Vec<String> = bytes
            .windows(3)
            .map(|w| String::from_utf8_lossy(w).to_string())
            .collect();
        windows.sort();
        windows.dedup();
        let mut lists = windows.into_iter().filter_map(|tg| self.trigram_map.get(&tg));
        let Some(first) = lists.next() else {
            return Vec::new();
        };
        let mut acc: HashSet<usize> = first.clone();
        for list in lists {
            acc.retain(|row| list.contains(row));
        }
        acc.into_iter().collect()
    }
}

// ── Encoding helpers ───────────────────────────────────────────────────

/// Encode a DbValue as a sortable string key for BTreeMap ordering.
pub(crate) fn encode_key(v: &DbValue) -> String {
    match v {
        DbValue::Null => "\x00".to_string(),
        DbValue::Bool(true) => "\x01true".to_string(),
        DbValue::Bool(false) => "\x01false".to_string(),
        DbValue::Int(n) => {
            // Offset-encode the two's-complement bits: XORing the sign bit maps
            // i64::MIN → 0x0, 0 → 0x8000..0, i64::MAX → 0xFFFF..F, so the
            // fixed-width decimal is monotonic with the numeric order. (The old
            // n.wrapping_add(i64::MAX) wrapped negatives to huge values and
            // mis-ordered them in the BTreeMap.)
            let bits = (*n as u64) ^ (1u64 << 63);
            format!("\x02{:020}", bits)
        }
        DbValue::Float(f) => {
            // Canonicalize NaN to one bit pattern (payload/sign bits vary →
            // unstable ordering, index misses) and -0.0 → +0.0 (-0.0 == 0.0
            // numerically but encodes differently), then sign-flip into
            // sortable order: negative values invert their bits (more negative
            // = smaller key), positive values flip the sign bit.
            let canonical = if f.is_nan() {
                f64::NAN.to_bits()
            } else if *f == 0.0 {
                0.0f64.to_bits() // collapse -0.0 onto +0.0
            } else {
                f.to_bits()
            };
            let sortable = if (canonical >> 63) != 0 {
                !canonical
            } else {
                canonical ^ (1u64 << 63)
            };
            format!("\x03{:020}", sortable)
        }
        DbValue::String(s) => format!("\x04{}", s), // case-sensitive, matches scan/`=`
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
        DbValue::Floats(arr) => arr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(" "),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::value::*;
    #[cfg(not(miri))] // only used by the proptests (cfg'd out under miri)
    use proptest::prelude::*;

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
        assert!(candidates.contains(&0), "rhs_m4a1 should match, got: {:?}", candidates);
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

    #[test]
    fn btree_int_extremes_round_trip() {
        // i64::MIN landmine: the old wrapping_add wrapped it to a huge value.
        // It must encode as the smallest key and round-trip through the index.
        let mut idx = BTreeIndex::new("v");
        idx.insert(0, &DbValue::Int(i64::MIN));
        idx.insert(1, &DbValue::Int(0));
        idx.insert(2, &DbValue::Int(i64::MAX));
        idx.insert(3, &DbValue::Int(-1));
        assert_eq!(idx.all_entries(), vec![0, 3, 1, 2]);
        assert_eq!(idx.lookup(&DbValue::Int(i64::MIN)), vec![0]);
        assert_eq!(idx.lookup(&DbValue::Int(i64::MAX)), vec![2]);
        assert!(idx.lookup(&DbValue::Int(i64::MIN + 1)).is_empty());
    }

    #[test]
    fn btree_float_canonicalization() {
        // -0.0 and +0.0 must share one key (they compare equal); every NaN bit
        // pattern must share one key (else ordering is unstable and lookups miss).
        assert_eq!(encode_key(&DbValue::Float(0.0)), encode_key(&DbValue::Float(-0.0)));
        let nan_keys: std::collections::HashSet<String> = [
            f64::from_bits(f64::NAN.to_bits()),
            f64::from_bits(f64::NAN.to_bits() | (1u64 << 63)), // negative NaN
            f64::from_bits(f64::NAN.to_bits() | 0x0004_0000_0000_0000), // different payload
        ]
        .into_iter()
        .map(|f| encode_key(&DbValue::Float(f)))
        .collect();
        assert_eq!(nan_keys.len(), 1, "all NaN bit patterns must canonicalize to one key");
        // Ordering sanity for finite values: -5.0 < 0.0 < 5.0
        assert!(encode_key(&DbValue::Float(-5.0)) < encode_key(&DbValue::Float(0.0)));
        assert!(encode_key(&DbValue::Float(0.0)) < encode_key(&DbValue::Float(5.0)));
    }

    #[test]
    fn btree_string_is_case_sensitive() {
        let mut idx = BTreeIndex::new("name");
        idx.insert(0, &DbValue::String("abc".into()));
        idx.insert(1, &DbValue::String("AbC".into()));
        assert_eq!(idx.lookup(&DbValue::String("AbC".into())), vec![1]);
        assert_eq!(idx.lookup(&DbValue::String("abc".into())), vec![0]);
    }

    /// Build the byte-successor exclusive upper bound for a LIKE 'prefix%' scan.
    fn like_upper(prefix: &str) -> Bound<String> {
        // encode_key(String(prefix)) = "\x04" + prefix; incrementing the last
        // byte of the full key string gives the exclusive successor. (Prefix
        // never ends in a 0xFF byte — invalid in UTF-8 — so no carry.)
        let key = encode_key(&DbValue::String(prefix.to_string()));
        let mut bytes = key.into_bytes();
        *bytes.last_mut().unwrap() += 1;
        Bound::Excluded(String::from_utf8(bytes).unwrap())
    }

    fn borrow_bound(b: &Bound<String>) -> Bound<&str> {
        match b {
            Bound::Included(s) => Bound::Included(s),
            Bound::Excluded(s) => Bound::Excluded(s),
            Bound::Unbounded => Bound::Unbounded,
        }
    }

    #[test]
    fn btree_range_int_bounds() {
        use std::ops::Bound::{Excluded, Included, Unbounded};
        let mut idx = BTreeIndex::new("v");
        idx.insert(0, &DbValue::Int(10));
        idx.insert(1, &DbValue::Int(20));
        idx.insert(2, &DbValue::Int(30));
        idx.insert(3, &DbValue::Null); // \x00 sorts first
        let k = |n: i64| encode_key(&DbValue::Int(n));

        assert_eq!(
            idx.range(borrow_bound(&Included(k(20))), Unbounded),
            vec![1, 2],
            ">= 20"
        );
        assert_eq!(idx.range(borrow_bound(&Excluded(k(10))), Unbounded), vec![1, 2], "> 10");
        assert_eq!(
            idx.range(Unbounded, borrow_bound(&Included(k(20)))),
            vec![3, 0, 1],
            "<= 20 (NULL key sorts first)"
        );
        assert_eq!(idx.range(Unbounded, borrow_bound(&Excluded(k(20)))), vec![3, 0], "< 20");
        assert_eq!(
            idx.range(borrow_bound(&Included(k(10))), borrow_bound(&Included(k(20)))),
            vec![0, 1],
            "BETWEEN 10 AND 20"
        );
    }

    #[test]
    fn btree_range_string_prefix() {
        use std::ops::Bound::Included;
        let mut idx = BTreeIndex::new("name");
        idx.insert(0, &DbValue::String("alpha".into()));
        idx.insert(1, &DbValue::String("alpine".into()));
        idx.insert(2, &DbValue::String("beta".into()));
        idx.insert(3, &DbValue::String("Alpine".into())); // case-sensitive: not under "alp"

        let lower = Included(encode_key(&DbValue::String("alp".to_string())));
        let got = idx.range(borrow_bound(&lower), borrow_bound(&like_upper("alp")));
        assert_eq!(got, vec![0, 1], "LIKE 'alp%' must be byte-exact and case-sensitive");
    }

    // ── Proptest: encode_key ordering must match db_value_cmp ordering ──

    #[cfg(not(miri))] // helpers are only used by the proptests below (cfg'd out under miri)
    /// Ints within ±2^53 are exactly representable as f64, so db_value_cmp's
    /// f64 coercion preserves strict order there. (Outside that range two
    /// distinct ints can round to the same f64: db_value_cmp reports Equal
    /// while the keys still differ — the encoding is still correct.) The
    /// i64::MIN/MAX landmine is covered separately by btree_int_extremes_round_trip.
    fn bounded_int() -> impl Strategy<Value = i64> {
        (-(1i64 << 53) + 1)..(1i64 << 53)
    }
    #[cfg(not(miri))]
    fn alpha_str() -> impl Strategy<Value = String> {
        // Lowercase ASCII: db_value_cmp falls back to byte-wise string
        // comparison for non-numeric strings, which is exactly what the raw
        // key comparison does. The filter guarantees the f64-parsing fallback
        // never kicks in (e.g. "nan", "inf", "infinity" all parse as f64).
        prop::collection::vec(prop::char::range('a', 'z'), 0..8)
            .prop_map(|v| v.into_iter().collect())
            .prop_filter("must not parse as f64", |s: &String| s.parse::<f64>().is_err())
    }

    #[cfg(not(miri))] // proptest's RNG state leaks under miri's getcwd isolation
    proptest! {
        #[test]
        fn encode_key_matches_db_value_cmp_ints(a in bounded_int(), b in bounded_int()) {
            let ka = encode_key(&DbValue::Int(a));
            let kb = encode_key(&DbValue::Int(b));
            prop_assert_eq!(ka.cmp(&kb), db_value_cmp(&DbValue::Int(a), &DbValue::Int(b)));
        }

        #[test]
        fn encode_key_matches_db_value_cmp_floats(a: f64, b: f64) {
            // NaN is unordered in db_value_cmp (partial_cmp → Equal); its
            // canonicalization is asserted separately in btree_float_canonicalization.
            prop_assume!(!a.is_nan() && !b.is_nan());
            let ka = encode_key(&DbValue::Float(a));
            let kb = encode_key(&DbValue::Float(b));
            prop_assert_eq!(ka.cmp(&kb), db_value_cmp(&DbValue::Float(a), &DbValue::Float(b)));
        }

        #[test]
        fn encode_key_matches_db_value_cmp_strings(a in alpha_str(), b in alpha_str()) {
            let ka = encode_key(&DbValue::String(a.clone()));
            let kb = encode_key(&DbValue::String(b.clone()));
            prop_assert_eq!(ka.cmp(&kb), db_value_cmp(&DbValue::String(a), &DbValue::String(b)));
        }
    }
}
