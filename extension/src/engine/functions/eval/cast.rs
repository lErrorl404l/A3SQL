// CAST helper — converts a DbValue to a target sqlparser DataType

use sqlparser::ast::DataType;

use super::super::super::value::DbValue;
use crate::engine::error::EngineError;

/// CAST a DbValue to the target sqlparser DataType.
pub(super) fn cast_db_value(val: DbValue, target: &DataType) -> Result<DbValue, EngineError> {
    use sqlparser::ast::DataType as DT;
    match target {
        DT::Bool | DT::Boolean => match val {
            DbValue::Bool(b) => Ok(DbValue::Bool(b)),
            DbValue::Int(i) => Ok(DbValue::Bool(i != 0)),
            DbValue::Float(_) => Ok(DbValue::Bool(true)),
            DbValue::String(s) => {
                let lower = s.to_lowercase();
                match lower.as_str() {
                    "true" | "1" | "yes" => Ok(DbValue::Bool(true)),
                    "false" | "0" | "no" => Ok(DbValue::Bool(false)),
                    _ => Err(EngineError::TypeError {
                        expected: "BOOL".into(),
                        actual: format!("string '{}'", s),
                    }),
                }
            }
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(EngineError::TypeError {
                expected: "BOOL".into(),
                actual: format!("{:?}", val),
            }),
        },
        DT::Int(_) | DT::BigInt(_) | DT::SmallInt(_) | DT::TinyInt(_) => match val {
            DbValue::Int(i) => Ok(DbValue::Int(i)),
            DbValue::Float(f) => Ok(DbValue::Int(f as i64)),
            DbValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Ok(DbValue::Int(0))
                } else {
                    trimmed
                        .parse::<i64>()
                        .map(DbValue::Int)
                        .map_err(|_| EngineError::TypeError {
                            expected: "INT".into(),
                            actual: format!("string '{}'", s),
                        })
                }
            }
            DbValue::Bool(b) => Ok(DbValue::Int(if b { 1 } else { 0 })),
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(EngineError::TypeError {
                expected: "INT".into(),
                actual: format!("{:?}", val),
            }),
        },
        DT::Float(_) | DT::Double(_) | DT::Real | DT::Decimal(_) | DT::Numeric(_) => match val {
            DbValue::Int(i) => Ok(DbValue::Float(i as f64)),
            DbValue::Float(f) => Ok(DbValue::Float(f)),
            DbValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Ok(DbValue::Float(0.0))
                } else {
                    trimmed
                        .parse::<f64>()
                        .map(DbValue::Float)
                        .map_err(|_| EngineError::TypeError {
                            expected: "FLOAT".into(),
                            actual: format!("string '{}'", s),
                        })
                }
            }
            DbValue::Bool(b) => Ok(DbValue::Float(if b { 1.0 } else { 0.0 })),
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(EngineError::TypeError {
                expected: "FLOAT".into(),
                actual: format!("{:?}", val),
            }),
        },
        DT::Varchar(_) | DT::Char(_) | DT::Text | DT::String(_) | DT::Uuid => Ok(DbValue::String(val.to_string())),
        _ => Ok(DbValue::String(val.to_string())),
    }
}
