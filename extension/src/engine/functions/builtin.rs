// Standard SQL function implementations — the big function dispatch and all
// built-in scalar functions that don't need the expression evaluator.

//! Built-in scalar functions — UPPER, LOWER, SUBSTR, TRIM, COALESCE, ROUND, ABS,
//! date functions (NOW, CURDATE, DATE_FORMAT, DATEDIFF, UNIX_TIMESTAMP),
//! and utility functions (LAST_INSERT_ROWID, CHANGES, TYPEOF).

use std::collections::HashMap;

use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, TableFactor, TableWithJoins, Value,
};

use super::super::database::Database;
use super::super::execute::{LAST_CHANGES, LAST_INSERT_ROWID, execute};
use super::super::index::IndexType as A3IndexType;
use super::super::stmts::ddl::object_name_str;
use super::super::table::{IndexImpl, Table};
use super::super::value::json_val_to_dbvalue;
use super::super::value::{Column, ColumnType, DbValue};

use super::eval::eval_expr;

use crate::engine::error::EngineError;
use std::cell::Cell;

thread_local! {
    /// Set by RAISE(ABORT, 'msg') to signal trigger abort.
    pub(crate) static RAISE_ABORTED: Cell<bool> = const { Cell::new(false) };
}

// ── String conversion ──────────────────────────────────────────────────

/// Convert a DbValue to its string representation.
pub(crate) fn value_to_string(v: &DbValue) -> String {
    match v {
        DbValue::Null => "NULL".into(),
        DbValue::Bool(b) => b.to_string(),
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => s.clone(),
        DbValue::Strings(arr) => arr.join(","),
        DbValue::Floats(arr) => arr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","),
    }
}

/// Convert a sqlparser Value to DbValue.
pub(crate) fn sql_val_to_db(v: &sqlparser::ast::Value) -> DbValue {
    match v {
        sqlparser::ast::Value::Null => DbValue::Null,
        sqlparser::ast::Value::Boolean(b) => DbValue::Bool(*b),
        sqlparser::ast::Value::Number(s, _) => {
            if s.contains('.') || s.contains('e') || s.contains('E') {
                s.parse::<f64>()
                    .map(DbValue::Float)
                    .or_else(|_| s.parse::<i64>().map(DbValue::Int))
                    .unwrap_or(DbValue::String(s.clone()))
            } else {
                s.parse::<i64>()
                    .map(DbValue::Int)
                    .or_else(|_| s.parse::<f64>().map(DbValue::Float))
                    .unwrap_or(DbValue::String(s.clone()))
            }
        }
        sqlparser::ast::Value::SingleQuotedString(s) | sqlparser::ast::Value::DoubleQuotedString(s) => {
            DbValue::String(s.clone())
        }
        _ => DbValue::String(format!("{:?}", v)),
    }
}

/// Compare two DbValues for equality (SQL NULL semantics).
pub(crate) fn values_equal(a: &DbValue, b: &DbValue) -> bool {
    // NULL != anything (including NULL), per SQL standard
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return false;
    }
    match (a, b) {
        (DbValue::Null, DbValue::Null) => true,
        (DbValue::Int(x), DbValue::Int(y)) => x == y,
        (DbValue::Float(x), DbValue::Float(y)) => (x - y).abs() < f64::EPSILON,
        (DbValue::Int(x), DbValue::Float(y)) => (*x as f64 - y).abs() < f64::EPSILON,
        (DbValue::Float(x), DbValue::Int(y)) => (x - *y as f64).abs() < f64::EPSILON,
        (DbValue::Bool(x), DbValue::Bool(y)) => x == y,
        (DbValue::String(x), DbValue::String(y)) => x == y,
        _ => value_to_string(a) == value_to_string(b),
    }
}

// ── Numeric helpers ─────────────────────────────────────────────────────

/// Convert a DbValue to f64 if it's numeric.
fn to_f64(v: &DbValue) -> Option<f64> {
    match v {
        DbValue::Int(n) => Some(*n as f64),
        DbValue::Float(f) => Some(*f),
        _ => None,
    }
}

// ── Date/time helpers ──────────────────────────────────────────────────

