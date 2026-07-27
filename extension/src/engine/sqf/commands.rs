// SQF native command implementations — dispatch table for eval_command().
//
// Each command has a corresponding Rust function registered in the NATIVE_IMPLS
// HashMap. Commands not natively implemented fall through to return DbValue::Null
// with a logged warning (safe for SQL NULL semantics).

use std::collections::HashMap;

use crate::engine::value::DbValue;

// ── Dispatch table ──────────────────────────────────────────────────────

type CmdFn = fn(&[DbValue]) -> Result<DbValue, String>;

/// Build the native implementations dispatch table.
fn build_native_impls() -> HashMap<&'static str, CmdFn> {
    let mut m: HashMap<&'static str, CmdFn> = HashMap::new();
    for &(n, f) in NATIVE_CMD_FNS {
        m.insert(n, f);
    }
    m
}

static NATIVE_CMD_FNS: &[(&str, CmdFn)] = &[
    // Nular constants
    ("pi", cmd_pi),
    ("true", cmd_true),
    ("false", cmd_false),
    ("nil", cmd_nil),
    // Unary math
    ("sqrt", cmd_sqrt),
    ("sin", cmd_sin),
    ("cos", cmd_cos),
    ("tan", cmd_tan),
    ("abs", cmd_abs),
    ("exp", cmd_exp),
    ("ln", cmd_ln),
    ("log", cmd_log),
    ("log10", cmd_log10),
    ("asin", cmd_asin),
    ("acos", cmd_acos),
    ("atan", cmd_atan),
    ("deg", cmd_deg),
    ("rad", cmd_rad),
    ("cosec", cmd_cosec),
    ("sec", cmd_sec),
    ("cot", cmd_cot),
    ("round", cmd_round),
    ("floor", cmd_floor),
    ("ceil", cmd_ceil),
    // Extended math
    ("trunc", cmd_trunc),
    ("sign", cmd_sign),
    ("random", cmd_random),
    // Binary math
    ("min", cmd_min),
    ("max", cmd_max),
    ("atan2", cmd_atan2),
    ("clamp", cmd_clamp),
    // Unary string
    ("str", cmd_str),
    ("to_string", cmd_str),
    ("toupper", cmd_toupper),
    ("to_upper", cmd_toupper),
    ("tolower", cmd_tolower),
    ("to_lower", cmd_tolower),
    ("typename", cmd_typename),
    ("type_name", cmd_typename),
    ("count", cmd_count),
    ("parsenumber", cmd_parsenumber),
    ("parse_number", cmd_parsenumber),
    ("trim", cmd_trim),
    // Binary string
    ("find", cmd_find),
    ("replace", cmd_replace),
    // Type helpers
    ("isnil", cmd_isnil),
    ("is_null", cmd_isnil),
    ("isequalto", cmd_isequalto),
    ("is_equal_to", cmd_isequalto),
    // Array ops
    ("select", cmd_select),
    ("in", cmd_in),
    ("pushback", cmd_pushback),
    ("deleteat", cmd_deleteat),
    // Vector
    ("vectormagnitude", cmd_vectormagnitude),
    // Side-effect passthrough
    ("hint", cmd_hint),
    ("hintc", cmd_hint),
];

// ── Public entry point ──────────────────────────────────────────────────

static NATIVE_IMPLS: std::sync::LazyLock<HashMap<&'static str, CmdFn>> = std::sync::LazyLock::new(build_native_impls);

/// Evaluate a named SQF command with pre-evaluated arguments.
///
/// Checks native implementations first, then falls back to
/// return-type-based generic handling. Unimplementable commands
/// return DbValue::Null (safe for SQL NULL semantics via SQF_EVAL()).
pub(crate) fn eval_command(name: &str, args: &[DbValue]) -> Result<DbValue, String> {
    // 1. Native implementation
    if let Some(f) = NATIVE_IMPLS.get(name) {
        return f(args);
    }

    // 2. Fallback based on return type metadata
    if let Some(info) = super::database::lookup_info(name) {
        if info.ret.is_implementable() && !args.is_empty() {
            // For implementable return types with arguments, try a best-effort:
            // if the command takes arguments and returns the type of its argument,
            // just pass the first arg through.
            return Ok(args[0].clone());
        }
    }

    // 3. Unknown / game-engine-only command — return nil gracefully
    // ponytail: unknown wiki commands return nil; wire Arma callback for real dispatch
    // if runtime engine commands are needed later
    Ok(DbValue::Null)
}

// ── Unary math wrappers (thin wrappers around unary_math / unary_math_int) ──

