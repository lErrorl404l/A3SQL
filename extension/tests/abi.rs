// a3sql — Integration tests via the public dispatch() API.
// A global mutex serializes each test function so --test-threads=1 isn't needed.

use a3sql::dispatch;
use std::sync::{Mutex, MutexGuard};

static TEST_MUTEX: Mutex<()> = Mutex::new(());

fn setup() -> MutexGuard<'static, ()> {
    let g = TEST_MUTEX.lock().unwrap();
    dispatch("reset", &[]);
    g
}

fn abi_call(input: &str) -> String {
    dispatch(input, &[])
}
fn abi_call_args(cmd: &str, args: &[&str]) -> String {
    dispatch(cmd, args)
}

fn validate_response(resp: &str) -> (i64, String, usize) {
    let v: Vec<serde_json::Value> = serde_json::from_str(resp).unwrap_or_default();
    if v.len() < 2 {
        return (-1, "INVALID_RESULT".into(), 0);
    }
    let code = v[0].as_i64().unwrap_or(-1);
    let text = v[1].as_str().unwrap_or("UNKNOWN").to_string();
    let data_len = v
        .get(2)
        .map(|x| match x {
            serde_json::Value::String(s) => s.len(),
            serde_json::Value::Array(a) => a.len(),
            _ => 0,
        })
        .unwrap_or(0);
    (code, text, data_len)
}

