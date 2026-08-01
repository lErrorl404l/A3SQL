// Edge case tests: empty tables, NULL handling, bulk operations

use super::helpers::*;

#[test]
fn empty_table_select() {
    let mut db = make_test_db();
    let result = parse_and_exec("SELECT * FROM items", &mut db).unwrap();
    assert_eq!(result, "[[\"id\",\"name\",\"value\"]]");
}

#[test]
fn empty_where_select() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items WHERE 1 = 0", &mut db).unwrap();
    assert_eq!(result, "[[\"id\",\"name\",\"value\"]]");
}

#[test]
fn null_insert() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('n', NULL, 99)", &mut db).unwrap();
    let result = parse_and_exec("SELECT * FROM items WHERE id = 'n'", &mut db).unwrap();
    assert!(result.contains("null"));
}

#[test]
fn bulk_insert_500() {
    let mut db = Database::new();
    let cols = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "v".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let t = Table::new("bulk".into(), cols).unwrap();
    db.create_table("bulk", t).unwrap();
    // ponytail: miri interprets every instruction; a small batch exercises the
    // same code path while keeping the miri CI job fast. Native runs full size.
    let n = if cfg!(miri) { 50 } else { 500 };
    for i in 0..n {
        parse_and_exec(&format!("INSERT INTO bulk VALUES ({},{})", i, i * 2), &mut db).unwrap();
    }
    let r = parse_and_exec("SELECT COUNT(*) FROM bulk", &mut db).unwrap();
    assert!(r.contains(&n.to_string()), "count: {}", r);
    let s = parse_and_exec("SELECT SUM(v) FROM bulk", &mut db).unwrap();
    assert!(s.contains(&(n * (n - 1)).to_string()), "sum: {}", s);
}

#[test]
fn string_with_semicolon() {
    let mut db = make_test_db();
    let sql = "INSERT INTO items VALUES ('sc', 'a;b', 1)";
    parse_and_exec(sql, &mut db).unwrap();
    let r = parse_and_exec("SELECT * FROM items WHERE id = 'sc'", &mut db).unwrap();
    assert!(r.contains("a;b"));
}

#[test]
fn order_empty_table() {
    let mut db = make_test_db();
    let r = parse_and_exec("SELECT * FROM items ORDER BY value", &mut db).unwrap();
    assert_eq!(r, "[[\"id\",\"name\",\"value\"]]");
}

#[test]
fn null_arithmetic() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('nx', 'null_test', NULL)", &mut db).unwrap();
    let r = parse_and_exec("SELECT * FROM items WHERE value IS NULL", &mut db).unwrap();
    assert!(r.contains("null_test"), "null: {}", r);
}

#[test]
fn fuzzy_fn_call_integration() {
    let mut db = make_test_db();
    parse_and_exec("INSERT INTO items VALUES ('fn_test', 'hello', 1)", &mut db).unwrap();
    let r = parse_and_exec("SELECT * FROM items WHERE id %% 'fn_t'", &mut db).unwrap();
    assert!(r.contains("fn_test"), "fuzzy fn: {}", r);
}
#[test]
#[cfg_attr(miri, ignore)] // SystemTime::now() — realtime clock blocked by miri's isolation
fn datetime_now_in_insert_values() {
    // Bug B regression: INSERT ... VALUES with datetime('now') must not fail
    // (mod SQL: INSERT INTO server_commands ... datetime('now'))
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INTEGER PRIMARY KEY, created_at TEXT)", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t VALUES (1, datetime('now'))", &mut db);
    assert!(r.is_ok(), "datetime('now') in VALUES: {:?}", r);
    let sel = parse_and_exec("SELECT created_at FROM t", &mut db).unwrap();
    // result shape: [["created_at"],["YYYY-MM-DD HH:MM:SS"]]
    let row = sel.split("],[").nth(1).expect("row present");
    let v = row.trim_matches(['[', ']', '"']);
    let parts: Vec<&str> = v.split(' ').collect();
    assert_eq!(parts.len(), 2, "expected 'date time', got: {}", v);
    assert_eq!(parts[0].len(), 10, "date part: {}", parts[0]);
    assert_eq!(parts[1].len(), 8, "time part: {}", parts[1]);
}

#[test]
#[cfg_attr(miri, ignore)] // SystemTime::now() — realtime clock blocked by miri's isolation
fn datetime_now_localtime_accepted() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INTEGER PRIMARY KEY, created_at TEXT)", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t VALUES (1, datetime('now','localtime'))", &mut db);
    assert!(r.is_ok(), "datetime('now','localtime') in VALUES: {:?}", r);
}

#[test]
fn datetime_bad_modifier_rejected() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INTEGER PRIMARY KEY, created_at TEXT)", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t VALUES (1, datetime('yesterday'))", &mut db);
    assert!(r.is_err(), "unsupported modifier must be rejected");
}

