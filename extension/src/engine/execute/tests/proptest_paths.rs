// Property tests: index fast paths must return EXACTLY the scan result.
//
// The fast paths (try_pk_index O(1) single-PK, composite-PK full-match,
// try_btree_index Eq/range/LIKE-prefix) all land in BOTH select dispatchers
// (execute/select.rs and stmts/select.rs) with a verify-rescan re-filter. The
// invariant these tests prove: for every WHERE shape a fast path handles,
// `index_result == scan_result`.
//
// Method: for each generated query we build an *equivalent* query the fast-path
// detectors reject, forcing the engine to scan. Both run through the real
// parser + executor; the matched row-key sets must be identical. We also assert
// the fast path FIRED on the index query and did NOT on the scan query —
// otherwise the comparison would prove nothing.
//
// Scan-forcing rewrites preserve semantics exactly:
//   - `col = X`       -> `col = X + 0`     (X + 0 == X for every i64)
//   - `col OP X`      -> `col OP X AND 1=1`
//   - `col BETWEEN X AND Y` -> `... AND 1=1`
//   - `col LIKE 'p%'` -> `col LIKE 'p%' AND 1=1`
// The `+ 0` RHS is a non-literal binary op, which extract_equality_conjuncts
// and try_btree_index's Eq arm both reject; the `AND 1=1` makes the top level
// a conjunction that range_bounds rejects. (1=1 is always truthy.)

use std::collections::HashSet;

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;

use super::helpers::*;
use crate::engine::index::IndexType;
use crate::engine::prelude::*;

// ── Shared plumbing ──────────────────────────────────────────────────────

fn col(name: &str, dtype: ColumnType, primary_key: bool) -> Column {
    Column {
        name: name.into(),
        dtype,
        primary_key,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }
}

/// Extract the WHERE clause of the first SELECT in `sql` (same AST the engine
/// passes to try_pk_index / try_btree_index, so the assertion matches what the
/// executor actually did).
fn where_expr(sql: &str) -> Option<Expr> {
    let stmts = crate::parser::parse_sql(sql).expect("parse_sql");
    stmts.into_iter().find_map(|stmt| match stmt {
        Statement::Query(q) => match &*q.body {
            SetExpr::Select(s) => s.selection.clone(),
            _ => None,
        },
        _ => None,
    })
}

/// Run `SELECT <proj> FROM t WHERE ...` and return the matched rows as a set
/// of key strings (each projected cell's JSON text, joined by a unit sep — so
/// composite projections form full row keys).
fn select_rows_as_set(db: &mut Database, sql: &str) -> HashSet<String> {
    let out = parse_and_exec(sql, db).unwrap_or_else(|e| panic!("{sql} failed: {e}"));
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&out).unwrap_or_else(|e| panic!("bad SELECT payload for {sql:?}: {e}\n{out}"));
    rows.into_iter()
        .skip(1)
        .filter_map(|r| {
            r.as_array()
                .map(|cells| cells.iter().map(|c| c.to_string()).collect::<Vec<_>>().join("\u{1f}"))
        })
        .collect()
}

/// Prove `index_q` (fast path) and `scan_q` (forced scan) return the same rows.
/// `expect_fire` is asserted for the fast-path detector on `index_q`; the scan
/// query must never fire it. Negative integer literals parse as
/// `UnaryOp(Minus, ...)`, which the literal extractors reject, so the fast path
/// (correctly) falls back to scan for those — the equality still applies.
fn assert_fast_path_eq(
    db: &mut Database,
    index_q: &str,
    scan_q: &str,
    fast: impl Fn(Option<&Expr>, &Table) -> bool,
    expect_fire: bool,
) {
    let table = db.get_table("t").unwrap();
    assert_eq!(
        fast(where_expr(index_q).as_ref(), table),
        expect_fire,
        "unexpected fast-path decision for: {index_q}"
    );
    assert!(
        !fast(where_expr(scan_q).as_ref(), table),
        "scan query hit fast path: {scan_q}"
    );
    let index = select_rows_as_set(db, index_q);
    let scan = select_rows_as_set(db, scan_q);
    assert_eq!(index, scan, "index path != scan path for: {index_q}");
}

