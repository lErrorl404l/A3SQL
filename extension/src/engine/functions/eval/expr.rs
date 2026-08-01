// Expression evaluation — main eval_expr, eval_literal_expr, exec_function, fuzzy/FTS helpers

use std::collections::HashMap;

use sqlparser::ast::{Expr, Function, FunctionArguments, TrimWhereField};

use super::super::super::execute::select::exec_subquery;
use super::super::super::table::Table;
use super::super::super::value::db_value_cmp;
use super::super::super::value::DbValue;
use super::super::builtin::{
    curdate_value, exec_std_function, extract_func_args, get_func_arg_unnamed, now_value, simple_like, sql_val_to_db,
    value_to_string,
};
use super::cast::cast_db_value;
use super::corr::rewrite_if_correlated;
use super::ops::{apply_binary_op, apply_unary_op, is_truthy};
use crate::engine::error::EngineError;

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
            let idx = col_map.get(&name).ok_or(EngineError::ColumnNotFound(name))?;
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
                    let last = parts
                        .last()
                        .ok_or_else(|| EngineError::ColumnNotFound(name.clone()))?
                        .value
                        .to_lowercase();
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
        Expr::Subquery(query) => {
            let subq_q = rewrite_if_correlated(query, row, col_map);
            let vals = exec_subquery(&subq_q)?;
            Ok(vals.into_iter().next().unwrap_or(DbValue::Null))
        }
        Expr::InSubquery {
            expr,
            subquery,
            negated,
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let subq_q = rewrite_if_correlated(subquery, row, col_map);
            let subq_result = exec_subquery(&subq_q)?;
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
            let subq_q = rewrite_if_correlated(subquery, row, col_map);
            let vals = exec_subquery(&subq_q)?;
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
                            Some(TrimWhereField::Leading) => s.trim_start_matches(c).to_string(),
                            Some(TrimWhereField::Trailing) => s.trim_end_matches(c).to_string(),
                            _ => s.trim_matches(c).to_string(),
                        }
                    } else {
                        s.trim().to_string()
                    }
                }
                (Some(TrimWhereField::Leading), _) => s.trim_start().to_string(),
                (Some(TrimWhereField::Trailing), _) => s.trim_end().to_string(),
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
        Expr::Subquery(query) => {
            let vals = exec_subquery(query)?;
            Ok(vals.into_iter().next().unwrap_or(DbValue::Null))
        }
        // Double-quoted identifiers used as values: "hello" → string
        Expr::Identifier(ident) => Ok(DbValue::String(ident.value.clone())),
        // ARRAY[...] literals — SQLite-style array constructor for
        // STRINGS[]/FLOATS[] columns
        Expr::Array(arr) => {
            let mut strings: Option<Vec<String>> = None;
            let mut floats: Option<Vec<f64>> = None;
            for elem in &arr.elem {
                let v = eval_literal_expr(elem)?;
                match v {
                    DbValue::String(s) => {
                        strings.get_or_insert_with(Vec::new).push(s);
                    }
                    DbValue::Int(n) => {
                        floats.get_or_insert_with(Vec::new).push(n as f64);
                    }
                    DbValue::Float(f) => {
                        floats.get_or_insert_with(Vec::new).push(f);
                    }
                    other => {
                        return Err(EngineError::Exec(format!(
                            "Array elements must be strings or numbers, got {:?}",
                            other
                        )))
                    }
                }
            }
            if let Some(f) = floats {
                Ok(DbValue::Floats(f))
            } else if let Some(s) = strings {
                Ok(DbValue::Strings(s))
            } else {
                Ok(DbValue::Strings(Vec::new()))
            }
        }
        // Function calls with no row context: datetime('now') and friends (SQLite-style)
        Expr::Function(func) => {
            let name = func.name.to_string().to_lowercase();
            if matches!(
                name.as_str(),
                "datetime" | "now" | "current_timestamp" | "curdate" | "current_date" | "unix_timestamp"
            ) {
                exec_std_function(func, name, &[], &HashMap::new())
            } else {
                Err(EngineError::Exec(format!(
                    "Complex expressions not supported in values: {:?}",
                    expr
                )))
            }
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
            // Check plugin/SQF registry for fn_ prefixed functions
            if let Some(fn_name) = name.strip_prefix("fn_") {
                // 1. Rust/C ABI plugin function
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
                // 2. SQF-registered function with body → notify via CALLBACK
                if crate::engine::plugin::get_sqf_function_body(fn_name).is_some() {
                    let args = extract_func_args(func);
                    let args_str: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    let cb_name = name.clone();
                    let mut cb_args = args_str.join(", ");
                    // Bound the args payload to 2048 bytes at a char boundary so
                    // Arma's callback never receives an over-long buffer.
                    if cb_args.len() > 2048 {
                        let mut end = 2048;
                        while !cb_args.is_char_boundary(end) {
                            end -= 1;
                        }
                        cb_args.truncate(end);
                    }
                    let cb_ctx = String::new();
                    if let Some(cb) = crate::ffi::CALLBACK.lock().unwrap().as_ref() {
                        if let (Ok(name_c), Ok(args_c), Ok(ctx_c)) = (
                            std::ffi::CString::new(cb_name),
                            std::ffi::CString::new(cb_args),
                            std::ffi::CString::new(cb_ctx),
                        ) {
                            // The CString buffers stay alive until the call returns,
                            // matching arma_rs's Extension::run_callbacks contract.
                            cb(name_c.as_ptr(), args_c.as_ptr(), ctx_c.as_ptr());
                        }
                    }
                    // ponytail: SQF handles the actual result; return placeholder
                    return Ok(DbValue::String(format!("<SQF: {}>", fn_name)));
                }
                return Err(EngineError::Exec(format!("Unknown plugin function '{}'", fn_name)));
            }
            // sqlparser parses reserved keyword USER/CURRENT_USER as a zero-arg
            // function (`user()`). Real mods store columns named `user`, so a
            // bare `WHERE user = 1` must resolve to the column (SQLite allows
            // unquoted `user`). Only when no argument list is present AND the
            // name is a column in scope do we treat it as a column reference.
            if matches!(func.args, FunctionArguments::None) {
                if let Some(&pos) = col_map.get(&name) {
                    return Ok(row[pos].clone());
                }
            }
            exec_std_function(func, name, row, col_map)
        }
    }
}