/// Evaluate a SQLite-style timeval + modifiers into a "YYYY-MM-DD HH:MM:SS"
/// string. The first arg must be 'now' (a fixed timeval is not supported);
/// remaining args are modifiers like '+1 day'. Validates all modifiers
/// BEFORE reading the clock (miri's isolation blocks SystemTime).
fn datetime_from_args(name: &str, args: &[FunctionArg]) -> Result<String, EngineError> {
    let mods: Vec<String> = args
        .iter()
        .map(|a| match a {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(v))) => match &v.value {
                Value::SingleQuotedString(s)
                | Value::DoubleQuotedString(s)
                | Value::TripleSingleQuotedString(s)
                | Value::TripleDoubleQuotedString(s) => s.to_lowercase(),
                _ => String::new(),
            },
            _ => String::new(),
        })
        .collect();
    if mods.is_empty() {
        return Err(EngineError::Exec(format!("{name}() requires at least 'now'")));
    }
    if mods[0] != "now" {
        return Err(EngineError::Exec(format!("{name}() base must be 'now'")));
    }
    let mut delta_secs: i64 = 0;
    for m in &mods[1..] {
        match parse_sqlite_date_modifier(m) {
            Some(d) => delta_secs += d,
            None => return Err(EngineError::Exec(format!("Unsupported {name}() modifier '{}'", m))),
        }
    }
    // ponytail: localtime modifier accepted but UTC returned — real offset
    // needs the time crate 'local-offset' feature
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
        + delta_secs;
    Ok(epoch_to_datetime(secs))
}

/// Format unix seconds as "YYYY-MM-DD HH:MM:SS" (Howard Hinnant civil-from-days).
fn epoch_to_datetime(secs: i64) -> String {
    let z = secs / 86400 + 719468;
    let era = (z / 146097) as u64;
    let doe = (z - era as i64 * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, min, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{s:02}")
}

/// Parse a SQLite `datetime()` modifier into a signed seconds delta.
/// Supports `+N days/hours/minutes/seconds`, `N months`, `N years`, and the
/// no-op markers `localtime`/`utc`. Unknown modifiers return None.
///
/// ponytail: month/year deltas are approximated as 30/365 days — exact
/// calendar math needs a real date library; add when a caller depends on
/// Jan-31 + 1 month semantics.
fn parse_sqlite_date_modifier(m: &str) -> Option<i64> {
    let m = m.trim();
    if m == "localtime" || m == "utc" {
        return Some(0);
    }
    let (num_str, unit) = if let Some(stripped) = m.strip_prefix('+') {
        // '+1 day' — optional + sign
        let mut it = stripped.splitn(2, char::is_whitespace);
        (it.next()?, it.next()?.trim())
    } else if let Some(stripped) = m.strip_prefix('-') {
        let mut it = stripped.splitn(2, char::is_whitespace);
        let n: i64 = it.next()?.parse().ok()?;
        let unit = it.next()?.trim();
        return match unit {
            "seconds" | "second" => Some(-n),
            "minutes" | "minute" => Some(-n * 60),
            "hours" | "hour" => Some(-n * 3600),
            "days" | "day" => Some(-n * 86400),
            "weeks" | "week" => Some(-n * 7 * 86400),
            "months" | "month" => Some(-n * 30 * 86400),
            "years" | "year" => Some(-n * 365 * 86400),
            _ => None,
        };
    } else {
        let mut it = m.splitn(2, char::is_whitespace);
        (it.next()?, it.next()?.trim())
    };
    let n: i64 = num_str.parse().ok()?;
    match unit {
        "seconds" | "second" => Some(n),
        "minutes" | "minute" => Some(n * 60),
        "hours" | "hour" => Some(n * 3600),
        "days" | "day" => Some(n * 86400),
        "weeks" | "week" => Some(n * 7 * 86400),
        "months" | "month" => Some(n * 30 * 86400),
        "years" | "year" => Some(n * 365 * 86400),
        _ => None,
    }
}

/// Return current date as YYYY-MM-DD string.
pub(crate) fn curdate_value() -> DbValue {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let z = secs / 86400 + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    DbValue::String(format!("{y:04}-{m:02}-{d:02}"))
}

/// Return the current timestamp as a DbValue (ISO 8601 format).
pub(crate) fn now_value() -> DbValue {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let z = secs / 86400 + 719468;
    let era = (z as i64 / 146097) as u64;
    let doe = (z as i64 - era as i64 * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, min, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    DbValue::String(format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}"))
}

/// Parse an ISO 8601 date/datetime string into (year, month, day, hour, min, sec).
fn parse_iso_date(s: &str) -> Option<(i64, i64, i64, i64, i64, i64)> {
    let s = s.trim();
    let (date_part, time_part) = if let Some(pos) = s.find('T').or_else(|| s.find(' ')) {
        (&s[..pos], Some(&s[pos + 1..]))
    } else {
        (s, None)
    };
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year = parts[0].parse::<i64>().ok()?;
    let month = parts[1].parse::<i64>().ok()?;
    let day = parts[2].parse::<i64>().ok()?;
    let (hour, min, sec) = match time_part {
        Some(tp) => {
            let t: Vec<&str> = tp.split(':').collect();
            let h = t.first().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            let m = t.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
            let s = t
                .get(2)
                .and_then(|s| {
                    let clean = s.split('.').next().unwrap_or(s);
                    clean.parse::<i64>().ok()
                })
                .unwrap_or(0);
            (h, m, s)
        }
        None => (0, 0, 0),
    };
    Some((year, month, day, hour, min, sec))
}

/// Compute days since epoch (1970-01-01) from date parts.
fn date_to_days(y: i64, m: i64, d: i64) -> i64 {
    let (adj_m, adj_y) = if m <= 2 { (m + 9, y - 1) } else { (m - 3, y) };
    let era = if adj_y >= 0 { adj_y / 400 } else { (adj_y - 399) / 400 };
    let yoe = adj_y - era * 400;
    let doy = (153 * adj_m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// ── LIKE pattern matching ──────────────────────────────────────────────

pub(crate) fn simple_like(value: &str, pattern: &str) -> bool {
    let val_chars: Vec<char> = value.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();
    like_match(&val_chars, &pat_chars, 0, 0)
}

fn like_match(val: &[char], pat: &[char], vi: usize, pi: usize) -> bool {
    if pi == pat.len() {
        return vi == val.len();
    }
    match pat[pi] {
        '%' => {
            let mut vi2 = vi;
            while vi2 <= val.len() {
                if like_match(val, pat, vi2, pi + 1) {
                    return true;
                }
                vi2 += 1;
            }
            false
        }
        '_' => vi < val.len() && like_match(val, pat, vi + 1, pi + 1),
        c => vi < val.len() && val[vi] == c && like_match(val, pat, vi + 1, pi + 1),
    }
}

// ── Argument extraction ────────────────────────────────────────────────

/// Get function argument as Expr, assuming Unnamed(FunctionArgExpr::Expr(e))
pub(crate) fn get_func_arg_unnamed(arg: &FunctionArg) -> Result<&Expr, EngineError> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => Ok(e),
        FunctionArg::Unnamed(_) => Err(EngineError::Exec("Expected expression argument".into())),
        FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } => match arg {
            FunctionArgExpr::Expr(e) => Ok(e),
            _ => Err(EngineError::Exec("Expected expression in named argument".into())),
        },
    }
}

/// Extract the first argument expression from a function.
pub(crate) fn extract_func_arg(func: &Function) -> Result<&Expr, EngineError> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err(EngineError::Exec("Function requires argument list".into())),
    };
    if args.is_empty() {
        return Err(EngineError::Exec("Function requires argument".into()));
    }
    get_func_arg_unnamed(&args[0])
}

