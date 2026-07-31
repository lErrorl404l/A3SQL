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
    for i in 0..500 {
        parse_and_exec(&format!("INSERT INTO bulk VALUES ({},{})", i, i * 2), &mut db).unwrap();
    }
    let r = parse_and_exec("SELECT COUNT(*) FROM bulk", &mut db).unwrap();
    assert!(r.contains("500"), "count: {}", r);
    let s = parse_and_exec("SELECT SUM(v) FROM bulk", &mut db).unwrap();
    assert!(s.contains("249500"), "sum: {}", s);
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
    for i in 0..5000 {
        parse_and_exec(&format!("INSERT INTO b VALUES ('k_{}', {})", i, i * 10), &mut db).unwrap();
    }
    // PK point lookup (fast path)
    let r = parse_and_exec("SELECT val FROM b WHERE id = 'k_2500'", &mut db).unwrap();
    assert!(r.contains("25000"), "pk lookup: {}", r);
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
    assert!(r.contains("49990"), "scalar subquery: {}", r);
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
