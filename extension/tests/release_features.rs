// Release 1.1.0 feature tests — dedicated unit coverage for the SQL
// additions verified in the docker smoke test and SLT corpus.
//
// Each feature gets happy-path, negative, and edge-case coverage:
//   - REGEXP / MATCH operators (Expr::RLike + preprocessor rewrite)
//   - NULLIF, CEIL, FLOOR
//   - CREATE TABLE AS SELECT (CTAS)
//   - Aggregate FILTER (WHERE ...)
//   - MIN / MAX NULL-skipping semantics
//
// The dispatch() path is the same code the extension ABI calls, so these
// tests exercise the production execution chain, not just parsers.

use a3sql::dispatch;
use std::sync::{Mutex, MutexGuard};

static TEST_MUTEX: Mutex<()> = Mutex::new(());
fn setup() -> MutexGuard<'static, ()> {
    let g = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    dispatch("reset", &[]);
    dispatch("CREATE TABLE r (id STRING PRIMARY KEY, name STRING, val INT)", &[]);
    dispatch(
        "INSERT INTO r VALUES ('a','Alpha',10), ('b','beta',20), ('c','Gamma',30), ('d','delta',40)",
        &[],
    );
    g
}

// ── REGEXP operator ────────────────────────────────────────────────────

#[test]
fn regexp_matches_prefix_star() {
    let _g = setup();
    // REGEXP is glob-style full-string matching: 'A*' = starts with A.
    let r = dispatch("SELECT id FROM r WHERE name REGEXP 'A*'", &[]);
    assert!(r.contains("a"), "REGEXP 'A*' matched Alpha: {r}");
    assert!(!r.contains("[\"b\"]"), "beta does not start with A: {r}");
}

#[test]
fn regexp_matches_question_wildcard() {
    let _g = setup();
    let r = dispatch("SELECT id FROM r WHERE name REGEXP 'Alp?a'", &[]);
    assert!(r.contains("a"), "REGEXP 'Alp?a' matched Alpha: {r}");
}

#[test]
fn regexp_matches_suffix_star() {
    let _g = setup();
    // Full-string glob: '*mm' matches names ending in "mm".
    dispatch("INSERT INTO r VALUES ('e','gamm',50)", &[]);
    let r = dispatch("SELECT id FROM r WHERE name REGEXP '*mm'", &[]);
    assert!(r.contains("e"), "REGEXP '*mm' matched gamm: {r}");
    assert!(!r.contains("[\"a\"]"), "Alpha does not end in mm: {r}");
}

#[test]
fn regexp_no_match_returns_empty() {
    let _g = setup();
    let r = dispatch("SELECT id FROM r WHERE name REGEXP 'xyz'", &[]);
    assert!(r.contains("\"OK\""), "query ok: {r}");
    assert!(r.ends_with("]]"), "no rows: {r}");
}

#[test]
fn regexp_not_negated() {
    let _g = setup();
    let r = dispatch("SELECT id FROM r WHERE name NOT REGEXP 'a*'", &[]);
    assert!(r.contains("c"), "NOT REGEXP keeps Gamma: {r}");
}

#[test]
fn regexp_on_empty_set_ok() {
    let _g = setup();
    dispatch("CREATE TABLE r_empty (id STRING PRIMARY KEY)", &[]);
    let r = dispatch("SELECT id FROM r_empty WHERE id REGEXP 'x'", &[]);
    assert!(r.contains("\"OK\""), "REGEXP on empty table: {r}");
}

// ── MATCH operator (preprocessor rewrite → match_search) ────────────────

#[test]
fn match_case_insensitive_substring() {
    let _g = setup();
    let r = dispatch("SELECT id FROM r WHERE name MATCH 'alph'", &[]);
    assert!(r.contains("a"), "MATCH 'alph' hit Alpha: {r}");
}

#[test]
fn match_upper_needle_lower_haystack() {
    let _g = setup();
    let r = dispatch("SELECT id FROM r WHERE name MATCH 'BETA'", &[]);
    assert!(r.contains("b"), "MATCH 'BETA' case-insensitive: {r}");
}

#[test]
fn match_no_hit_returns_empty() {
    let _g = setup();
    let r = dispatch("SELECT id FROM r WHERE name MATCH 'zzz'", &[]);
    assert!(r.contains("\"OK\""), "query ok: {r}");
    assert!(!r.contains("[\"a\"]"), "no row: {r}");
}

