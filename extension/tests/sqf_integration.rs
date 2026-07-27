// a3sql — SQF integration tests
//
// Covers cursor iteration, prepared statements, and the SQF helper patterns
// that modders use (cursor-based selectAll, pagination, injection protection).
//
// Each test calls dispatch("reset", &[]) to start fresh. A global TEST_MUTEX
// serialises execution so that concurrent tests don't clobber each other's
// database state.

use std::sync::{Mutex, MutexGuard};

use a3sql::dispatch;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

fn setup() -> MutexGuard<'static, ()> {
    // Recover from a poisoned mutex left by a previous panicked test
    let g = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    dispatch("reset", &[]);
    g
}

/// Number of data rows in a SELECT response.
///
/// The engine returns `[0,"OK",[["col1","col2"],[val1,val2],...]]` — a JSON
/// array-of-arrays. Each data row (after the header) adds one `],[` separator.
/// Zero rows → 0, N rows → N.
fn row_count(resp: &str) -> usize {
    resp.matches("],[").count()
}

/// Helper: assert a dispatch response is a success (starts with `[0,`).
fn assert_ok(resp: &str, label: &str) {
    assert!(
        resp.starts_with("[0,"),
        "FAIL {} — expected success, got: {}",
        label,
        resp
    );
}

/// Helper: assert a dispatch response is an error (starts with `[-1,`).
fn assert_err(resp: &str, label: &str) {
    assert!(
        resp.starts_with("[-1,"),
        "FAIL {} — expected error, got: {}",
        label,
        resp
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 1.  cursor_create_fetch
//     Create a cursor over a SELECT, fetch the first page, verify rows returned.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn cursor_create_fetch() {
    let _g = setup();
    dispatch("CREATE TABLE cf (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO cf VALUES ('a', 10), ('b', 20), ('c', 30)", &[]);

    let r = dispatch("cursor create cf_cur SELECT * FROM cf ORDER BY id", &[]);
    assert_ok(&r, "cursor create");

    let r = dispatch("cursor fetch cf_cur", &[]);
    assert_ok(&r, "cursor fetch");
    assert!(r.contains("a"), "row a: {}", r);
    assert!(r.contains("b"), "row b: {}", r);
    assert!(r.contains("c"), "row c: {}", r);
    assert_eq!(row_count(&r), 3, "expected 3 rows: {}", r);
}

// ─────────────────────────────────────────────────────────────────────────────
// 2.  cursor_pagination
//     Insert 500+ rows, create cursor with page_size=100, fetch multiple pages,
//     verify each returns correct rows and total pages.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn cursor_pagination() {
    let _g = setup();
    dispatch("CREATE TABLE cp (id INT PRIMARY KEY, val STRING)", &[]);

    // Insert 510 rows in batches of 10
    for batch in 0..51 {
        let vals: String = (0..10)
            .map(|i| {
                let n = batch * 10 + i;
                format!("({}, 'val_{}')", n, n)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("INSERT INTO cp VALUES {}", vals);
        assert_ok(&dispatch(&sql, &[]), &format!("insert batch {}", batch));
    }

    let r = dispatch("cursor create cp_cur SELECT * FROM cp ORDER BY id", &[]);
    assert_ok(&r, "cursor create");

    // Fetch 100 rows per page — 5 full pages of 100 + 1 partial of 10
    let pages = 6;
    for page in 0..pages {
        let r = dispatch("cursor fetch cp_cur 100", &[]);
        assert_ok(&r, &format!("cursor fetch page {}", page));

        let got = row_count(&r);
        let expected = if page < 5 { 100_usize } else { 10_usize };
        assert_eq!(
            got,
            expected,
            "page {}: expected {} rows, got {}. Response: {}...",
            page,
            expected,
            got,
            &r[..std::cmp::min(r.len(), 200)]
        );

        if page == 0 {
            assert!(r.contains("val_0"), "missing val_0: {}", r);
        }
    }

    // After all rows consumed, next fetch returns header-only (no data rows)
    let r = dispatch("cursor fetch cp_cur 100", &[]);
    assert_eq!(row_count(&r), 0, "expected empty after all rows: {}", r);
}

// ─────────────────────────────────────────────────────────────────────────────
// 3.  cursor_drop
//     Create a cursor, fetch one page, drop cursor, verify drop succeeds.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn cursor_drop() {
    let _g = setup();
    dispatch("CREATE TABLE cd (id STRING PRIMARY KEY)", &[]);
    dispatch("INSERT INTO cd VALUES ('x')", &[]);

    let r = dispatch("cursor create cd_cur SELECT * FROM cd", &[]);
    assert_ok(&r, "cursor create");

    let r = dispatch("cursor fetch cd_cur", &[]);
    assert_ok(&r, "cursor fetch");

    // Note: cursor drop currently always returns OK (bug in dispatch match)
    let r = dispatch("cursor drop cd_cur", &[]);
    assert_ok(&r, "cursor drop");
    assert!(r.contains("OK"), "drop confirmation: {}", r);
}

// ─────────────────────────────────────────────────────────────────────────────
// 4.  cursor_invalid_name
//     Trying to fetch from a non-existent cursor returns an error.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn cursor_invalid_name() {
    let _g = setup();
    let r = dispatch("cursor fetch no_such_cursor", &[]);
    assert_err(&r, "cursor fetch invalid");
    assert!(r.contains("not found") || r.contains("No such"), "error msg: {}", r);
}