/// Extract function arguments as Vec<DbValue> for plugin dispatch.
pub(crate) fn extract_func_args(func: &Function) -> Vec<DbValue> {
    let mut args = Vec::new();
    if let sqlparser::ast::FunctionArguments::List(list) = &func.args {
        for arg in &list.args {
            use sqlparser::ast::FunctionArg::*;
            use sqlparser::ast::FunctionArgExpr;
            match arg {
                Unnamed(FunctionArgExpr::Expr(expr))
                | Named {
                    arg: FunctionArgExpr::Expr(expr),
                    ..
                } => {
                    // ponytail: no eval context, pass raw SQL repr
                    args.push(DbValue::String(format!("{:?}", expr)));
                }
                Unnamed(FunctionArgExpr::Wildcard) => {
                    args.push(DbValue::String("*".into()));
                }
                _ => {}
            }
        }
    }
    args
}

// ── Standard SQL scalar functions ──────────────────────────────────────

/// Evaluate a standard SQL scalar function.
pub(crate) fn exec_std_function(
    func: &Function,
    name: String,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, EngineError> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Ok(now_value()), // e.g. CURRENT_TIMESTAMP without parens
    };
    let eval_args = |count: usize| -> Result<Vec<DbValue>, EngineError> {
        if args.len() < count {
            return Err(EngineError::Exec(format!("'{}' requires {} argument(s)", name, count)));
        }
        args.iter()
            .take(count)
            .map(|a| eval_expr(get_func_arg_unnamed(a)?, row, col_map))
            .collect()
    };

    match name.as_str() {
        "upper" | "ucase" => {
            let vals = eval_args(1)?;
            let s = value_to_string(&vals[0]);
            Ok(DbValue::String(s.to_uppercase()))
        }
        "lower" | "lcase" => {
            let vals = eval_args(1)?;
            let s = value_to_string(&vals[0]);
            Ok(DbValue::String(s.to_lowercase()))
        }
        "length" | "len" => {
            let vals = eval_args(1)?;
            let s = value_to_string(&vals[0]);
            Ok(DbValue::Int(s.len() as i64))
        }
        "substr" | "substring" => {
            let vals = eval_args(3).or_else(|_| eval_args(2))?;
            let s = value_to_string(&vals[0]);
            let start = match vals[1] {
                DbValue::Int(i) => i.max(1) as usize - 1, // SQL is 1-indexed
                _ => {
                    return Err(EngineError::TypeError {
                        expected: "integer".into(),
                        actual: format!("{:?}", vals[1]),
                    });
                }
            };
            if vals.len() >= 3 {
                let length = match vals[2] {
                    DbValue::Int(i) => i as usize,
                    _ => {
                        return Err(EngineError::TypeError {
                            expected: "integer".into(),
                            actual: format!("{:?}", vals[2]),
                        });
                    }
                };
                Ok(DbValue::String(s.chars().skip(start).take(length).collect()))
            } else {
                Ok(DbValue::String(s.chars().skip(start).collect()))
            }
        }
        "trim" => {
            let vals = eval_args(1)?;
            let s = value_to_string(&vals[0]);
            Ok(DbValue::String(s.trim().to_string()))
        }
        "coalesce" | "ifnull" => {
            for a in args {
                let v = eval_expr(get_func_arg_unnamed(a)?, row, col_map)?;
                if v != DbValue::Null {
                    return Ok(v);
                }
            }
            Ok(DbValue::Null)
        }
        "round" => {
            let vals = eval_args(2).or_else(|_| eval_args(1))?;
            let num = match vals[0] {
                DbValue::Float(f) => f,
                DbValue::Int(i) => i as f64,
                _ => {
                    return Err(EngineError::TypeError {
                        expected: "numeric".into(),
                        actual: format!("{:?}", vals[0]),
                    });
                }
            };
            let decimals = if vals.len() >= 2 {
                match vals[1] {
                    DbValue::Int(i) => i as u32,
                    _ => {
                        return Err(EngineError::TypeError {
                            expected: "integer".into(),
                            actual: format!("{:?}", vals[1]),
                        });
                    }
                }
            } else {
                0
            };
            let multiplier = 10_f64.powi(decimals as i32);
            Ok(DbValue::Float((num * multiplier).round() / multiplier))
        }
        "abs" => {
            let v = eval_args(1)?.swap_remove(0);
            match v {
                DbValue::Int(i) => Ok(DbValue::Int(i.abs())),
                DbValue::Float(f) => Ok(DbValue::Float(f.abs())),
                _ => Err(EngineError::TypeError {
                    expected: "numeric".into(),
                    actual: format!("{:?}", v),
                }),
            }
        }
        "now" | "current_timestamp" => Ok(now_value()),
        "curdate" | "current_date" => Ok(curdate_value()),
        // SQLite-compatible datetime('now') / datetime('now','localtime') with
        // date arithmetic modifiers: '+1 day', '-30 days', '+3 hours', etc.
        "datetime" => Ok(DbValue::String(datetime_from_args("datetime", args)?)),
        // SQLite date('now', modifiers) — YYYY-MM-DD
        "date" => {
            let dt = datetime_from_args(&name, args)?;
            Ok(DbValue::String(dt.chars().take(10).collect::<String>()))
        }
        // SQLite time('now', modifiers) — HH:MM:SS
        "time" => {
            let dt = datetime_from_args(&name, args)?;
            Ok(DbValue::String(dt.chars().skip(11).take(8).collect::<String>()))
        }
        // SQLite strftime(format, timeval, modifiers) — subset: %Y %m %d %H %M %S %j %w
        "strftime" => {
            let fmt = match args.first() {
                Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(v)))) => {
                    if let Value::SingleQuotedString(s) | Value::DoubleQuotedString(s) = &v.value {
                        s.clone()
                    } else {
                        return Err(EngineError::Exec("strftime format must be a string".into()));
                    }
                }
                _ => return Err(EngineError::Exec("strftime format must be a string".into())),
            };
            // Skip the format arg; the rest are timeval + modifiers
            let rest = args.iter().skip(1).cloned().collect::<Vec<_>>();
            let dt = datetime_from_args(&name, &rest)?;
            // dt is "YYYY-MM-DD HH:MM:SS"
            let y = &dt[0..4];
            let m = &dt[5..7];
            let d = &dt[8..10];
            let h = &dt[11..13];
            let mi = &dt[14..16];
            let s = &dt[17..19];
            let mut out = String::new();
            let mut chars = fmt.chars().peekable();
            while let Some(c) = chars.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                match chars.next() {
                    Some('Y') => out.push_str(y),
                    Some('m') => out.push_str(m),
                    Some('d') => out.push_str(d),
                    Some('H') => out.push_str(h),
                    Some('M') => out.push_str(mi),
                    Some('S') => out.push_str(s),
                    Some('%') => out.push('%'),
                    Some(other) => {
                        out.push('%');
                        out.push(other);
                    }
                    None => out.push('%'),
                }
            }
            Ok(DbValue::String(out))
        }
        "concat" => {
            let mut result = String::new();
            for a in args {
                let v = eval_expr(get_func_arg_unnamed(a)?, row, col_map)?;
                result.push_str(&value_to_string(&v));
            }
            Ok(DbValue::String(result))
        }
        "last_insert_rowid" => {
            let rowid = LAST_INSERT_ROWID.with(|r| r.borrow().clone());
            Ok(DbValue::String(rowid.unwrap_or_else(|| "0".to_string())))
        }
        "changes" => {
            let n = LAST_CHANGES.with(|c| *c.borrow());
            Ok(DbValue::Int(n as i64))
        }
        "unix_timestamp" => {
            if args.is_empty() {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                Ok(DbValue::Int(secs as i64))
            } else {
                let vals = eval_args(1)?;
                let s = value_to_string(&vals[0]);
                if let Some((y, m, d, h, mi, sec)) = parse_iso_date(&s) {
                    let days = date_to_days(y, m, d);
                    Ok(DbValue::Int(days * 86400 + h * 3600 + mi * 60 + sec))
                } else {
                    Err(EngineError::Parse(format!("Cannot parse date: '{}'", s)))
                }
            }
        }
        "date_format" => {
            let vals = eval_args(2)?;
            let s = value_to_string(&vals[0]);
            let fmt = value_to_string(&vals[1]);
            let (y, m, d, h, mi, sec) =
                parse_iso_date(&s).ok_or_else(|| EngineError::Parse(format!("Cannot parse date: '{}'", s)))?;
            let mut result = String::new();
            let mut chars = fmt.chars();
            while let Some(c) = chars.next() {
                if c == '%' {
                    match chars.next() {
                        Some('Y') => result.push_str(&format!("{:04}", y)),
                        Some('m') => result.push_str(&format!("{:02}", m)),
                        Some('d') => result.push_str(&format!("{:02}", d)),
                        Some('H') => result.push_str(&format!("{:02}", h)),
                        Some('M') => result.push_str(&format!("{:02}", mi)),
                        Some('S') => result.push_str(&format!("{:02}", sec)),
                        Some(o) => result.push(o),
                        None => result.push('%'),
                    }
                } else {
                    result.push(c);
                }
            }
            Ok(DbValue::String(result))
        }
        "datediff" => {
            let vals = eval_args(2)?;
            let s1 = value_to_string(&vals[0]);
            let s2 = value_to_string(&vals[1]);
            let (y1, m1, d1, _, _, _) =
                parse_iso_date(&s1).ok_or_else(|| EngineError::Parse(format!("Cannot parse date: '{}'", s1)))?;
            let (y2, m2, d2, _, _, _) =
                parse_iso_date(&s2).ok_or_else(|| EngineError::Parse(format!("Cannot parse date: '{}'", s2)))?;
            let days1 = date_to_days(y1, m1, d1);
            let days2 = date_to_days(y2, m2, d2);
            Ok(DbValue::Int(days1 - days2))
        }
        // RAISE(ABORT, 'msg') — SQLite trigger abort. Returns an error message.
        "raise" => {
            // RAISE(ABORT, 'msg') — sqlparser passes ABORT as a keyword, not an arg.
            // Try 2 args, fall back to 1, fall back to 0 (the flag is what matters).
            let vals = eval_args(2).or_else(|_| eval_args(1)).unwrap_or_default();
            let msg = vals.last().map(value_to_string).unwrap_or_default();
            RAISE_ABORTED.with(|r| r.set(true));
            Err(EngineError::Exec(format!("RAISE: {}", msg)))
        }
        // SQF_EVAL(expr) — evaluate an SQF expression
        "sqf_eval" => {
            let vals = eval_args(1)?;
            let expr_str = value_to_string(&vals[0]);
            match crate::engine::sqf::eval_sqf(&expr_str, &HashMap::new()) {
                Ok(v) => Ok(v),
                Err(e) => Err(EngineError::Exec(format!("SQF_EVAL error: {}", e))),
            }
        }
        // POW(base, exp) — exponentiation
        "pow" | "power" => {
            let vals = eval_args(2)?;
            let base = to_f64(&vals[0]);
            let exp = to_f64(&vals[1]);
            match (base, exp) {
                (Some(b), Some(e)) => Ok(DbValue::Float(b.powf(e))),
                (None, _) => Err(EngineError::TypeError {
                    expected: "numeric".into(),
                    actual: format!("{:?}", vals[0]),
                }),
                _ => Err(EngineError::TypeError {
                    expected: "numeric".into(),
                    actual: format!("{:?}", vals[1]),
                }),
            }
        }
        // SQRT(x) — square root
        "sqrt" => {
            let vals = eval_args(1)?;
            let x = to_f64(&vals[0]).ok_or_else(|| EngineError::TypeError {
                expected: "numeric".into(),
                actual: format!("{:?}", vals[0]),
            })?;
            if x < 0.0 {
                return Err(EngineError::Exec("SQRT: negative argument".into()));
            }
            Ok(DbValue::Float(x.sqrt()))
        }
        // CEIL(x) — ceiling
        "ceil" | "ceiling" => {
            let vals = eval_args(1)?;
            let x = to_f64(&vals[0]).ok_or_else(|| EngineError::TypeError {
                expected: "numeric".into(),
                actual: format!("{:?}", vals[0]),
            })?;
            Ok(DbValue::Float(x.ceil()))
        }
        // FLOOR(x) — floor
        "floor" => {
            let vals = eval_args(1)?;
            let x = to_f64(&vals[0]).ok_or_else(|| EngineError::TypeError {
                expected: "numeric".into(),
                actual: format!("{:?}", vals[0]),
            })?;
            Ok(DbValue::Float(x.floor()))
        }
        // SIGN(x) — signum (-1, 0, 1)
        "sign" => {
            let vals = eval_args(1)?;
            let x = to_f64(&vals[0]).ok_or_else(|| EngineError::TypeError {
                expected: "numeric".into(),
                actual: format!("{:?}", vals[0]),
            })?;
            Ok(DbValue::Int(if x > 0.0 {
                1
            } else if x < 0.0 {
                -1
            } else {
                0
            }))
        }
        // REPLACE(s, from, to) — string replace
        "replace" => {
            let vals = eval_args(3).or_else(|_| eval_args(2))?;
            let s = value_to_string(&vals[0]);
            let from = value_to_string(&vals[1]);
            let to = if vals.len() >= 3 {
                value_to_string(&vals[2])
            } else {
                String::new()
            };
            Ok(DbValue::String(s.replace(&from, &to)))
        }
        // ── Common SQLite functions ─────────────────────────────────
        "instr" => {
            let vals = eval_args(2)?;
            let hay = value_to_string(&vals[0]);
            let needle = value_to_string(&vals[1]);
            // SQLite instr is 1-based; 0 when not found
            match hay.find(&needle) {
                Some(idx) => Ok(DbValue::Int(idx as i64 + 1)),
                None => Ok(DbValue::Int(0)),
            }
        }
        "ltrim" => {
            let vals = eval_args(2).or_else(|_| eval_args(1))?;
            let s = value_to_string(&vals[0]);
            let chars = if vals.len() >= 2 {
                value_to_string(&vals[1])
            } else {
                " ".into()
            };
            Ok(DbValue::String(s.trim_start_matches(|c| chars.contains(c)).to_string()))
        }
        "rtrim" => {
            let vals = eval_args(2).or_else(|_| eval_args(1))?;
            let s = value_to_string(&vals[0]);
            let chars = if vals.len() >= 2 {
                value_to_string(&vals[1])
            } else {
                " ".into()
            };
            Ok(DbValue::String(s.trim_end_matches(|c| chars.contains(c)).to_string()))
        }
        "typeof" => {
            let vals = eval_args(1)?;
            let tp = match &vals[0] {
                DbValue::Null => "null",
                DbValue::Bool(_) => "integer",
                DbValue::Int(_) => "integer",
                DbValue::Float(_) => "real",
                DbValue::String(_) => "text",
                DbValue::Strings(_) => "array",
                DbValue::Floats(_) => "array",
            };
            Ok(DbValue::String(tp.into()))
        }
        "random" => {
            // SQLite random() returns a 64-bit signed int
            let r: i64 = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0))
                ^ std::process::id() as i64;
            Ok(DbValue::Int(r))
        }
        "char" => {
            // SQLite char(N...) — build a string from code points
            let mut out = String::new();
            for i in 0..args.len() {
                let v = eval_args(i + 1)?;
                if let Some(n) = to_f64(&v[i])
                    && let Some(c) = char::from_u32(n as u32)
                {
                    out.push(c);
                }
            }
            Ok(DbValue::String(out))
        }
        _ => Err(EngineError::Exec(format!("Unknown function '{}'", name))),
    }
}

