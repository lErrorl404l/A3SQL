// Standard SQL function implementations — the big function dispatch and all
// built-in scalar functions that don't need the expression evaluator.

use std::collections::HashMap;

use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, TableFactor, TableWithJoins,
};

use super::super::database::Database;
use super::super::execute::{execute, json_val_to_dbvalue, object_name_str, LAST_CHANGES, LAST_INSERT_ROWID};
use super::super::index::IndexType as A3IndexType;
use super::super::table::{IndexImpl, Table};
use super::super::value::{Column, ColumnType, DbValue};

use super::eval::eval_expr;

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
            if s.contains('.') {
                s.parse::<f64>()
                    .map(DbValue::Float)
                    .unwrap_or(DbValue::String(s.clone()))
            } else {
                s.parse::<i64>().map(DbValue::Int).unwrap_or(DbValue::String(s.clone()))
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

// ── Date/time helpers ──────────────────────────────────────────────────

/// Return current date as YYYY-MM-DD string.
pub(crate) fn curdate_value() -> DbValue {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let z = secs / 86400 + 719468;
    let era = z as i64 / 146097;
    let doe = z as i64 - era * 146097;
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
pub(crate) fn get_func_arg_unnamed(arg: &FunctionArg) -> Result<&Expr, String> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => Ok(e),
        FunctionArg::Unnamed(_) => Err("Expected expression argument".into()),
        FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } => match arg {
            FunctionArgExpr::Expr(e) => Ok(e),
            _ => Err("Expected expression in named argument".into()),
        },
    }
}

/// Extract the first argument expression from a function.
pub(crate) fn extract_func_arg(func: &Function) -> Result<&Expr, String> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err("Function requires argument list".into()),
    };
    if args.is_empty() {
        return Err("Function requires argument".into());
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
) -> Result<DbValue, String> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Ok(now_value()), // e.g. CURRENT_TIMESTAMP without parens
    };
    let eval_args = |count: usize| -> Result<Vec<DbValue>, String> {
        if args.len() < count {
            return Err(format!("'{}' requires {} argument(s)", name, count));
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
                _ => return Err("SUBSTR start must be integer".into()),
            };
            if vals.len() >= 3 {
                let length = match vals[2] {
                    DbValue::Int(i) => i as usize,
                    _ => return Err("SUBSTR length must be integer".into()),
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
                _ => return Err("ROUND requires numeric argument".into()),
            };
            let decimals = if vals.len() >= 2 {
                match vals[1] {
                    DbValue::Int(i) => i as u32,
                    _ => return Err("ROUND decimals must be integer".into()),
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
                _ => Err("ABS requires numeric argument".into()),
            }
        }
        "now" | "current_timestamp" => Ok(now_value()),
        "curdate" | "current_date" => Ok(curdate_value()),
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
                    Err(format!("Cannot parse date: '{}'", s))
                }
            }
        }
        "date_format" => {
            let vals = eval_args(2)?;
            let s = value_to_string(&vals[0]);
            let fmt = value_to_string(&vals[1]);
            let (y, m, d, h, mi, sec) = parse_iso_date(&s).ok_or_else(|| format!("Cannot parse date: '{}'", s))?;
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
            let (y1, m1, d1, _, _, _) = parse_iso_date(&s1).ok_or_else(|| format!("Cannot parse date: '{}'", s1))?;
            let (y2, m2, d2, _, _, _) = parse_iso_date(&s2).ok_or_else(|| format!("Cannot parse date: '{}'", s2))?;
            let days1 = date_to_days(y1, m1, d1);
            let days2 = date_to_days(y2, m2, d2);
            Ok(DbValue::Int(days1 - days2))
        }
        _ => Err(format!("Unknown function '{}'", name)),
    }
}

// ── Table resolution ───────────────────────────────────────────────────

/// Materialize a view by executing its SQL and inserting the results as a temp table.
pub(crate) fn materialize_view(name: &str, db: &mut Database) -> Result<(), String> {
    let sql = db
        .get_view(name)
        .ok_or_else(|| format!("View '{}' not found", name))?
        .clone();

    let stmts = crate::parser::parse_sql(&sql).map_err(|e| format!("{}", e))?;
    let stmt = stmts.into_iter().next().ok_or("View definition is empty")?;

    let result = execute(&stmt, db)?;

    let rows: Vec<Vec<serde_json::Value>> =
        serde_json::from_str(&result).map_err(|e| format!("View result parse: {}", e))?;

    if rows.len() >= 2 {
        let header = &rows[0];
        let cols: Vec<Column> = header
            .iter()
            .map(|h| Column {
                name: h.as_str().unwrap_or("col").to_lowercase(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
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

/// Resolve a table factor, materialising a view if the name is not a real table.
pub(crate) fn resolve_table_factor(
    factor: &TableFactor,
    db: &mut Database,
) -> Result<(String, crate::engine::table::Table), String> {
    match factor {
        TableFactor::Table { name, .. } => {
            let tname = object_name_str(name);
            if !db.has_table(&tname) && db.has_view(&tname) {
                materialize_view(&tname, db)?;
            }
            let table = db.get_table(&tname)?.clone();
            Ok((tname, table))
        }
        _ => Err("Only simple table references supported in FROM".into()),
    }
}

pub(crate) fn resolve_single_table<'a>(from: &[TableWithJoins], db: &'a Database) -> Result<&'a Table, String> {
    let tf = from.first().ok_or("No FROM clause")?;
    match &tf.relation {
        TableFactor::Table { name, .. } => db.get_table(&object_name_str(name)),
        _ => Err("Only simple table references supported in FROM".into()),
    }
}

// ── Index-assisted lookup ─────────────────────────────────────────────

/// Try to use a BTreeIndex for a simple `col = literal` WHERE clause.
/// Returns `Some(rows)` if an index was used, `None` to fall back to full scan.
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
    let IndexImpl::Trigram(ref idx) = trigram else {
        unreachable!()
    };

    let candidates = idx.candidates(&pattern_val);
    if candidates.is_empty() || candidates.len() >= table.rows.len() {
        return None; // not worth it, full scan is similar cost
    }

    Some(candidates.into_iter().map(|i| table.rows[i].as_slice()).collect())
}
