// Expression evaluation and function dispatch
// ── eval_expr / eval_literal_expr evaluate AST expressions ──
// ── exec_function dispatches to exec_std_function (in builtin.rs) or plugin fns ──

//! Expression evaluator — resolves `Expr` AST nodes against a row context.
//! Handles binary ops, unary ops, CAST, BETWEEN, IN, EXISTS, subqueries,
//! and dispatches function calls to `builtin` or plugin registry.

use std::collections::HashMap;

use sqlparser::ast::{BinaryOperator, DataType, Expr, Function, FunctionArguments, UnaryOperator};

use super::super::execute::select::exec_subquery;
use super::super::table::Table;
use super::super::value::db_value_cmp;
use super::super::value::DbValue;
use super::builtin::{
    curdate_value, exec_std_function, extract_func_args, get_func_arg_unnamed, now_value, simple_like, sql_val_to_db,
    value_to_string,
};
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
    match (a, b) {
        (DbValue::Int(x), DbValue::Int(y)) => Ok(DbValue::Int(int_op(*x, *y))),
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

/// Inline reference to values_equal in builtin.rs to avoid name collision
/// with the private helper below used only by apply_binary_op.
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

fn apply_unary_op(op: &UnaryOperator, val: &DbValue) -> Result<DbValue, EngineError> {
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

// ── CAST helper ────────────────────────────────────────────────────────

/// CAST a DbValue to the target sqlparser DataType.
fn cast_db_value(val: DbValue, target: &DataType) -> Result<DbValue, EngineError> {
    use sqlparser::ast::DataType as DT;
    match target {
        DT::Bool | DT::Boolean => match val {
            DbValue::Bool(b) => Ok(DbValue::Bool(b)),
            DbValue::Int(i) => Ok(DbValue::Bool(i != 0)),
            DbValue::Float(_) => Ok(DbValue::Bool(true)),
            DbValue::String(s) => {
                let lower = s.to_lowercase();
                match lower.as_str() {
                    "true" | "1" | "yes" => Ok(DbValue::Bool(true)),
                    "false" | "0" | "no" => Ok(DbValue::Bool(false)),
                    _ => Err(EngineError::TypeError {
                        expected: "BOOL".into(),
                        actual: format!("string '{}'", s),
                    }),
                }
            }
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(EngineError::TypeError {
                expected: "BOOL".into(),
                actual: format!("{:?}", val),
            }),
        },
        DT::Int(_) | DT::BigInt(_) | DT::SmallInt(_) | DT::TinyInt(_) => match val {
            DbValue::Int(i) => Ok(DbValue::Int(i)),
            DbValue::Float(f) => Ok(DbValue::Int(f as i64)),
            DbValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Ok(DbValue::Int(0))
                } else {
                    trimmed
                        .parse::<i64>()
                        .map(DbValue::Int)
                        .map_err(|_| EngineError::TypeError {
                            expected: "INT".into(),
                            actual: format!("string '{}'", s),
                        })
                }
            }
            DbValue::Bool(b) => Ok(DbValue::Int(if b { 1 } else { 0 })),
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(EngineError::TypeError {
                expected: "INT".into(),
                actual: format!("{:?}", val),
            }),
        },
        DT::Float(_) | DT::Double(_) | DT::Real | DT::Decimal(_) | DT::Numeric(_) => match val {
            DbValue::Int(i) => Ok(DbValue::Float(i as f64)),
            DbValue::Float(f) => Ok(DbValue::Float(f)),
            DbValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Ok(DbValue::Float(0.0))
                } else {
                    trimmed
                        .parse::<f64>()
                        .map(DbValue::Float)
                        .map_err(|_| EngineError::TypeError {
                            expected: "FLOAT".into(),
                            actual: format!("string '{}'", s),
                        })
                }
            }
            DbValue::Bool(b) => Ok(DbValue::Float(if b { 1.0 } else { 0.0 })),
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(EngineError::TypeError {
                expected: "FLOAT".into(),
                actual: format!("{:?}", val),
            }),
        },
        DT::Varchar(_) | DT::Char(_) | DT::Text | DT::String(_) | DT::Uuid => Ok(DbValue::String(val.to_string())),
        _ => Ok(DbValue::String(val.to_string())),
    }
}

// ── Fuzzy-match / FTS helpers ──────────────────────────────────────────

fn exec_fuzzy_match(
    func: &Function,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err(EngineError::Exec("fuzzy_match requires argument list".into())),
    };

    if args.len() < 2 {
        return Err(EngineError::Exec("fuzzy_match requires at least 2 arguments".into()));
    }

    let col_val = eval_expr(get_func_arg_unnamed(&args[0])?, row, col_map)?;
    let pat_val = eval_expr(get_func_arg_unnamed(&args[1])?, row, col_map)?;

    let threshold = if args.len() >= 3 {
        let t = eval_expr(get_func_arg_unnamed(&args[2])?, row, col_map)?;
        match t {
            DbValue::Float(f) => f,
            DbValue::Int(i) => i as f64,
            _ => 0.3,
        }
    } else {
        0.3
    };

    let similarity = Table::trigram_similarity(&value_to_string(&col_val), &value_to_string(&pat_val));
    Ok(DbValue::Bool(similarity >= threshold))
}