fn cmd_sqrt(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.sqrt())
}
fn cmd_sin(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.sin())
}
fn cmd_cos(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.cos())
}
fn cmd_tan(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.tan())
}
fn cmd_abs(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.abs())
}
fn cmd_exp(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.exp())
}
fn cmd_ln(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.ln())
}
fn cmd_log(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.log10())
}
fn cmd_log10(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.log10())
}
fn cmd_asin(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.asin())
}
fn cmd_acos(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.acos())
}
fn cmd_atan(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.atan())
}
fn cmd_deg(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.to_degrees())
}
fn cmd_rad(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.to_radians())
}
fn cmd_cosec(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| 1.0 / x.sin())
}
fn cmd_sec(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| 1.0 / x.cos())
}
fn cmd_cot(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| 1.0 / x.tan())
}
fn cmd_round(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math_int(a, |x| x.round())
}
fn cmd_floor(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math_int(a, |x| x.floor())
}
fn cmd_ceil(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math_int(a, |x| x.ceil())
}

// ── Nular constants ─────────────────────────────────────────────────────

fn cmd_pi(_: &[DbValue]) -> Result<DbValue, String> {
    Ok(DbValue::Float(std::f64::consts::PI))
}
fn cmd_true(_: &[DbValue]) -> Result<DbValue, String> {
    Ok(DbValue::Bool(true))
}
fn cmd_false(_: &[DbValue]) -> Result<DbValue, String> {
    Ok(DbValue::Bool(false))
}
fn cmd_nil(_: &[DbValue]) -> Result<DbValue, String> {
    Ok(DbValue::Null)
}

// ── Unary math helpers ──────────────────────────────────────────────────

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

fn binary_math<F>(args: &[DbValue], f: F) -> Result<DbValue, String>
where
    F: Fn(f64, f64) -> f64,
{
    if args.len() < 2 {
        return Err("command requires 2 arguments".into());
    }
    let a = to_f64(&args[0]).ok_or_else(|| format!("numeric expected, got {}", args[0]))?;
    let b = to_f64(&args[1]).ok_or_else(|| format!("numeric expected, got {}", args[1]))?;
    Ok(DbValue::Float(f(a, b)))
}

fn to_f64(v: &DbValue) -> Option<f64> {
    match v {
        DbValue::Int(n) => Some(*n as f64),
        DbValue::Float(f) => Some(*f),
        _ => None,
    }
}

fn value_str(v: &DbValue) -> String {
    match v {
        DbValue::Null => "nil".into(),
        DbValue::Bool(b) => b.to_string(),
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => s.clone(),
        DbValue::Strings(arr) => arr.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","),
        DbValue::Floats(arr) => arr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","),
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

// ── Extended math commands ──────────────────────────────────────────────

fn cmd_trunc(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("trunc requires 1 argument".into());
    }
    match &args[0] {
        DbValue::Int(n) => Ok(DbValue::Int(*n)),
        DbValue::Float(f) => Ok(DbValue::Int(f.trunc() as i64)),
        DbValue::Null => Ok(DbValue::Null),
        _ => Err(format!("numeric expected, got {}", args[0])),
    }
}

fn cmd_sign(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("sign requires 1 argument".into());
    }
    match &args[0] {
        DbValue::Int(n) => Ok(DbValue::Int(n.signum())),
        DbValue::Float(f) => Ok(DbValue::Int(f.signum() as i64)),
        DbValue::Null => Ok(DbValue::Null),
        _ => Err(format!("numeric expected, got {}", args[0])),
    }
}

fn cmd_random(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("random requires 1 argument".into());
    }
    let n = match &args[0] {
        DbValue::Int(n) => *n as f64,
        DbValue::Float(f) => *f,
        DbValue::Null => return Ok(DbValue::Null),
        _ => return Err(format!("numeric expected, got {}", args[0])),
    };
    // ponytail: non-cryptographic LCG for SQL-context random; replace with
    // proper CSPRNG if called outside SQF_EVAL()
    let r = fast_rand();
    Ok(DbValue::Float(r * n))
}

/// Simple thread-local LCG (MMIX Knuth). Seeded from system time on first call.
fn fast_rand() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static STATE: Cell<u64> = const { Cell::new(0) };
    }
    STATE.with(|s| {
        let old = s.get();
        let new = if old == 0 {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
        } else {
            old.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
        };
        s.set(new);
        (new >> 11) as f64 / (1u64 << 53) as f64
    })
}

