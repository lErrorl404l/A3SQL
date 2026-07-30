// Math / arithmetic SQF command implementations.

use crate::engine::value::DbValue;

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
    let a = super::to_f64(&args[0]).ok_or_else(|| format!("numeric expected, got {}", args[0]))?;
    let b = super::to_f64(&args[1]).ok_or_else(|| format!("numeric expected, got {}", args[1]))?;
    Ok(DbValue::Float(f(a, b)))
}

// ── Nular constants ─────────────────────────────────────────────────────

pub(super) fn cmd_pi(_: &[DbValue]) -> Result<DbValue, String> {
    Ok(DbValue::Float(std::f64::consts::PI))
}
pub(super) fn cmd_true(_: &[DbValue]) -> Result<DbValue, String> {
    Ok(DbValue::Bool(true))
}
pub(super) fn cmd_false(_: &[DbValue]) -> Result<DbValue, String> {
    Ok(DbValue::Bool(false))
}
pub(super) fn cmd_nil(_: &[DbValue]) -> Result<DbValue, String> {
    Ok(DbValue::Null)
}

// ── Unary math wrappers ─────────────────────────────────────────────────

pub(super) fn cmd_sqrt(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.sqrt())
}
pub(super) fn cmd_sin(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.sin())
}
pub(super) fn cmd_cos(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.cos())
}
pub(super) fn cmd_tan(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.tan())
}
pub(super) fn cmd_abs(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.abs())
}
pub(super) fn cmd_exp(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.exp())
}
pub(super) fn cmd_ln(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.ln())
}
pub(super) fn cmd_log(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.log10())
}
pub(super) fn cmd_log10(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.log10())
}
pub(super) fn cmd_asin(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.asin())
}
pub(super) fn cmd_acos(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.acos())
}
pub(super) fn cmd_atan(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.atan())
}
pub(super) fn cmd_deg(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.to_degrees())
}
pub(super) fn cmd_rad(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| x.to_radians())
}
pub(super) fn cmd_cosec(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| 1.0 / x.sin())
}
pub(super) fn cmd_sec(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| 1.0 / x.cos())
}
pub(super) fn cmd_cot(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math(a, |x| 1.0 / x.tan())
}
pub(super) fn cmd_round(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math_int(a, |x| x.round())
}
pub(super) fn cmd_floor(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math_int(a, |x| x.floor())
}
pub(super) fn cmd_ceil(a: &[DbValue]) -> Result<DbValue, String> {
    unary_math_int(a, |x| x.ceil())
}

// ── Extended math commands ──────────────────────────────────────────────

pub(super) fn cmd_trunc(args: &[DbValue]) -> Result<DbValue, String> {
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

pub(super) fn cmd_sign(args: &[DbValue]) -> Result<DbValue, String> {
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

pub(super) fn cmd_random(args: &[DbValue]) -> Result<DbValue, String> {
    if args.is_empty() {
        return Err("random requires 1 argument".into());
    }
    let n = match &args[0] {
        DbValue::Int(n) => *n as f64,
        DbValue::Float(f) => *f,
        DbValue::Null => return Ok(DbValue::Null),
        _ => return Err(format!("numeric expected, got {}", args[0])),
    };
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

pub(super) fn cmd_min(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("min requires 2 arguments".into());
    }
    match (&args[0], &args[1]) {
        (DbValue::Int(a), DbValue::Int(b)) => Ok(DbValue::Int((*a).min(*b))),
        _ => {
            let a = super::to_f64(&args[0]).ok_or_else(|| format!("numeric expected, got {}", args[0]))?;
            let b = super::to_f64(&args[1]).ok_or_else(|| format!("numeric expected, got {}", args[1]))?;
            Ok(DbValue::Float(a.min(b)))
        }
    }
}

pub(super) fn cmd_max(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 2 {
        return Err("max requires 2 arguments".into());
    }
    match (&args[0], &args[1]) {
        (DbValue::Int(a), DbValue::Int(b)) => Ok(DbValue::Int((*a).max(*b))),
        _ => {
            let a = super::to_f64(&args[0]).ok_or_else(|| format!("numeric expected, got {}", args[0]))?;
            let b = super::to_f64(&args[1]).ok_or_else(|| format!("numeric expected, got {}", args[1]))?;
            Ok(DbValue::Float(a.max(b)))
        }
    }
}

pub(super) fn cmd_atan2(args: &[DbValue]) -> Result<DbValue, String> {
    binary_math(args, |y, x| y.atan2(x))
}

pub(super) fn cmd_clamp(args: &[DbValue]) -> Result<DbValue, String> {
    if args.len() < 3 {
        return Err("clamp requires 3 arguments".into());
    }
    let val = super::to_f64(&args[0]).ok_or_else(|| format!("numeric expected, got {}", args[0]))?;
    let lo = super::to_f64(&args[1]).ok_or_else(|| format!("numeric expected, got {}", args[1]))?;
    let hi = super::to_f64(&args[2]).ok_or_else(|| format!("numeric expected, got {}", args[2]))?;
    Ok(DbValue::Float(val.clamp(lo, hi)))
}
