// Array, vector, and type-helper SQF command implementations.

use crate::engine::value::DbValue;

// ── Array ops ───────────────────────────────────────────────────────────

pub(super) fn cmd_select(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("select requires 2 arguments".into());
    }
    let idx = match &args[1] {
        DbValue::Int(i) => *i,
        _ => return Err(format!("numeric index expected, got {}", args[1])),
    };
    match &args[0] {
        DbValue::Strings(v) => {
            if idx >= 0 && (idx as usize) < v.len() {
                Ok(DbValue::String(v[idx as usize].clone()))
            } else {
                Ok(DbValue::Null)
            }
        }
        DbValue::Floats(v) => {
            if idx >= 0 && (idx as usize) < v.len() {
                Ok(DbValue::Float(v[idx as usize]))
            } else {
                Ok(DbValue::Null)
            }
        }
        DbValue::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            if idx >= 0 && (idx as usize) < chars.len() {
                Ok(DbValue::String(chars[idx as usize].to_string()))
            } else {
                Ok(DbValue::Null)
            }
        }
        _ => Err(format!("select on unsupported type: {}", args[0])),
    }
}

pub(super) fn cmd_in(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("in requires 2 arguments".into());
    }
    let val = &args[0];
    match &args[1] {
        DbValue::Strings(v) => Ok(DbValue::Bool(v.iter().any(|s| DbValue::String(s.clone()) == *val))),
        DbValue::Floats(v) => Ok(DbValue::Bool(v.iter().any(|f| DbValue::Float(*f) == *val))),
        _ => Err(format!("in expects array, got {}", args[1])),
    }
}

pub(super) fn cmd_pushback(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("pushBack requires 2 arguments".into());
    }
    let val = &args[1];
    let mut array = match &args[0] {
        DbValue::Strings(v) => v.clone(),
        DbValue::Floats(v) => {
            let f = match val {
                DbValue::Float(f) => *f,
                DbValue::Int(i) => *i as f64,
                _ => return Err("type mismatch for array pushBack".into()),
            };
            let mut r = v.clone();
            r.push(f);
            return Ok(DbValue::Floats(r));
        }
        _ => return Err(format!("pushBack on non-array: {}", args[0])),
    };
    array.push(super::value_str(val));
    Ok(DbValue::Strings(array))
}

pub(super) fn cmd_deleteat(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("deleteAt requires 2 arguments".into());
    }
    let idx = match &args[1] {
        DbValue::Int(i) => *i,
        _ => return Err(format!("numeric index expected, got {}", args[1])),
    };
    match &args[0] {
        DbValue::Strings(v) => {
            let mut r = v.clone();
            if idx >= 0 && (idx as usize) < r.len() {
                r.remove(idx as usize);
            }
            Ok(DbValue::Strings(r))
        }
        DbValue::Floats(v) => {
            let mut r = v.clone();
            if idx >= 0 && (idx as usize) < r.len() {
                r.remove(idx as usize);
            }
            Ok(DbValue::Floats(r))
        }
        _ => Err(format!("deleteAt on non-array: {}", args[0])),
    }
}

// ── Vector (simplified) ─────────────────────────────────────────────────

pub(super) fn cmd_vectormagnitude(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("vectorMagnitude requires 1 argument".into());
    }
    let components: Vec<f64> = match &args[0] {
        DbValue::Floats(v) => v.clone(),
        DbValue::Strings(v) => v.iter().filter_map(|s| s.parse::<f64>().ok()).collect(),
        _ => return Err(format!("vector expected, got {}", args[0])),
    };
    let sum: f64 = components.iter().map(|c| c * c).sum();
    Ok(DbValue::Float(sum.sqrt()))
}

// ── Type helpers ────────────────────────────────────────────────────────

pub(super) fn cmd_isnil(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("isNil requires 1 argument".into());
    }
    Ok(DbValue::Bool(matches!(args[0], DbValue::Null)))
}

pub(super) fn cmd_isequalto(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("isEqualTo requires 2 arguments".into());
    }
    Ok(DbValue::Bool(args[0] == args[1]))
}

// ── Side-effect passthrough ─────────────────────────────────────────────

pub(super) fn cmd_hint(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("hint requires 1 argument".into());
    }
    Ok(DbValue::String(super::value_str(&args[0])))
}
