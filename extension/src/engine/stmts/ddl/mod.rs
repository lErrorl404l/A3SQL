// DDL statements — split into create.rs and misc.rs
// Helpers shared by both sub-modules live here.

mod create;
mod misc;

pub(crate) use create::*;
pub(crate) use misc::*;

use crate::engine::value::{ColumnType, DbValue};
use sqlparser::ast::{DataType, ObjectName, ObjectNamePart};

/// Convert an `ObjectName` (qualified or simple) to a dot-separated lowercase string.
pub(crate) fn object_name_str(name: &ObjectName) -> String {
    name.0
        .iter()
        .filter_map(|p| match p {
            ObjectNamePart::Identifier(i) => Some(i.value.to_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Convert a `serde_json::Value` to the engine's `DbValue`.
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

/// Parse a SQL data type into the engine's `ColumnType`.
pub(crate) fn parse_data_type(dt: &DataType) -> Result<ColumnType, String> {
    match dt {
        DataType::Int(_)
        | DataType::Integer(_)
        | DataType::BigInt(_)
        | DataType::SmallInt(_)
        | DataType::TinyInt(_) => Ok(ColumnType::Int),
        DataType::Float(_)
        | DataType::Double(_)
        | DataType::Real
        | DataType::Decimal(_)
        | DataType::Dec(_)
        | DataType::Numeric(_) => Ok(ColumnType::Float),
        DataType::String(_) | DataType::Text | DataType::Varchar(_) | DataType::Char(_) | DataType::Uuid => {
            Ok(ColumnType::String)
        }
        DataType::Boolean | DataType::Bool => Ok(ColumnType::Bool),
        DataType::Array(elem) => {
            use sqlparser::ast::ArrayElemTypeDef;
            let inner = match elem {
                ArrayElemTypeDef::SquareBracket(dt, _) => dt.as_ref(),
                ArrayElemTypeDef::AngleBracket(dt) => dt.as_ref(),
                ArrayElemTypeDef::Parenthesis(dt) => dt.as_ref(),
                ArrayElemTypeDef::None => return Ok(ColumnType::Strings),
            };
            match inner {
                DataType::String(_) | DataType::Varchar(_) | DataType::Text | DataType::Char(_) => {
                    Ok(ColumnType::Strings)
                }
                DataType::Float(_) | DataType::Double(_) | DataType::Real => Ok(ColumnType::Floats),
                _ if inner.to_string().to_lowercase() == "string" => Ok(ColumnType::Strings),
                _ => Err(format!("Unsupported array element type: {}", inner)),
            }
        }
        DataType::Custom(name, _) => {
            let s = name.to_string().to_uppercase();
            match s.as_str() {
                "STRINGS" => Ok(ColumnType::Strings),
                "FLOATS" => Ok(ColumnType::Floats),
                "STRING" => Ok(ColumnType::String),
                "INT" | "INTEGER" | "BIGINT" | "TINYINT" | "SMALLINT" => Ok(ColumnType::Int),
                "FLOAT" | "DOUBLE" => Ok(ColumnType::Float),
                "BOOL" | "BOOLEAN" => Ok(ColumnType::Bool),
                _ => Err(format!("Unknown custom type '{}'", s)),
            }
        }
        _ => Err(format!("Unsupported data type: {:?}", dt)),
    }
}

/// Convert a SQL parser value literal into the engine's `DbValue`.
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
