// SQF expression evaluator — evaluates the AST to DbValue.
//
// Fast-path evaluation (Rust-only, no Arma callbacks):
// - Arithmetic: +, -, *, /, %
// - Comparison: ==, !=, <, >, <=, >=
// - Logical: &&, ||, !
// - String concat (+)
// - Unary negation and boolean not
// - Variables are unresolved (return Error unless in bindings)

use std::collections::HashMap;

use super::parser::{Expr, Op, UnaryOp};
use crate::engine::value::DbValue;

/// Evaluate an SQF expression AST to a DbValue.
///
/// `bindings` maps variable names (with `_` prefix) to values.
/// Pass an empty map for standalone expression evaluation.
pub(crate) fn eval(expr: &Expr, bindings: &HashMap<String, DbValue>) -> Result<DbValue, String> {
    match expr {
        Expr::Int(n) => Ok(DbValue::Int(*n)),
        Expr::Float(f) => Ok(DbValue::Float(*f)),
        Expr::String(s) => Ok(DbValue::String(s.clone())),
        Expr::Bool(b) => Ok(DbValue::Bool(*b)),
        Expr::Null => Ok(DbValue::Null),

        Expr::Variable(name) => match bindings.get(name) {
            Some(v) => Ok(v.clone()),
            None => Err(format!("undefined variable in SQF expression: {}", name)),
        },

        Expr::Command(name, args) => {
            let mut evaluated = Vec::with_capacity(args.len());
            for a in args {
                evaluated.push(eval(a, bindings)?);
            }
            eval_command(name, &evaluated)
        }

        Expr::Unary(op, rhs) => {
            let r = eval(rhs, bindings)?;
            match op {
                UnaryOp::Neg => match r {
                    DbValue::Int(n) => Ok(DbValue::Int(-n)),
                    DbValue::Float(f) => Ok(DbValue::Float(-f)),
                    _ => Err(format!("cannot negate {}", r)),
                },
                UnaryOp::Not => match r {
                    DbValue::Bool(b) => Ok(DbValue::Bool(!b)),
                    DbValue::Int(n) => Ok(DbValue::Bool(n == 0)),
                    _ => Err(format!("cannot apply ! to {}", r)),
                },
            }
        }

        Expr::Binary(op, lhs, rhs) => {
            let l = eval(lhs, bindings)?;
            let r = eval(rhs, bindings)?;
            apply_binary(op, &l, &r)
        }
    }
}