#[test]
fn select_from_view_materializes() {
    // Bug regression: SELECT on a created view reported "Table does not exist"
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE w (id TEXT, name TEXT, barrelLength FLOAT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO w VALUES ('a', 'A', 100.0)", &mut db).unwrap();
    let r = parse_and_exec(
        "CREATE VIEW v_short AS SELECT * FROM w WHERE barrelLength < 300",
        &mut db,
    );
    assert!(r.is_ok(), "create view: {:?}", r);
    let sel = parse_and_exec("SELECT * FROM v_short", &mut db);
    assert!(sel.is_ok(), "select from view: {:?}", sel);
}

#[test]
fn select_from_empty_view_resolves() {
    // Bug regression: SELECT on a view over an EMPTY table reported
    // "Table does not exist" because materialize_view skipped creating the
    // table when the result had no data rows (header only).
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE w (id TEXT, name TEXT)", &mut db).unwrap();
    parse_and_exec("CREATE VIEW v AS SELECT * FROM w", &mut db).unwrap();
    let sel = parse_and_exec("SELECT * FROM v", &mut db);
    assert!(sel.is_ok(), "select from empty view: {:?}", sel);
    // And still works once rows exist
    parse_and_exec("INSERT INTO w VALUES ('a', 'A')", &mut db).unwrap();
    let sel2 = parse_and_exec("SELECT * FROM v", &mut db);
    assert!(sel2.is_ok(), "select from populated view: {:?}", sel2);
    assert!(sel2.unwrap().contains("a"));
}

#[test]
fn array_columns_accept_array_literals() {
    // Bug regression: STRINGS[]/FLOATS[] columns were downgraded to plain
    // STRING/FLOAT by the preprocessor, so array values were rejected.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t_arr (s STRINGS[], f FLOATS[])", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t_arr VALUES (ARRAY['a','b'], ARRAY[1.5, 2.5])", &mut db);
    assert!(r.is_ok(), "array insert: {:?}", r);
    let sel = parse_and_exec("SELECT s FROM t_arr", &mut db).unwrap();
    assert!(sel.contains("a"), "array value present: {}", sel);
}

#[test]
#[cfg_attr(miri, ignore)] // SystemTime::now() — realtime clock blocked by miri's isolation
fn sqlite_date_modifiers_work() {
    // Path B: SQLite-style datetime() arithmetic ('+1 day', '-30 days')
    let mut db = Database::new();
    let r = parse_and_exec("SELECT datetime('now', '+1 day')", &mut db);
    assert!(r.is_ok(), "+1 day: {:?}", r);
    let v = r.unwrap();
    // 'YYYY-MM-DD HH:MM:SS' present
    assert!(v.len() >= 19, "datetime shape: {}", v);
    let r2 = parse_and_exec("SELECT datetime('now', '-30 days')", &mut db);
    assert!(r2.is_ok(), "-30 days: {:?}", r2);
    let r3 = parse_and_exec("SELECT datetime('now', '+3 hours')", &mut db);
    assert!(r3.is_ok(), "+3 hours: {:?}", r3);
}

#[test]
#[cfg_attr(miri, ignore)] // SystemTime::now() — realtime clock blocked by miri's isolation
fn sqlite_string_functions_work() {
    // Path B: instr/ltrim/rtrim/typeof/char/strftime/date/time
    let mut db = Database::new();
    assert!(
        parse_and_exec("SELECT instr('hello', 'll')", &mut db)
            .unwrap()
            .contains('3')
    );
    assert!(
        parse_and_exec("SELECT ltrim('  x')", &mut db)
            .unwrap()
            .contains("\"x\"")
    );
    assert!(
        parse_and_exec("SELECT rtrim('x  ')", &mut db)
            .unwrap()
            .contains("\"x\"")
    );
    assert!(
        parse_and_exec("SELECT typeof(42)", &mut db)
            .unwrap()
            .contains("integer")
    );
    assert!(parse_and_exec("SELECT char(65, 66)", &mut db).unwrap().contains("AB"));
    assert!(parse_and_exec("SELECT strftime('%Y', 'now')", &mut db).is_ok());
    assert!(parse_and_exec("SELECT date('now')", &mut db).is_ok());
    assert!(parse_and_exec("SELECT time('now')", &mut db).is_ok());
}

#[test]
fn insert_or_replace_ignores_work() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t_ir (id INTEGER PRIMARY KEY, v TEXT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t_ir VALUES (1, 'old')", &mut db).unwrap();
    let r = parse_and_exec("INSERT OR REPLACE INTO t_ir VALUES (1, 'new')", &mut db);
    assert!(r.is_ok(), "or replace: {:?}", r);
    let sel = parse_and_exec("SELECT v FROM t_ir", &mut db).unwrap();
    assert!(sel.contains("new"), "replaced value: {}", sel);
    // OR IGNORE on duplicate: no error, 0 rows
    let r2 = parse_and_exec("INSERT OR IGNORE INTO t_ir VALUES (1, 'other')", &mut db);
    assert!(r2.is_ok(), "or ignore: {:?}", r2);
    let sel2 = parse_and_exec("SELECT count(*) FROM t_ir", &mut db).unwrap();
    assert!(sel2.contains('1'), "count unchanged: {}", sel2);
}

#[test]
fn integer_primary_key_auto_assigns_rowid() {
    // SQLite semantics: bare `INTEGER PRIMARY KEY` auto-assigns a rowid when
    // omitted — the most common SQLite idiom (INSERT without id column).
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)", &mut db).unwrap();
    assert!(parse_and_exec("INSERT INTO t (v) VALUES ('a')", &mut db).is_ok());
    assert!(parse_and_exec("INSERT INTO t (v) VALUES ('b')", &mut db).is_ok());
    let sel = parse_and_exec("SELECT id, v FROM t", &mut db).unwrap();
    assert!(
        sel.contains("[1,\"a\"]") && sel.contains("[2,\"b\"]"),
        "auto rowids: {}",
        sel
    );
}

#[test]
fn update_self_referential_assignment() {
    // Corpus gap: SET col = col + N (increment pattern used by admin
    // command systems and stat trackers) failed with "Complex expressions
    // not supported in values" because UPDATE used eval_literal_expr.
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, bans INTEGER DEFAULT 0)",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 0)", &mut db).unwrap();
    assert!(parse_and_exec("UPDATE t SET bans = bans + 1 WHERE id = 1", &mut db).is_ok());
    assert!(parse_and_exec("UPDATE t SET bans = bans + 10 WHERE id = 1", &mut db).is_ok());
    let sel = parse_and_exec("SELECT bans FROM t", &mut db).unwrap();
    assert!(sel.contains("11"), "incremented twice: {}", sel);
}

#[test]
fn composite_primary_key_enforced() {
    // Corpus gap: table-level PRIMARY KEY (a, b) was silently dropped —
    // CREATE accepted it but duplicates weren't rejected. Now enforced via
    // the existing pk_set (joined "a|b" keys).
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE ps (uid TEXT, state_key TEXT, state_value TEXT, PRIMARY KEY (uid, state_key))",
        &mut db,
    )
    .unwrap();
    assert!(parse_and_exec("INSERT INTO ps VALUES ('u1', 'role', 'medic')", &mut db).is_ok());
    let dup = parse_and_exec("INSERT INTO ps VALUES ('u1', 'role', 'other')", &mut db);
    assert!(dup.is_err(), "composite duplicate must be rejected: {:?}", dup);
    assert!(parse_and_exec("INSERT INTO ps VALUES ('u1', 'squad', 'alpha')", &mut db).is_ok());
    let sel = parse_and_exec("SELECT count(*) FROM ps", &mut db).unwrap();
    assert!(sel.contains('2'), "exactly 2 rows: {}", sel);
}

#[test]
#[cfg_attr(miri, ignore)] // DEFAULT datetime('now') — realtime clock blocked by miri's isolation
fn default_function_expression_evaluated_at_insert() {
    // Bug regression: `DEFAULT datetime('now')` was rejected at CREATE
    // ("DEFAULT only supports literal values") — every real mod schema uses
    // timestamp defaults.
    let mut db = Database::new();
    let r = parse_and_exec(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, created_at TEXT DEFAULT datetime('now'))",
        &mut db,
    );
    assert!(r.is_ok(), "create with fn default: {:?}", r);
    let r2 = parse_and_exec("INSERT INTO t (id) VALUES (1)", &mut db);
    assert!(r2.is_ok(), "insert: {:?}", r2);
    let sel = parse_and_exec("SELECT created_at FROM t", &mut db).unwrap();
    // 'YYYY-MM-DD HH:MM:SS'
    assert!(sel.len() >= 19, "timestamp default: {}", sel);
    // literal default still works alongside
    let r3 = parse_and_exec("CREATE TABLE t2 (id INTEGER PRIMARY KEY, flag INT DEFAULT 0)", &mut db);
    assert!(r3.is_ok(), "create with literal default: {:?}", r3);
    let r4 = parse_and_exec("INSERT INTO t2 (id) VALUES (1)", &mut db);
    assert!(r4.is_ok(), "insert literal default: {:?}", r4);
    let sel2 = parse_and_exec("SELECT flag FROM t2", &mut db).unwrap();
    assert!(sel2.contains("0"), "literal default applied: {}", sel2);
}

#[test]
fn insert_or_replace_updates_in_place() {
    // Bug regression: INSERT OR REPLACE did delete+reinsert (O(n) per op);
    // replace_by_pk overwrites in place and keeps the row count stable.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id TEXT PRIMARY KEY, v TEXT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('a', 'old')", &mut db).unwrap();
    let r = parse_and_exec("INSERT OR REPLACE INTO t VALUES ('a', 'new')", &mut db);
    assert!(r.is_ok(), "replace: {:?}", r);
    let sel = parse_and_exec("SELECT v FROM t WHERE id = 'a'", &mut db).unwrap();
    assert!(sel.contains("new"), "replaced in place: {}", sel);
    let cnt = parse_and_exec("SELECT count(*) FROM t", &mut db).unwrap();
    assert!(cnt.contains("1"), "count unchanged: {}", cnt);
}

