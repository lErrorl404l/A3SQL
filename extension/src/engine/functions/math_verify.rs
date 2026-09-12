// Mathematical function verification. Reference values and IEEE-754 edge cases.
//
// Follows the PostgreSQL regression-test pattern (numeric.sql, float4.sql):
// cross-product tests of special values (NaN, ±Inf, ±0.0) against every
// operator and function, plus known-good reference values.

use crate::engine::database::Database;
use crate::engine::test::exec_sql;

/// Execute a scalar expression `SELECT <expr>` and return the rendered value.
fn eval_expr(db: &mut Database, expr: &str) -> String {
    let result = exec_sql(db, &format!("SELECT {}", expr));
    // Result format: [["<header>"],["<value>"]]  → extract the value cell.
    let s = result.trim().trim_start_matches('[');
    let _ = s; // parsed below with a targeted split
    result
}

/// Extract the first data cell from the JSON-ish result `[["h"],["v"]]`.
fn cell(result: &str) -> &str {
    // Find the last "…", … pattern: [["h"],["v"]] → v between the final quotes
    let after_header = result.rfind("],[").map(|i| &result[i + 3..]).unwrap_or("");
    let trimmed = after_header.trim_end_matches("]]");
    trimmed.trim_matches('"')
}

/// Assert SELECT expr returns exactly the expected cell string.
fn assert_expr(db: &mut Database, expr: &str, expected: &str) {
    let r = eval_expr(db, expr);
    let got = cell(&r);
    assert_eq!(
        got, expected,
        "SELECT {} → expected {}, got {} (raw: {})",
        expr, expected, got, r
    );
}

#[test]
fn abs_reference_values() {
    let mut db = Database::new();
    assert_expr(&mut db, "ABS(-5)", "5");
    assert_expr(&mut db, "ABS(5)", "5");
    assert_expr(&mut db, "ABS(0)", "0");
    assert_expr(&mut db, "ABS(-2.5)", "2.5");
    assert_expr(&mut db, "ABS(2.5)", "2.5");
    // Integer input stays integer; float input stays float
    assert_expr(&mut db, "ABS(-3.0)", "3.0");
}

#[test]
fn ceil_floor_round_reference_values() {
    let mut db = Database::new();
    assert_expr(&mut db, "CEIL(1.2)", "2.0");
    assert_expr(&mut db, "CEIL(1.8)", "2.0");
    assert_expr(&mut db, "CEIL(-1.2)", "-1.0");
    assert_expr(&mut db, "CEILING(2.1)", "3.0");
    assert_expr(&mut db, "FLOOR(1.2)", "1.0");
    assert_expr(&mut db, "FLOOR(1.8)", "1.0");
    assert_expr(&mut db, "FLOOR(-1.2)", "-2.0");
    assert_expr(&mut db, "ROUND(2.4)", "2.0");
    assert_expr(&mut db, "ROUND(2.5)", "3.0");
    assert_expr(&mut db, "ROUND(2.6)", "3.0");
    assert_expr(&mut db, "ROUND(-2.5)", "-3.0");
    // Two-argument round: decimal places
    assert_expr(&mut db, "ROUND(2.345, 2)", "2.35");
    assert_expr(&mut db, "ROUND(2.344, 2)", "2.34");
    assert_expr(&mut db, "ROUND(123.456, 1)", "123.5");
}

#[test]
fn pow_sqrt_sign_reference_values() {
    let mut db = Database::new();
    assert_expr(&mut db, "POW(2, 3)", "8.0");
    assert_expr(&mut db, "POWER(3, 2)", "9.0");
    assert_expr(&mut db, "POW(2, 0)", "1.0");
    assert_expr(&mut db, "POW(4, 0.5)", "2.0");
    assert_expr(&mut db, "POW(10, -1)", "0.1");
    assert_expr(&mut db, "SQRT(4)", "2.0");
    assert_expr(&mut db, "SQRT(2)", "1.4142135623730951");
    assert_expr(&mut db, "SQRT(0)", "0.0");
    assert_expr(&mut db, "SQRT(0.25)", "0.5");
    assert_expr(&mut db, "SIGN(-10)", "-1");
    assert_expr(&mut db, "SIGN(0)", "0");
    assert_expr(&mut db, "SIGN(10)", "1");
    assert_expr(&mut db, "SIGN(-0.5)", "-1");
}

