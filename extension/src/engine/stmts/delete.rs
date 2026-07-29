//! DELETE execution — WHERE filter, RETURNING clause support.

use std::collections::HashSet;

use sqlparser::ast::{Delete, FromTable, ReferentialAction, TableFactor};

use super::super::database::Database;
use super::super::error::EngineError;
use super::super::execute::{format_projected_result, LAST_CHANGES};
use super::super::functions::eval::{eval_expr, is_truthy};
use super::super::value::DbValue;
use super::ddl::object_name_str;
use crate::engine::trigger::{fire_triggers, fire_triggers_before};

pub(crate) fn exec_delete(del: &Delete, db: &mut Database) -> Result<String, EngineError> {
    let table_name = match &del.from {
        FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => match tables.first() {
            Some(tj) => match &tj.relation {
                TableFactor::Table { name, .. } => object_name_str(name),
                _ => {
                    return Err(EngineError::Exec(
                        "DELETE: only simple table references supported".into(),
                    ))
                }
            },
            None => return Err(EngineError::Exec("DELETE must specify a table".into())),
        },
    };

    let returning = del.returning.clone();

    // Clone col_index to avoid borrow conflict with table.delete()
    let col_idx = db
        .get_table(&table_name)
        .map_err(|_| EngineError::TableNotFound(table_name.clone()))?
        .col_index
        .clone();
    let pred = del.selection.clone();

    // Collect referencing tables with FK pointing to this table
    let fk_refs: Vec<(String, usize)> = {
        let names: Vec<&str> = db.table_names();
        let mut refs = Vec::new();
        for tn in names {
            if let Ok(t) = db.get_table(tn) {
                if tn == table_name {
                    continue;
                }
                for fk in &t.foreign_keys {
                    if fk.foreign_table == table_name {
                        if let Some(&ci) = t.col_index.get(&fk.local_column) {
                            refs.push((tn.to_string(), ci));
                        }
                    }
                }
            }
        }
        refs
    };

    // Collect PK values of rows matched for deletion (needed for FK validation)
    let deleted_pks: Vec<(usize, String)> = {
        let pk_idx = db.get_table(&table_name)?.columns.iter().position(|c| c.primary_key);
        match pk_idx {
            Some(pi) => {
                let t = db.get_table(&table_name)?;
                match &pred {
                    Some(expr) => t
                        .rows
                        .iter()
                        .enumerate()
                        .filter(|(_, row)| eval_expr(expr, row, &col_idx).map(|v| is_truthy(&v)).unwrap_or(false))
                        .map(|(i, row)| (i, row[pi].to_string().to_lowercase()))
                        .collect(),
                    None => t
                        .rows
                        .iter()
                        .enumerate()
                        .map(|(i, row)| (i, row[pi].to_string().to_lowercase()))
                        .collect(),
                }
            }
            None => Vec::new(),
        }
    };

    // Validate FK before deleting — CASCADE, SET NULL, or RESTRICT
    for (ref_table, ref_col_idx) in &fk_refs {
        let fk_info = {
            let t = db.get_table(ref_table)?;
            t.foreign_keys
                .iter()
                .find(|fk| fk.foreign_table == table_name && t.col_index.get(&fk.local_column) == Some(ref_col_idx))
                .cloned()
        };
        let on_delete = fk_info.as_ref().and_then(|f| f.on_delete);

        match on_delete {
            Some(ReferentialAction::Cascade) => {
                // Delete child rows referencing the deleted PKs
                let child_pks_to_delete: Vec<String> = {
                    let t = db
                        .get_table(ref_table)
                        .map_err(|_| EngineError::TableNotFound(ref_table.clone()))?;
                    let pk_idx = t.columns.iter().position(|c| c.primary_key);
                    t.rows
                        .iter()
                        .filter(|row| {
                            let child_val = row[*ref_col_idx].to_string().to_lowercase();
                            deleted_pks.iter().any(|(_, pk)| *pk == child_val)
                        })
                        .map(|row| pk_idx.map(|pi| row[pi].to_string().to_lowercase()).unwrap_or_default())
                        .collect()
                };
                for child_pk in &child_pks_to_delete {
                    if child_pk.is_empty() {
                        continue;
                    }
                    let child_table = db
                        .get_table_mut(ref_table)
                        .map_err(|_| EngineError::TableNotFound(ref_table.clone()))?;
                    let pk_idx = child_table.columns.iter().position(|c| c.primary_key);
                    if let Some(pi) = pk_idx {
                        child_table.delete(|row| row[pi].to_string().to_lowercase() == *child_pk);
                    }
                }
            }
            Some(ReferentialAction::SetNull) => {
                // Set FK column to NULL in child rows
                if let Ok(child_table) = db.get_table_mut(ref_table) {
                    for row in &mut child_table.rows {
                        let child_val = row[*ref_col_idx].to_string().to_lowercase();
                        if deleted_pks.iter().any(|(_, pk)| *pk == child_val) {
                            row[*ref_col_idx] = DbValue::Null;
                        }
                    }
                }
            }
            _ => {
                // RESTRICT / NO ACTION / None — reject
                let t = db
                    .get_table(ref_table)
                    .map_err(|_| EngineError::TableNotFound(ref_table.clone()))?;
                let ref_pk_set: HashSet<String> = t
                    .rows
                    .iter()
                    .map(|row| row[*ref_col_idx].to_string().to_lowercase())
                    .collect();
                for (_, pk_val) in &deleted_pks {
                    if ref_pk_set.contains(pk_val) {
                        return Err(EngineError::Exec(format!(
                            "FOREIGN KEY constraint violation: '{}' references '{}'",
                            ref_table, pk_val
                        )));
                    }
                }
            }
        }
    }

    // Capture old rows for RETURNING (before deletion)
    let old_rows: Vec<Vec<DbValue>> = if returning.is_some() {
        let t = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        match &pred {
            Some(expr) => t
                .rows
                .iter()
                .filter(|row| eval_expr(expr, row, &col_idx).map(|v| is_truthy(&v)).unwrap_or(false))
                .cloned()
                .collect(),
            None => t.rows.to_vec(),
        }
    } else {
        Vec::new()
    };

    let _table_name_c = table_name.clone();
    let col_idx = {
        let table = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        table.col_index.clone()
    };

    // Fire BEFORE DELETE triggers (aborts if trigger returns error)
    if let Err(e) = fire_triggers_before(&table_name, "DELETE", db, &[], &[]) {
        return Err(EngineError::Exec(e));
    }

    let table = db
        .get_table_mut(&table_name)
        .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
    let count = match pred {
        Some(expr) => table.delete(|row| eval_expr(&expr, row, &col_idx).map(|v| is_truthy(&v)).unwrap_or(false)),
        None => table.delete(|_| true),
    };
    let _ = table;
    db.last_changes = count;
    LAST_CHANGES.with(|c| *c.borrow_mut() = count);

    if let Some(returning) = &returning {
        let table = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        let ref_rows: Vec<&[DbValue]> = old_rows.iter().map(|r| r.as_slice()).collect();
        return Ok(format_projected_result(ref_rows, returning, &table.col_index, table));
    }

    fire_triggers(
        &table_name,
        "DELETE",
        db,
        &[],
        &old_rows.last().map(|r| r.as_slice()).unwrap_or(&[]),
    );
    Ok(format!("\"Deleted {} row(s)\"", count))
}