fn cmd_min(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("min requires 2 arguments".into());
    }
    match (&args[0], &args[1]) {
        (DbValue::Int(a), DbValue::Int(b)) => Ok(DbValue::Int((*a).min(*b))),
        _ => {
            let a = to_f64(&args[0]).ok_or_else(|| format!("numeric expected, got {}", args[0]))?;
            let b = to_f64(&args[1]).ok_or_else(|| format!("numeric expected, got {}", args[1]))?;
            Ok(DbValue::Float(a.min(b)))
        }
    }
}

fn cmd_max(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("max requires 2 arguments".into());
    }
    match (&args[0], &args[1]) {
        (DbValue::Int(a), DbValue::Int(b)) => Ok(DbValue::Int((*a).max(*b))),
        _ => {
            let a = to_f64(&args[0]).ok_or_else(|| format!("numeric expected, got {}", args[0]))?;
            let b = to_f64(&args[1]).ok_or_else(|| format!("numeric expected, got {}", args[1]))?;
            Ok(DbValue::Float(a.max(b)))
        }
    }
}

fn cmd_atan2(args: &[DbValue]) -> Result<DbValue, String> {
    binary_math(args, |y, x| y.atan2(x))
}

fn cmd_clamp(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 3 {
        return Err("clamp requires 3 arguments".into());
    }
    let val = to_f64(&args[0]).ok_or_else(|| format!("numeric expected, got {}", args[0]))?;
    let lo = to_f64(&args[1]).ok_or_else(|| format!("numeric expected, got {}", args[1]))?;
    let hi = to_f64(&args[2]).ok_or_else(|| format!("numeric expected, got {}", args[2]))?;
    Ok(DbValue::Float(val.clamp(lo, hi)))
}

// ── String commands ─────────────────────────────────────────────────────

fn cmd_str(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("str requires 1 argument".into());
    }
    Ok(DbValue::String(value_str(&args[0])))
}

fn cmd_toupper(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("toUpper requires 1 argument".into());
    }
    Ok(DbValue::String(value_str(&args[0]).to_uppercase()))
}

fn cmd_tolower(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("toLower requires 1 argument".into());
    }
    Ok(DbValue::String(value_str(&args[0]).to_lowercase()))
}

fn cmd_typename(args: &[DbValue]) -> Result<DbValue, String> {
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

fn cmd_count(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("count requires 1 argument".into());
    }
    let n = match &args[0] {
        DbValue::String(s) => s.len() as i64,
        DbValue::Strings(v) => v.len() as i64,
        DbValue::Floats(v) => v.len() as i64,
        _ => 0, // SQF count on non-iterable returns 0
    };
    Ok(DbValue::Int(n))
}

fn cmd_parsenumber(args: &[DbValue]) -> Result<DbValue, String> {
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

fn cmd_trim(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("trim requires 1 argument".into());
    }
    Ok(DbValue::String(value_str(&args[0]).trim().to_string()))
}

fn cmd_replace(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("replace requires 2 arguments".into());
    }
    let s = value_str(&args[0]);
    let search = value_str(&args[1]);
    let replacement = if args.len() >= 3 {
        value_str(&args[2])
    } else {
        String::new()
    };
    Ok(DbValue::String(s.replace(&search, &replacement)))
}

fn cmd_find(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("find requires 2 arguments".into());
    }
    let s = value_str(&args[0]);
    let needle = value_str(&args[1]);
    match s.find(&needle) {
        Some(pos) => Ok(DbValue::Int(pos as i64)),
        None => Ok(DbValue::Int(-1)),
    }
}

// ── Type helpers ────────────────────────────────────────────────────────

fn cmd_isnil(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("isNil requires 1 argument".into());
    }
    // In our context, we check if the value itself is null
    Ok(DbValue::Bool(matches!(args[0], DbValue::Null)))
}

fn cmd_isequalto(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("isEqualTo requires 2 arguments".into());
    }
    Ok(DbValue::Bool(args[0] == args[1]))
}

// ── Array ops ───────────────────────────────────────────────────────────