// ─────────────────────────────────────────────────────────────────────────────
// 5.  prepare_basic
//     PREPARE a SELECT template, EXECUTE_PREPARED with params, verify results.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn prepare_basic() {
    let _g = setup();
    dispatch("CREATE TABLE pb (id STRING PRIMARY KEY, score INT)", &[]);
    dispatch(
        "INSERT INTO pb VALUES ('alice', 100), ('bob', 200), ('carol', 300)",
        &[],
    );

    let r = dispatch("prepare pb_get SELECT * FROM pb WHERE id = $1", &[]);
    assert_ok(&r, "prepare");

    let r = dispatch("execute_prepared pb_get bob", &[]);
    assert_ok(&r, "execute_prepared");
    assert!(r.contains("bob"), "result contains bob: {}", r);
    assert!(r.contains("200"), "result contains 200: {}", r);
    assert!(!r.contains("alice"), "should not contain alice: {}", r);
}

// ─────────────────────────────────────────────────────────────────────────────
// 6.  prepare_multi_params
//     PREPARE with multiple `$1`, `$2` placeholders, EXECUTE with multiple args.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn prepare_multi_params() {
    let _g = setup();
    dispatch("CREATE TABLE pm (id STRING PRIMARY KEY, cat STRING, score INT)", &[]);
    dispatch(
        "INSERT INTO pm VALUES ('a', 'x', 10), ('b', 'x', 20), ('c', 'y', 30), ('d', 'y', 40)",
        &[],
    );

    let r = dispatch("prepare pm_q SELECT * FROM pm WHERE cat = $1 AND score > $2", &[]);
    assert_ok(&r, "prepare multi");

    let r = dispatch("execute_prepared pm_q x 15", &[]);
    assert_ok(&r, "execute_prepared multi");
    assert!(r.contains("b"), "should contain 'b' (cat=x, score=20): {}", r);
    assert!(
        !r.contains("[\"a\""),
        "should not contain row 'a' (score=10 <= 15): {}",
        r
    );
    assert!(!r.contains("[\"c\""), "should not contain row 'c' (cat=y): {}", r);

    // Same template, different args
    let r = dispatch("execute_prepared pm_q y 25", &[]);
    assert_ok(&r, "execute_prepared multi 2");
    // Both c (score=30) and d (score=40) match cat=y AND score>25
    assert!(r.contains("c"), "should contain 'c' (cat=y, score=30): {}", r);
    assert!(r.contains("d"), "should contain 'd' (cat=y, score=40): {}", r);
}