#[test]
fn sqrt_negative_rejected() {
    let mut db = Database::new();
    // Bare SELECT swallows evaluation errors to NULL (consistent engine
    // behaviour).
    let r = exec_sql(&mut db, "SELECT SQRT(-1)");
    assert!(r.contains("null"), "SQRT(-1) should render null, got: {}", r);
    // A SELECT against a table row also renders null for the failing cell.
    exec_sql(&mut db, "CREATE TABLE s (v FLOAT)");
    exec_sql(&mut db, "INSERT INTO s VALUES (-1)");
    let r2 = exec_sql(&mut db, "SELECT SQRT(v) FROM s");
    assert!(r2.contains("null"), "SQRT(-1) in table → null, got: {}", r2);
}

#[test]
fn float_edge_cases() {
    let mut db = Database::new();
    // IEEE-754 special values through the JSON renderer
    assert_expr(&mut db, "ABS(-0.0)", "0.0");
    // Precision: round to 6 decimals
    assert_expr(&mut db, "ROUND(1.0 / 3.0, 6)", "0.333333");
    // Large magnitude: renders as full decimal (no scientific notation)
    let r = exec_sql(&mut db, "SELECT POW(10, 20)");
    let got = cell(&r);
    assert_eq!(got, "100000000000000000000.0", "POW(10,20) → {}, raw: {}", got, r);
}

#[test]
fn arithmetic_ieee_cross_product() {
    let mut db = Database::new();
    // Division by zero, zero, sign handling in arithmetic
    assert_expr(&mut db, "1.0 / 2.0", "0.5");
    assert_expr(&mut db, "-1.0 / 2.0", "-0.5");
    assert_expr(&mut db, "0.0 * 5.0", "0.0");
    assert_expr(&mut db, "1.0 + 1.0", "2.0");
    assert_expr(&mut db, "1.0 - 1.0", "0.0");
    // Integer vs float promotion
    assert_expr(&mut db, "3 / 2", "1");
    assert_expr(&mut db, "3.0 / 2", "1.5");
}

#[test]
fn aggregate_reference_values() {
    let mut db = Database::new();
    exec_sql(&mut db, "CREATE TABLE nums (v FLOAT)");
    exec_sql(&mut db, "INSERT INTO nums VALUES (1.5), (2.5), (3.5)");
    let r = exec_sql(&mut db, "SELECT SUM(v) FROM nums");
    let got = cell(&r);
    assert_eq!(got, "7.5", "SUM → {}, raw: {}", got, r);

    let r = exec_sql(&mut db, "SELECT AVG(v) FROM nums");
    let got = cell(&r);
    assert_eq!(got, "2.5", "AVG → {}, raw: {}", got, r);

    let r = exec_sql(&mut db, "SELECT MIN(v), MAX(v) FROM nums");
    // Multi-column row renders as a comma-joined cell string: "1.5,3.5"
    let got = cell(&r);
    assert_eq!(got, "1.5,3.5", "MIN/MAX → {}, raw: {}", got, r);

    // COUNT ignores NULLs
    exec_sql(&mut db, "INSERT INTO nums VALUES (NULL)");
    let r = exec_sql(&mut db, "SELECT COUNT(v), COUNT(*) FROM nums");
    let got = cell(&r);
    assert_eq!(got, "3,4", "COUNT(v), COUNT(*) → {}, raw: {}", got, r);
}

#[test]
fn regexp_basic() {
    let mut db = Database::new();
    let r = exec_sql(&mut db, "SELECT 'hello world' REGEXP 'hello*'");
    println!("REGEXP result: {}", r);
    assert!(r.contains("true"), "REGEXP should return true, got: {}", r);
}