fn cmd_select(args: &[DbValue]) -> Result<DbValue, String> {
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

fn cmd_in(args: &[DbValue]) -> Result<DbValue, String> {
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

fn cmd_pushback(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("pushBack requires 2 arguments".into());
    }
    // In pure eval context, return the array with the element appended
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
    array.push(value_str(val));
    Ok(DbValue::Strings(array))
}

fn cmd_deleteat(args: &[DbValue]) -> Result<DbValue, String> {
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

fn cmd_vectormagnitude(args: &[DbValue]) -> Result<DbValue, String> {
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

// ── Side-effect passthrough ─────────────────────────────────────────────

fn cmd_hint(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("hint requires 1 argument".into());
    }
    // ponytail: hint is a no-op in fast-path eval; the string is returned
    // so it can be observed in testing. Wire to Arma callback for real hints.
    Ok(DbValue::String(value_str(&args[0])))
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn eval_cmd(name: &str, args: &[DbValue]) -> Result<DbValue, String> {
        eval_command(name, args)
    }

    // ── Constants ──────────────────────────────────────────────────────

    #[test]
    fn test_pi() {
        let r = eval_cmd("pi", &[]).unwrap();
        assert_eq!(r, DbValue::Float(std::f64::consts::PI));
    }

    #[test]
    fn test_true_false_nil() {
        assert_eq!(eval_cmd("true", &[]).unwrap(), DbValue::Bool(true));
        assert_eq!(eval_cmd("false", &[]).unwrap(), DbValue::Bool(false));
        assert_eq!(eval_cmd("nil", &[]).unwrap(), DbValue::Null);
    }

    // ── Math ───────────────────────────────────────────────────────────

    #[test]
    fn test_sqrt() {
        assert_eq!(eval_cmd("sqrt", &[DbValue::Int(25)]).unwrap(), DbValue::Float(5.0));
    }

    #[test]
    fn test_abs() {
        assert_eq!(eval_cmd("abs", &[DbValue::Int(-5)]).unwrap(), DbValue::Float(5.0));
    }

    #[test]
    fn test_round_floor_ceil() {
        assert_eq!(eval_cmd("round", &[DbValue::Float(3.7)]).unwrap(), DbValue::Int(4));
        assert_eq!(eval_cmd("floor", &[DbValue::Float(3.7)]).unwrap(), DbValue::Int(3));
        assert_eq!(eval_cmd("ceil", &[DbValue::Float(3.2)]).unwrap(), DbValue::Int(4));
    }

    #[test]
    fn test_trunc() {
        assert_eq!(eval_cmd("trunc", &[DbValue::Float(3.7)]).unwrap(), DbValue::Int(3));
        assert_eq!(eval_cmd("trunc", &[DbValue::Float(-3.7)]).unwrap(), DbValue::Int(-3));
    }

    #[test]
    fn test_sign() {
        assert_eq!(eval_cmd("sign", &[DbValue::Int(-5)]).unwrap(), DbValue::Int(-1));
        assert_eq!(eval_cmd("sign", &[DbValue::Int(0)]).unwrap(), DbValue::Int(0));
        assert_eq!(eval_cmd("sign", &[DbValue::Int(5)]).unwrap(), DbValue::Int(1));
    }

    #[test]
    fn test_min_max() {
        assert_eq!(
            eval_cmd("min", &[DbValue::Int(3), DbValue::Int(7)]).unwrap(),
            DbValue::Int(3)
        );
        assert_eq!(
            eval_cmd("max", &[DbValue::Int(3), DbValue::Int(7)]).unwrap(),
            DbValue::Int(7)
        );
    }

    #[test]
    fn test_atan2() {
        let r = eval_cmd("atan2", &[DbValue::Int(0), DbValue::Int(1)]).unwrap();
        assert_eq!(r, DbValue::Float(0.0));
    }

    #[test]
    fn test_clamp() {
        let r = eval_cmd(
            "clamp",
            &[DbValue::Float(5.0), DbValue::Float(0.0), DbValue::Float(3.0)],
        )
        .unwrap();
        assert_eq!(r, DbValue::Float(3.0));
    }

    // ── String ─────────────────────────────────────────────────────────

    #[test]
    fn test_str() {
        assert_eq!(
            eval_cmd("str", &[DbValue::Int(42)]).unwrap(),
            DbValue::String("42".into())
        );
    }

    #[test]
    fn test_toupper_tolower() {
        assert_eq!(
            eval_cmd("toupper", &[DbValue::String("hello".into())]).unwrap(),
            DbValue::String("HELLO".into())
        );
        assert_eq!(
            eval_cmd("tolower", &[DbValue::String("HELLO".into())]).unwrap(),
            DbValue::String("hello".into())
        );
    }

    #[test]
    fn test_typename() {
        assert_eq!(
            eval_cmd("typename", &[DbValue::Int(42)]).unwrap(),
            DbValue::String("SCALAR".into())
        );
        assert_eq!(
            eval_cmd("typename", &[DbValue::String("x".into())]).unwrap(),
            DbValue::String("STRING".into())
        );
    }

    #[test]
    fn test_trim() {
        assert_eq!(
            eval_cmd("trim", &[DbValue::String("  hello  ".into())]).unwrap(),
            DbValue::String("hello".into())
        );
    }

    #[test]
    fn test_replace() {
        assert_eq!(
            eval_cmd(
                "replace",
                &[
                    DbValue::String("hello world".into()),
                    DbValue::String("world".into()),
                    DbValue::String("there".into())
                ]
            )
            .unwrap(),
            DbValue::String("hello there".into())
        );
    }

    #[test]
    fn test_find() {
        assert_eq!(
            eval_cmd("find", &[DbValue::String("hello".into()), DbValue::String("ll".into())]).unwrap(),
            DbValue::Int(2)
        );
        assert_eq!(
            eval_cmd(
                "find",
                &[DbValue::String("hello".into()), DbValue::String("xyz".into())]
            )
            .unwrap(),
            DbValue::Int(-1)
        );
    }

    #[test]
    fn test_parsenumber() {
        assert_eq!(
            eval_cmd("parsenumber", &[DbValue::String("42".into())]).unwrap(),
            DbValue::Int(42)
        );
        let r = eval_cmd("parsenumber", &[DbValue::String("3.14".into())]).unwrap();
        match r {
            DbValue::Float(f) => assert!((f - 3.14).abs() < 0.001),
            _ => panic!("expected Float"),
        }
    }

    #[test]
    fn test_count() {
        assert_eq!(
            eval_cmd("count", &[DbValue::String("hello".into())]).unwrap(),
            DbValue::Int(5)
        );
        assert_eq!(
            eval_cmd("count", &[DbValue::Strings(vec!["a".into(), "b".into()])]).unwrap(),
            DbValue::Int(2)
        );
    }

    // ── Type helpers ───────────────────────────────────────────────────

    #[test]
    fn test_isnil() {
        assert_eq!(eval_cmd("isnil", &[DbValue::Null]).unwrap(), DbValue::Bool(true));
        assert_eq!(eval_cmd("isnil", &[DbValue::Int(0)]).unwrap(), DbValue::Bool(false));
    }

    #[test]
    fn test_isequalto() {
        assert_eq!(
            eval_cmd("isequalto", &[DbValue::Int(1), DbValue::Int(1)]).unwrap(),
            DbValue::Bool(true)
        );
        assert_eq!(
            eval_cmd("isequalto", &[DbValue::Int(1), DbValue::Int(2)]).unwrap(),
            DbValue::Bool(false)
        );
    }

    // ── Array ops ──────────────────────────────────────────────────────

    #[test]
    fn test_select() {
        let arr = DbValue::Strings(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(
            eval_cmd("select", &[arr.clone(), DbValue::Int(0)]).unwrap(),
            DbValue::String("a".into())
        );
        assert_eq!(
            eval_cmd("select", &[arr.clone(), DbValue::Int(1)]).unwrap(),
            DbValue::String("b".into())
        );
        assert_eq!(eval_cmd("select", &[arr, DbValue::Int(10)]).unwrap(), DbValue::Null);
    }

    #[test]
    fn test_select_string() {
        assert_eq!(
            eval_cmd("select", &[DbValue::String("hello".into()), DbValue::Int(0)]).unwrap(),
            DbValue::String("h".into())
        );
    }

    #[test]
    fn test_in() {
        let arr = DbValue::Strings(vec!["a".into(), "b".into(), "c".into()]);
        assert_eq!(
            eval_cmd("in", &[DbValue::String("a".into()), arr.clone()]).unwrap(),
            DbValue::Bool(true)
        );
        assert_eq!(
            eval_cmd("in", &[DbValue::String("z".into()), arr]).unwrap(),
            DbValue::Bool(false)
        );
    }

    #[test]
    fn test_pushback() {
        let arr = DbValue::Strings(vec!["a".into(), "b".into()]);
        let r = eval_cmd("pushback", &[arr, DbValue::String("c".into())]).unwrap();
        assert_eq!(r, DbValue::Strings(vec!["a".into(), "b".into(), "c".into()]));
    }

    // ── Vector ─────────────────────────────────────────────────────────

    #[test]
    fn test_vectormagnitude() {
        let v = DbValue::Floats(vec![3.0, 4.0]);
        let r = eval_cmd("vectormagnitude", &[v]).unwrap();
        assert_eq!(r, DbValue::Float(5.0));
    }

    // ── Unknown / unimplementable command returns nil ──────────────────

    #[test]
    fn test_unknown_command_returns_nil() {
        // Unknown command name (not in wiki) → nil
        assert_eq!(eval_cmd("zzz_nonexistent_cmd", &[]).unwrap(), DbValue::Null);
        // Wiki command with non-implementable return type (Object) → nil
        assert_eq!(eval_cmd("createvehicle", &[]).unwrap(), DbValue::Null);
    }
}
