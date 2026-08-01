// a3sql core types — column types and runtime values

//! Core value types — [`DbValue`], [`Column`], [`ColumnType`], and comparison/serialization helpers.
//!
//! These types are the foundation of the engine's data model. Every cell in
//! every row is a `DbValue`.

use std::fmt;
use std::hash::{Hash, Hasher};

use super::functions::builtin::value_to_string;
use super::functions::eval::to_float;

/// Supported column data types.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ColumnType {
    Bool,
    Int,
    Float,
    String,
    Strings, // STRINGS[]
    Floats,  // FLOATS[]
}

impl std::fmt::Display for ColumnType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColumnType::Bool => write!(f, "BOOL"),
            ColumnType::Int => write!(f, "INT"),
            ColumnType::Float => write!(f, "FLOAT"),
            ColumnType::String => write!(f, "STRING"),
            ColumnType::Strings => write!(f, "STRINGS[]"),
            ColumnType::Floats => write!(f, "FLOATS[]"),
        }
    }
}

/// A column definition.
#[derive(Debug, Clone)]
pub(crate) struct Column {
    pub name: String,
    pub dtype: ColumnType,
    pub primary_key: bool,
    pub not_null: bool,
    /// Static literal default (e.g. `DEFAULT 0`, `DEFAULT 'x'`).
    pub default: Option<DbValue>,
    /// Non-literal default expression (e.g. `DEFAULT datetime('now')`),
    /// evaluated at INSERT time. Takes precedence over `default`.
    pub default_expr: Option<sqlparser::ast::Expr>,
    pub auto_increment: bool,
    pub unique: bool,
}

/// Runtime value stored in cells.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DbValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Strings(Vec<String>),
    Floats(Vec<f64>),
}

impl DbValue {
    /// Attempt to coerce a string into this value's type for comparison.
    #[allow(dead_code, reason = "value coercion for comparison not yet wired")]
    pub fn coerce(&self, raw: &str) -> Self {
        match self {
            DbValue::Int(_) | DbValue::Null => raw
                .parse::<i64>()
                .map(DbValue::Int)
                .unwrap_or(DbValue::String(raw.to_string())),
            DbValue::Float(_) => raw
                .parse::<f64>()
                .map(DbValue::Float)
                .unwrap_or(DbValue::String(raw.to_string())),
            DbValue::Bool(_) => match raw.to_lowercase().as_str() {
                "true" | "1" => DbValue::Bool(true),
                "false" | "0" => DbValue::Bool(false),
                _ => DbValue::String(raw.to_string()),
            },
            _ => DbValue::String(raw.to_string()),
        }
    }

    /// Format as a JSON-compatible string for the SQF result.
    pub fn to_json_string(&self) -> String {
        match self {
            DbValue::Null => "null".to_string(),
            DbValue::Bool(b) => b.to_string(),
            DbValue::Int(n) => n.to_string(),
            DbValue::Float(f) => {
                if f.fract() == 0.0 && f.is_finite() {
                    format!("{:.1}", f) // ensure decimal point
                } else {
                    format!("{}", f)
                }
            }
            DbValue::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            DbValue::Strings(v) => {
                let inner: Vec<String> = v.iter().map(|s| format!("\"{}\"", s)).collect();
                format!("[{}]", inner.join(","))
            }
            DbValue::Floats(v) => {
                let inner: Vec<String> = v.iter().map(|f| f.to_string()).collect();
                format!("[{}]", inner.join(","))
            }
        }
    }
}

impl fmt::Display for DbValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbValue::Null => write!(f, "NULL"),
            DbValue::Bool(b) => write!(f, "{}", b),
            DbValue::Int(n) => write!(f, "{}", n),
            DbValue::Float(n) => write!(f, "{}", n),
            DbValue::String(s) => write!(f, "'{}'", s),
            DbValue::Strings(v) => write!(f, "[{}]", v.join(",")),
            DbValue::Floats(v) => write!(f, "[{}]", v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",")),
        }
    }
}