#[test]
fn match_not_operator() {
    let _g = setup();
    let r = dispatch("SELECT id FROM r WHERE name NOT MATCH 'mma'", &[]);
    assert!(r.contains("b"), "NOT MATCH keeps beta: {r}");
    assert!(r.contains("d"), "NOT MATCH keeps delta: {r}");
}

#[test]
fn match_not_with_string_literal_operand() {
    let _g = setup();
    // Regression: the NOT-detection must also handle a quoted left operand.
    let r = dispatch("SELECT 'hello world' NOT MATCH 'xyz'", &[]);
    assert!(r.contains("true"), "NOT MATCH no-hit → true: {r}");
    let r2 = dispatch("SELECT 'hello world' NOT MATCH 'HELLO'", &[]);
    assert!(r2.contains("false"), "NOT MATCH hit → false: {r2}");
}

#[test]
fn match_with_bind_params_rewrite() {
    let _g = setup();
    let r = dispatch("SELECT id FROM r WHERE name MATCH $1", &["gamma"]);
    assert!(r.contains("c"), "MATCH with param: {r}");
}

#[test]
fn match_inside_string_literal_not_rewritten() {
    let _g = setup();
    // A literal containing "MATCH" must not be treated as an operator.
    let r = dispatch("SELECT 'this MATCH is literal' AS lit", &[]);
    assert!(r.contains("this MATCH is literal"), "literal preserved: {r}");
}

#[test]
fn match_trailing_backslash_no_panic() {
    let _g = setup();
    // Regression: a trailing backslash before the closing quote used to
    // overshoot the operand slice and panic in the rewriter. The backslash
    // is a literal character, so the query is valid and must return false
    // ('x' does not contain 'abc\'), never panic.
    let r = dispatch("SELECT 'x' MATCH 'abc\\'", &[]);
    assert!(r.contains("\"OK\""), "no panic, valid query: {r}");
    assert!(r.contains("false"), "no match → false: {r}");
}

// ── NULLIF ──────────────────────────────────────────────────────────────

#[test]
fn nullif_equal_returns_null() {
    let _g = setup();
    let r = dispatch("SELECT NULLIF(10, 10)", &[]);
    assert!(r.contains("null"), "NULLIF equal → null: {r}");
}

#[test]
fn nullif_unequal_returns_first() {
    let _g = setup();
    let r = dispatch("SELECT NULLIF(10, 20)", &[]);
    assert!(r.contains("10"), "NULLIF unequal → first: {r}");
}

#[test]
fn nullif_with_columns() {
    let _g = setup();
    let r = dispatch("SELECT NULLIF(val, 20) FROM r WHERE id = 'b'", &[]);
    assert!(r.contains("null"), "NULLIF(val,20) on val=20 → null: {r}");
    let r2 = dispatch("SELECT NULLIF(val, 20) FROM r WHERE id = 'a'", &[]);
    assert!(r2.contains("10"), "NULLIF(val,20) on val=10 → 10: {r2}");
}

#[test]
fn nullif_string_equality() {
    let _g = setup();
    let r = dispatch("SELECT NULLIF('x', 'x')", &[]);
    assert!(r.contains("null"), "NULLIF string equal: {r}");
    let r2 = dispatch("SELECT NULLIF('x', 'y')", &[]);
    assert!(r2.contains("\"x\""), "NULLIF string unequal: {r2}");
}

// ── CEIL / FLOOR ────────────────────────────────────────────────────────

#[test]
fn ceil_rounds_up() {
    let _g = setup();
    let r = dispatch("SELECT CEIL(1.1)", &[]);
    assert!(r.contains("2"), "CEIL(1.1) → 2: {r}");
    let r2 = dispatch("SELECT CEIL(-1.9)", &[]);
    assert!(r2.contains("-1"), "CEIL(-1.9) → -1: {r2}");
}

#[test]
fn floor_rounds_down() {
    let _g = setup();
    let r = dispatch("SELECT FLOOR(1.9)", &[]);
    assert!(r.contains("1"), "FLOOR(1.9) → 1: {r}");
    let r2 = dispatch("SELECT FLOOR(-1.1)", &[]);
    assert!(r2.contains("-2"), "FLOOR(-1.1) → -2: {r2}");
}

#[test]
fn ceil_floor_integer_arg() {
    let _g = setup();
    let r = dispatch("SELECT CEIL(5)", &[]);
    assert!(r.contains("5"), "CEIL(5) → 5: {r}");
    let r2 = dispatch("SELECT FLOOR(5)", &[]);
    assert!(r2.contains("5"), "FLOOR(5) → 5: {r2}");
}

