// String manipulation SQF command implementations.

use crate::engine::value::DbValue;

// ── String commands ─────────────────────────────────────────────────────

pub(super) fn cmd_str(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("str requires 1 argument".into());
    }
    Ok(DbValue::String(super::value_str(&args[0])))
}

pub(super) fn cmd_toupper(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("toUpper requires 1 argument".into());
    }
    Ok(DbValue::String(super::value_str(&args[0]).to_uppercase()))
}

pub(super) fn cmd_tolower(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("toLower requires 1 argument".into());
    }
    Ok(DbValue::String(super::value_str(&args[0]).to_lowercase()))
}

pub(super) fn cmd_typename(args: &[DbValue]) -> Result<DbValue, String> {
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

pub(super) fn cmd_count(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("count requires 1 argument".into());
    }
    let n = match &args[0] {
        DbValue::String(s) => s.len() as i64,
        DbValue::Strings(v) => v.len() as i64,
        DbValue::Floats(v) => v.len() as i64,
        _ => 0,
    };
    Ok(DbValue::Int(n))
}

pub(super) fn cmd_parsenumber(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("parseNumber requires 1 argument".into());
    }
    let s = super::value_str(&args[0]);
    match s.trim().parse::<f64>() {
        Ok(n) => {
            if n.fract() == 0.0 && n.is_finite() && (n as i64 as f64 == n) {
                Ok(DbValue::Int(n as i64))
            } else {
                Ok(DbValue::Float(n))
            }
        }
        Err(_) => Ok(DbValue::Int(0)),
    }
}

pub(super) fn cmd_trim(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("trim requires 1 argument".into());
    }
    Ok(DbValue::String(super::value_str(&args[0]).trim().to_string()))
}

pub(super) fn cmd_replace(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("replace requires 2 arguments".into());
    }
    let s = super::value_str(&args[0]);
    let search = super::value_str(&args[1]);
    let replacement = if args.len() >= 3 {
        super::value_str(&args[2])
    } else {
        String::new()
    };
    Ok(DbValue::String(s.replace(&search, &replacement)))
}

pub(super) fn cmd_find(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("find requires 2 arguments".into());
    }
    let s = super::value_str(&args[0]);
    let needle = super::value_str(&args[1]);
    match s.find(&needle) {
        Some(pos) => Ok(DbValue::Int(pos as i64)),
        None => Ok(DbValue::Int(-1)),
    }
}