// ── Table resolution ───────────────────────────────────────────────────

/// Materialize a view by executing its SQL and inserting the results as a temp table.
pub(crate) fn materialize_view(name: &str, db: &mut Database) -> Result<(), EngineError> {
    let sql = db
        .get_view(name)
        .ok_or_else(|| EngineError::ViewNotFound(name.into()))?
        .clone();

    let stmts = crate::parser::parse_sql(&sql).map_err(|e| EngineError::Parse(e.to_string()))?;
    let stmt = stmts
        .into_iter()
        .next()
        .ok_or(EngineError::Exec("View definition is empty".into()))?;

    let result = execute(&stmt, db)?;

    let rows: Vec<Vec<serde_json::Value>> =
        serde_json::from_str(&result).map_err(|e| EngineError::Exec(format!("View result parse: {}", e)))?;

    // The SELECT always returns at least the header row — materialize the
    // table even with zero data rows (an empty view must still resolve, or
    // SELECT on it reports "Table does not exist"). Column types default to
    // String when there is no data row to infer from.
    if !rows.is_empty() {
        let header = &rows[0];
        let cols: Vec<Column> = header
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let dtype = rows
                    .get(1)
                    .and_then(|r| r.get(i))
                    .map(json_type_to_column)
                    .unwrap_or(ColumnType::String);
                Column {
                    name: h.as_str().unwrap_or("col").to_lowercase(),
                    dtype,
                    primary_key: false,
                    not_null: false,
                    default: None,
                    default_expr: None,
                    auto_increment: false,
                    unique: false,
                }
            })
            .collect();

        if let Ok(mut table) = Table::new(name.to_string(), cols) {
            for row_data in &rows[1..] {
                let db_row: Vec<DbValue> = row_data.iter().map(json_val_to_dbvalue).collect();
                let _ = table.insert(db_row);
            }
            db.add_table(name.to_string(), table);
        }
    }

    Ok(())
}