#[test]
fn select_by_pk_uses_fast_path_and_subqueries_still_work() {
    // Bug regression: SELECT by PK was a full scan (O(n)); now O(1) via
    // pk_row_index. The subquery DB snapshot is only cloned when the query
    // contains a subquery — verify both paths work.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE b (id TEXT PRIMARY KEY, val INT)", &mut db).unwrap();
    // ponytail: miri interprets every instruction; a small batch exercises the
    // same code path while keeping the miri CI job fast. Native runs full size.
    let n = if cfg!(miri) { 100 } else { 5000 };
    for i in 0..n {
        parse_and_exec(&format!("INSERT INTO b VALUES ('k_{}', {})", i, i * 10), &mut db).unwrap();
    }
    // PK point lookup (fast path)
    let half = n / 2;
    let r = parse_and_exec(&format!("SELECT val FROM b WHERE id = 'k_{}'", half), &mut db).unwrap();
    assert!(r.contains(&(half * 10).to_string()), "pk lookup: {}", r);
    // Missing PK returns empty
    let r = parse_and_exec("SELECT val FROM b WHERE id = 'missing'", &mut db).unwrap();
    assert!(!r.contains("25000"), "missing pk: {}", r);
    // Subquery still needs the snapshot — IN-subquery in WHERE
    let r = parse_and_exec(
        "SELECT val FROM b WHERE val IN (SELECT val FROM b WHERE id = 'k_3')",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("30"), "in-subquery: {}", r);
    // Scalar subquery in projection
    let r = parse_and_exec("SELECT (SELECT max(val) FROM b) AS m FROM b WHERE id = 'k_1'", &mut db).unwrap();
    assert!(r.contains(&((n - 1) * 10).to_string()), "scalar subquery: {}", r);
}

#[test]
fn composite_pk_partial_match_falls_back_to_scan() {
    // Bug regression: SELECT on ONE column of a composite PK wrongly used the
    // single-PK fast path, built a partial key (NULL |value) that never hit,
    // and returned empty — hiding real rows. Must fall back to the scan.
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE settings (ns TEXT, set_key TEXT, val TEXT, PRIMARY KEY (ns, set_key))",
        &mut db,
    )
    .unwrap();
    parse_and_exec(
        "INSERT INTO settings VALUES ('mission', 'log_level', '2'), ('profile', 'radio', 'x')",
        &mut db,
    )
    .unwrap();
    // Query by ONE PK column only — must return the row(s)
    let r = parse_and_exec("SELECT val FROM settings WHERE set_key = 'log_level'", &mut db).unwrap();
    assert!(r.contains("2"), "partial-PK match: {}", r);
    let r2 = parse_and_exec(
        "SELECT val FROM settings WHERE set_key = 'log_level' AND ns = 'mission'",
        &mut db,
    )
    .unwrap();
    assert!(r2.contains("2"), "full composite match: {}", r2);
}

#[test]
fn insert_or_replace_composite_pk_replaces_in_place() {
    // Bug regression: INSERT OR REPLACE on a composite-PK table built the PK
    // key from a PARTIAL row (first pk col + NULLs), never matched the real
    // composite key in pk_row_index, and fell through to insert() →
    // DuplicateKey. Must build the key from the full row.
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE settings (ns TEXT, set_key TEXT, val TEXT, PRIMARY KEY (ns, set_key))",
        &mut db,
    )
    .unwrap();
    parse_and_exec(
        "INSERT INTO settings VALUES ('', 's1', 'old'), ('', 's2', 'v2')",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT OR REPLACE INTO settings VALUES ('', 's1', 'new')", &mut db).unwrap();
    let r = parse_and_exec("SELECT COUNT(*) FROM settings", &mut db).unwrap();
    assert!(r.contains("2"), "replace must not grow the table: {}", r);
    let r2 = parse_and_exec("SELECT val FROM settings WHERE ns='' AND set_key='s1'", &mut db).unwrap();
    assert!(r2.contains("new"), "replaced value: {}", r2);
}

#[test]
fn join_empty_result_is_valid_json_and_projection_respected() {
    // Bug regression: JOIN with zero matching rows returned a trailing comma
    // (`[["h1",...],]` — invalid JSON), and JOIN ignored the SELECT list
    // (returned ALL columns regardless of projection).
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE users (uid INT PRIMARY KEY, name STRING)", &mut db).unwrap();
    parse_and_exec("CREATE TABLE files (path STRING PRIMARY KEY, owner INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO users VALUES (1, 'matt'), (2, 'sgt')", &mut db).unwrap();
    parse_and_exec("INSERT INTO files VALUES ('/a', 2)", &mut db).unwrap();
    // Zero matching rows — must be `[["u.name"]]`, valid JSON
    let r = parse_and_exec(
        "SELECT u.name FROM files f JOIN users u ON u.uid = f.owner WHERE f.path = '/zzz'",
        &mut db,
    )
    .unwrap();
    assert_eq!(r, "[\"u.name\"]", "empty JOIN result: {}", r);
    // Projection respected — only u.name, not all columns
    let r2 = parse_and_exec("SELECT u.name FROM files f JOIN users u ON u.uid = f.owner", &mut db).unwrap();
    assert!(!r2.contains("path"), "projection must drop f.path: {}", r2);
    assert!(r2.contains("sgt"), "joined row: {}", r2);
}

#[test]
fn reserved_keyword_user_as_column_name() {
    // Bug regression: sqlparser maps USER/CURRENT_USER to a zero-arg function
    // call; `WHERE user=1` evaluated user() → NULL → 0 rows. Must resolve the
    // zero-arg "function" as a column when it exists in the table.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE procs (pid INT PRIMARY KEY, user INT, cpu REAL)", &mut db).unwrap();
    parse_and_exec(
        "INSERT INTO procs VALUES (1, 1, 0.5), (2, 0, 0.9), (3, 1, 0.2)",
        &mut db,
    )
    .unwrap();
    let r = parse_and_exec("SELECT COUNT(*) FROM procs WHERE user = 1", &mut db).unwrap();
    assert!(r.contains("2"), "user col WHERE: {}", r);
}

#[test]
fn order_by_after_group_by_sorts_groups() {
    // Bug regression: aggregate path returned early, ORDER BY after GROUP BY
    // was ignored (groups came back in insertion order).
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (m TEXT, v REAL)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('b', 1), ('a', 2), ('c', 3), ('a', 4)", &mut db).unwrap();
    let r = parse_and_exec("SELECT m FROM t GROUP BY m ORDER BY m", &mut db).unwrap();
    let a_pos = r.find("a").unwrap();
    let b_pos = r.find("b").unwrap();
    let c_pos = r.find("c").unwrap();
    assert!(a_pos < b_pos && b_pos < c_pos, "sorted groups: {}", r);
    // ORDER BY aggregate function (COUNT) — evaluated over the whole group
    let r2 = parse_and_exec(
        "SELECT m, COUNT(*) FROM t GROUP BY m ORDER BY COUNT(*) DESC, m ASC",
        &mut db,
    )
    .unwrap();
    let a_idx = r2.find("\"a\"").unwrap();
    let b_idx = r2.find("\"b\"").unwrap();
    assert!(a_idx < b_idx, "COUNT DESC puts a(2) first: {}", r2);
}

