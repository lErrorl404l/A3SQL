// a3sql — Confirmed bug regression tests
// Once fixed, these should pass. If they fail, the bug has regressed.

use a3sql::dispatch;

fn ok(resp: &str, label: &str) {
    if !resp.contains("[0,") {
        panic!("FAIL {} — expected success, got: {}", label, resp);
    }
}

#[test]
fn bug_trim_returns_null() {
    let r = dispatch("SELECT TRIM('  hello  ') AS t", &[]);
    assert!(r.contains("hello"), "TRIM should return 'hello', got: {}", r);
}

#[test]
fn bug_substr_returns_null() {
    let r = dispatch("SELECT SUBSTR('hello', 1, 2) AS s", &[]);
    assert!(r.contains("he"), "SUBSTR should return 'he', got: {}", r);
}

#[test]
fn bug_replace_into_duplicate_key() {
    ok(
        &dispatch("CREATE TABLE a_br (id STRING PRIMARY KEY, v INT)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_br VALUES ('a', 10)", &[]), "INSERT");
    let r = dispatch("REPLACE INTO a_br VALUES ('a', 99)", &[]);
    assert!(r.contains("[0,"), "REPLACE INTO should work, got: {}", r);
}

#[test]
fn bug_trim_with_table() {
    ok(
        &dispatch("CREATE TABLE a_bt (id STRING PRIMARY KEY, name STRING)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_bt VALUES ('a', '  hello  ')", &[]), "INSERT");
    let r = dispatch("SELECT TRIM(name) FROM a_bt WHERE id = 'a'", &[]);
    assert!(r.contains("hello"), "TRIM with table should work, got: {}", r);
}

#[test]
fn bug_substr_with_table() {
    ok(
        &dispatch("CREATE TABLE a_bs (id STRING PRIMARY KEY, name STRING)", &[]),
        "CREATE",
    );
    ok(&dispatch("INSERT INTO a_bs VALUES ('a', 'hello')", &[]), "INSERT");
    let r = dispatch("SELECT SUBSTR(name, 1, 2) FROM a_bs WHERE id = 'a'", &[]);
    assert!(r.contains("he"), "SUBSTR with table should work, got: {}", r);
}
