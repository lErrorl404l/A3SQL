// Miscellaneous DDL/DML statements: MERGE, VACUUM, COPY, COMMENT, CALL, ANALYZE, SHOW COLUMNS, SHOW CREATE, DROP TRIGGER

//! Miscellaneous DDL/DML — VACUUM, COPY (TO/FROM stdin), COMMENT ON, CALL, ANALYZE.

use super::object_name_str;
use crate::engine::database::Database;
use crate::engine::error::EngineError;
use sqlparser::ast::{
    Analyze, CopySource, CopyTarget, Function, ObjectName, ShowCreateObject, ShowStatementOptions, VacuumStatement,
};

// ── SHOW COLUMNS ────────────────────────────────────────────────────────

pub(crate) fn exec_show_columns(so: &ShowStatementOptions, db: &Database) -> Result<String, EngineError> {
    let tn = so
        .show_in
        .as_ref()
        .and_then(|si| si.parent_name.as_ref())
        .map(object_name_str)
        .ok_or_else(|| EngineError::Exec("SHOW COLUMNS requires FROM".to_string()))?;
    let t = db.get_table(&tn).map_err(|_| EngineError::TableNotFound(tn.clone()))?;
    let cols: Vec<String> = t
        .columns
        .iter()
        .map(|c| {
            let nn = if c.not_null { "NO" } else { "YES" };
            let pk = if c.primary_key { ",PK" } else { "" };
            format!("\"{},{}{}\"", c.name, nn, pk)
        })
        .collect();
    Ok(format!("[{}]", cols.join(",")))
}

// ── SHOW CREATE ─────────────────────────────────────────────────────────

pub(crate) fn exec_show_create(ot: &ShowCreateObject, on: &ObjectName, db: &Database) -> Result<String, EngineError> {
    let name = object_name_str(on);
    match ot {
        ShowCreateObject::Table => {
            let t = db
                .get_table(&name)
                .map_err(|_| EngineError::TableNotFound(name.clone()))?;
            let cols: Vec<String> = t
                .columns
                .iter()
                .map(|c| {
                    let pk = if c.primary_key { " PRIMARY KEY" } else { "" };
                    let nn = if c.not_null { " NOT NULL" } else { "" };
                    format!("\"{}{}{}\"", c.name, pk, nn)
                })
                .collect();
            Ok(format!("\"CREATE TABLE {} ( {} )\"", name, cols.join(", ")))
        }
        _ => Err(EngineError::Exec("SHOW CREATE only supports TABLE".into())),
    }
}

// ── DROP TRIGGER ────────────────────────────────────────────────────────

pub(crate) fn exec_drop_trigger(
    tn: &ObjectName,
    table: Option<&ObjectName>,
    db: &mut Database,
) -> Result<String, EngineError> {
    let name = object_name_str(tn);
    let names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    let target_names = if let Some(t) = table {
        vec![object_name_str(t)]
    } else {
        names
    };
    for tn2 in &target_names {
        if let Ok(t) = db.get_table_mut(tn2)
            && t.triggers.iter().any(|tr| tr.name == name)
        {
            t.triggers.retain(|tr| tr.name != name);
            return Ok(format!("\"Trigger '{}' dropped\"", name));
        }
    }
    Err(EngineError::Exec(format!("Trigger '{}' not found", name)))
}

// ── VACUUM ──────────────────────────────────────────────────────────────

pub(crate) fn exec_vacuum(v: &VacuumStatement, db: &mut Database) -> Result<String, EngineError> {
    let tables: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    for tn in tables {
        if let Ok(t) = db.get_table_mut(&tn) {
            t.rebuild_index();
        }
    }
    if v.reindex {
        Ok("\"REINDEX complete\"".into())
    } else {
        Ok("\"VACUUM complete\"".into())
    }
}

// ── COPY ────────────────────────────────────────────────────────────────

