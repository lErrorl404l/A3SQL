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
    assert!(parse_and_exec("SELECT instr('hello', 'll')", &mut db)
        .unwrap()
        .contains('3'));
    assert!(parse_and_exec("SELECT ltrim('  x')", &mut db)
        .unwrap()
        .contains("\"x\""));
    assert!(parse_and_exec("SELECT rtrim('x  ')", &mut db)
        .unwrap()
        .contains("\"x\""));
    assert!(parse_and_exec("SELECT typeof(42)", &mut db)
        .unwrap()
        .contains("integer"));
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