#[test]
fn join_group_by_aggregates_work() {
    // Bug regression: JOIN path had no grouping — SELECT ... JOIN ... GROUP BY
    // returned raw rows instead of grouped aggregates.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE users (uid INT PRIMARY KEY, name STRING)", &mut db).unwrap();
    parse_and_exec("CREATE TABLE files (path STRING PRIMARY KEY, owner INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO users VALUES (1, 'matt'), (2, 'sgt')", &mut db).unwrap();
    parse_and_exec("INSERT INTO files VALUES ('/a', 1), ('/b', 1), ('/c', 2)", &mut db).unwrap();
    let r = parse_and_exec(
        "SELECT u.name, COUNT(f.path) FROM users u LEFT JOIN files f ON f.owner = u.uid \
         GROUP BY u.name ORDER BY u.name",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("\"matt\",2"), "matt count: {}", r);
    assert!(r.contains("\"sgt\",1"), "sgt count: {}", r);
    // reserved keyword in JOIN ON clause
    let r2 = parse_and_exec("CREATE TABLE procs (pid INT PRIMARY KEY, user INT)", &mut db)
        .and_then(|_| parse_and_exec("INSERT INTO procs VALUES (1, 1), (2, 2)", &mut db))
        .and_then(|_| {
            parse_and_exec(
                "SELECT u.name FROM procs p JOIN users u ON u.uid = p.user ORDER BY u.name",
                &mut db,
            )
        })
        .unwrap();
    assert!(r2.contains("matt") && r2.contains("sgt"), "JOIN ON user col: {}", r2);
}

#[test]
fn subquery_result_is_cached_and_invalidated_per_statement() {
    // Bug regression: subquery in WHERE re-cloned the whole DB snapshot per
    // outer row → O(n²). Results are now memoized per statement; the cache
    // must not leak stale values across statements (UPDATE then re-query).
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE kv (k STRING PRIMARY KEY, v STRING, n INT)", &mut db).unwrap();
    let mut vals: Vec<String> = Vec::new();
    for i in 0..100 {
        vals.push(format!("('k{:05}', 'v{}', {})", i, i, i));
    }
    parse_and_exec(&format!("INSERT INTO kv VALUES {}", vals.join(",")), &mut db).unwrap();
    let r = parse_and_exec("SELECT COUNT(*) FROM kv WHERE n = (SELECT 42)", &mut db).unwrap();
    assert!(r.contains("1"), "initial: {}", r);
    parse_and_exec("UPDATE kv SET n = 42 WHERE k = 'k00000'", &mut db).unwrap();
    let r2 = parse_and_exec("SELECT COUNT(*) FROM kv WHERE n = (SELECT 42)", &mut db).unwrap();
    assert!(r2.contains("2"), "stale cache would give 1: {}", r2);
}

// ── M0 baseline red regressions ─────────────────────────────────────────
// These tests assert CORRECT behavior the engine violates on HEAD (8599a5b).
// They are the acceptance criteria for the fix milestones — each must fail
// on HEAD for the documented reason, not for a typo. Do not fix the engine
// here; the tests stay red until the corresponding bug is fixed.
#[test]
fn t1_btree_index_int_ordering_negatives() {
    // Bug T1: index.rs encode_key shifts ints with n.wrapping_add(i64::MAX)
    // then formats with {:020}. For every positive n, n + i64::MAX overflows
    // i64 and wraps to a negative number that prints with a leading '-' — so
    // in the BTreeMap the keys for v=3 and v=100 sort BEFORE the keys for
    // v=-5 and v=-1. The index's ordered entries are scrambled: numeric order
    // must be [-5, -1, 0, 3, 100]. (Range queries are not wired into the
    // executor yet, so this asserts the encoding order directly — the fix
    // milestone makes the index walkable in true numeric order.)
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_v ON t(v)", &mut db).unwrap();
    parse_and_exec(
        "INSERT INTO t VALUES (1, -5), (2, -1), (3, 0), (4, 3), (5, 100)",
        &mut db,
    )
    .unwrap();
    let table = db.get_table("t").unwrap();
    let idx = table.find_index("v", crate::engine::index::IndexType::BTree).unwrap();
    let ordered = match idx {
        crate::engine::table::IndexImpl::BTree(b) => b.all_entries(),
        crate::engine::table::IndexImpl::Trigram(_) => unreachable!(),
    };
    assert_eq!(
        ordered,
        vec![0, 1, 2, 3, 4],
        "bug T1: btree int encode (n.wrapping_add(i64::MAX) + {{:020}}) mis-orders negatives — \
         entries must walk in numeric order [-5,-1,0,3,100] i.e. row ids [0,1,2,3,4]"
    );
}

#[test]
fn t2_pk_key_pipe_collision_composite() {
    // Bug T2: schema.rs pk_key_static joins PK column Displays with an
    // unescaped '|' separator. String Display wraps values in quotes, so
    // plain pipes don't collide — but separator chars INSIDE a value do:
    // ('a'|'b', 'c') and ('a', 'b'|'c') both encode to "'a'|'b'|'c'" and the
    // second row is wrongly rejected as DuplicateKey. Distinct composite
    // keys must both be insertable.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('a|b', 'c')", &mut db).unwrap();
    match parse_and_exec("INSERT INTO t VALUES ('a''|''b', 'c')", &mut db) {
        Ok(_) => {}
        Err(e) => panic!(
            "bug T2: pk_key joins Display parts with unescaped '|' — ('a''|''b','c') collides \
             with ('a','b''|''c')... second of the pair wrongly rejected: {}",
            e
        ),
    }
    match parse_and_exec("INSERT INTO t VALUES ('a', 'b''|''c')", &mut db) {
        Ok(_) => {}
        Err(e) => panic!(
            "bug T2: pk_key joins Display parts with unescaped '|' — ('a','b''|''c') collides \
             with ('a''|''b','c') under the same encoded key; distinct composite keys must both \
             insert, got: {}",
            e
        ),
    }
    let r = parse_and_exec("SELECT COUNT(*) FROM t", &mut db).unwrap();
    assert!(
        r.contains("[3]"),
        "bug T2: all three composite keys are distinct (pk_key '|' collision): {}",
        r
    );
}

#[test]
fn t4_btree_index_string_case_divergence() {
    // Bug T4: index.rs encode_key lowercases string values (s.to_lowercase())
    // on BOTH insert and lookup, so the index cannot distinguish rows that
    // differ only by case: 'abc' and 'AbC' share the key "\x04abc". Equality
    // is case-SENSITIVE — `WHERE name = 'AbC'` must return exactly the
    // case-matching row, but the index path returns both.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, name TEXT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_name ON t(name)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 'abc'), (2, 'AbC')", &mut db).unwrap();
    let r = parse_and_exec("SELECT id FROM t WHERE name = 'AbC'", &mut db).unwrap();
    let ok = r.contains("[2]") && !r.contains("[1]");
    assert!(
        ok,
        "bug T4: encode_key lowercases strings (index.rs to_lowercase) — case-sensitive \
         equality `name = 'AbC'` must return exactly the case-matching row [2], got: {}",
        r
    );
}

#[test]
fn t5_order_by_lexicographic_instead_of_numeric() {
    // Bug T5: sort.rs sort_rows (and the GROUP BY path, aggregate.rs) order
    // by value_to_string().cmp() — lexicographic. Ints 2, 10, 100 sort as
    // "10" < "100" < "2". Numeric ORDER BY must return [2, 10, 100].
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 2), (2, 10), (3, 100)", &mut db).unwrap();
    let r = parse_and_exec("SELECT v FROM t ORDER BY v ASC", &mut db).unwrap();
    let p2 = r.find("[2]").unwrap_or(usize::MAX);
    let p10 = r.find("[10]").unwrap_or(usize::MAX);
    let p100 = r.find("[100]").unwrap_or(usize::MAX);
    assert!(
        p2 < p10 && p10 < p100,
        "bug T5: ORDER BY compares value_to_string() lexicographically — v ASC must be \
         [2],[10],[100] (numeric), got: {}",
        r
    );
}