/// Infer ColumnType from a JSON value (shared with CTE code).
fn json_type_to_column(v: &serde_json::Value) -> ColumnType {
    match v {
        serde_json::Value::Null => ColumnType::String,
        serde_json::Value::Bool(_) => ColumnType::Bool,
        serde_json::Value::Number(n) => {
            if n.is_f64() {
                ColumnType::Float
            } else {
                ColumnType::Int
            }
        }
        serde_json::Value::String(_) => ColumnType::String,
        _ => ColumnType::String,
    }
}

/// Resolve a table factor, materialising a view if the name is not a real table.
pub(crate) fn resolve_table_factor(
    factor: &TableFactor,
    db: &mut Database,
) -> Result<(String, crate::engine::table::Table), EngineError> {
    match factor {
        TableFactor::Table { name, .. } => {
            let tname = object_name_str(name);
            if !db.has_table(&tname) && db.has_view(&tname) {
                materialize_view(&tname, db)?;
            }
            let table = db.get_table(&tname).map_err(EngineError::Exec)?.clone();
            Ok((tname, table))
        }
        _ => Err(EngineError::Exec(
            "Only simple table references supported in FROM".into(),
        )),
    }
}

pub(crate) fn resolve_single_table<'a>(from: &[TableWithJoins], db: &'a Database) -> Result<&'a Table, EngineError> {
    let tf = from.first().ok_or(EngineError::Exec("No FROM clause".into()))?;
    match &tf.relation {
        TableFactor::Table { name, .. } => db.get_table(&object_name_str(name)).map_err(EngineError::Exec),
        _ => Err(EngineError::Exec(
            "Only simple table references supported in FROM".into(),
        )),
    }
}