/// Evaluate a named SQF command with pre-evaluated arguments.
fn eval_command(name: &str, args: &[DbValue]) -> Result<DbValue, String> {
    match name {
        // Nular constants
        "pi" => Ok(DbValue::Float(std::f64::consts::PI)),
        "true" => Ok(DbValue::Bool(true)),
        "false" => Ok(DbValue::Bool(false)),
        "nil" => Ok(DbValue::Null),

        // Unary math commands
        "sqrt" => unary_math(args, |x| x.sqrt()),
        "sin" => unary_math(args, |x| x.sin()),
        "cos" => unary_math(args, |x| x.cos()),
        "tan" => unary_math(args, |x| x.tan()),
        "abs" => unary_math(args, |x| x.abs()),
        "exp" => unary_math(args, |x| x.exp()),
        "ln" => unary_math(args, |x| x.ln()),
        "log" => unary_math(args, |x| x.log10()),
        "log10" => unary_math(args, |x| x.log10()),
        "round" => unary_math_int(args, |x| x.round()),
        "floor" => unary_math_int(args, |x| x.floor()),
        "ceil" => unary_math_int(args, |x| x.ceil()),
        "asin" => unary_math(args, |x| x.asin()),
        "acos" => unary_math(args, |x| x.acos()),
        "atan" => unary_math(args, |x| x.atan()),
        "deg" => unary_math(args, |x| x.to_degrees()),
        "rad" => unary_math(args, |x| x.to_radians()),

        // Unary string commands
        "str" | "to_string" => {
            if args.is_empty() {
                return Err("str requires 1 argument".into());
            }
            Ok(DbValue::String(value_str(&args[0])))
        }
        "toupper" | "to_upper" => {
            if args.is_empty() {
                return Err("toUpper requires 1 argument".into());
            }
            Ok(DbValue::String(value_str(&args[0]).to_uppercase()))
        }
        "tolower" | "to_lower" => {
            if args.is_empty() {
                return Err("toLower requires 1 argument".into());
            }
            Ok(DbValue::String(value_str(&args[0]).to_lowercase()))
        }
        "typename" | "type_name" => {
            if args.is_empty() {
                return Err("typeName requires 1 argument".into());
            }
            let type_str = match &args[0] {
                DbValue::Null => "NULL",
                DbValue::Bool(_) => "BOOL",
                DbValue::Int(_) => "SCALAR",
                DbValue::Float(_) => "SCALAR",
                DbValue::String(_) => "STRING",
                DbValue::Strings(_) => "ARRAY",
                DbValue::Floats(_) => "ARRAY",
            };
            Ok(DbValue::String(type_str.into()))
        }
        "count" => {
            if args.is_empty() {
                return Err("count requires 1 argument".into());
            }
            let n = match &args[0] {
                DbValue::String(s) => s.len() as i64,
                DbValue::Strings(v) => v.len() as i64,
                DbValue::Floats(v) => v.len() as i64,
                _ => {
                    // SQF count on non-iterable returns 0
                    0
                }
            };
            Ok(DbValue::Int(n))
        }
        "parsenumber" | "parse_number" => {
            if args.is_empty() {
                return Err("parseNumber requires 1 argument".into());
            }
            let s = value_str(&args[0]);
            match s.trim().parse::<f64>() {
                Ok(n) => {
                    if n.fract() == 0.0 && n.is_finite() && (n as i64 as f64 == n) {
                        Ok(DbValue::Int(n as i64))
                    } else {
                        Ok(DbValue::Float(n))
                    }
                }
                Err(_) => Ok(DbValue::Int(0)), // SQF returns 0 on parse failure
            }
        }
        "hint" | "hintc" => {
            // In fast-path eval, hint is a no-op (it needs engine callbacks).
            // Return the message as string so it can be observed in testing.
            if args.is_empty() {
                return Err("hint requires 1 argument".into());
            }
            Ok(DbValue::String(value_str(&args[0])))
        }

        _ => Err(format!("unknown or unsupported command: {}", name)),
    }
}

/// Helper: apply a math function to a unary command argument.
fn unary_math<F>(args: &[DbValue], f: F) -> Result<DbValue, String>
where
    F: Fn(f64) -> f64,
{
    if args.is_empty() {
        return Err("command requires 1 argument".into());
    }
    let x = match &args[0] {
        DbValue::Int(n) => *n as f64,
        DbValue::Float(n) => *n,
        DbValue::Null => return Ok(DbValue::Null),
        _ => return Err(format!("numeric argument expected, got {}", args[0])),
    };
    Ok(DbValue::Float(f(x)))
}

/// Helper: apply a math function that returns a rounded integer.
fn unary_math_int<F>(args: &[DbValue], f: F) -> Result<DbValue, String>
where
    F: Fn(f64) -> f64,
{
    if args.is_empty() {
        return Err("command requires 1 argument".into());
    }
    let x = match &args[0] {
        DbValue::Int(n) => *n as f64,
        DbValue::Float(n) => *n,
        DbValue::Null => return Ok(DbValue::Null),
        _ => return Err(format!("numeric argument expected, got {}", args[0])),
    };
    Ok(DbValue::Int(f(x) as i64))
}

fn apply_binary(op: &Op, l: &DbValue, r: &DbValue) -> Result<DbValue, String> {
    match op {
        // Arithmetic
        Op::Add => add(l, r),
        Op::Sub => arith(l, r, |a, b| a - b, |a, b| a - b),
        Op::Mul => arith(l, r, |a, b| a * b, |a, b| a * b),
        Op::Div => arith(l, r, |a, b| a / b, |a, b| a / b),
        Op::Mod => arith_int(l, r, |a, b| a % b),

        // Comparison
        Op::Eq => cmp(l, r, |o| o.is_eq()),
        Op::Neq => cmp(l, r, |o| !o.is_eq()),
        Op::Lt => cmp(l, r, |o| o.is_lt()),
        Op::Gt => cmp(l, r, |o| o.is_gt()),
        Op::Le => cmp(l, r, |o| o.is_le()),
        Op::Ge => cmp(l, r, |o| o.is_ge()),

        // Logical
        Op::And => logical_and(l, r),
        Op::Or => logical_or(l, r),
    }
}

