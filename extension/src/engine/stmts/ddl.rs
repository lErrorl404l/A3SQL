// DDL statements — split into create.rs and misc.rs
// Helpers shared by both sub-modules live here.

//! DDL statement handlers — CREATE, DROP, ALTER, and inspection commands.
//! Sub-modules: `create` (TABLE, VIEW, INDEX, TRIGGER), `inspect` (DESCRIBE, SHOW),
//! `misc` (VACUUM, COPY, COMMENT, CALL, ANALYZE).

mod create;
mod inspect;
mod misc;

pub(crate) use create::{
    exec_create_index, exec_create_sequence, exec_create_table, exec_create_trigger, exec_create_view,
    exec_create_virtual_table,
};
pub(crate) use inspect::{describe_table, show_create_table};
pub(crate) use misc::{
    exec_analyze, exec_call, exec_comment_on, exec_copy, exec_drop_trigger, exec_show_columns, exec_show_create,
    exec_vacuum,
};

use crate::engine::error::EngineError;
use crate::engine::value::{ColumnType, DbValue, json_val_to_dbvalue};
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

/// Parse a SQL data type into the engine's `ColumnType`.
pub(crate) fn parse_data_type(dt: &DataType) -> Result<ColumnType, EngineError> {
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
                DataType::Custom(name, _) => {
                    let s = name.to_string().to_uppercase();
                    match s.as_str() {
                        "STRINGS" => Ok(ColumnType::Strings),
                        "FLOATS" => Ok(ColumnType::Floats),
                        _ => Err(EngineError::Parse(format!("Unsupported array element type: {}", inner))),
                    }
                }
                _ if inner.to_string().to_lowercase() == "string" => Ok(ColumnType::Strings),
                _ => Err(EngineError::Parse(format!("Unsupported array element type: {}", inner))),
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
                _ => Err(EngineError::Parse(format!("Unknown custom type '{}'", s))),
            }
        }
        _ => Err(EngineError::Parse(format!("Unsupported data type: {:?}", dt))),
    }
}

/// Convert a SQL parser value literal into the engine's `DbValue`.
pub(crate) fn sql_val_to_db(v: &sqlparser::ast::Value) -> DbValue {
    match v {
        sqlparser::ast::Value::Null => DbValue::Null,
        sqlparser::ast::Value::Boolean(b) => DbValue::Bool(*b),
        sqlparser::ast::Value::Number(s, _) => {
            if s.contains('.') || s.contains('e') || s.contains('E') {
                s.parse::<f64>()
                    .map(DbValue::Float)
                    .or_else(|_| s.parse::<i64>().map(DbValue::Int))
                    .unwrap_or(DbValue::String(s.clone()))
            } else {
                s.parse::<i64>()
                    .map(DbValue::Int)
                    .or_else(|_| s.parse::<f64>().map(DbValue::Float))
                    .unwrap_or(DbValue::String(s.clone()))
            }
        }
        sqlparser::ast::Value::SingleQuotedString(s) | sqlparser::ast::Value::DoubleQuotedString(s) => {
            DbValue::String(s.clone())
        }
        _ => DbValue::String(format!("{:?}", v)),
    }
}