/// Return the trigram similarity score between two strings (for ranked FTS).
/// Called as `fts_score(col, pattern)` in SQL.
fn exec_fts_score(func: &Function, row: &[DbValue], col_map: &HashMap<String, usize>) -> Result<DbValue, EngineError> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err(EngineError::Exec("fts_score requires argument list".into())),
    };
    if args.len() < 2 {
        return Err(EngineError::Exec("fts_score requires at least 2 arguments".into()));
    }
    let col_val = eval_expr(get_func_arg_unnamed(&args[0])?, row, col_map)?;
    let pat_val = eval_expr(get_func_arg_unnamed(&args[1])?, row, col_map)?;
    let similarity = Table::trigram_similarity(&value_to_string(&col_val), &value_to_string(&pat_val));
    Ok(DbValue::Float(similarity))
}

// ── Expression evaluator ────────────────────────────────────────────────

pub(crate) fn eval_expr(
    expr: &Expr,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    match expr {
        Expr::Identifier(ident) => {
            let name = ident.value.to_lowercase();
            if name == "current_timestamp" || name == "current_time" {
                return Ok(now_value());
            }
            if name == "current_date" {
                return Ok(curdate_value());
            }
            let idx = col_map.get(&name).ok_or_else(|| EngineError::ColumnNotFound(name))?;
            Ok(row[*idx].clone())
        }
        Expr::CompoundIdentifier(parts) => {
            // e.g. EXCLUDED.v → "excluded.v", table.col → "table.col"
            let name = parts
                .iter()
                .map(|p| p.value.to_lowercase())
                .collect::<Vec<_>>()
                .join(".");
            match col_map.get(&name) {
                Some(&pos) => Ok(row[pos].clone()),
                None => {
                    // Fallback: try just the last (column) part
                    let last = parts.last().unwrap().value.to_lowercase();
                    match col_map.get(&last) {
                        Some(&pos) => Ok(row[pos].clone()),
                        None => Err(EngineError::ColumnNotFound(name)),
                    }
                }
            }
        }
        Expr::Value(v) => Ok(sql_val_to_db(v)),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr(left, row, col_map)?;
            let r = eval_expr(right, row, col_map)?;
            apply_binary_op(&l, op, &r)
        }
        Expr::UnaryOp { op, expr } => {
            let val = eval_expr(expr, row, col_map)?;
            apply_unary_op(op, &val)
        }
        Expr::Nested(inner) => eval_expr(inner, row, col_map),
        Expr::IsNull(expr) => {
            let val = eval_expr(expr, row, col_map)?;
            Ok(DbValue::Bool(matches!(val, DbValue::Null)))
        }
        Expr::IsNotNull(expr) => {
            let val = eval_expr(expr, row, col_map)?;
            Ok(DbValue::Bool(!matches!(val, DbValue::Null)))
        }
        Expr::Like {
            negated, expr, pattern, ..
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let pat = eval_expr(pattern, row, col_map)?;
            let matched = simple_like(&value_to_string(&val), &value_to_string(&pat));
            Ok(DbValue::Bool(if *negated { !matched } else { matched }))
        }
        Expr::SimilarTo {
            negated, expr, pattern, ..
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let pat = eval_expr(pattern, row, col_map)?;
            // ponytail: SIMILAR TO uses LIKE-style matching (%, _ wildcards)
            let matched = simple_like(&value_to_string(&val), &value_to_string(&pat));
            Ok(DbValue::Bool(if *negated { !matched } else { matched }))
        }
        Expr::InList { expr, list, negated } => {
            let val = eval_expr(expr, row, col_map)?;
            let mut found = false;
            for item in list {
                let item_val = eval_expr(item, row, col_map)?;
                if val == item_val {
                    found = true;
                    break;
                }
            }
            Ok(DbValue::Bool(if *negated { !found } else { found }))
        }
        Expr::Function(func) => exec_function(func, row, col_map),
        Expr::InSubquery {
            expr,
            subquery,
            negated,
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let subq_result = exec_subquery(subquery)?;
            let found = subq_result.contains(&val);
            Ok(DbValue::Bool(if *negated { !found } else { found }))
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_val = operand.as_ref().map(|o| eval_expr(o, row, col_map)).transpose()?;
            for cw in conditions.iter() {
                let matched = match &operand_val {
                    Some(ref op_val) => *op_val == eval_expr(&cw.condition, row, col_map)?,
                    None => is_truthy(&eval_expr(&cw.condition, row, col_map)?),
                };
                if matched {
                    return eval_expr(&cw.result, row, col_map);
                }
            }
            match else_result {
                Some(expr) => eval_expr(expr, row, col_map),
                None => Ok(DbValue::Null),
            }
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let l = eval_expr(low, row, col_map)?;
            let h = eval_expr(high, row, col_map)?;
            use std::cmp::Ordering;
            let ge = db_value_cmp(&val, &l) != Ordering::Less;
            let le = db_value_cmp(&val, &h) != Ordering::Greater;
            Ok(DbValue::Bool(if *negated { !(ge && le) } else { ge && le }))
        }
        Expr::Exists { subquery, negated } => {
            let vals = exec_subquery(subquery)?;
            Ok(DbValue::Bool(if *negated { vals.is_empty() } else { !vals.is_empty() }))
        }
        Expr::Cast { expr, data_type, .. } => {
            let val = eval_expr(expr, row, col_map)?;
            cast_db_value(val, data_type)
        }
        Expr::Trim {
            expr,
            trim_where,
            trim_characters,
            ..
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let s = value_to_string(&val);
            let trimmed = match (trim_where, trim_characters) {
                (_, Some(chars_exprs)) => {
                    if let Some(chars_expr) = chars_exprs.first() {
                        let chars_val = eval_expr(chars_expr, row, col_map)?;
                        let chars = value_to_string(&chars_val);
                        let c = chars.chars().next().unwrap_or(' ');
                        match trim_where {
                            Some(sqlparser::ast::TrimWhereField::Leading) => s.trim_start_matches(c).to_string(),
                            Some(sqlparser::ast::TrimWhereField::Trailing) => s.trim_end_matches(c).to_string(),
                            _ => s.trim_matches(c).to_string(),
                        }
                    } else {
                        s.trim().to_string()
                    }
                }
                (Some(sqlparser::ast::TrimWhereField::Leading), _) => s.trim_start().to_string(),
                (Some(sqlparser::ast::TrimWhereField::Trailing), _) => s.trim_end().to_string(),
                _ => s.trim().to_string(),
            };
            Ok(DbValue::String(trimmed))
        }
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let s = value_to_string(&val);
            let start: usize = match substring_from {
                Some(from) => {
                    let from_val = eval_expr(from, row, col_map)?;
                    match from_val {
                        DbValue::Int(i) => i.max(1) as usize - 1,
                        _ => {
                            return Err(EngineError::TypeError {
                                expected: "integer start".into(),
                                actual: format!("{:?}", from_val),
                            })
                        }
                    }
                }
                None => 0,
            };
            let result = match substring_for {
                Some(len_expr) => {
                    let len_val = eval_expr(len_expr, row, col_map)?;
                    match len_val {
                        DbValue::Int(i) => s.chars().skip(start).take(i as usize).collect(),
                        _ => {
                            return Err(EngineError::TypeError {
                                expected: "integer length".into(),
                                actual: format!("{:?}", len_val),
                            })
                        }
                    }
                }
                None => s.chars().skip(start).collect(),
            };
            Ok(DbValue::String(result))
        }
        _ => Err(EngineError::Exec(format!("Unsupported expression: {:?}", expr))),
    }
}

