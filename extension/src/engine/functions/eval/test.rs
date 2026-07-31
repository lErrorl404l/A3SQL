use std::os::raw::{c_char, c_int};
use std::sync::Mutex;

/// Captured SQF callback invocations: (name, args, ctx).
static CB_CALLS: Mutex<Vec<(String, String, String)>> = Mutex::new(Vec::new());

/// Serialises the callback tests: they mutate the crate-global `CALLBACK`
/// and `CB_CALLS` statics, so they must not run concurrently. Any test that
/// executes SQL through the SQF-callback path must hold this lock. Recovered
/// from poisoning so one failing assertion does not abort the other tests.
static CALLBACK_TEST_LOCK: Mutex<()> = Mutex::new(());

/// RAII guard: restores the global `CALLBACK` to `None` on drop so a panicking
/// test cannot leave the mock callback installed for concurrent tests.
struct CallbackRestore;
impl Drop for CallbackRestore {
    fn drop(&mut self) {
        *crate::ffi::CALLBACK.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// Lock `CB_CALLS`, surviving poisoning from a failed assertion in another test.
fn cb_calls() -> std::sync::MutexGuard<'static, Vec<(String, String, String)>> {
    CB_CALLS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Mock SQF callback matching the arma-rs `Callback` ABI:
/// `extern "system" fn(*const c_char, *const c_char, *const c_char) -> c_int`.
extern "system" fn mock_callback(name: *const c_char, args: *const c_char, ctx: *const c_char) -> c_int {
    // SAFETY: the extension under test always passes valid, NUL-terminated buffers.
    unsafe {
        let name = std::ffi::CStr::from_ptr(name).to_string_lossy().into_owned();
        let args = std::ffi::CStr::from_ptr(args).to_string_lossy().into_owned();
        let ctx = std::ffi::CStr::from_ptr(ctx).to_string_lossy().into_owned();
        CB_CALLS.lock().unwrap().push((name, args, ctx));
    }
    0
}

#[test]
fn test_sqf_function_calls_callback_with_abi() {
    // Given: a mock SQF callback installed and an SQF function with a body
    let _guard = CALLBACK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    cb_calls().clear();
    *crate::ffi::CALLBACK.lock().unwrap() = Some(mock_callback);
    let _restore = CallbackRestore;
    crate::engine::plugin::register_sqf_function("_ut_cb_fn", 2, "systemChat 'called'");

    // When: SQL invoking the registered function is executed
    let mut db = crate::engine::database::Database::new();
    let _ = crate::engine::test::exec_sql(&mut db, "CREATE TABLE t (id STRING PRIMARY KEY)");
    let result = crate::engine::test::exec_sql(&mut db, "SELECT fn__ut_cb_fn('hello', 42)");

    // Then: the callback fired once with the registered name, the call values, and an empty ctx
    let calls = cb_calls();
    assert_eq!(calls.len(), 1, "callback should have fired exactly once");
    let (name, args, ctx) = &calls[0];
    assert_eq!(
        name, "fn__ut_cb_fn",
        "callback name should be the registered function name"
    );
    assert!(
        args.contains("hello") && args.contains("42"),
        "callback args should contain the call values: {}",
        args
    );
    assert!(ctx.is_empty(), "callback ctx should be empty: {}", ctx);
    drop(calls);

    // And: the query returns the SQF placeholder result without panicking
    assert!(
        result.contains("<SQF: _ut_cb_fn>"),
        "expected SQF placeholder in result: {}",
        result
    );
}

#[test]
fn test_sqf_function_callback_args_truncated_at_2048() {
    // Given: a mock SQF callback installed and a long argument
    let _guard = CALLBACK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    cb_calls().clear();
    *crate::ffi::CALLBACK.lock().unwrap() = Some(mock_callback);
    let _restore = CallbackRestore;
    crate::engine::plugin::register_sqf_function("_ut_cb_long", 1, "systemChat 'called'");

    let long_arg = "x".repeat(3000);
    let sql = format!("SELECT fn__ut_cb_long('{}')", long_arg);

    // When: SQL invoking the registered function with an over-long arg is executed
    let mut db = crate::engine::database::Database::new();
    let _ = crate::engine::test::exec_sql(&mut db, "CREATE TABLE t (id STRING PRIMARY KEY)");
    let _ = crate::engine::test::exec_sql(&mut db, &sql);

    // Then: the callback received an args payload capped at 2048 bytes
    let calls = cb_calls();
    assert_eq!(calls.len(), 1, "callback should have fired exactly once");
    let args = &calls[0].1;
    assert!(
        args.len() <= 2048,
        "callback args must be capped at 2048 bytes, got {}",
        args.len()
    );
}

#[test]
fn test_register_and_dispatch_sqf_function() {
    // This test executes SQL through the SQF-callback path, so it must not
    // run concurrently with the callback tests.
    let _guard = CALLBACK_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Register an SQF function with a body
    crate::engine::plugin::register_sqf_function("_ut_sqf_fn", 1, "systemChat 'called'");

    // Verify registration
    assert!(crate::engine::plugin::is_registered("_ut_sqf_fn"));

    // Verify body is retrievable
    let body = crate::engine::plugin::get_sqf_function_body("_ut_sqf_fn");
    assert_eq!(body, Some("systemChat 'called'".to_string()));

    // Execute SQL that invokes fn__ut_sqf_fn('hello')
    let mut db = crate::engine::database::Database::new();
    let _ = crate::engine::test::exec_sql(&mut db, "CREATE TABLE t (id STRING PRIMARY KEY)");
    let result = crate::engine::test::exec_sql(&mut db, "SELECT fn__ut_sqf_fn('hello')");
    assert!(
        result.contains("<SQF: _ut_sqf_fn>"),
        "expected SQF placeholder in result: {}",
        result
    );
}

#[test]
fn test_sqf_function_no_body_not_dispatchable() {
    // Register an SQF function WITHOUT a body (empty string)
    crate::engine::plugin::register_sqf_function("_ut_sqf_nb", 1, "");

    // is_registered should still return true (name is tracked)
    assert!(crate::engine::plugin::is_registered("_ut_sqf_nb"));

    // get_sqf_function_body should return None for empty body
    let body = crate::engine::plugin::get_sqf_function_body("_ut_sqf_nb");
    assert_eq!(body, None);
}

#[test]
fn test_sqf_function_roundtrip() {
    // Full roundtrip: register with body → lookup → verify
    crate::engine::plugin::register_sqf_function("_ut_sqf_rt", 2, "hint str _this");
    assert!(crate::engine::plugin::is_registered("_ut_sqf_rt"));
    let body = crate::engine::plugin::get_sqf_function_body("_ut_sqf_rt");
    assert_eq!(body, Some("hint str _this".to_string()));
}