/// Convert a serde_json::Value to DbValue.
pub(crate) fn json_val_to_dbvalue(v: &serde_json::Value) -> DbValue {
    match v {
        serde_json::Value::Null => DbValue::Null,
        serde_json::Value::Bool(b) => DbValue::Bool(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(DbValue::Int)
            .or_else(|| n.as_f64().map(DbValue::Float))
            .unwrap_or(DbValue::Null),
        serde_json::Value::String(s) => DbValue::String(s.clone()),
        _ => DbValue::String(v.to_string()),
    }
}

/// Compare two `DbValue`s for ordering.
pub(crate) fn db_value_cmp(a: &DbValue, b: &DbValue) -> std::cmp::Ordering {
    match (to_float(a), to_float(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => value_to_string(a).cmp(&value_to_string(b)),
    }
}

/// Grouping key for GROUP BY / DISTINCT — wraps the evaluated key values.
///
/// `Eq`/`Hash` match `DbValue`'s derived `PartialEq` (so `Int(5)` and
/// `String("5")` are distinct — the derived equality never compares them
/// equal), with one deliberate divergence: **all NaNs are equal** (derived
/// `PartialEq` says `NaN != NaN`, so every NaN row previously formed its own
/// group — undefined-feeling but deterministic per-row; now all NaN rows land
/// in one group). `-0.0` and `0.0` stay equal (derived `PartialEq` already
/// says so), and the hash canonicalizes `-0.0 → +0.0` bits to match.
/// Variant tags keep unequal variants (`Int(5)` vs `String("5")`, `String`
/// vs `Strings`) from ever sharing a bucket.
#[derive(Clone, Debug)]
pub(crate) struct GroupKey(pub(crate) Vec<DbValue>);

impl PartialEq for GroupKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().zip(&other.0).all(|(a, b)| group_values_equal(a, b))
    }
}

impl Eq for GroupKey {}

impl Hash for GroupKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.len().hash(state);
        for v in &self.0 {
            hash_group_value(v, state);
        }
    }
}

/// Equality used for grouping: derived `PartialEq` except all NaNs compare
/// equal (floats and FLOATS[] elements).
fn group_values_equal(a: &DbValue, b: &DbValue) -> bool {
    match (a, b) {
        (DbValue::Float(x), DbValue::Float(y)) => x == y || (x.is_nan() && y.is_nan()),
        (DbValue::Floats(x), DbValue::Floats(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| x == y || (x.is_nan() && y.is_nan()))
        }
        _ => a == b,
    }
}

fn hash_group_value<H: Hasher>(v: &DbValue, state: &mut H) {
    match v {
        DbValue::Null => 0u8.hash(state),
        DbValue::Bool(b) => {
            1u8.hash(state);
            b.hash(state);
        }
        DbValue::Int(n) => {
            2u8.hash(state);
            n.hash(state);
        }
        DbValue::Float(f) => {
            3u8.hash(state);
            canonical_float_bits(*f).hash(state);
        }
        DbValue::String(s) => {
            4u8.hash(state);
            s.hash(state);
        }
        DbValue::Strings(arr) => {
            5u8.hash(state);
            arr.len().hash(state);
            for s in arr {
                s.hash(state);
            }
        }
        DbValue::Floats(arr) => {
            6u8.hash(state);
            arr.len().hash(state);
            for f in arr {
                canonical_float_bits(*f).hash(state);
            }
        }
    }
}

/// NaN → one sentinel bit pattern; `-0.0` → `+0.0` bits; else raw bits.
/// Injective on the equivalence classes `group_values_equal` defines.
fn canonical_float_bits(f: f64) -> u64 {
    if f.is_nan() {
        f64::NAN.to_bits()
    } else if f == 0.0 {
        0.0f64.to_bits() // collapse -0.0 onto +0.0
    } else {
        f.to_bits()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_to_json() {
        assert_eq!(DbValue::Null.to_json_string(), "null");
        assert_eq!(DbValue::Bool(true).to_json_string(), "true");
        assert_eq!(DbValue::Int(42).to_json_string(), "42");
        assert_eq!(DbValue::Float(3.5).to_json_string(), "3.5");
        assert_eq!(DbValue::String("hello".into()).to_json_string(), r#""hello""#);
    }

    #[test]
    fn value_coerce() {
        let target = DbValue::Int(0);
        assert_eq!(target.coerce("42"), DbValue::Int(42));
        assert_eq!(target.coerce("not_a_num"), DbValue::String("not_a_num".into()));
    }
}