#[test]
fn ceil_floor_on_columns() {
    let _g = setup();
    let r = dispatch("SELECT CEIL(val) FROM r WHERE id = 'a'", &[]);
    assert!(r.contains("10"), "CEIL(10) → 10: {r}");
}

#[test]
fn ceil_header_name() {
    let _g = setup();
    let r = dispatch("SELECT CEIL(val) FROM r LIMIT 1", &[]);
    assert!(r.contains("CEIL"), "header is CEIL: {r}");
}

#[test]
fn floor_header_name() {
    let _g = setup();
    let r = dispatch("SELECT FLOOR(val) FROM r LIMIT 1", &[]);
    assert!(r.contains("FLOOR"), "header is FLOOR: {r}");
}

// ── CREATE TABLE AS SELECT (CTAS) ──────────────────────────────────────

#[test]
fn ctas_copies_rows_and_columns() {
    let _g = setup();
    ok_ctas("CREATE TABLE r_copy AS SELECT * FROM r", "full copy");
    let r = dispatch("SELECT COUNT(*) FROM r_copy", &[]);
    assert!(r.contains("4"), "CTAS copied 4 rows: {r}");
}

#[test]
fn ctas_with_where() {
    let _g = setup();
    ok_ctas(
        "CREATE TABLE r_heavy AS SELECT id, name FROM r WHERE val >= 30",
        "filtered copy",
    );
    let r = dispatch("SELECT id FROM r_heavy", &[]);
    assert!(r.contains("c") && r.contains("d"), "only heavy rows: {r}");
    assert!(!r.contains("a"), "light row excluded: {r}");
}

#[test]
fn ctas_distinct() {
    let _g = setup();
    dispatch("INSERT INTO r VALUES ('e','Alpha',50)", &[]);
    ok_ctas("CREATE TABLE r_names AS SELECT DISTINCT name FROM r", "distinct copy");
    let r = dispatch("SELECT COUNT(*) FROM r_names", &[]);
    assert!(r.contains("4"), "4 distinct names: {r}");
}

#[test]
fn ctas_empty_result() {
    let _g = setup();
    ok_ctas("CREATE TABLE r_none AS SELECT * FROM r WHERE val > 999", "empty copy");
    let r = dispatch("SELECT COUNT(*) FROM r_none", &[]);
    assert!(r.contains("0"), "empty CTAS → 0 rows: {r}");
}

#[test]
fn ctas_does_not_copy_primary_key() {
    let _g = setup();
    ok_ctas("CREATE TABLE r_pk AS SELECT * FROM r", "copy");
    // Duplicate id must be allowed in the copy (PK not carried over).
    let r = dispatch("INSERT INTO r_pk VALUES ('a','dup',1)", &[]);
    assert!(r.contains("OK"), "no PK on CTAS copy: {r}");
}

#[test]
fn ctas_derived_from_expression() {
    let _g = setup();
    ok_ctas(
        "CREATE TABLE r_squared AS SELECT id, val * val AS sq FROM r",
        "expression copy",
    );
    let r = dispatch("SELECT sq FROM r_squared WHERE id = 'a'", &[]);
    assert!(r.contains("100"), "10² = 100: {r}");
}

#[test]
fn ctas_if_not_exists_ok() {
    let _g = setup();
    ok_ctas("CREATE TABLE r_ine AS SELECT * FROM r", "first create");
    let r = dispatch("CREATE TABLE IF NOT EXISTS r_ine AS SELECT * FROM r", &[]);
    assert!(r.contains("OK"), "IF NOT EXISTS on existing: {r}");
}

// ── Aggregate FILTER (WHERE ...) ────────────────────────────────────────

#[test]
fn filter_count_basic() {
    let _g = setup();
    let r = dispatch("SELECT COUNT(*) FILTER (WHERE val >= 30) AS big FROM r", &[]);
    assert!(r.contains("2"), "COUNT FILTER ≥30 → 2: {r}");
}

#[test]
fn filter_sum() {
    let _g = setup();
    let r = dispatch("SELECT SUM(val) FILTER (WHERE val >= 30) FROM r", &[]);
    assert!(r.contains("70"), "SUM FILTER → 30+40=70: {r}");
}

#[test]
fn filter_with_group_by() {
    let _g = setup();
    let r = dispatch("SELECT id, COUNT(*) FILTER (WHERE val > 15) FROM r GROUP BY id", &[]);
    assert!(r.contains("a") && r.contains("1"), "a has 1 big: {r}");
    assert!(r.contains("c") && r.contains("1"), "c has 1 big: {r}");
}

