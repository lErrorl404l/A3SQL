// a3sql serialization — JSON format

//! JSON serialization — structured JSON for external tool interoperability.

use super::super::database::Database;
use super::super::table::Table;
use super::super::value::{Column, ColumnType, DbValue};

/// Export a table as JSON.
pub(crate) fn export_json(table: &Table) -> String {
    let cols_json: Vec<String> = table
        .columns
        .iter()
        .map(|c| {
            format!(
                r#"{{"name":"{}","type":"{}","primary_key":{},"not_null":{}}}"#,
                c.name, c.dtype, c.primary_key, c.not_null
            )
        })
        .collect();

    let rows_json: Vec<String> = table
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<String> = row.iter().map(|v| v.to_json_string()).collect();
            format!("[{}]", cells.join(","))
        })
        .collect();

    format!(
        r#"{{"name":"{}","columns":[{}],"rows":[{}]}}"#,
        table.name,
        cols_json.join(","),
        rows_json.join(",")
    )
}

/// Export full database as JSON.
#[allow(dead_code, reason = "full DB export not yet exposed as command")]
pub(crate) fn export_json_db(db: &Database) -> String {
    let tables: Vec<String> = db
        .table_names()
        .iter()
        .filter_map(|name| db.get_table(name).ok())
        .map(export_json)
        .collect();
    format!(r#"{{"tables":[{}]}}"#, tables.join(","))
}

/// Import a table from JSON data.
pub(crate) fn import_json(table_name: &str, json_str: &str, db: &mut Database) -> Result<(), String> {
    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;

    let obj = parsed.as_object().ok_or("Expected JSON object")?;

    // Parse columns
    let columns = match obj.get("columns") {
        Some(serde_json::Value::Array(arr)) => {
            let mut cols = Vec::new();
            for col_val in arr {
                let col_obj = col_val.as_object().ok_or("Invalid column definition")?;
                let name = col_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Column missing 'name'")?
                    .to_string();
                let type_str = col_obj.get("type").and_then(|v| v.as_str()).unwrap_or("String");
                let primary_key = col_obj.get("primary_key").and_then(|v| v.as_bool()).unwrap_or(false);
                let dtype = match type_str.to_lowercase().as_str() {
                    "bool" | "boolean" => ColumnType::Bool,
                    "int" | "integer" => ColumnType::Int,
                    "float" | "double" => ColumnType::Float,
                    "string" | "text" => ColumnType::String,
                    "strings" => ColumnType::Strings,
                    "floats" => ColumnType::Floats,
                    _ => ColumnType::String,
                };
                cols.push(Column {
                    name,
                    dtype,
                    primary_key,
                    not_null: obj.get("not_null").and_then(|v| v.as_bool()).unwrap_or(false),
                    default: None,
                    auto_increment: false,
                });
            }
            cols
        }
        _ => return Err("Missing or invalid 'columns' array".into()),
    };

    let mut table = Table::new(table_name.into(), columns).map_err(|e| format!("Schema: {}", e))?;

    // Parse rows
    if let Some(serde_json::Value::Array(rows)) = obj.get("rows") {
        for row_val in rows {
            let cells = row_val.as_array().ok_or("Row must be an array")?;
            let mut db_row = Vec::with_capacity(cells.len());
            for (i, cell) in cells.iter().enumerate() {
                let col_type = &table.columns[i].dtype;
                db_row.push(json_to_dbvalue(cell, col_type));
            }
            table.insert(db_row).map_err(|e| format!("Row insert: {}", e))?;
        }
    }

    db.create_table(table_name, table)
}

fn json_to_dbvalue(v: &serde_json::Value, expected: &ColumnType) -> DbValue {
    match (v, expected) {
        (serde_json::Value::Null, _) => DbValue::Null,
        (serde_json::Value::Bool(b), _) => DbValue::Bool(*b),
        (serde_json::Value::Number(n), ColumnType::Int) => n.as_i64().map(DbValue::Int).unwrap_or(DbValue::Null),
        (serde_json::Value::Number(n), ColumnType::Float) => n.as_f64().map(DbValue::Float).unwrap_or(DbValue::Null),
        (serde_json::Value::Number(n), _) => n
            .as_f64()
            .map(|f| {
                if f.fract() == 0.0 {
                    DbValue::Int(f as i64)
                } else {
                    DbValue::Float(f)
                }
            })
            .unwrap_or(DbValue::Null),
        (serde_json::Value::String(s), ColumnType::Strings) => DbValue::Strings(vec![s.clone()]),
        (serde_json::Value::String(s), _) => DbValue::String(s.clone()),
        (serde_json::Value::Array(arr), ColumnType::Strings) => {
            let strs: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            DbValue::Strings(strs)
        }
        (serde_json::Value::Array(arr), ColumnType::Floats) => {
            let flts: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
            DbValue::Floats(flts)
        }
        (serde_json::Value::Array(arr), _) => {
            // Mixed array — try to parse
            let strs: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            if !strs.is_empty() {
                DbValue::Strings(strs)
            } else {
                DbValue::String(format!("{:?}", arr))
            }
        }
        _ => DbValue::String(format!("{}", v)),
    }
}