// ── Strategies ───────────────────────────────────────────────────────────

/// (stored PKs, query value v) — v drawn from the stored set or uniform over a
/// wider range (usually absent). A coincidental hit on a stored value is fine —
/// the equality assertion holds either way.
fn pk_eq_case() -> impl Strategy<Value = (Vec<i64>, i64)> {
    prop::collection::hash_set(-100_000i64..100_000, 1..25).prop_flat_map(|pks| {
        let stored: Vec<i64> = pks.into_iter().collect();
        let present = {
            let s = stored.clone();
            (0..s.len()).prop_map(move |i| s[i])
        };
        (Just(stored), prop_oneof![present, -1_000_000i64..1_000_000])
    })
}

fn alpha_str(len: std::ops::Range<usize>) -> impl Strategy<Value = String> {
    prop::collection::vec(prop::char::range('a', 'z'), len).prop_map(|v| v.into_iter().collect())
}

/// (stored (a, b) pairs, query (x, y)) — full composite-PK match, present/absent.
fn composite_pk_case() -> impl Strategy<Value = (Vec<(i64, String)>, i64, String)> {
    prop::collection::vec((-50i64..50, alpha_str(0..6)), 1..20)
        .prop_filter("composite keys must be distinct", |pairs| {
            let mut seen = HashSet::new();
            pairs.iter().all(|p| seen.insert(p.clone()))
        })
        .prop_flat_map(|pairs| {
            let present = {
                let s = pairs.clone();
                (0..s.len()).prop_map(move |i| s[i].clone())
            };
            (Just(pairs), prop_oneof![present, (-1000..1000i64, alpha_str(0..6))])
                .prop_map(|(pairs, (x, y))| (pairs, x, y))
        })
}

/// (indexed v values, eq value) — v ∈ [-100,100], x wider so both hit/miss.
fn btree_eq_case() -> impl Strategy<Value = (Vec<i64>, i64)> {
    (prop::collection::vec(-100i64..100, 2..30), -200i64..200)
}

/// (v values, predicate kind, bound x, bound y) — the bounds are derived from
/// the data's own min/max so the range is ALWAYS strictly selective (at least
/// one extreme row excluded → candidates < n → the fast path fires, with no
/// prop_assume rejection churn). kind: 0=Gt 1=Ge 2=Lt 3=Le 4=Between.
///
/// Values are non-negative: a negative bound parses as UnaryOp(Minus, ...),
/// which range_bounds' literal extractor rejects (same fallback as the Eq
/// tests) — keeping the domain non-negative guarantees the fast path fires for
/// every generated case. Negative-value range semantics are the same encode_key
/// machinery already proven by the M2 ordering proptests and the negative Eq
/// cases.
fn btree_range_case() -> impl Strategy<Value = (Vec<i64>, u32, i64, i64)> {
    prop::collection::vec(0i64..100, 5..30)
        .prop_filter("min and max at least 2 apart", |v: &Vec<i64>| {
            v.iter().max().copied().unwrap_or(0) - v.iter().min().copied().unwrap_or(0) >= 2
        })
        .prop_flat_map(|vals| {
            let lo = *vals.iter().min().unwrap();
            let hi = *vals.iter().max().unwrap();
            // Gt/Ge: bound in (min, max] — the min row is excluded.
            // Lt/Le: bound in [min, max) — the max row is excluded.
            // Between: y in (min, max), then x in (min, y] — x <= y (reversed
            // bounds are scan-only by design) and both extremes excluded.
            (0..5u32).prop_flat_map(move |p| {
                let xy: BoxedStrategy<(i64, i64)> = match p {
                    0 | 1 => ((lo + 1)..=hi).prop_map(|x| (x, 0)).boxed(),
                    2 | 3 => (lo..hi).prop_map(|x| (x, 0)).boxed(),
                    _ => (lo + 1..hi)
                        .prop_flat_map(move |y| ((lo + 1)..=y).prop_map(move |x| (x, y)))
                        .boxed(),
                };
                let rows = vals.clone();
                xy.prop_map(move |(x, y)| (rows.clone(), p, x, y))
            })
        })
}