#[test]
fn t6_float_negative_zero_index_lookup() {
    // Bug T6: the pk/index key for floats encodes the raw f64 bits, so -0.0
    // and +0.0 produce DIFFERENT keys even though cmp_values (ops.rs) treats
    // them as equal. `WHERE f = 0.0` on an indexed column misses the -0.0
    // row via the index path.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, f REAL)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_f ON t(f)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, -0.0)", &mut db).unwrap();
    let r = parse_and_exec("SELECT id FROM t WHERE f = 0.0", &mut db).unwrap();
    assert!(
        r.contains("[1]"),
        "bug T6: float key encoding separates -0.0 from 0.0 — `f = 0.0` must match the \
         -0.0 row (cmp_values says -0.0 == 0.0), got: {}",
        r
    );
}

#[test]
fn composite_pk_partial_conjunct_falls_back_to_scan() {
    // Baseline lock (NOT a bug): WHERE on a single column of a composite PK
    // (partial conjunct) must still find the row — the executor must NOT
    // answer it with the composite-pk key path (which would produce a
    // partial key that never matches and hide real rows). Locks current
    // behavior so the M5 partial-match change cannot silently break it.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', 'y')", &mut db).unwrap();
    let r = parse_and_exec("SELECT b FROM t WHERE a = 'x'", &mut db).unwrap();
    assert!(
        r.contains("y"),
        "partial conjunct on composite PK must fall back to scan and find the row: {}",
        r
    );
}

// ── M5 composite-PK O(1) full-match fast path ─────────────────────────
// A WHERE clause matching ALL columns of a composite PK resolves via
// pk_row_index in O(1) instead of a full scan. Literals are coerced to the
// column's declared type (mirroring insert-time coercion) so the built key
// matches the stored row's key. Partial conjuncts and non-PK conjuncts must
// still fall back to scan.

#[test]
fn m5_composite_pk_full_conjunct_hit() {
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (ns TEXT, setting_key TEXT, v INT, PRIMARY KEY (ns, setting_key))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES ('', 'x', 42)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('', 'y', 43)", &mut db).unwrap();
    let r = parse_and_exec("SELECT v FROM t WHERE ns = '' AND setting_key = 'x'", &mut db).unwrap();
    assert!(
        r.contains("[42]") && !r.contains("[43]"),
        "full composite-PK conjunct must hit the exact row via pk_row_index: {}",
        r
    );
}

#[test]
fn m5_composite_pk_reversed_conjunct_order_same_row() {
    // Reversed conjunct order plus Nested unwrap plus literal-on-the-left
    // (`'' = ns`) must all resolve to the same row.
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (ns TEXT, setting_key TEXT, v INT, PRIMARY KEY (ns, setting_key))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES ('', 'x', 42)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('', 'y', 43)", &mut db).unwrap();
    let r = parse_and_exec("SELECT v FROM t WHERE (setting_key = 'x') AND ('' = ns)", &mut db).unwrap();
    assert!(
        r.contains("[42]") && !r.contains("[43]"),
        "reversed/nested composite-PK conjunct must return the same row: {}",
        r
    );
}

#[test]
fn m5_composite_pk_int_col_string_literal_coerces() {
    // '5' on an INT pk column must coerce to Int(5) so the pk_key matches the
    // stored row (insert stores Int(5), key 'i1:5').
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (pk_int INT, pk_str TEXT, v INT, PRIMARY KEY (pk_int, pk_str))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES (5, 'a', 1)", &mut db).unwrap();
    let r = parse_and_exec("SELECT v FROM t WHERE pk_int = '5' AND pk_str = 'a'", &mut db).unwrap();
    assert!(
        r.contains("[1]"),
        "'5' on an INT pk column must coerce to Int(5) and hit the row: {}",
        r
    );
}

#[test]
fn m5_composite_pk_text_col_int_literal_coerces() {
    // Insert coerces Int 42 into the TEXT pk column, storing String("42")
    // (key 's2:42'). An int literal 42 in the WHERE must coerce the same way
    // and hit the row.
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (pk_int INT, pk_str TEXT, v INT, PRIMARY KEY (pk_int, pk_str))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES (5, 42, 2)", &mut db).unwrap();
    let r = parse_and_exec("SELECT v FROM t WHERE pk_int = 5 AND pk_str = 42", &mut db).unwrap();
    assert!(
        r.contains("[2]"),
        "int literal on a TEXT pk column must coerce to String and hit the row: {}",
        r
    );
}

#[test]
fn m5_composite_pk_partial_conjunct_still_scans() {
    // Complement of the lock test: a conjunct on the OTHER column of a
    // composite PK is still a partial key and must fall back to scan.
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (ns TEXT, setting_key TEXT, v INT, PRIMARY KEY (ns, setting_key))",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES ('', 'x', 42)", &mut db).unwrap();
    let r = parse_and_exec("SELECT v FROM t WHERE setting_key = 'x'", &mut db).unwrap();
    assert!(
        r.contains("[42]"),
        "partial conjunct on composite PK (second col) must fall back to scan: {}",
        r
    );
}

#[test]
fn m5_composite_pk_non_pk_conjunct_falls_back() {
    // A conjunct on a non-PK column — or an extra conjunct beyond the PK
    // columns — cannot be answered by the pk key alone; must fall back to
    // scan and still find the row.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, v INT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', 'y', 7)", &mut db).unwrap();
    let r1 = parse_and_exec("SELECT v FROM t WHERE a = 'x' AND v = 7", &mut db).unwrap();
    assert!(
        r1.contains("[7]"),
        "non-PK conjunct must fall back to scan and find the row: {}",
        r1
    );
    let r2 = parse_and_exec("SELECT v FROM t WHERE a = 'x' AND b = 'y' AND v = 7", &mut db).unwrap();
    assert!(
        r2.contains("[7]"),
        "extra conjunct beyond the PK columns must fall back to scan: {}",
        r2
    );
}

// ── M6 subquery per-row overhead + cache soundness regressions ──────────
// These lock the M6 fixes: the correlation-rewrite Cow refactor (no per-row
// AST clone for uncorrelated subqueries), the correlated/nondeterministic
// cache skips (a structurally-identical AST must never freeze a value that
// must vary per row), the dispatch-level cache/snapshot refresh for
// non-Query statements, and the INSERT...VALUES((SELECT ...)) support.

#[test]
fn m6_random_in_subquery_varies_per_row() {
    // Bug: `SELECT a.id, (SELECT random()) FROM t` is structurally identical
    // on every row → same Debug-format cache key → the first evaluation was
    // cached forever and every row got the SAME frozen random value. The
    // nondeterminism cache skip (lookup AND insert) must force a fresh
    // random() per row.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY)", &mut db).unwrap();
    let vals: Vec<String> = (0..50i64).map(|i| format!("({})", i)).collect();
    parse_and_exec(&format!("INSERT INTO t VALUES {}", vals.join(",")), &mut db).unwrap();
    let r = parse_and_exec("SELECT id, (SELECT random()) FROM t", &mut db).unwrap();
    let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&r).unwrap();
    let mut distinct: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for row in rows.iter().skip(1) {
        if let Some(v) = row.get(1).and_then(|v| v.as_i64()) {
            distinct.insert(v);
        }
    }
    assert!(
        distinct.len() >= 2,
        "M6: random() in a subquery must vary per row (nondeterministic cache skip), got {} distinct values: {}",
        distinct.len(),
        r
    );
}