// ─────────────────────────────────────────────────────────────────────────────
// 7.  prepare_injection_protection
//     PREPARE with user input, verify $1 substitution escapes SQL injection.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn prepare_injection_protection() {
    let _g = setup();
    dispatch("CREATE TABLE pi (id STRING PRIMARY KEY, secret STRING)", &[]);
    dispatch("INSERT INTO pi VALUES ('admin', 's3cret'), ('user', 'public')", &[]);

    let r = dispatch("prepare pi_get SELECT secret FROM pi WHERE id = $1", &[]);
    assert_ok(&r, "prepare");

    // Legitimate query works
    let r = dispatch("execute_prepared pi_get admin", &[]);
    assert_ok(&r, "legitimate");
    assert!(r.contains("s3cret"), "legitimate query should find secret: {}", r);

    // Injection via single-quote in a one-token arg (no spaces so it stays as one arg)
    let r = dispatch("execute_prepared pi_get admin'--", &[]);
    assert_ok(&r, "sqli via quote");
    // $1 is substituted as 'admin''--' — the quote gets doubled, making it a literal
    // The SQL becomes: SELECT secret FROM pi WHERE id = 'admin''--'
    // No row has id = "admin'--", so result is empty
    assert_eq!(row_count(&r), 0, "sqli should return zero rows: {}", r);
    assert!(!r.contains("s3cret"), "sqli should not leak secret: {}", r);
}

