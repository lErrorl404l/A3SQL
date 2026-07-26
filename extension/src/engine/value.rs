// a3sql core types — column types and runtime values

//! Core value types — [`DbValue`], [`Column`], [`ColumnType`], and comparison/serialization helpers.
//!
//! These types are the foundation of the engine's data model. Every cell in
//! every row is a `DbValue`.

use std::fmt;

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
    pub default: Option<DbValue>,
    pub auto_increment: bool,
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