fn range_queries(pred: u32, x: i64, y: i64) -> (String, String) {
    let cond = match pred {
        0 => format!("v > {x}"),
        1 => format!("v >= {x}"),
        2 => format!("v < {x}"),
        3 => format!("v <= {x}"),
        _ => format!("v BETWEEN {x} AND {y}"),
    };
    (
        format!("SELECT id FROM t WHERE {cond}"),
        format!("SELECT id FROM t WHERE {cond} AND 1 = 1"),
    )
}

/// (stored strings, LIKE prefix) — prefix is a non-empty prefix of a stored string.
fn btree_like_case() -> impl Strategy<Value = (Vec<String>, String)> {
    prop::collection::vec(alpha_str(1..10), 2..20).prop_flat_map(|strs| {
        let prefix = {
            let s = strs.clone();
            (0..s.len()).prop_flat_map(move |i| {
                let base = s[i].clone();
                (1..=base.len()).prop_map(move |l| base[..l].to_string())
            })
        };
        (Just(strs), prefix)
    })
}

// ── DB builders ──────────────────────────────────────────────────────────

/// Build a single-table Database (table "t") with `rows` inserted and an
/// optional secondary index.
fn make_db(
    cols: Vec<Column>,
    rows: impl IntoIterator<Item = Vec<DbValue>>,
    index: Option<(&str, &str, IndexType)>,
) -> Database {
    let mut db = Database::new();
    let mut t = Table::new("t".into(), cols).unwrap();
    for row in rows {
        t.insert(row).unwrap();
    }
    if let Some((name, column, ty)) = index {
        t.create_index(name, column, ty).unwrap();
    }
    db.create_table("t", t).unwrap();
    db
}

fn pk_db(pks: &[i64]) -> Database {
    make_db(
        vec![col("id", ColumnType::Int, true)],
        pks.iter().map(|&p| vec![DbValue::Int(p)]),
        None,
    )
}

fn composite_db(pairs: &[(i64, String)]) -> Database {
    make_db(
        vec![col("a", ColumnType::Int, true), col("b", ColumnType::String, true)],
        pairs
            .iter()
            .map(|(a, b)| vec![DbValue::Int(*a), DbValue::String(b.clone())]),
        None,
    )
}

fn btree_int_db(vals: &[i64]) -> Database {
    make_db(
        vec![col("id", ColumnType::Int, true), col("v", ColumnType::Int, false)],
        vals.iter()
            .enumerate()
            .map(|(i, &v)| vec![DbValue::Int(i as i64), DbValue::Int(v)]),
        Some(("btree_v", "v", IndexType::BTree)),
    )
}

fn btree_str_db(strs: &[String]) -> Database {
    make_db(
        vec![col("id", ColumnType::Int, true), col("s", ColumnType::String, false)],
        strs.iter()
            .enumerate()
            .map(|(i, s)| vec![DbValue::Int(i as i64), DbValue::String(s.clone())]),
        Some(("btree_s", "s", IndexType::BTree)),
    )
}

// ── The invariants ───────────────────────────────────────────────────────

