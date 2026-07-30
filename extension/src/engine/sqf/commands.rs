// SQF native command implementations — dispatch table for eval_command().
//
// Sub-modules hold the per-category command function bodies.

use std::collections::HashMap;

use crate::engine::value::DbValue;

// ── Sub-modules ────────────────────────────────────────────────────────
mod array;
mod math;
mod string;
use array::*;
use math::*;
use string::*;

// ── Dispatch table ──────────────────────────────────────────────────────
type CmdFn = fn(&[DbValue]) -> Result<DbValue, String>;

fn build_native_impls() -> HashMap<&'static str, CmdFn> {
    let mut m = HashMap::new();
    for &(n, f) in NATIVE_CMD_FNS {
        m.insert(n, f);
    }
    m
}

pub(crate) static NATIVE_CMD_FNS: &[(&str, CmdFn)] = &[
    // Nular
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
    // Extended
    ("trunc", cmd_trunc),
    ("sign", cmd_sign),
    ("random", cmd_random),
    // Binary
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
    // Passthrough
    ("hint", cmd_hint),
    ("hintc", cmd_hint),
];
// ── Public entry points ────────────────────────────────────────────────
static NATIVE_IMPLS: std::sync::LazyLock<HashMap<&'static str, CmdFn>> = std::sync::LazyLock::new(build_native_impls);
/// Evaluate a named SQF command with pre-evaluated arguments.
pub(crate) fn eval_command(name: &str, args: &[DbValue]) -> Result<DbValue, String> {
    // 1. Native implementation
    if let Some(f) = NATIVE_IMPLS.get(name) {
        return f(args);
    }

    // 2. Fallback based on return type metadata
    if let Some(info) = super::database::lookup_info(name) {
        if info.ret.is_implementable() && !args.is_empty() {
            return Ok(args[0].clone());
        }
    }

    // 3. Unknown / game-engine-only command — return nil gracefully
    Ok(DbValue::Null)
}

// ── Shared helpers (used by sub-modules) ────────────────────────────────

pub(super) fn to_f64(v: &DbValue) -> Option<f64> {
    match v {
        DbValue::Int(n) => Some(*n as f64),
        DbValue::Float(f) => Some(*f),
        _ => None,
    }
}

pub(super) fn value_str(v: &DbValue) -> String {
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

#[allow(dead_code, reason = "boolean coercion for SQF comparison not yet used")]
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

// ── Tests ───────────────────────────────────────────────────────────────
#[cfg(test)]
mod test;