#[test]
fn dispatch_create_table() {
    let _g = setup();
    assert!(dispatch("CREATE TABLE int_dispatch_test_t (id STRING PRIMARY KEY)", &[]).contains("\"OK\""));
}
#[test]
fn dispatch_select() {
    let _g = setup();
    dispatch("CREATE TABLE int_dispatch_test_s (id STRING PRIMARY KEY)", &[]);
    dispatch("INSERT INTO int_dispatch_test_s VALUES ('x')", &[]);
    assert!(
        dispatch("SELECT * FROM int_dispatch_test_s", &[]).contains("\"OK\""),
        "SELECT failed"
    );
}
#[test]
fn dispatch_fuzzy() {
    let _g = setup();
    dispatch("CREATE TABLE int_dispatch_test_f (id STRING PRIMARY KEY)", &[]);
    dispatch("INSERT INTO int_dispatch_test_f VALUES ('rhs_m4a1')", &[]);
    assert!(
        dispatch("SELECT * FROM int_dispatch_test_f WHERE id %% 'rhs_m4'", &[]).contains("rhs_m4a1"),
        "fuzzy"
    );
}
#[test]
fn dispatch_bad_sql() {
    let _g = setup();
    assert!(dispatch("NOT VALID SQL $$$", &[]).contains("ERR_PARSE"));
}
#[test]
fn dispatch_empty() {
    let _g = setup();
    assert!(dispatch("", &[]).contains("\"OK\""));
}
#[test]
fn dispatch_multi_statement() {
    let _g = setup();
    assert!(
        dispatch(
            "CREATE TABLE int_ms_test (id STRING PRIMARY KEY); INSERT INTO int_ms_test VALUES ('a')",
            &[]
        )
        .contains("\"OK\""),
        "multi"
    );
}
#[test]
fn dispatch_version() {
    let _g = setup();
    assert!(dispatch("version", &[]).contains("a3sql"));
}
#[test]
fn abi_full_lifecycle() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_lifecycle_t (id STRING PRIMARY KEY, val INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_lifecycle_t VALUES ('a', 10)").contains("\"OK\""));
    assert!(abi_call("SELECT * FROM int_lifecycle_t").contains("\"OK\""));
    assert!(abi_call("UPDATE int_lifecycle_t SET val = 20 WHERE id = 'a'").contains("\"OK\""));
    assert!(abi_call("DELETE FROM int_lifecycle_t WHERE id = 'a'").contains("\"OK\""));
}
#[test]
fn abi_fuzzy_match() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_fuzzy_t (id STRING PRIMARY KEY, name STRING)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_fuzzy_t VALUES ('1', 'rhs_m4a1_outro')").contains("\"OK\""));
    assert!(abi_call("SELECT name FROM int_fuzzy_t WHERE name %% 'm4a1_out'").contains("rhs_m4a1_outro"));
}
#[test]
fn abi_transactions() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_tx_t (id STRING PRIMARY KEY, val INT)").contains("\"OK\""));
    assert!(abi_call("BEGIN").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_tx_t VALUES ('a', 1)").contains("\"OK\""));
    assert!(abi_call("ROLLBACK").contains("\"OK\""));
    assert!(abi_call("SELECT * FROM int_tx_t").contains("\"OK\""));
}
#[test]
fn abi_save_load() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_sl_t (id STRING PRIMARY KEY, val INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_sl_t VALUES ('x', 42)").contains("\"OK\""));
    assert!(abi_call("save int_test_save.bin").contains("\"OK\""));
    assert!(abi_call("load int_test_save.bin").contains("\"OK\""));
    assert!(abi_call("SELECT val FROM int_sl_t WHERE id = 'x'").contains("42"));
    let _ = std::fs::remove_file("a3sql_data/int_test_save.bin");
}
#[test]
fn abi_export_import_json_roundtrip() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_json_rt_t (id STRING PRIMARY KEY, v INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_json_rt_t VALUES ('a', 10), ('b', 20)").contains("\"OK\""));
    let json_resp = abi_call("export json int_json_rt_t");
    assert!(json_resp.contains("\"OK\""));
    let data: serde_json::Value = serde_json::from_str(&json_resp).unwrap();
    let exported = data[2].to_string();
    assert!(abi_call("DROP TABLE int_json_rt_t").contains("\"OK\""));
    assert!(abi_call_args("import json int_json_rt_t", &[&exported]).contains("\"OK\""));
    assert!(abi_call("SELECT v FROM int_json_rt_t WHERE id = 'a'").contains("10"));
}
#[test]
fn abi_multi_statement() {
    let _g = setup();
    assert!(
        abi_call("CREATE TABLE int_ms2_t (id STRING PRIMARY KEY, v INT); INSERT INTO int_ms2_t VALUES ('a', 1)")
            .contains("\"OK\"")
    );
}
#[test]
fn abi_errors() {
    let _g = setup();
    assert!(abi_call("SELECT * FROM nonexistent").contains("ERR_EXEC"));
}
#[test]
fn abi_index_equality() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_idx_eq_t (id STRING PRIMARY KEY, val INT)").contains("\"OK\""));
    assert!(abi_call("CREATE INDEX int_idx_eq_t_val ON int_idx_eq_t (val)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_idx_eq_t VALUES ('a', 10), ('b', 20), ('c', 30)").contains("\"OK\""));
    assert!(abi_call("SELECT id FROM int_idx_eq_t WHERE val = 20").contains("b"));
}
#[test]
fn abi_create_with_defaults() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_def_t (id STRING PRIMARY KEY, v INT DEFAULT 42)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_def_t (id) VALUES ('a')").contains("\"OK\""));
    assert!(abi_call("SELECT v FROM int_def_t WHERE id = 'a'").contains("42"));
}
#[test]
fn abi_order_by_limit() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_ob_t (id STRING PRIMARY KEY, v INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_ob_t VALUES ('a', 3), ('b', 1), ('c', 2)").contains("\"OK\""));
    assert!(abi_call("SELECT id FROM int_ob_t ORDER BY v LIMIT 1").contains("b"));
    assert!(abi_call("SELECT id FROM int_ob_t ORDER BY v DESC LIMIT 1").contains("a"));
    assert!(abi_call("SELECT id FROM int_ob_t ORDER BY v LIMIT 1 OFFSET 1").contains("c"));
}
#[test]
fn abi_aggregates() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_agg_t (id STRING PRIMARY KEY, v INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_agg_t VALUES ('a', 10), ('b', 20), ('c', 10)").contains("\"OK\""));
    assert!(abi_call("SELECT v, COUNT(*) FROM int_agg_t GROUP BY v").contains("\"OK\""));
}
#[test]
fn abi_null_arithmetic() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_null_t (id STRING PRIMARY KEY, v INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_null_t VALUES ('a', NULL)").contains("\"OK\""));
    assert!(abi_call("SELECT v + 1 FROM int_null_t").contains("\"OK\""));
}
#[test]
fn abi_insert_select_with_index_after_delete() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_is_t (id STRING PRIMARY KEY, v INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_is_t VALUES ('a', 10), ('b', 20)").contains("\"OK\""));
    assert!(abi_call("CREATE INDEX int_is_t_v ON int_is_t (v)").contains("\"OK\""));
    assert!(abi_call("DELETE FROM int_is_t WHERE id = 'a'").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_is_t SELECT 'c', 30").contains("\"OK\""));
    assert!(abi_call("SELECT v FROM int_is_t WHERE v = 20").contains("20"));
}
#[test]
fn abi_update_with_index() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_ui_t (id STRING PRIMARY KEY, v INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_ui_t VALUES ('a', 1), ('b', 2)").contains("\"OK\""));
    assert!(abi_call("CREATE INDEX int_ui_t_v ON int_ui_t (v)").contains("\"OK\""));
    assert!(abi_call("UPDATE int_ui_t SET v = 3 WHERE id = 'a'").contains("\"OK\""));
    assert!(abi_call("SELECT v FROM int_ui_t WHERE v = 3").contains("3"));
}
#[test]
fn abi_extern_full_sequence() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_ext_full_t (id STRING PRIMARY KEY, name STRING, val INT)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_ext_full_t VALUES ('k1', 'alpha', 10)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_ext_full_t VALUES ('k2', 'beta', 20)").contains("\"OK\""));
    assert!(abi_call("SELECT * FROM int_ext_full_t").contains("\"OK\""));
    assert!(abi_call("UPDATE int_ext_full_t SET val = 15 WHERE id = 'k1'").contains("\"OK\""));
    assert!(abi_call("DELETE FROM int_ext_full_t WHERE id = 'k2'").contains("\"OK\""));
}
#[test]
fn abi_like_operator() {
    let _g = setup();
    assert!(abi_call("CREATE TABLE int_like_t (id STRING PRIMARY KEY, name STRING)").contains("\"OK\""));
    assert!(abi_call("INSERT INTO int_like_t VALUES ('1', 'hello'), ('2', 'world')").contains("\"OK\""));
    assert!(abi_call("SELECT id FROM int_like_t WHERE name LIKE 'hel%'").contains("1"));
    assert!(abi_call("SELECT id FROM int_like_t WHERE name LIKE '%orld'").contains("2"));
}
#[test]
fn resp_fmt_full_sequence() {
    let _g = setup();
    let r = abi_call("this is not sql");
    let (code, err, _) = validate_response(&r);
    assert_eq!(code, -1);
    assert_eq!(err, "ERR_PARSE");
    let r = abi_call("ping");
    let (code, text, _) = validate_response(&r);
    assert_eq!(code, 0);
    assert_eq!(text, "OK");
    let r = abi_call("version");
    let (code, text, _) = validate_response(&r);
    assert_eq!(code, 0);
    assert_eq!(text, "OK");
    let r = abi_call("CREATE TABLE int_fmt_test (id STRING PRIMARY KEY, v INT)");
    let (code, text, _) = validate_response(&r);
    assert_eq!(code, 0);
    assert_eq!(text, "OK");
    let r = abi_call("INSERT INTO int_fmt_test VALUES ('a', 1)");
    assert!(r.contains("\"OK\""));
    let r = abi_call("SELECT * FROM int_fmt_test");
    let (code, text, data) = validate_response(&r);
    assert_eq!(code, 0);
    assert_eq!(text, "OK");
    assert!(data > 0);
}
#[test]
fn dispatch_large_result_truncation_guard() {
    let _g = setup();
    let r = abi_call("CREATE TABLE int_trunc_test (id STRING PRIMARY KEY)");
    assert!(r.contains("\"OK\""), "create");
    // ~5000 rows ≈ 60KB serialized — comfortably over the 30KB output cap
    let mut sql = "INSERT INTO int_trunc_test VALUES ".to_string();
    let rows: Vec<String> = (0..5000).map(|i| format!("('row_{}')", i)).collect();
    sql.push_str(&rows.join(", "));
    assert!(abi_call(&sql).contains("\"OK\""), "insert");
    assert!(
        abi_call("SELECT * FROM int_trunc_test").contains("ERR_INTERNAL"),
        "size guard"
    );
}
#[test]
fn plugins_list_empty() {
    let _g = setup();
    assert!(abi_call("plugins").contains("\"OK\""));
}
#[test]
fn plugins_register_sqf() {
    let _g = setup();
    assert!(abi_call("register_function int_test_fn 2").contains("\"OK\""));
}
#[test]
fn plugins_echo_builtin() {
    let _g = setup();
    assert!(abi_call("SELECT fn_echo('hello')").contains("hello"));
}