#[test]
fn m6_insert_then_select_with_subquery_is_fresh() {
    // Bug: subqueries inside INSERT read a stale (or missing) SUBQ_DB snapshot
    // because clear_subq_cache + the snapshot reset fired only in the
    // single-table SELECT dispatchers. Each INSERT must see the count of the
    // table AS IT WAS before that insert — never a cached value from an
    // earlier statement.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE kv (k TEXT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO kv VALUES ('a', 1), ('b', 2)", &mut db).unwrap();
    // Subquery inside INSERT...VALUES reads kv itself — must see count=2.
    parse_and_exec("INSERT INTO kv VALUES ('c', (SELECT count(*) FROM kv))", &mut db).unwrap();
    let r = parse_and_exec("SELECT v FROM kv WHERE k = 'c'", &mut db).unwrap();
    assert!(
        r.contains("2"),
        "M6: INSERT subquery must see pre-insert count 2: {}",
        r
    );
    // Second self-read — a stale cache/snapshot from the first INSERT would
    // freeze count=2 forever; the fresh pre-insert count is now 3.
    parse_and_exec("INSERT INTO kv VALUES ('d', (SELECT count(*) FROM kv))", &mut db).unwrap();
    let r2 = parse_and_exec("SELECT v FROM kv WHERE k = 'd'", &mut db).unwrap();
    assert!(
        r2.contains("3"),
        "M6: INSERT subquery must see pre-insert count 3 (stale snapshot/cache would give 2): {}",
        r2
    );
}

#[test]
fn m6_insert_values_scalar_subquery_correct_count() {
    // Bug: `INSERT INTO t VALUES ((SELECT count(*) FROM t))` errored with
    // "Subquery not supported in this context" (no snapshot for the INSERT
    // dispatch) or read stale state. The inserted value must be the count of
    // t BEFORE this insert, and the statement must succeed.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, n INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 100), (2, 200), (3, 300)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (4, (SELECT count(*) FROM t))", &mut db).unwrap();
    // The inserted row carries the pre-insert count (3), not the post-insert 4.
    let r = parse_and_exec("SELECT n FROM t WHERE id = 4", &mut db).unwrap();
    assert!(
        r.contains("3"),
        "M6: INSERT...VALUES((SELECT count(*))) must insert the pre-insert count: {}",
        r
    );
    let r2 = parse_and_exec("SELECT COUNT(*) FROM t", &mut db).unwrap();
    assert!(r2.contains("4"), "M6: table must now have 4 rows: {}", r2);
}

#[test]
fn m9_composite_pk_duplicate_plain_insert_rejected() {
    // Bug M9-B regression: composite-PK duplicates must be rejected on plain
    // INSERT (mirrors the single-col PK path via pk_set) — the pk_row_index
    // rebuild loop (table.rs) must never see a duplicate to last-wins over.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, v TEXT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', '1', 'first')", &mut db).unwrap();
    let dup = parse_and_exec("INSERT INTO t VALUES ('x', '1', 'second')", &mut db);
    assert!(
        dup.is_err() && dup.clone().unwrap_err().contains("Duplicate key"),
        "composite dup must be rejected with Duplicate key: {:?}",
        dup
    );
    // Distinct composite keys still insert.
    parse_and_exec("INSERT INTO t VALUES ('x', '2', 'v2'), ('y', '1', 'v3')", &mut db).unwrap();
    let cnt = parse_and_exec("SELECT COUNT(*) FROM t", &mut db).unwrap();
    assert!(cnt.contains("3"), "exactly 3 rows: {}", cnt);
}

#[test]
fn m9_insert_or_replace_composite_keeps_upsert_semantics() {
    // Bug M9-B regression: INSERT OR REPLACE on a composite PK must replace
    // in place (count unchanged, values updated) — enforcement of the plain
    // INSERT duplicate check must not leak into the upsert path.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, v TEXT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', '1', 'old'), ('x', '2', 'v2')", &mut db).unwrap();
    parse_and_exec("INSERT OR REPLACE INTO t VALUES ('x', '1', 'new')", &mut db).unwrap();
    let cnt = parse_and_exec("SELECT COUNT(*) FROM t", &mut db).unwrap();
    assert!(cnt.contains("2"), "replace must not grow the table: {}", cnt);
    let r = parse_and_exec("SELECT v FROM t WHERE a = 'x' AND b = '1'", &mut db).unwrap();
    assert!(r.contains("new"), "replaced value: {}", r);
}

#[test]
fn m9_upsert_composite_do_update_works() {
    // Bug M9-B regression: ON CONFLICT DO UPDATE (UPSERT) on a composite PK
    // must keep upsert semantics after the duplicate-check hardening.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, v INT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', '1', 10)", &mut db).unwrap();
    parse_and_exec(
        "INSERT INTO t VALUES ('x', '1', 99) ON CONFLICT (a, b) DO UPDATE SET v = EXCLUDED.v",
        &mut db,
    )
    .unwrap();
    let r = parse_and_exec("SELECT v FROM t WHERE a = 'x' AND b = '1'", &mut db).unwrap();
    assert!(r.contains("99"), "upsert updated v: {}", r);
    let cnt = parse_and_exec("SELECT COUNT(*) FROM t", &mut db).unwrap();
    assert!(cnt.contains("1"), "upsert must not add a row: {}", cnt);
}

#[test]
fn m9_null_in_pk_column_rejected_on_insert() {
    // Bug M9-A regression: PK implies NOT NULL. A NULL in any PK column must
    // be rejected with a clear error, NOT collide with other NULL-PK rows via
    // the encode_part(Null) = "n0:" key.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, v TEXT, PRIMARY KEY (a, b))", &mut db).unwrap();
    let r1 = parse_and_exec("INSERT INTO t VALUES (NULL, 'x', '1')", &mut db);
    assert!(
        r1.is_err()
            && r1
                .as_ref()
                .unwrap_err()
                .contains("NULL value in primary key column 'a'"),
        "NULL in PK part must be rejected: {:?}",
        r1
    );
    // A second NULL-PK row with a different partner must ALSO be rejected —
    // not accepted and colliding with the first.
    let r2 = parse_and_exec("INSERT INTO t VALUES (NULL, 'y', '2')", &mut db);
    assert!(
        r2.is_err()
            && r2
                .as_ref()
                .unwrap_err()
                .contains("NULL value in primary key column 'a'"),
        "second NULL-PK row must also be rejected: {:?}",
        r2
    );
    // The other PK column's NULL is rejected too.
    let r3 = parse_and_exec("INSERT INTO t VALUES ('z', NULL, '3')", &mut db);
    assert!(
        r3.is_err()
            && r3
                .as_ref()
                .unwrap_err()
                .contains("NULL value in primary key column 'b'"),
        "NULL in second PK part must be rejected: {:?}",
        r3
    );
    // Table stays empty — no NULL-PK rows were admitted.
    let cnt = parse_and_exec("SELECT COUNT(*) FROM t", &mut db).unwrap();
    assert!(cnt.contains("0"), "no rows admitted: {}", cnt);
}

#[test]
fn m9_null_in_single_col_pk_rejected_on_insert() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id TEXT PRIMARY KEY, v TEXT)", &mut db).unwrap();
    let r = parse_and_exec("INSERT INTO t VALUES (NULL, 'x')", &mut db);
    assert!(
        r.is_err()
            && r.as_ref()
                .unwrap_err()
                .contains("NULL value in primary key column 'id'"),
        "NULL single-col PK must be rejected: {:?}",
        r
    );
}

