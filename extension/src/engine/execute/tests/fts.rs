// Trigram full-text search and FTS score tests

use super::helpers::*;

#[test]
fn trigram_index_fuzzy_after_insert() {
    let mut db = make_indexed_db();
    parse_and_exec("INSERT INTO idx_test VALUES ('rhs_m4a1', 1)", &mut db).unwrap();
    let result = parse_and_exec("SELECT k FROM idx_test WHERE k %% 'rhs_m4'", &mut db).unwrap();
    assert!(result.contains("rhs_m4a1"), "trigram index: {}", result);
}

#[test]
fn trigram_index_used_for_fuzzy_match() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, name STRING)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('1', 'rhs_m4a1')", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('2', 'rhs_m4a1_carryhandle')", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('3', 'hlc_ak74')", &mut db).unwrap();
    parse_and_exec("CREATE INDEX trigram_name ON t (name) USING TRIGRAM", &mut db).unwrap();
    let r = parse_and_exec("SELECT id FROM t WHERE name %% 'rhs_m4'", &mut db).unwrap();
    assert!(r.contains("1"), "should match rhs_m4a1: {}", r);
    assert!(!r.contains("3"), "should NOT match hlc_ak74: {}", r);
}

#[test]
fn fts_score_function() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, name STRING)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('1', 'hello world')", &mut db).unwrap();
    let r = parse_and_exec("SELECT fts_score(name, 'hello') FROM t WHERE id = '1'", &mut db).unwrap();
    assert!(
        r.contains("0.") || r.contains("1."),
        "fts_score should be a float: {}",
        r
    );
}