pub(crate) fn eval_literal_expr(expr: &Expr) -> Result<DbValue, EngineError> {
    match expr {
        Expr::Value(v) => Ok(sql_val_to_db(&v.value)),
        Expr::Nested(inner) => eval_literal_expr(inner),
        Expr::UnaryOp { op, expr } => {
            let val = eval_literal_expr(expr)?;
            apply_unary_op(op, &val)
        }
        _ => Err(EngineError::Exec(format!(
            "Complex expressions not supported in values: {:?}",
            expr
        ))),
    }
}

// ── Function dispatch ────────────────────────────────────────────────────

/// Dispatch a function call to either a built-in standard function, a plugin fn_* function,
/// or a fuzzy match / FTS score function.
pub(crate) fn exec_function(
    func: &Function,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    let name = func.name.to_string().to_lowercase();
    match name.as_str() {
        "fuzzy_match" => exec_fuzzy_match(func, row, col_map),
        "fts_score" => exec_fts_score(func, row, col_map),
        _ => {
            // Check plugin registry for fn_ prefixed functions
            if let Some(fn_name) = name.strip_prefix("fn_") {
                if let Some((pfunc, _plugin)) = crate::engine::plugin::lookup_function(fn_name) {
                    let args = extract_func_args(func);
                    if args.len() < pfunc.min_args {
                        return Err(EngineError::Exec(format!(
                            "{}: expected {} args, got {}",
                            fn_name,
                            pfunc.min_args,
                            args.len()
                        )));
                    }
                    return (pfunc.func)(&args);
                }
                return Err(EngineError::Exec(format!("Unknown plugin function '{}'", fn_name)));
            }
            exec_std_function(func, name, row, col_map)
        }
    }
}