#[test]
fn m9_update_setting_pk_to_null_rejected() {
    // Bug M9-A regression: UPDATE must not be able to null out a PK column.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, v TEXT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', '1', 'v1')", &mut db).unwrap();
    let r = parse_and_exec("UPDATE t SET a = NULL WHERE a = 'x' AND b = '1'", &mut db);
    assert!(
        r.is_err() && r.as_ref().unwrap_err().contains("NULL value in primary key column 'a'"),
        "UPDATE to NULL PK must be rejected: {:?}",
        r
    );
    // Row is untouched after the rejected update.
    let sel = parse_and_exec("SELECT v FROM t WHERE a = 'x' AND b = '1'", &mut db).unwrap();
    assert!(sel.contains("v1"), "row intact: {}", sel);
}

#[test]
fn m9_update_non_pk_column_to_null_still_allowed() {
    // Bug M9-A scope guard: only PK columns are protected; a non-PK column
    // may still be set to NULL.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, v TEXT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('x', '1', 'v1')", &mut db).unwrap();
    let r = parse_and_exec("UPDATE t SET v = NULL WHERE a = 'x' AND b = '1'", &mut db);
    assert!(r.is_ok(), "non-PK to NULL must be allowed: {:?}", r);
    let sel = parse_and_exec("SELECT v FROM t WHERE a = 'x' AND b = '1'", &mut db).unwrap();
    assert!(sel.contains("null"), "v is NULL: {}", sel);
}

#[test]
fn m9_empty_string_and_utf8_composite_pk_roundtrip() {
    // Bug M9-D regression: the injective pk_key encoding must round-trip
    // composite PKs whose parts are the empty string and multi-byte UTF-8 —
    // insert and point lookup must both work (exercises encode_part length
    // prefixing on byte lengths).
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (a TEXT, b TEXT, v TEXT, PRIMARY KEY (a, b))", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('', 'key1', 'empty-part')", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('日本語', 'n3', 'utf8')", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('', '日本語', 'both')", &mut db).unwrap();
    let cnt = parse_and_exec("SELECT COUNT(*) FROM t", &mut db).unwrap();
    assert!(cnt.contains("3"), "all three rows: {}", cnt);
    // Point lookups round-trip through the encoded composite key.
    let r1 = parse_and_exec("SELECT v FROM t WHERE a = '' AND b = 'key1'", &mut db).unwrap();
    assert!(r1.contains("empty-part"), "empty-part lookup: {}", r1);
    let r2 = parse_and_exec("SELECT v FROM t WHERE a = '日本語' AND b = 'n3'", &mut db).unwrap();
    assert!(r2.contains("utf8"), "utf8 lookup: {}", r2);
    let r3 = parse_and_exec("SELECT v FROM t WHERE a = '' AND b = '日本語'", &mut db).unwrap();
    assert!(r3.contains("both"), "utf8-in-second-part lookup: {}", r3);
}

#[test]
fn m9_reserved_keywords_date_time_current_timestamp_as_columns() {
    // Bug M9-C regression: `date`, `time`, `current_timestamp` must work as
    // column names in CREATE/INSERT/SELECT (the dialect sweep exercises them
    // as functions; edge_cases only covered `user`).
    let mut db = Database::new();
    parse_and_exec(
        "CREATE TABLE t (id TEXT PRIMARY KEY, date INT, time INT, current_timestamp INT)",
        &mut db,
    )
    .unwrap();
    parse_and_exec("INSERT INTO t VALUES ('a', 1, 2, 3)", &mut db).unwrap();
    let rd = parse_and_exec("SELECT date FROM t WHERE id = 'a'", &mut db).unwrap();
    assert!(rd.contains("[1]"), "date col: {}", rd);
    let rt = parse_and_exec("SELECT time FROM t WHERE id = 'a'", &mut db).unwrap();
    assert!(rt.contains("[2]"), "time col: {}", rt);
    let rc = parse_and_exec("SELECT current_timestamp FROM t WHERE id = 'a'", &mut db).unwrap();
    assert!(rc.contains("[3]"), "current_timestamp col: {}", rc);
}

#[test]
fn m6_correlated_subquery_random_not_frozen() {
    // Bug: a correlated subquery whose outer values are constant rewrites to
    // the SAME query on every row — identical cache key — so a cached
    // random() froze one value for the whole statement. The correlated +
    // nondeterministic cache skips must re-evaluate per row.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t1 (id INT PRIMARY KEY, g INT)", &mut db).unwrap();
    parse_and_exec("CREATE TABLE t2 (id INT PRIMARY KEY)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t1 VALUES (1, 5), (2, 5), (3, 5), (4, 5), (5, 5)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t2 VALUES (5)", &mut db).unwrap();
    let r = parse_and_exec("SELECT (SELECT random() FROM t2 WHERE t2.id = t1.g) FROM t1", &mut db).unwrap();
    let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&r).unwrap();
    let mut distinct: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for row in rows.iter().skip(1) {
        if let Some(v) = row.first().and_then(|v| v.as_i64()) {
            distinct.insert(v);
        }
    }
    assert!(
        distinct.len() >= 2,
        "M6: correlated subquery with random() must not freeze one value (cache skip), got {} distinct: {}",
        distinct.len(),
        r
    );
}

#[test]
fn m6_dml_subqueries_reflect_fresh_data() {
    // Bug: UPDATE SET / DELETE WHERE / JOIN-path statements containing
    // subqueries read a stale snapshot/cache (clear_subq_cache fired only in
    // single-table SELECT dispatchers). Every statement must refresh the
    // snapshot and clear the cache before evaluating its subqueries.
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE items (id INT PRIMARY KEY, grp INT, qty INT)", &mut db).unwrap();
    parse_and_exec("CREATE TABLE refs (gid INT PRIMARY KEY, total INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES (1, 1, 10), (2, 1, 20), (3, 2, 5)", &mut db).unwrap();
    parse_and_exec("INSERT INTO refs VALUES (1, 0), (2, 0)", &mut db).unwrap();

    // UPDATE SET with an uncorrelated subquery over the same table.
    parse_and_exec("UPDATE refs SET total = (SELECT count(*) FROM items)", &mut db).unwrap();
    let r = parse_and_exec("SELECT total FROM refs WHERE gid = 1", &mut db).unwrap();
    assert!(r.contains("3"), "M6: UPDATE SET subquery sees count 3: {}", r);

    // DML changes the data, then the same UPDATE must see the new count.
    parse_and_exec("INSERT INTO items VALUES (4, 3, 7)", &mut db).unwrap();
    parse_and_exec("UPDATE refs SET total = (SELECT count(*) FROM items)", &mut db).unwrap();
    let r2 = parse_and_exec("SELECT total FROM refs WHERE gid = 1", &mut db).unwrap();
    assert!(r2.contains("4"), "M6: stale cache would give 3, got: {}", r2);

    // DELETE WHERE with a subquery — avg over the CURRENT 4 rows is 10.5, so
    // only qty < 5.5 (the qty=5 row) is removed, leaving 3 rows.
    parse_and_exec(
        "DELETE FROM items WHERE qty < (SELECT avg(qty) FROM items) - 5",
        &mut db,
    )
    .unwrap();
    let r3 = parse_and_exec("SELECT COUNT(*) FROM items", &mut db).unwrap();
    assert!(
        r3.contains("3"),
        "M6: DELETE WHERE subquery: only qty < avg-5 must go: {}",
        r3
    );

    // JOIN-path subquery — a subquery whose inner query is a JOIN must see
    // fresh data after more DML (items grp 1,1,3 now; refs gid 1,2).
    let r4 = parse_and_exec(
        "SELECT (SELECT count(*) FROM items JOIN refs ON items.grp = refs.gid)",
        &mut db,
    )
    .unwrap();
    assert!(r4.contains("2"), "M6: JOIN subquery sees 2 matches: {}", r4);
    parse_and_exec("INSERT INTO items VALUES (5, 2, 50)", &mut db).unwrap();
    let r5 = parse_and_exec(
        "SELECT (SELECT count(*) FROM items JOIN refs ON items.grp = refs.gid)",
        &mut db,
    )
    .unwrap();
    assert!(
        r5.contains("3"),
        "M6: stale JOIN subquery would give 2 matches, got: {}",
        r5
    );
}