// ── Index-assisted lookup ─────────────────────────────────────────────

/// Try to use a BTreeIndex for a simple `col = literal` WHERE clause.
/// Returns `Some(rows)` if an index was used, `None` to fall back to full scan.
/// O(1) PK lookup: `WHERE pk_col = literal` via pk_row_index.
/// Returns Some(row) for exactly the matching row, Some(empty) when the key
/// is absent, None to fall back to scan-based resolution.
pub(crate) fn try_pk_index<'a>(where_expr: Option<&Expr>, table: &'a Table) -> Option<Vec<&'a [DbValue]>> {
    let expr = where_expr?;
    let (col_name, value) = match expr {
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Eq,
            right,
        } => match (left.as_ref(), right.as_ref()) {
            (Expr::Identifier(ident), Expr::Value(v)) | (Expr::Value(v), Expr::Identifier(ident)) => {
                (ident.value.to_lowercase(), sql_val_to_db(&v.value))
            }
            _ => return None,
        },
        _ => return None,
    };
    let ci = table.col_index.get(&col_name)?;
    // Only single-column PKs can use the O(1) path. A composite PK matched on
    // one column produces a partial key (other cols NULL) that never hits —
    // returning Some(empty) would wrongly hide real rows, so fall back to scan.
    let pk_count = table.columns.iter().filter(|c| c.primary_key).count();
    if pk_count != 1 || !table.columns[*ci].primary_key {
        return None;
    }
    // Build the pk_key for this value (mirrors pk_key() formatting)
    let mut key_row: Vec<DbValue> = (0..table.columns.len()).map(|_| DbValue::Null).collect();
    key_row[*ci] = value;
    let key = table.pk_key(&key_row)?;
    match table.pk_row_index.get(&key) {
        Some(&idx) => Some(vec![table.rows[idx].as_slice()]),
        None => Some(Vec::new()),
    }
}