#[test]
fn filter_all_excluded() {
    let _g = setup();
    let r = dispatch("SELECT COUNT(*) FILTER (WHERE val > 999) FROM r", &[]);
    assert!(r.contains("0"), "no rows pass filter: {r}");
}

#[test]
fn filter_multiple_aggregates() {
    let _g = setup();
    let r = dispatch(
        "SELECT COUNT(*) FILTER (WHERE val >= 30) AS hi, COUNT(*) FILTER (WHERE val < 30) AS lo FROM r",
        &[],
    );
    assert!(r.contains("2"), "hi = 2: {r}");
    assert!(r.contains("\"lo\""), "lo column present: {r}");
}

#[test]
fn filter_with_avg() {
    let _g = setup();
    let r = dispatch("SELECT AVG(val) FILTER (WHERE val >= 30) FROM r", &[]);
    assert!(r.contains("35"), "AVG FILTER → (30+40)/2 = 35: {r}");
}

#[test]
fn filter_complex_condition() {
    let _g = setup();
    let r = dispatch("SELECT COUNT(*) FILTER (WHERE val BETWEEN 15 AND 35) FROM r", &[]);
    assert!(r.contains("2"), "BETWEEN filter → b(20)+c(30): {r}");
}

#[test]
fn filter_with_string_predicate() {
    let _g = setup();
    let r = dispatch("SELECT COUNT(*) FILTER (WHERE name LIKE 'A%') FROM r", &[]);
    assert!(r.contains("1"), "LIKE filter → Alpha: {r}");
}

// ── MIN / MAX NULL-skipping ─────────────────────────────────────────────

#[test]
fn min_skips_null() {
    let _g = setup();
    dispatch("CREATE TABLE r_null (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO r_null VALUES ('a', 10), ('b', NULL), ('c', 5)", &[]);
    let r = dispatch("SELECT MIN(v) FROM r_null", &[]);
    assert!(r.contains("5"), "MIN skips NULL → 5: {r}");
}

#[test]
fn max_skips_null() {
    let _g = setup();
    dispatch("CREATE TABLE r_null2 (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO r_null2 VALUES ('a', 10), ('b', NULL), ('c', 5)", &[]);
    let r = dispatch("SELECT MAX(v) FROM r_null2", &[]);
    assert!(r.contains("10"), "MAX skips NULL → 10: {r}");
}

#[test]
fn min_max_all_null_returns_null() {
    let _g = setup();
    // SQLite semantics: MIN/MAX over an all-NULL column returns NULL.
    dispatch("CREATE TABLE r_null3 (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO r_null3 VALUES ('a', NULL), ('b', NULL)", &[]);
    let r = dispatch("SELECT MIN(v) FROM r_null3", &[]);
    assert!(r.contains("null"), "MIN all-NULL → null: {r}");
    let r2 = dispatch("SELECT MAX(v) FROM r_null3", &[]);
    assert!(r2.contains("null"), "MAX all-NULL → null: {r2}");
}

#[test]
fn min_max_empty_table_returns_null() {
    let _g = setup();
    // SQLite semantics: MIN/MAX over an empty table returns NULL.
    dispatch("CREATE TABLE r_null4 (id STRING PRIMARY KEY, v INT)", &[]);
    let r = dispatch("SELECT MIN(v) FROM r_null4", &[]);
    assert!(r.contains("null"), "MIN empty table → null: {r}");
    let r2 = dispatch("SELECT MAX(v) FROM r_null4", &[]);
    assert!(r2.contains("null"), "MAX empty table → null: {r2}");
}

#[test]
fn min_max_with_string_nulls() {
    let _g = setup();
    dispatch("CREATE TABLE r_null5 (id STRING PRIMARY KEY, name STRING)", &[]);
    dispatch(
        "INSERT INTO r_null5 VALUES ('a', 'delta'), ('b', NULL), ('c', 'alpha')",
        &[],
    );
    let r = dispatch("SELECT MIN(name) FROM r_null5", &[]);
    assert!(r.contains("alpha"), "MIN string skips NULL: {r}");
    let r2 = dispatch("SELECT MAX(name) FROM r_null5", &[]);
    assert!(r2.contains("delta"), "MAX string skips NULL: {r2}");
}

fn ok_ctas(sql: &str, label: &str) {
    let r = dispatch(sql, &[]);
    assert!(r.contains("[0,"), "FAIL {label}: {r}");
}