pub(crate) fn exec_copy(
    source: &CopySource,
    to: bool,
    target: &CopyTarget,
    db: &mut Database,
) -> Result<String, EngineError> {
    let table_name = match source {
        CopySource::Table { table_name, .. } => object_name_str(table_name),
        _ => return Err(EngineError::Exec("COPY only supports table source".into())),
    };

    if to {
        let t = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        Ok(format!("\"COPY: {} rows\"", t.row_count()))
    } else if matches!(target, CopyTarget::Stdin) {
        let data = crate::engine::execute::COPY_STDIN
            .with(|s| s.borrow_mut().take())
            .unwrap_or_default();
        if data.is_empty() {
            return Err(EngineError::Exec("COPY FROM stdin: no data provided".into()));
        }
        let table = db
            .get_table_mut(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        let mut count = 0usize;
        for line in data.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let values: Vec<String> = if trimmed.starts_with('{') || trimmed.starts_with('[') {
                // JSON array row
                return Err(EngineError::Exec(
                    "COPY FROM stdin: JSON format not yet supported".into(),
                ));
            } else {
                // CSV-like: comma-separated values (with optional quoting)
                let mut fields = Vec::new();
                let mut current = String::new();
                let mut in_quote = false;
                for c in trimmed.chars() {
                    match c {
                        '"' => in_quote = !in_quote,
                        ',' if !in_quote => {
                            fields.push(std::mem::take(&mut current));
                        }
                        _ => current.push(c),
                    }
                }
                fields.push(current);
                fields
            };
            if values.len() != table.col_count() {
                return Err(EngineError::Exec(format!(
                    "COPY FROM stdin: expected {} columns, got {}",
                    table.col_count(),
                    values.len()
                )));
            }
            let row: Vec<crate::engine::value::DbValue> = values
                .iter()
                .zip(table.columns.iter())
                .map(|(v, col)| -> Result<crate::engine::value::DbValue, EngineError> {
                    let trimmed = v.trim();
                    if trimmed.is_empty() || trimmed == "NULL" || trimmed == "null" {
                        return Ok(crate::engine::value::DbValue::Null);
                    }
                    match col.dtype {
                        crate::engine::value::ColumnType::Int => trimmed
                            .parse::<i64>()
                            .map(crate::engine::value::DbValue::Int)
                            .map_err(|_| EngineError::Exec(format!("COPY FROM stdin: invalid integer '{}'", trimmed))),
                        crate::engine::value::ColumnType::Float => trimmed
                            .parse::<f64>()
                            .map(crate::engine::value::DbValue::Float)
                            .map_err(|_| EngineError::Exec(format!("COPY FROM stdin: invalid float '{}'", trimmed))),
                        crate::engine::value::ColumnType::Bool => Ok(match trimmed.to_lowercase().as_str() {
                            "true" | "1" => crate::engine::value::DbValue::Bool(true),
                            _ => crate::engine::value::DbValue::Bool(false),
                        }),
                        _ => Ok(crate::engine::value::DbValue::String(trimmed.to_string())),
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            table
                .insert(row)
                .map_err(|e| EngineError::Exec(format!("COPY FROM stdin: {}", e)))?;
            count += 1;
        }
        Ok(format!("\"COPY FROM: {} rows inserted\"", count))
    } else {
        Err(EngineError::Exec("COPY only supports TO or FROM stdin".into()))
    }
}

// ── COMMENT ON ──────────────────────────────────────────────────────────

pub(crate) fn exec_comment_on(
    _ot: &str,
    on: &ObjectName,
    comment: Option<&str>,
    db: &mut Database,
) -> Result<String, EngineError> {
    db.set_config(&format!("comment_{}", object_name_str(on)), comment.unwrap_or(""));
    Ok("\"COMMENT (stored)\"".into())
}

// ── CALL ────────────────────────────────────────────────────────────────

pub(crate) fn exec_call(func: &Function, _db: &mut Database) -> Result<String, EngineError> {
    let empty = Vec::new();
    let empty_map = std::collections::HashMap::new();
    match crate::engine::functions::eval::exec_function(func, &empty, &empty_map) {
        Ok(val) => Ok(format!("\"CALL returned: {}\"", val)),
        Err(e) => Err(EngineError::Exec(format!("CALL error: {}", e))),
    }
}

// ── ANALYZE ─────────────────────────────────────────────────────────────

pub(crate) fn exec_analyze(a: &Analyze, db: &mut Database) -> Result<String, EngineError> {
    let names: Vec<String> = if let Some(tn) = &a.table_name {
        vec![object_name_str(tn)]
    } else {
        db.table_names().iter().map(|s| s.to_string()).collect()
    };
    for tn in names {
        let (rc, cc) = if let Ok(t) = db.get_table(&tn) {
            (t.row_count(), t.col_count())
        } else {
            continue;
        };
        db.set_config(&format!("stat_rows_{}", tn), &rc.to_string());
        db.set_config(&format!("stat_cols_{}", tn), &cc.to_string());
    }
    Ok("\"ANALYZE complete\"".into())
}
