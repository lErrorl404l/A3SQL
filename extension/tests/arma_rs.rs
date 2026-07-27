// a3sql — arma-rs command routing integration tests
//
// Validates that the arma-rs Extension routing layer correctly deserialises
// SQF-encoded arguments and dispatches to sql_handler. This covers the path
// that the actual Arma 3 callExtension interface uses, which is NOT exercised
// by the dispatch() unit/integration tests.

use std::sync::{Mutex, MutexGuard};

use a3sql::ffi;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

fn setup() -> MutexGuard<'static, ()> {
    let g = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    a3sql::dispatch("reset", &[]);
    g
}

/// Helper: SQF-encoded Vec<String> payload for the sql command.
fn sql_payload(stmt: &str) -> Vec<String> {
    vec![format!(r#"["{}"]"#, stmt)]
}

/// Helper: SQF-encoded Vec<String> payload with bind params.
fn sql_payload_with_args(stmt: &str, params: &[&str]) -> Vec<String> {
    let inner = std::iter::once(format!(r#""{}""#, stmt))
        .chain(params.iter().map(|p| format!(r#""{}""#, p)))
        .collect::<Vec<_>>()
        .join(", ");
    vec![format!("[{}]", inner)]
}

/// arma-rs correctly routes the "sql" command to sql_handler, deserialising
/// the SQF-encoded Vec<String> payload.
#[test]
fn arma_rs_routes_sql_command() {
    let _g = setup();
    let ext = ffi::build_extension().testing();

    let (output, code) = ext.call(
        "sql",
        Some(sql_payload(
            "CREATE TABLE ar_route_test (id STRING PRIMARY KEY, val INT)",
        )),
    );
    assert_eq!(code, 0, "CREATE TABLE: code={}, output={}", code, output);
    assert!(output.contains("\"OK\""), "CREATE TABLE OK: {}", output);

    let (output, code) = ext.call(
        "sql",
        Some(sql_payload("INSERT INTO ar_route_test VALUES ('a', 10), ('b', 20)")),
    );
    assert_eq!(code, 0, "INSERT: code={}, output={}", code, output);
    assert!(output.contains("\"OK\""), "INSERT OK: {}", output);

    let (output, code) = ext.call("sql", Some(sql_payload("SELECT * FROM ar_route_test ORDER BY id")));
    assert_eq!(code, 0, "SELECT: code={}, output={}", code, output);
    assert!(output.contains("a"), "should contain 'a': {}", output);
    assert!(output.contains("b"), "should contain 'b': {}", output);
}

/// Bind parameters ($1, $2) passed through the arma-rs Vec<String> layer
/// are correctly forwarded to dispatch as separate args.
#[test]
fn arma_rs_passes_bind_params() {
    let _g = setup();
    let ext = ffi::build_extension().testing();

    a3sql::dispatch("CREATE TABLE ar_bind_test (id STRING PRIMARY KEY, val INT)", &[]);
    a3sql::dispatch("INSERT INTO ar_bind_test VALUES ('x', 42), ('y', 99)", &[]);

    let (output, code) = ext.call(
        "sql",
        Some(sql_payload_with_args(
            "SELECT val FROM ar_bind_test WHERE id = $1",
            &["x"],
        )),
    );
    assert_eq!(code, 0, "bind: code={}, output={}", code, output);
    assert!(output.contains("42"), "expected 42: {}", output);
    assert!(!output.contains("99"), "should not contain 99: {}", output);
}

/// Unknown commands produce arma-rs error code 1 (not found).
#[test]
fn arma_rs_unknown_command_returns_not_found() {
    let ext = ffi::build_extension().testing();
    let (output, code) = ext.call("nonexistent", None);
    assert_eq!(code, 1, "expected code 1 (not found), got: {}", code);
    assert_eq!(output, "", "expected empty output for unknown command");
}

/// Calling "sql" with zero args (when Vec<String> is expected) returns
/// arma-rs error code 20 (wrong argument count, received 0).
#[test]
fn arma_rs_sql_without_args_returns_arg_count_error() {
    let ext = ffi::build_extension().testing();
    let (output, code) = ext.call("sql", None);
    assert_eq!(code, 20, "expected code 20 (wrong arg count, got 0), got: {}", code);
    assert_eq!(output, "", "expected empty output for arg count error");
}

/// Empty SQL string payload returns OK (dispatch("", &[]) clears the DB).
#[test]
fn arma_rs_sql_empty_payload_returns_ok() {
    let _g = setup();
    let ext = ffi::build_extension().testing();
    let (output, code) = ext.call("sql", Some(sql_payload("")));
    assert_eq!(code, 0, "empty: code={}, output={}", code, output);
    assert!(output.contains("\"OK\""), "expected OK for empty payload: {}", output);
}
