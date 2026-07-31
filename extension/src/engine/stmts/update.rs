//! UPDATE execution — SET column = expr, WHERE filter,
//! RETURNING clause support.

use std::collections::HashSet;

use super::super::database::Database;
use super::super::error::EngineError;
use super::super::execute::{format_projected_result, LAST_CHANGES};
use super::super::functions::eval::{eval_expr, is_truthy};
use super::super::stmts::select::joins::resolve_table_from_joins;
use super::super::value::DbValue;
use crate::engine::trigger::{fire_triggers, fire_triggers_before};
use sqlparser::ast::{Expr, ReferentialAction, Update};

pub(crate) fn exec_update(upd: &Update, db: &mut Database) -> Result<String, EngineError> {
    let table_name = resolve_table_from_joins(&upd.table)?;
    let returning = upd.returning.clone();
    let table = db
        .get_table_mut(&table_name)
        .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;

    let where_expr = upd.selection.as_ref();

    // Collect row indices to update
    let indices: Vec<usize> = {
        let mut idxs = Vec::new();
        for (i, row) in table.rows.iter().enumerate() {
            let matches = match where_expr {
                Some(expr) => is_truthy(&eval_expr(expr, row, &table.col_index)?),
                None => true,
            };
            if matches {
                idxs.push(i);
            }
        }
        idxs
    };

    // Pre-resolve column indices to avoid borrow conflict
    let assign_indices: Vec<(usize, &Expr)> = upd
        .assignments
        .iter()
        .map(|assign| {
            let col_name = assign.target.to_string().to_lowercase();
            let idx = table
                .col_idx(&col_name)
                .ok_or_else(|| EngineError::ColumnNotFoundInTable {
                    name: col_name.clone(),
                    table: table_name.clone(),
                })?;
            Ok((idx, &assign.value))
        })
        .collect::<Result<Vec<_>, EngineError>>()?;

    // Collect all assignments and validate CHECK constraints upfront
    let mut updates: Vec<(usize, usize, DbValue)> = Vec::new();

    for i in &indices {
        for (col_ci, val_expr) in &assign_indices {
            // Evaluate with row context so `SET bans = bans + 1` works (the
            // assignment can reference the current row's column values).
            let row = &table.rows[*i];
            let new_val = eval_expr(val_expr, row, &table.col_index)?;
            updates.push((*i, *col_ci, new_val));
        }
    }

    // Validate CHECK constraints for each updated row (drop table borrow first)
    let validated_updates = {
        let t = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        updates
            .iter()
            .filter_map(|(row_i, col_ci, val)| {
                let mut row = t.rows[*row_i].clone();
                row[*col_ci] = val.clone();
                for expr in &t.check_constraints {
                    let v = eval_expr(expr, &row, &t.col_index).ok()?;
                    if !is_truthy(&v) {
                        return Some(Err(EngineError::Exec("CHECK constraint failed".into())));
                    }
                }
                Some(Ok((*row_i, *col_ci, val.clone())))
            })
            .collect::<Result<Vec<(usize, usize, DbValue)>, EngineError>>()
    }?;

    // Validate FOREIGN KEY constraints for each update
    // Pre-collect PK update cascade data (must outlive the immutable borrow block)
    let mut pk_cascade_queue: Vec<(String, usize, String, DbValue)> = Vec::new();
    {
        let t = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        // Pre-collect FK lookup data: local_column → (foreign_table, pk_set)
        let fk_lookups: Vec<(String, HashSet<String>)> = t
            .foreign_keys
            .iter()
            .filter_map(|fk| {
                db.get_table(&fk.foreign_table)
                    .ok()
                    .map(|ref_t| (fk.local_column.clone(), ref_t.pk_set.clone()))
            })
            .collect();
        // Pre-collect referencing tables for PK updates (RESTRICT)
        let pk_col_idx = t.columns.iter().position(|c| c.primary_key);
        let fk_refs: Vec<(String, usize)> = if pk_col_idx.is_some() {
            let names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
            let mut refs = Vec::new();
            for tn in &names {
                if tn == &table_name {
                    continue;
                }
                if let Ok(child) = db.get_table(tn) {
                    for fk in &child.foreign_keys {
                        if fk.foreign_table == table_name {
                            if let Some(&ci) = child.col_index.get(&fk.local_column) {
                                refs.push((tn.to_string(), ci));
                            }
                        }
                    }
                }
            }
            refs
        } else {
            Vec::new()
        };

        for (row_i, col_ci, val) in &validated_updates {
            // (a) FK local column update: new value must exist in referenced table
            for (local_col, ref_pks) in &fk_lookups {
                if let Some(&local_ci) = t.col_index.get(local_col) {
                    if *col_ci == local_ci && !matches!(val, DbValue::Null) {
                        let val_str = val.to_string().to_lowercase();
                        if !ref_pks.contains(&val_str) {
                            return Err(EngineError::Exec(format!(
                                "FOREIGN KEY constraint: '{}' value '{}' not found in referenced table",
                                local_col, val_str
                            )));
                        }
                    }
                }
            }
            // (b) PK column update: collect CASCADE info during immutable phase
            if let Some(pk_ci) = pk_col_idx {
                if *col_ci == pk_ci {
                    let old_val = t.rows[*row_i][pk_ci].to_string().to_lowercase();
                    let new_val_cascade = val.clone();
                    // (use outer pk_cascade_queue)
                    for (ref_table, ref_col_idx) in &fk_refs {
                        let child = db
                            .get_table(ref_table)
                            .map_err(|_| EngineError::TableNotFound(ref_table.clone()))?;
                        let on_update = child
                            .foreign_keys
                            .iter()
                            .find(|fk| {
                                fk.foreign_table == table_name
                                    && child.col_index.get(&fk.local_column) == Some(ref_col_idx)
                            })
                            .and_then(|fk| fk.on_update);
                        let has_ref = child
                            .rows
                            .iter()
                            .any(|r| r[*ref_col_idx].to_string().to_lowercase() == old_val);
                        if has_ref {
                            match on_update {
                                Some(ReferentialAction::Cascade) => {
                                    pk_cascade_queue.push((
                                        ref_table.clone(),
                                        *ref_col_idx,
                                        old_val.clone(),
                                        new_val_cascade.clone(),
                                    ));
                                }
                                _ => {
                                    return Err(EngineError::Exec(format!(
                                        "FOREIGN KEY constraint violation: '{}' has reference to '{}' in '{}'",
                                        ref_table, old_val, table_name
                                    )));
                                }
                            }
                        }
                    }
                    // Apply CASCADE after immutable block (see note after block close)
                }
            }
        }
    }

    // Apply PK CASCADE updates to child rows (after immutable borrow is dropped)
    for (ref_table, ref_col_idx, old_val, new_val) in &pk_cascade_queue {
        if let Ok(child) = db.get_table_mut(ref_table) {
            for row in &mut child.rows {
                let child_val = row[*ref_col_idx].to_string().to_lowercase();
                if child_val == *old_val {
                    row[*ref_col_idx] = new_val.clone();
                }
            }
        }
    }

    // Capture old rows for RETURNING (before applying updates)
    let old_rows: Vec<Vec<DbValue>> = if returning.is_some() {
        let t = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        let mut seen = HashSet::new();
        validated_updates
            .iter()
            .filter(|(row_i, _, _)| seen.insert(*row_i))
            .map(|(row_i, _, _)| t.rows[*row_i].clone())
            .collect()
    } else {
        Vec::new()
    };

    {
        let _t = db
            .get_table_mut(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        // Fire BEFORE UPDATE triggers (drop mutable borrow first)
    };

    if let Err(e) = fire_triggers_before(&table_name, "UPDATE", db, &[], &[]) {
        return Err(EngineError::Exec(e));
    }

    let t = db
        .get_table_mut(&table_name)
        .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
    for (row_i, col_ci, val) in &validated_updates {
        t.update_cell(*row_i, *col_ci, val.clone());
    }
    let _ = t;
    db.last_changes = validated_updates.len();
    LAST_CHANGES.with(|c| *c.borrow_mut() = validated_updates.len());

    if let Some(returning) = &returning {
        let table = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        let ref_rows: Vec<&[DbValue]> = old_rows.iter().map(|r| r.as_slice()).collect();
        return Ok(format_projected_result(ref_rows, returning, &table.col_index, table));
    }

    fire_triggers(
        &table_name,
        "UPDATE",
        db,
        &[],
        old_rows.last().map(|r| r.as_slice()).unwrap_or(&[]),
    );
    Ok(format!("\"Updated {} row(s)\"", validated_updates.len()))
}
