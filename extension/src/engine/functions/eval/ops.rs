// Expression operators — binary, unary, arithmetic, comparison, truthiness, wildcard matching
// ── Private helpers for the eval module; re-exported from parent ──

use sqlparser::ast::{BinaryOperator, UnaryOperator};

use super::super::super::value::DbValue;
use super::super::builtin::value_to_string;
use crate::engine::error::EngineError;

// ── Numeric helpers ────────────────────────────────────────────────────

pub(crate) fn to_float(v: &DbValue) -> Option<f64> {
    match v {
        DbValue::Int(n) => Some(*n as f64),
        DbValue::Float(f) => Some(*f),
        DbValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn arith_op<F, G>(a: &DbValue, b: &DbValue, int_op: F, float_op: G) -> Result<DbValue, EngineError>
where
    F: Fn(i64, i64) -> i64,
    G: Fn(f64, f64) -> f64,
{
    // SQL: NULL in any arithmetic → NULL
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return Ok(DbValue::Null);
    }
    // Integer division by zero guard: pass a wrapper that checks
    let safe_int = |x: i64, y: i64| -> Result<i64, EngineError> {
        if y == 0 {
            Err(EngineError::Exec("Division by zero".into()))
        } else {
            Ok(int_op(x, y))
        }
    };
    match (a, b) {
        (DbValue::Int(x), DbValue::Int(y)) => safe_int(*x, *y).map(DbValue::Int),
        _ => match (to_float(a), to_float(b)) {
            (Some(x), Some(y)) => Ok(DbValue::Float(float_op(x, y))),
            _ => Err(EngineError::TypeError {
                expected: "numeric type".into(),
                actual: format!("{} vs {}", a, b),
            }),
        },
    }
}

fn cmp_values<F>(a: &DbValue, b: &DbValue, cmp: F) -> Result<DbValue, EngineError>
where
    F: Fn(std::cmp::Ordering) -> bool,
{
    // SQL: NULL compared to anything is NULL (treated as false)
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return Ok(DbValue::Bool(false));
    }
    let ord = match (to_float(a), to_float(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => value_to_string(a).cmp(&value_to_string(b)),
    };
    Ok(DbValue::Bool(cmp(ord)))
}

// ── Wildcard matching (REGEXP operator) ────────────────────────────────

/// Simple wildcard matching: `*` matches any sequence, `?` matches single char.
fn wildcard_match(val: &[char], pat: &[char], vi: usize, pi: usize) -> bool {
    if pi == pat.len() {
        return vi == val.len();
    }
    match pat[pi] {
        '*' => {
            let mut vi2 = vi;
            while vi2 <= val.len() {
                if wildcard_match(val, pat, vi2, pi + 1) {
                    return true;
                }
                vi2 += 1;
            }
            false
        }
        '?' => vi < val.len() && wildcard_match(val, pat, vi + 1, pi + 1),
        c => vi < val.len() && val[vi] == c && wildcard_match(val, pat, vi + 1, pi + 1),
    }
}

// ── Binary operators ───────────────────────────────────────────────────

pub(crate) fn apply_binary_op(left: &DbValue, op: &BinaryOperator, right: &DbValue) -> Result<DbValue, EngineError> {
    match op {
        BinaryOperator::Eq => Ok(DbValue::Bool(values_equal_builtin(left, right))),
        BinaryOperator::NotEq => Ok(DbValue::Bool(!values_equal_builtin(left, right))),
        BinaryOperator::Lt => cmp_values(left, right, |o| o.is_lt()),
        BinaryOperator::LtEq => cmp_values(left, right, |o| o.is_le()),
        BinaryOperator::Gt => cmp_values(left, right, |o| o.is_gt()),
        BinaryOperator::GtEq => cmp_values(left, right, |o| o.is_ge()),
        BinaryOperator::Plus => arith_op(left, right, |a, b| a + b, |a, b| a + b),
        BinaryOperator::Minus => arith_op(left, right, |a, b| a - b, |a, b| a - b),
        BinaryOperator::Multiply => arith_op(left, right, |a, b| a * b, |a, b| a * b),
        BinaryOperator::Divide => arith_op(left, right, |a, b| a / b, |a, b| a / b),
        BinaryOperator::Modulo => match (to_float(left), to_float(right)) {
            (Some(a), Some(b)) if b != 0.0 => Ok(DbValue::Float(a % b)),
            _ => Err(EngineError::TypeError {
                expected: "numeric operands".into(),
                actual: format!("{:?} and {:?}", left, right),
            }),
        },
        BinaryOperator::And => Ok(DbValue::Bool(is_truthy(left) && is_truthy(right))),
        BinaryOperator::Or => Ok(DbValue::Bool(is_truthy(left) || is_truthy(right))),
        BinaryOperator::StringConcat => Ok(DbValue::String(format!(
            "{}{}",
            value_to_string(left),
            value_to_string(right)
        ))),
        BinaryOperator::Regexp => {
            let s = value_to_string(left);
            let pat = value_to_string(right);
            let val: Vec<char> = s.chars().collect();
            let p: Vec<char> = pat.chars().collect();
            Ok(DbValue::Bool(wildcard_match(&val, &p, 0, 0)))
        }
        BinaryOperator::Match => {
            let s = value_to_string(left);
            let pat = value_to_string(right);
            Ok(DbValue::Bool(s.to_lowercase().contains(&pat.to_lowercase())))
        }
        _ => Err(EngineError::Exec(format!("Unsupported operator: {:?}", op))),
    }
}

/// Inline reference to values_equal in builtin.rs to avoid name collision.
fn values_equal_builtin(a: &DbValue, b: &DbValue) -> bool {
    // NULL != anything (including NULL), per SQL standard
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return false;
    }
    match (a, b) {
        (DbValue::Int(x), DbValue::Int(y)) => x == y,
        (DbValue::Float(x), DbValue::Float(y)) => (x - y).abs() < f64::EPSILON,
        (DbValue::Int(x), DbValue::Float(y)) => (*x as f64 - y).abs() < f64::EPSILON,
        (DbValue::Float(x), DbValue::Int(y)) => (x - *y as f64).abs() < f64::EPSILON,
        (DbValue::Bool(x), DbValue::Bool(y)) => x == y,
        (DbValue::String(x), DbValue::String(y)) => x == y,
        _ => value_to_string(a) == value_to_string(b),
    }
}

// ── Unary operators ────────────────────────────────────────────────────

pub(super) fn apply_unary_op(op: &UnaryOperator, val: &DbValue) -> Result<DbValue, EngineError> {
    match op {
        UnaryOperator::Not => Ok(DbValue::Bool(!is_truthy(val))),
        UnaryOperator::Plus => Ok(val.clone()),
        UnaryOperator::Minus => match val {
            DbValue::Int(n) => Ok(DbValue::Int(-n)),
            DbValue::Float(f) => Ok(DbValue::Float(-f)),
            _ => Err(EngineError::TypeError {
                expected: "numeric".into(),
                actual: format!("{}", val),
            }),
        },
        _ => Err(EngineError::Exec(format!("Unsupported unary operator: {:?}", op))),
    }
}

// ── Truthiness ─────────────────────────────────────────────────────────

pub(crate) fn is_truthy(v: &DbValue) -> bool {
    match v {
        DbValue::Null => false,
        DbValue::Bool(b) => *b,
        DbValue::Int(n) => *n != 0,
        DbValue::Float(f) => *f != 0.0,
        DbValue::String(s) => !s.is_empty(),
        DbValue::Strings(arr) => !arr.is_empty(),
        DbValue::Floats(arr) => !arr.is_empty(),
    }
}