proptest! {
    /// O(1) single-PK fast path: `WHERE id = v` (v present AND absent).
    #[test]
    fn pk_eq_equals_scan((pks, v) in pk_eq_case()) {
        let mut db = pk_db(&pks);
        let index_q = format!("SELECT id FROM t WHERE id = {v}");
        let scan_q = format!("SELECT id FROM t WHERE id = {v} + 0");
        assert_fast_path_eq(&mut db, &index_q, &scan_q, |e, t| try_pk_index(e, t).is_some(), v >= 0);
    }

    /// Composite-PK full match (M5): `WHERE a = x AND b = y`, both pinned.
    #[test]
    fn composite_pk_equals_scan((pairs, x, y) in composite_pk_case()) {
        let mut db = composite_db(&pairs);
        let index_q = format!("SELECT a, b FROM t WHERE a = {x} AND b = '{y}'");
        let scan_q = format!("SELECT a, b FROM t WHERE a = {x} + 0 AND b = '{y}'");
        assert_fast_path_eq(&mut db, &index_q, &scan_q, |e, t| try_pk_index(e, t).is_some(), x >= 0);
    }

    /// Partial composite conjunct MUST NOT take the fast path: try_pk_index
    /// returns None (falls back to scan), so a partial key can never wrongly
    /// hide real rows. The result must still equal the scan.
    #[test]
    fn composite_pk_partial_conjunct_scan((pairs, x) in composite_pk_case()
        .prop_map(|(pairs, x, _y)| (pairs, x))) {
        let mut db = composite_db(&pairs);
        let index_q = format!("SELECT a, b FROM t WHERE a = {x}");
        let scan_q = format!("SELECT a, b FROM t WHERE a = {x} + 0");
        {
            let table = db.get_table("t").unwrap();
            assert!(
                try_pk_index(where_expr(&index_q).as_ref(), table).is_none(),
                "partial composite conjunct must fall back to scan: {index_q}"
            );
            assert!(try_pk_index(where_expr(&scan_q).as_ref(), table).is_none());
        }
        let index = select_rows_as_set(&mut db, &index_q);
        let scan = select_rows_as_set(&mut db, &scan_q);
        assert_eq!(index, scan, "query: {index_q}");
    }

    /// BTree Eq: `WHERE v = x` on an indexed column (hit and miss).
    #[test]
    fn btree_eq_equals_scan((vals, x) in btree_eq_case()) {
        let mut db = btree_int_db(&vals);
        let index_q = format!("SELECT id FROM t WHERE v = {x}");
        let scan_q = format!("SELECT id FROM t WHERE v = {x} + 0");
        assert_fast_path_eq(&mut db, &index_q, &scan_q, |e, t| try_btree_index(e, t).is_some(), x >= 0);
    }

    /// BTree range via BTreeMap::range (M7): `>`, `>=`, `<`, `<=`, BETWEEN.
    #[test]
    fn btree_range_equals_scan((vals, pred, x, y) in btree_range_case()) {
        let mut db = btree_int_db(&vals);
        let (index_q, scan_q) = range_queries(pred, x, y);
        assert_fast_path_eq(&mut db, &index_q, &scan_q, |e, t| try_btree_index(e, t).is_some(), true);
    }

    /// BTree LIKE prefix: `WHERE s LIKE 'pre%'` (byte-prefix range, case-sensitive).
    #[test]
    fn btree_like_prefix_equals_scan((strs, prefix) in btree_like_case()) {
        let mut db = btree_str_db(&strs);
        let index_q = format!("SELECT id FROM t WHERE s LIKE '{prefix}%'");
        let scan_q = format!("SELECT id FROM t WHERE s LIKE '{prefix}%' AND 1 = 1");
        {
            let table = db.get_table("t").unwrap();
            prop_assume!(
                try_btree_index(where_expr(&index_q).as_ref(), table).is_some(),
                "not selective: {}",
                index_q
            );
        }
        assert_fast_path_eq(&mut db, &index_q, &scan_q, |e, t| try_btree_index(e, t).is_some(), true);
    }
}

/// Regression (found by T2 proptests): reversed BETWEEN bounds (`v BETWEEN
/// 50 AND 10`) used to PANIC inside BTreeMap::range ("range start is greater
/// than range end") — the fast path must fall back to the scan, which
/// evaluates the always-false predicate to an empty result.
#[test]
fn btree_between_reversed_bounds_no_panic() {
    let mut db = btree_int_db(&[3, 17, 42, 99]);
    let out = parse_and_exec("SELECT id FROM t WHERE v BETWEEN 50 AND 10", &mut db).unwrap();
    assert_eq!(out, "[[\"id\"]]", "reversed BETWEEN must be empty, got: {out}");
}