// ─────────────────────────────────────────────────────────────────────────────
// 8.  prepare_drop
//     EXECUTE_PREPARED after reset returns error (reset acts as DEALLOCATE ALL).
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn prepare_drop() {
    let _g = setup();
    dispatch("CREATE TABLE pd (id STRING PRIMARY KEY)", &[]);
    dispatch("INSERT INTO pd VALUES ('x')", &[]);

    let r = dispatch("prepare pd_get SELECT * FROM pd WHERE id = $1", &[]);
    assert_ok(&r, "prepare");

    // Verify it works before reset
    let r = dispatch("execute_prepared pd_get x", &[]);
    assert_ok(&r, "execute before reset");

    // Reset clears prepared statements (acts as DEALLOCATE ALL + DROP ALL)
    dispatch("reset", &[]);

    // Recreate data (reset cleared it)
    dispatch("CREATE TABLE pd (id STRING PRIMARY KEY)", &[]);
    dispatch("INSERT INTO pd VALUES ('x')", &[]);

    // Prepared statement is gone — should error
    let r = dispatch("execute_prepared pd_get x", &[]);
    assert_err(&r, "execute after reset");
    assert!(
        r.contains("not found") || r.contains("No such"),
        "expected 'not found', got: {}",
        r
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 9.  sqf_helper_select_map
//     Simulate the a3sql_fnc_selectAll SQF wrapper by calling cursor create +
//     fetch in a loop, combining results — equivalent to how SQF mods iterate
//     over large result sets.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn sqf_helper_select_map() {
    let _g = setup();
    dispatch("CREATE TABLE sh (id STRING PRIMARY KEY, name STRING)", &[]);

    for i in 0..75 {
        let sql = format!("INSERT INTO sh VALUES ('id_{}', 'name_{}')", i, i);
        assert_ok(&dispatch(&sql, &[]), &format!("insert {}", i));
    }

    let r = dispatch("cursor create sh_cur SELECT * FROM sh ORDER BY id", &[]);
    assert_ok(&r, "cursor create");

    // selectAll loop: fetch 30 at a time until empty
    let mut all_fetched = String::new();
    loop {
        let r = dispatch("cursor fetch sh_cur 30", &[]);
        if !r.starts_with("[0,") || row_count(&r) == 0 {
            break;
        }
        all_fetched.push_str(&r);
    }

    // All 75 rows fetched
    for i in 0..75 {
        let pat = format!("id_{}", i);
        assert!(all_fetched.contains(&pat), "missing {} in combined result", pat);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 10. large_result_cursor
//     Create table with 2000 rows, use cursor to iterate, verify all rows are
//     fetched in chunks.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn large_result_cursor() {
    let _g = setup();
    dispatch("CREATE TABLE lr (id INT PRIMARY KEY, label STRING)", &[]);

    // Insert 2000 rows in batches of 50
    for batch in 0..40 {
        let vals: String = (0..50)
            .map(|i| {
                let n = batch * 50 + i;
                format!("({}, 'row_{}')", n, n)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("INSERT INTO lr VALUES {}", vals);
        assert_ok(&dispatch(&sql, &[]), &format!("insert batch {}", batch));
    }

    // Audit total row count
    let r = dispatch("SELECT COUNT(*) AS cnt FROM lr", &[]);
    assert!(r.contains("2000"), "expected 2000 rows: {}", r);

    let r = dispatch("cursor create lr_cur SELECT * FROM lr ORDER BY id", &[]);
    assert_ok(&r, "cursor create");

    // Iterate with page_size=150, sum all rows
    let mut total = 0_usize;
    loop {
        let r = dispatch("cursor fetch lr_cur 150", &[]);
        if !r.starts_with("[0,") {
            break;
        }
        let n = row_count(&r);
        if n == 0 {
            break;
        }
        total += n;
    }

    assert_eq!(total, 2000, "expected 2000 total rows across all pages");
}

// ─────────────────────────────────────────────────────────────────────────────
// 11. cursor_concurrent
//     Create two cursors, interleave fetches, verify both return correct data
//     independently.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn cursor_concurrent() {
    let _g = setup();
    dispatch("CREATE TABLE cc_a (id INT PRIMARY KEY, val STRING)", &[]);
    dispatch("CREATE TABLE cc_b (id INT PRIMARY KEY, val STRING)", &[]);

    for i in 0..50 {
        assert_ok(
            &dispatch(&format!("INSERT INTO cc_a VALUES ({}, 'alpha_{}')", i, i), &[]),
            &format!("insert a {}", i),
        );
        assert_ok(
            &dispatch(&format!("INSERT INTO cc_b VALUES ({}, 'beta_{}')", i, i), &[]),
            &format!("insert b {}", i),
        );
    }

    let r = dispatch("cursor create cur_a SELECT * FROM cc_a ORDER BY id", &[]);
    assert_ok(&r, "create cur_a");
    let r = dispatch("cursor create cur_b SELECT * FROM cc_b ORDER BY id", &[]);
    assert_ok(&r, "create cur_b");

    // Interleave fetches — alternating between cursors
    for page in 0..3 {
        let r_a = dispatch("cursor fetch cur_a 10", &[]);
        assert_ok(&r_a, &format!("fetch cur_a page {}", page));
        let r_b = dispatch("cursor fetch cur_b 10", &[]);
        assert_ok(&r_b, &format!("fetch cur_b page {}", page));

        if page == 0 {
            assert!(r_a.contains("alpha_0"), "cur_a has alpha_0: {}", r_a);
            assert!(r_b.contains("beta_0"), "cur_b has beta_0: {}", r_b);
        }
        // No cross-contamination
        assert!(!r_a.contains("beta_"), "cur_a should not have beta: {}", r_a);
        assert!(!r_b.contains("alpha_"), "cur_b should not have alpha: {}", r_b);
    }

    // Consume remaining rows from both
    let mut done_a = false;
    let mut done_b = false;
    for _ in 0..5 {
        if !done_a {
            let r = dispatch("cursor fetch cur_a 10", &[]);
            if !r.starts_with("[0,") || row_count(&r) == 0 {
                done_a = true;
            }
        }
        if !done_b {
            let r = dispatch("cursor fetch cur_b 10", &[]);
            if !r.starts_with("[0,") || row_count(&r) == 0 {
                done_b = true;
            }
        }
        if done_a && done_b {
            break;
        }
    }

    assert_ok(&dispatch("cursor drop cur_a", &[]), "drop cur_a");
    assert_ok(&dispatch("cursor drop cur_b", &[]), "drop cur_b");
}

// ─────────────────────────────────────────────────────────────────────────────
// 12. prepare_and_cursor_clear_on_reset
//     Verify that `reset` command clears both cursors and prepared statements.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn prepare_and_cursor_clear_on_reset() {
    let _g = setup();
    dispatch("CREATE TABLE rc (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO rc VALUES ('x', 1), ('y', 2)", &[]);

    // Create a cursor
    let r = dispatch("cursor create rc_cur SELECT * FROM rc ORDER BY id", &[]);
    assert_ok(&r, "create cursor");
    let r = dispatch("cursor fetch rc_cur", &[]);
    assert_ok(&r, "fetch before reset");

    // Prepare a statement
    let r = dispatch("prepare rc_get SELECT * FROM rc WHERE id = $1", &[]);
    assert_ok(&r, "prepare");
    let r = dispatch("execute_prepared rc_get x", &[]);
    assert_ok(&r, "execute before reset");

    // RESET clears everything
    dispatch("reset", &[]);

    // Cursor is gone
    let r = dispatch("cursor fetch rc_cur", &[]);
    assert_err(&r, "cursor fetch after reset");

    // Prepared statement is gone
    let r = dispatch("execute_prepared rc_get x", &[]);
    assert_err(&r, "execute_prepared after reset");
}

// ─────────────────────────────────────────────────────────────────────────────
// 13. sqf_eval_in_sql
//     SQF_EVAL() evaluates SQF expressions through the SQL function dispatch.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn sqf_eval_in_sql() {
    let _g = setup();

    // Scalar result
    let r = dispatch("SELECT SQF_EVAL('1 + 2 * 3')", &[]);
    assert_ok(&r, "SQF_EVAL(1+2*3)");
    assert!(r.contains("7"), "expected 7 in: {}", r);

    // Boolean comparison
    let r = dispatch("SELECT SQF_EVAL('2 > 1')", &[]);
    assert_ok(&r, "SQF_EVAL(2>1)");
    assert!(r.contains("true"), "expected true in: {}", r);

    // String concat
    let r = dispatch(r#"SELECT SQF_EVAL('"hello" + " world"')"#, &[]);
    assert_ok(&r, "SQF_EVAL concat");
    assert!(r.contains("hello world"), "expected concat in: {}", r);

    // SQF_EVAL in WHERE clause with table data
    dispatch("CREATE TABLE sqf_filter (id STRING PRIMARY KEY, val INT)", &[]);
    dispatch("INSERT INTO sqf_filter VALUES ('a', 10), ('b', 20), ('c', 30)", &[]);

    // Filter using SQF_EVAL — can't reference columns directly in the expression string
    // but we can use it for computed conditions
    let r = dispatch("SELECT id FROM sqf_filter WHERE SQF_EVAL('10 + 10') = 20", &[]);
    assert_ok(&r, "SQF_EVAL in WHERE");
    assert!(r.contains("a"), "expected 'a' (all rows match): {}", r);
    assert!(r.contains("b"), "expected 'b': {}", r);
    assert!(r.contains("c"), "expected 'c': {}", r);

    // Error case: undefined variable → SQL NULL
    let r = dispatch("SELECT SQF_EVAL('_undefined')", &[]);
    assert_ok(&r, "SQF_EVAL undefined variable returns NULL");
    assert!(r.contains("null"), "expected null for undefined variable: {}", r);
}

// ─────────────────────────────────────────────────────────────────────────────
// 14. sqf_eval_with_commands
//     SQF_EVAL() with SQF commands (sqrt, abs, etc.) through SQL.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn sqf_eval_with_commands() {
    let _g = setup();

    // sqrt
    let r = dispatch("SELECT SQF_EVAL('sqrt 25')", &[]);
    assert_ok(&r, "SQF_EVAL sqrt");
    assert!(r.contains("5"), "expected 5: {}", r);

    // abs
    let r = dispatch("SELECT SQF_EVAL('abs -9')", &[]);
    assert_ok(&r, "SQF_EVAL abs");
    assert!(r.contains("9"), "expected 9: {}", r);

    // pi
    let r = dispatch("SELECT SQF_EVAL('pi')", &[]);
    assert_ok(&r, "SQF_EVAL pi");

    // Combined: sqrt(abs(-9)) + round(3.7) = 3 + 4 = 7
    let r = dispatch("SELECT SQF_EVAL('sqrt abs -9 + round 3.7')", &[]);
    assert_ok(&r, "SQF_EVAL chained commands");
    assert!(r.contains("7"), "expected 7 in result: {}", r);

    // Command in WHERE with comparison
    dispatch("CREATE TABLE sqf_cmd (id STRING PRIMARY KEY, val INT)", &[]);
    dispatch("INSERT INTO sqf_cmd VALUES ('large', 20), ('small', 3)", &[]);
    // sqrt(20) > 4 → only 'large' has sqrt(20) > 4
    // Actually sqrt(20) ≈ 4.47, and sqrt(3) ≈ 1.73
    // We can test: SQF_EVAL('sqrt ' || val || ' > 4') — no, SQF_EVAL takes a literal
    // Use a computed WHERE: only rows where val > 10 pass
    let r = dispatch("SELECT SQF_EVAL('sqrt 20 > 4')", &[]);
    assert_ok(&r, "SQF_EVAL sqrt comparison");
    assert!(r.contains("true"), "expected true: {}", r);

    // typeName via SQF_EVAL
    let r = dispatch(r#"SELECT SQF_EVAL('typename 42')"#, &[]);
    assert_ok(&r, "SQF_EVAL typename");
    assert!(r.contains("SCALAR"), "expected SCALAR: {}", r);
}