pub(crate) fn try_btree_index<'a>(where_expr: Option<&Expr>, table: &'a Table) -> Option<Vec<&'a [DbValue]>> {
    let expr = where_expr?;
    let (col_name, value) = match expr {
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Eq,
            right,
        } => match (left.as_ref(), right.as_ref()) {
            (Expr::Identifier(ident), Expr::Value(v)) | (Expr::Value(v), Expr::Identifier(ident)) => {
                (ident.value.to_lowercase(), sql_val_to_db(&v.value))
            }
            _ => return None,
        },
        _ => return None,
    };
    let indices = table.btree_lookup(&col_name, &value)?;
    Some(indices.into_iter().map(|i| table.rows[i].as_slice()).collect())
}

/// Try to use a TrigramIndex for a `fuzzy_match(col, pattern)` WHERE clause.
/// Returns `Some(candidate_rows)` if a trigram index exists, `None` to fall back to full scan.
pub(crate) fn try_trigram_index<'a>(where_expr: Option<&Expr>, table: &'a Table) -> Option<Vec<&'a [DbValue]>> {
    let expr = where_expr?;
    let (col_name, pattern_val) = match expr {
        Expr::Function(f) if f.name.to_string().to_lowercase() == "fuzzy_match" => {
            let args = match &f.args {
                FunctionArguments::List(list) => &list.args,
                _ => return None,
            };
            if args.len() < 2 {
                return None;
            }
            let col_expr = get_func_arg_unnamed(&args[0]).ok()?;
            let pat_expr = get_func_arg_unnamed(&args[1]).ok()?;
            let col_name = match col_expr {
                Expr::Identifier(ident) => ident.value.to_lowercase(),
                Expr::CompoundIdentifier(parts) => parts
                    .iter()
                    .map(|p| p.value.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("."),
                _ => return None,
            };
            let pattern_val = match pat_expr {
                Expr::Value(v) => match &v.value {
                    sqlparser::ast::Value::SingleQuotedString(s) => s.clone(),
                    sqlparser::ast::Value::DoubleQuotedString(s) => s.clone(),
                    sqlparser::ast::Value::Null => return None,
                    _ => return None,
                },
                _ => return None,
            };
            (col_name, pattern_val)
        }
        _ => return None,
    };

    // Check for trigram index on this column
    let trigram = table.find_index(&col_name, A3IndexType::Trigram)?;
    let IndexImpl::Trigram(idx) = trigram else {
        unreachable!()
    };

    let candidates = idx.candidates(&pattern_val);
    if candidates.is_empty() || candidates.len() >= table.rows.len() {
        return None; // not worth it, full scan is similar cost
    }

    Some(candidates.into_iter().map(|i| table.rows[i].as_slice()).collect())
}