fn to_f64(v: &DbValue) -> Option<f64> {
    match v {
        DbValue::Int(n) => Some(*n as f64),
        DbValue::Float(f) => Some(*f),
        _ => None,
    }
}

fn to_bool(v: &DbValue) -> bool {
    match v {
        DbValue::Bool(b) => *b,
        DbValue::Int(n) => *n != 0,
        DbValue::Float(f) => *f != 0.0,
        DbValue::Null => false,
        DbValue::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn add(l: &DbValue, r: &DbValue) -> Result<DbValue, String> {
    // String concat if either operand is a string
    if matches!(l, DbValue::String(_)) || matches!(r, DbValue::String(_)) {
        return Ok(DbValue::String(format!("{}{}", value_str(l), value_str(r))));
    }
    arith(l, r, |a, b| a + b, |a, b| a + b)
}

fn value_str(v: &DbValue) -> String {
    match v {
        DbValue::Null => "nil".into(),
        DbValue::Bool(b) => b.to_string(),
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => s.clone(),
        DbValue::Strings(arr) => arr.join(","),
        DbValue::Floats(arr) => arr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","),
    }
}

fn arith<F, G>(l: &DbValue, r: &DbValue, int_op: F, float_op: G) -> Result<DbValue, String>
where
    F: Fn(i64, i64) -> i64,
    G: Fn(f64, f64) -> f64,
{
    if matches!(l, DbValue::Null) || matches!(r, DbValue::Null) {
        return Ok(DbValue::Null);
    }
    match (l, r) {
        (DbValue::Int(a), DbValue::Int(b)) => Ok(DbValue::Int(int_op(*a, *b))),
        _ => match (to_f64(l), to_f64(r)) {
            (Some(a), Some(b)) => Ok(DbValue::Float(float_op(a, b))),
            _ => Err(format!("type mismatch: {} {} {}", l, op_name(), r)),
        },
    }
}

fn arith_int<F>(l: &DbValue, r: &DbValue, op: F) -> Result<DbValue, String>
where
    F: Fn(i64, i64) -> i64,
{
    if matches!(l, DbValue::Null) || matches!(r, DbValue::Null) {
        return Ok(DbValue::Null);
    }
    match (l, r) {
        (DbValue::Int(a), DbValue::Int(b)) => Ok(DbValue::Int(op(*a, *b))),
        _ => Err(format!("type mismatch: {} {} {}", l, op_name(), r)),
    }
}

fn op_name() -> &'static str {
    // not ideal but this is a helper for error messages
    "op"
}

fn cmp<F>(l: &DbValue, r: &DbValue, cmp: F) -> Result<DbValue, String>
where
    F: Fn(std::cmp::Ordering) -> bool,
{
    if matches!(l, DbValue::Null) || matches!(r, DbValue::Null) {
        return Ok(DbValue::Bool(false));
    }
    let ord = match (to_f64(l), to_f64(r)) {
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
        _ => value_str(l).cmp(&value_str(r)),
    };
    Ok(DbValue::Bool(cmp(ord)))
}

fn logical_and(l: &DbValue, r: &DbValue) -> Result<DbValue, String> {
    Ok(DbValue::Bool(to_bool(l) && to_bool(r)))
}

