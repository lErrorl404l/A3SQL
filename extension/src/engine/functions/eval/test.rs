#[test]
fn test_register_and_dispatch_sqf_function() {
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
