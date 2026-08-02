// P1 — SQL parse cache tests: identical text must skip the re-parse, the
// cached AST must always evaluate against the live Database, multi-statement
// input must cache per split statement, and the cache must cap + evict + clear.

use super::helpers::*;
use crate::engine::execute::execute;

fn exec(db: &mut Database, stmts: &[sqlparser::ast::Statement]) -> String {
    let mut last = String::new();
    for stmt in stmts {
        last = execute(stmt, db).unwrap();
    }
    last
}

#[test]
fn identical_sql_twice_returns_identical_results() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    let sql = "SELECT * FROM items WHERE id = 'a'";
    let stmts1 = db.cached_parse(sql).unwrap();
    let r1 = exec(&mut db, &stmts1);
    let stmts2 = db.cached_parse(sql).unwrap();
    let r2 = exec(&mut db, &stmts2);
    assert_eq!(r1, r2, "cached and uncached executions must be byte-identical");
    let (len, hits) = db.cache_stats();
    assert_eq!((len, hits), (1, 1), "one entry; the second parse must be a cache hit");
}

#[test]
fn cached_ast_reevaluates_against_live_db_after_schema_change() {
    // The cache holds ASTs only (no resolved tables/columns/views). After
    // DROP + recreate with different data, the cached AST must produce the
    // NEW data — proof that no stale semantics can leak from the cache.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (k STRING PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', 1)", &mut db).unwrap();
    let sql = "SELECT v FROM t WHERE k = 'x'";
    let stmts1 = db.cached_parse(sql).unwrap();
    let r1 = exec(&mut db, &stmts1);
    assert!(r1.contains("1"), "first result: {}", r1);
    assert_eq!(db.cache_stats().1, 0, "first parse is a miss");

    // DROP + recreate the table with different data.
    parse_and_exec("DROP TABLE t", &mut db).unwrap();
    parse_and_exec("CREATE TABLE t (k STRING PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', 99)", &mut db).unwrap();

    // Same text → served from cache, but must see the live table.
    let stmts = db.cached_parse(sql).unwrap();
    assert_eq!(db.cache_stats().1, 1, "re-parse of same text must be a cache hit");
    let r2 = exec(&mut db, &stmts);
    assert!(
        r2.contains("99") && !r2.contains("1"),
        "cached AST must reflect live data, got: {}",
        r2
    );
}

#[test]
fn multi_statement_input_cached_per_statement() {
    // dispatch splits input on ';' (split_sql) and parses each part
    // separately; the cache must memoize each part so a repeated
    // multi-statement input re-parses nothing.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (k STRING PRIMARY KEY, v INT)", &mut db).unwrap();
    let parts = crate::dispatch::split_sql("INSERT INTO t VALUES ('a', 1); SELECT * FROM t");
    assert_eq!(parts.len(), 2, "split_sql must yield both statements");
    for p in &parts {
        db.cached_parse(p).unwrap();
    }
    assert_eq!(db.cache_stats(), (2, 0), "two distinct parts parsed once each");
    // Repeat the same multi-statement input — both parts must hit.
    for p in &parts {
        db.cached_parse(p).unwrap();
    }
    assert_eq!(db.cache_stats(), (2, 2), "repeat must hit both cached parts");
}

#[test]
#[cfg_attr(miri, ignore)] // 513 sqlparser parses are too slow under miri's interpreter
fn cache_evicts_oldest_beyond_capacity() {
    let mut db = Database::new();
    let cap = crate::engine::database::sql_cache::DEFAULT_CAPACITY;
    // Insert cap + 1 distinct statements — the first is evicted, the last resident.
    for i in 0..=cap {
        db.cached_parse(&format!("SELECT 1 AS x WHERE {} = {}", i, i)).unwrap();
    }
    let (len, _) = db.cache_stats();
    assert_eq!(len, cap, "cache must hold at most capacity entries");
    // FIFO: the first-inserted statement was evicted → re-parse is a miss.
    let (_, hits0) = db.cache_stats();
    db.cached_parse("SELECT 1 AS x WHERE 0 = 0").unwrap();
    let (_, hits1) = db.cache_stats();
    assert_eq!(hits1, hits0, "evicted entry must be a cache miss");
    // Re-parsed → now resident → next parse hits.
    db.cached_parse("SELECT 1 AS x WHERE 0 = 0").unwrap();
    let (_, hits2) = db.cache_stats();
    assert_eq!(hits2, hits0 + 1, "resident entry must hit");
    // The most-recently-inserted statement is still resident.
    db.cached_parse(&format!("SELECT 1 AS x WHERE {} = {}", cap, cap))
        .unwrap();
    let (_, hits3) = db.cache_stats();
    assert_eq!(hits3, hits0 + 2, "most recent entry stays resident");
}

#[test]
fn cache_cleared_on_db_clear() {
    let mut db = Database::new();
    db.cached_parse("SELECT 1").unwrap();
    assert_eq!(db.cache_stats().0, 1);
    db.clear(); // reset / db.clear() must drop cached ASTs
    assert_eq!(db.cache_stats().0, 0, "reset/db.clear() must clear the parse cache");
}
