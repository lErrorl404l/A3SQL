// DDL statements: CREATE, DROP, ALTER, SHOW

use super::super::database::Database;
use sqlparser::ast::{ObjectName, ObjectNamePart, ShowCreateObject, ShowStatementOptions};

fn object_name_str(name: &ObjectName) -> String {
    name.0
        .iter()
        .filter_map(|part| match part {
            ObjectNamePart::Identifier(i) => Some(i.value.to_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// SHOW COLUMNS FROM table
pub(crate) fn exec_show_columns(show_options: &ShowStatementOptions, db: &Database) -> Result<String, String> {
    let table_name = show_options
        .show_in
        .as_ref()
        .and_then(|si| si.parent_name.as_ref())
        .map(|n| object_name_str(n))
        .ok_or_else(|| "SHOW COLUMNS requires FROM table".to_string())?;

    let table = db.get_table(&table_name)?;
    let cols: Vec<String> = table
        .columns
        .iter()
        .map(|c| {
            let nullable = if c.not_null { "NO" } else { "YES" };
            let pk = if c.primary_key { ", PRIMARY_KEY" } else { "" };
            format!("\"{},{}{}\"", c.name, nullable, pk)
        })
        .collect();
    Ok(format!("[{}]", cols.join(",")))
}

/// SHOW CREATE TABLE
pub(crate) fn exec_show_create(
    obj_type: &ShowCreateObject,
    obj_name: &ObjectName,
    db: &Database,
) -> Result<String, String> {
    let name = object_name_str(obj_name);
    match obj_type {
        ShowCreateObject::Table => {
            let table = db.get_table(&name)?;
            let cols: Vec<String> = table
                .columns
                .iter()
                .map(|c| {
                    let pk = if c.primary_key { " PRIMARY KEY" } else { "" };
                    let nn = if c.not_null { " NOT NULL" } else { "" };
                    let def = c
                        .default
                        .as_ref()
                        .map(|d| format!(" DEFAULT {}", d))
                        .unwrap_or_default();
                    format!("\"{}{}{}{}\"", c.name, pk, nn, def)
                })
                .collect();
            Ok(format!("\"CREATE TABLE {} ( {} )\"", name, cols.join(", ")))
        }
        _ => Err("SHOW CREATE only supports TABLE".into()),
    }
}

/// DROP TRIGGER name
pub(crate) fn exec_drop_trigger(
    trigger_name: &ObjectName,
    table_name: Option<&ObjectName>,
    db: &mut Database,
) -> Result<String, String> {
    let name = object_name_str(trigger_name);
    let table_names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    if let Some(tn) = table_name {
        let tname = object_name_str(tn);
        if let Ok(table) = db.get_table_mut(&tname) {
            let len_before = table.triggers.len();
            table.triggers.retain(|t| t.name != name);
            if table.triggers.len() < len_before {
                return Ok(format!("\"Trigger '{}' dropped\"", name));
            }
        }
        Err(format!("Trigger '{}' not found on '{}'", name, tname))
    } else {
        for tn in table_names {
            if let Ok(table) = db.get_table_mut(&tn) {
                if table.triggers.iter().any(|t| t.name == name) {
                    table.triggers.retain(|t| t.name != name);
                    return Ok(format!("\"Trigger '{}' dropped\"", name));
                }
            }
        }
        Err(format!("Trigger '{}' not found", name))
    }
}