fn logical_or(l: &DbValue, r: &DbValue) -> Result<DbValue, String> {
    Ok(DbValue::Bool(to_bool(l) || to_bool(r)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sqf::lexer::tokenize;
    use crate::engine::sqf::parser::parse;

    fn eval_str(input: &str) -> Result<DbValue, String> {
        let tokens = tokenize(input)?;
        let expr = parse(tokens)?;
        eval(&expr, &HashMap::new())
    }

    fn eval_with_bindings(input: &str, bindings: &[(&str, DbValue)]) -> Result<DbValue, String> {
        let tokens = tokenize(input)?;
        let expr = parse(tokens)?;
        let map: HashMap<String, DbValue> = bindings.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        eval(&expr, &map)
    }

    #[test]
    fn test_literal_int() {
        assert_eq!(eval_str("42").unwrap(), DbValue::Int(42));
    }

    #[test]
    fn test_literal_float() {
        assert_eq!(eval_str("3.14").unwrap(), DbValue::Float(3.14));
    }

    #[test]
    fn test_literal_string() {
        assert_eq!(eval_str(r#""hello""#).unwrap(), DbValue::String("hello".into()));
    }

    #[test]
    fn test_literal_bool() {
        assert_eq!(eval_str("true").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("false").unwrap(), DbValue::Bool(false));
    }

    #[test]
    fn test_literal_nil() {
        assert_eq!(eval_str("nil").unwrap(), DbValue::Null);
    }

    #[test]
    fn test_add_int() {
        assert_eq!(eval_str("1 + 2").unwrap(), DbValue::Int(3));
    }

    #[test]
    fn test_add_float() {
        assert_eq!(eval_str("1.5 + 2.5").unwrap(), DbValue::Float(4.0));
    }

    #[test]
    fn test_string_concat() {
        assert_eq!(
            eval_str(r#""hello" + " world""#).unwrap(),
            DbValue::String("hello world".into())
        );
    }

    #[test]
    fn test_sub() {
        assert_eq!(eval_str("10 - 3").unwrap(), DbValue::Int(7));
    }

    #[test]
    fn test_mul() {
        assert_eq!(eval_str("3 * 4").unwrap(), DbValue::Int(12));
    }

    #[test]
    fn test_div() {
        assert_eq!(eval_str("10 / 3").unwrap(), DbValue::Int(3));
    }

    #[test]
    fn test_div_float() {
        let r = eval_str("10.0 / 3").unwrap();
        let expected = DbValue::Float(10.0 / 3.0);
        match (&r, &expected) {
            (DbValue::Float(a), DbValue::Float(b)) => assert!((a - b).abs() < 0.0001),
            _ => panic!("expected Float, got {:?}", r),
        }
    }

    #[test]
    fn test_mod() {
        assert_eq!(eval_str("10 % 3").unwrap(), DbValue::Int(1));
    }

    #[test]
    fn test_precedence() {
        assert_eq!(eval_str("1 + 2 * 3").unwrap(), DbValue::Int(7));
        assert_eq!(eval_str("(1 + 2) * 3").unwrap(), DbValue::Int(9));
    }

    #[test]
    fn test_comparison() {
        assert_eq!(eval_str("1 == 1").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("1 == 2").unwrap(), DbValue::Bool(false));
        assert_eq!(eval_str("1 != 2").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("1 < 2").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("2 > 1").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("1 <= 1").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("1 >= 2").unwrap(), DbValue::Bool(false));
    }

    #[test]
    fn test_logical() {
        assert_eq!(eval_str("true && true").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("true && false").unwrap(), DbValue::Bool(false));
        assert_eq!(eval_str("true || false").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("false || false").unwrap(), DbValue::Bool(false));
    }

    #[test]
    fn test_unary_neg() {
        assert_eq!(eval_str("-5").unwrap(), DbValue::Int(-5));
        assert_eq!(eval_str("--5").unwrap(), DbValue::Int(5));
    }

    #[test]
    fn test_unary_not() {
        assert_eq!(eval_str("!true").unwrap(), DbValue::Bool(false));
        assert_eq!(eval_str("!false").unwrap(), DbValue::Bool(true));
    }

    #[test]
    fn test_chained_comparison() {
        assert_eq!(eval_str("1 < 2 && 2 < 3").unwrap(), DbValue::Bool(true));
        assert_eq!(eval_str("1 < 2 && 3 < 2").unwrap(), DbValue::Bool(false));
    }

    #[test]
    fn test_variable_binding() {
        let r = eval_with_bindings("_x + 1", &[("_x", DbValue::Int(41))]).unwrap();
        assert_eq!(r, DbValue::Int(42));
    }

    #[test]
    fn test_undefined_variable() {
        assert!(eval_str("_x").is_err());
    }

    #[test]
    fn test_null_arithmetic() {
        assert_eq!(eval_str("nil + 1").unwrap(), DbValue::Null);
        assert_eq!(eval_str("1 + nil").unwrap(), DbValue::Null);
    }

    #[test]
    fn test_null_comparison() {
        assert_eq!(eval_str("nil == nil").unwrap(), DbValue::Bool(false)); // SQL-style
        assert_eq!(eval_str("nil == 1").unwrap(), DbValue::Bool(false));
    }

    #[test]
    fn test_complex_expression() {
        // (1 + 2) * 3 - 4 / 2 == 7
        let r = eval_str("(1 + 2) * 3 - 4 / 2").unwrap();
        assert_eq!(r, DbValue::Int(7));
    }

    // ── Command tests ───────────────────────────────────────────────────

    #[test]
    fn test_command_pi() {
        assert_eq!(eval_str("pi").unwrap(), DbValue::Float(std::f64::consts::PI));
    }

    #[test]
    fn test_command_sqrt() {
        let r = eval_str("sqrt 25").unwrap();
        assert_eq!(r, DbValue::Float(5.0));
    }

    #[test]
    fn test_command_sqrt_expr() {
        let r = eval_str("sqrt 9 + 16").unwrap();
        assert_eq!(r, DbValue::Float(19.0)); // (sqrt 9) + 16
    }

    #[test]
    fn test_command_abs_neg() {
        assert_eq!(eval_str("abs -5").unwrap(), DbValue::Float(5.0));
    }

    #[test]
    fn test_command_sin() {
        let r = eval_str("sin 0").unwrap();
        assert_eq!(r, DbValue::Float(0.0));
    }

    #[test]
    fn test_command_round() {
        assert_eq!(eval_str("round 3.7").unwrap(), DbValue::Int(4));
        assert_eq!(eval_str("floor 3.7").unwrap(), DbValue::Int(3));
        assert_eq!(eval_str("ceil 3.2").unwrap(), DbValue::Int(4));
    }

    #[test]
    fn test_command_toupper() {
        assert_eq!(eval_str(r#"toUpper "hello""#).unwrap(), DbValue::String("HELLO".into()));
    }

    #[test]
    fn test_command_tolower() {
        assert_eq!(eval_str(r#"toLower "HELLO""#).unwrap(), DbValue::String("hello".into()));
    }

    #[test]
    fn test_command_str() {
        let r = eval_str("str 42").unwrap();
        assert_eq!(r, DbValue::String("42".into()));
    }

    #[test]
    fn test_command_typename() {
        assert_eq!(eval_str("typeName 42").unwrap(), DbValue::String("SCALAR".into()));
        assert_eq!(
            eval_str(r#"typeName "hello""#).unwrap(),
            DbValue::String("STRING".into())
        );
        assert_eq!(eval_str("typeName true").unwrap(), DbValue::String("BOOL".into()));
        assert_eq!(eval_str("typeName nil").unwrap(), DbValue::String("NULL".into()));
    }

    #[test]
    fn test_command_parsenumber() {
        assert_eq!(eval_str(r#"parseNumber "42""#).unwrap(), DbValue::Int(42));
        let r = eval_str(r#"parseNumber "3.14""#).unwrap();
        match r {
            DbValue::Float(f) => assert!((f - 3.14).abs() < 0.001),
            _ => panic!("expected Float"),
        }
    }

    #[test]
    fn test_command_count_string() {
        assert_eq!(eval_str(r#"count "hello""#).unwrap(), DbValue::Int(5));
    }

    #[test]
    fn test_command_expression_chain() {
        // sqrt(abs(-9)) + round(3.7)
        let r = eval_str("sqrt abs -9 + round 3.7").unwrap();
        assert_eq!(r, DbValue::Float(7.0)); // sqrt(9) + 4 = 3 + 4 = 7
    }
}