// ── M7: range predicates via BTreeMap::range ─────────────────────────────
// `>`, `<`, `>=`, `<=`, BETWEEN and LIKE 'prefix%' must route through the
// BTree index when the column is indexed, and the candidate set must be
// re-verified against the real predicate (so NULLs and coercion residuals
// can never leak through).
#[test]
fn m7_btree_range_int_comparisons() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_v ON t(v)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 10), (2, 20), (3, 30)", &mut db).unwrap();

    let r = parse_and_exec("SELECT id FROM t WHERE v > 20", &mut db).unwrap();
    assert!(
        r.contains("[3]") && !r.contains("[1]") && !r.contains("[2]"),
        "v > 20: {}",
        r
    );
    let r = parse_and_exec("SELECT id FROM t WHERE v >= 20", &mut db).unwrap();
    assert!(
        r.contains("[2]") && r.contains("[3]") && !r.contains("[1]"),
        "v >= 20: {}",
        r
    );
    let r = parse_and_exec("SELECT id FROM t WHERE v < 20", &mut db).unwrap();
    assert!(
        r.contains("[1]") && !r.contains("[2]") && !r.contains("[3]"),
        "v < 20: {}",
        r
    );
    let r = parse_and_exec("SELECT id FROM t WHERE v <= 20", &mut db).unwrap();
    assert!(
        r.contains("[1]") && r.contains("[2]") && !r.contains("[3]"),
        "v <= 20: {}",
        r
    );
}

#[test]
fn m7_btree_range_flipped_operand() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_v ON t(v)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 10), (2, 20), (3, 30)", &mut db).unwrap();

    // Literal on the left: `20 < v` ≡ `v > 20`.
    let r = parse_and_exec("SELECT id FROM t WHERE 20 < v", &mut db).unwrap();
    assert!(r.contains("[3]") && !r.contains("[2]"), "20 < v: {}", r);
    let r = parse_and_exec("SELECT id FROM t WHERE 25 >= v", &mut db).unwrap();
    assert!(
        r.contains("[1]") && r.contains("[2]") && !r.contains("[3]"),
        "25 >= v: {}",
        r
    );
}

#[test]
fn m7_btree_between_inclusive() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_v ON t(v)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 10), (2, 20), (3, 30)", &mut db).unwrap();

    let r = parse_and_exec("SELECT id FROM t WHERE v BETWEEN 15 AND 25", &mut db).unwrap();
    assert!(
        r.contains("[2]") && !r.contains("[1]") && !r.contains("[3]"),
        "BETWEEN 15 AND 25: {}",
        r
    );
    // Inclusive on both ends.
    let r = parse_and_exec("SELECT id FROM t WHERE v BETWEEN 10 AND 20", &mut db).unwrap();
    assert!(
        r.contains("[1]") && r.contains("[2]") && !r.contains("[3]"),
        "BETWEEN 10 AND 20: {}",
        r
    );
}

#[test]
fn m7_btree_like_prefix_case_sensitive() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, name TEXT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_name ON t(name)", &mut db).unwrap();
    parse_and_exec(
        "INSERT INTO t VALUES (1, 'alpha'), (2, 'alpine'), (3, 'beta'), (4, 'Alpine')",
        &mut db,
    )
    .unwrap();

    let r = parse_and_exec("SELECT id FROM t WHERE name LIKE 'alp%'", &mut db).unwrap();
    assert!(
        r.contains("[1]") && r.contains("[2]") && !r.contains("[3]") && !r.contains("[4]"),
        "LIKE 'alp%' must be case-sensitive: {}",
        r
    );
    // Mid-pattern wildcard must NOT use the prefix fast path (still correct).
    let r = parse_and_exec("SELECT id FROM t WHERE name LIKE '%lpine'", &mut db).unwrap();
    assert!(r.contains("[2]") && !r.contains("[1]"), "LIKE '%lpine': {}", r);
}

#[test]
fn m7_btree_like_unicode_prefix() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, name TEXT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_name ON t(name)", &mut db).unwrap();
    parse_and_exec(
        "INSERT INTO t VALUES (1, 'hél'), (2, 'héllo'), (3, 'help'), (4, 'hélène')",
        &mut db,
    )
    .unwrap();

    let r = parse_and_exec("SELECT id FROM t WHERE name LIKE 'hél%'", &mut db).unwrap();
    assert!(
        r.contains("[1]") && r.contains("[2]") && r.contains("[4]") && !r.contains("[3]"),
        "LIKE 'hél%' — multi-byte prefix must stay exact: {}",
        r
    );
}

#[test]
fn m7_btree_range_null_boundary() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_v ON t(v)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, NULL), (2, 5), (3, 10)", &mut db).unwrap();

    // NULL encodes \x00 (sorts first) — a `>= 1` lower bound on the value
    // prefix must exclude NULL rows naturally.
    let r = parse_and_exec("SELECT id FROM t WHERE v >= 1", &mut db).unwrap();
    assert!(
        r.contains("[2]") && r.contains("[3]") && !r.contains("[1]"),
        "v >= 1: {}",
        r
    );
    // Upper-bound scan picks up the NULL key; the verify-rescan removes it.
    let r = parse_and_exec("SELECT id FROM t WHERE v <= 5", &mut db).unwrap();
    assert!(
        r.contains("[2]") && !r.contains("[1]") && !r.contains("[3]"),
        "v <= 5: {}",
        r
    );
}

#[test]
fn m7_btree_range_null_bound_falls_back_to_scan() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_v ON t(v)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 10), (2, 20)", &mut db).unwrap();

    // NULL bound → no fast path; scan semantics: every comparison is false.
    let r = parse_and_exec("SELECT id FROM t WHERE v > NULL", &mut db).unwrap();
    assert!(
        !r.contains("[1]") && !r.contains("[2]"),
        "v > NULL must match nothing: {}",
        r
    );
}

#[test]
fn m7_btree_range_without_index_falls_back_to_scan() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 10), (2, 20), (3, 30)", &mut db).unwrap();

    let r = parse_and_exec("SELECT id FROM t WHERE v > 20", &mut db).unwrap();
    assert!(
        r.contains("[3]") && !r.contains("[1]"),
        "no index → scan, still correct: {}",
        r
    );
    let r = parse_and_exec("SELECT id FROM t WHERE v BETWEEN 10 AND 20", &mut db).unwrap();
    assert!(
        r.contains("[1]") && r.contains("[2]") && !r.contains("[3]"),
        "no index BETWEEN: {}",
        r
    );
}

#[test]
fn m7_btree_between_null_bound_falls_back_to_scan() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id INT PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("CREATE INDEX idx_v ON t(v)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES (1, 10), (2, 20)", &mut db).unwrap();

    let r = parse_and_exec("SELECT id FROM t WHERE v BETWEEN NULL AND 15", &mut db).unwrap();
    assert!(
        !r.contains("[1]") && !r.contains("[2]"),
        "BETWEEN NULL → matches nothing: {}",
        r
    );
}
