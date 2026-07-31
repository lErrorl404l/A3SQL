// Insert statement handler

//! INSERT execution — single row, multi-row, INSERT FROM SELECT,
//! INSERT OR REPLACE, INSERT OR ROLLBACK, INSERT OR IGNORE,
//! RETURNING clause support.

use super::super::database::Database;
use super::super::error::EngineError;
use super::super::execute::{format_projected_result, LAST_CHANGES, LAST_INSERT_ROWID};
use super::super::functions::eval::{eval_expr, eval_literal_expr, is_truthy};
use super::super::stmts::select::exec_select;
use super::super::value::DbValue;
use super::ddl::object_name_str;
use crate::engine::trigger::{fire_triggers, fire_triggers_before};
use sqlparser::ast::{Expr, Insert, OnInsert, Query, SetExpr, SqliteOnConflict, Value, Values};
use std::collections::HashSet;

pub(crate) fn exec_insert(ins: &Insert, db: &mut Database) -> Result<String, EngineError> {
    let table_name = ins.table.to_string().to_lowercase();
    let returning = ins.returning.clone();

    // Parse source into expression rows — do this BEFORE borrowing table mutably
    // since SetExpr::Select needs a mutable db reference via exec_select.
    let on_conflict = ins.or;
    let is_replace = matches!(on_conflict, Some(SqliteOnConflict::Replace)) || ins.replace_into;
    let rows: Vec<Vec<Expr>> = match &ins.source {
        Some(q) => match &*q.as_ref().body {
            SetExpr::Values(Values { rows, .. }) => rows.iter().map(|parens| parens.content.clone()).collect(),
            SetExpr::Select(s) => {
                let sq = Query {
                    with: None,
                    body: Box::new(SetExpr::Select(s.clone())),
                    order_by: None,
                    limit_clause: None,
                    fetch: None,
                    locks: vec![],
                    for_clause: None,
                    settings: None,
                    format_clause: None,
                    pipe_operators: vec![],
                };
                let json = exec_select(&sq, db)?;
                let parsed: Vec<Vec<serde_json::Value>> =
                    serde_json::from_str(&json).map_err(|e| EngineError::Exec(format!("SELECT parse: {}", e)))?;
                let to_val = |v: &serde_json::Value| -> Expr {
                    Expr::Value(
                        match v {
                            serde_json::Value::String(s) => Value::SingleQuotedString(s.clone()),
                            serde_json::Value::Number(n) => Value::Number(
                                n.as_i64()
                                    .map_or_else(|| format!("{}", n.as_f64().unwrap_or(0.0)), |i| format!("{}", i)),
                                false,
                            ),
                            serde_json::Value::Bool(b) => Value::Boolean(*b),
                            serde_json::Value::Null => Value::Null,
                            other => Value::SingleQuotedString(format!("{}", other)),
                        }
                        .into(),
                    )
                };
                parsed
                    .iter()
                    .skip(1)
                    .map(|row| row.iter().map(to_val).collect())
                    .collect()
            }
            _ => return Err(EngineError::Exec("INSERT source must be VALUES or SELECT".into())),
        },
        None => return Err(EngineError::Exec("INSERT must have a source".into())),
    };

    // Pre-collect FOREIGN KEY lookup data (foreign table pk_sets) before mutable table borrow
    let fk_lookups: Vec<(String, HashSet<String>)> = {
        let self_table = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        self_table
            .foreign_keys
            .iter()
            .filter_map(|fk| {
                db.get_table(&fk.foreign_table)
                    .ok()
                    .map(|t| (fk.local_column.clone(), t.pk_set.clone()))
            })
            .collect()
    };

    // Fire BEFORE INSERT triggers (aborts if trigger raises error)
    if let Err(e) = fire_triggers_before(&table_name, "INSERT", db, &[], &[]) {
        return Err(EngineError::Exec(e));
    }

    // Now we have the rows; borrow table for column mapping and insertion
    let table = db
        .get_table_mut(&table_name)
        .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;

    // Pre-collect auto_increment indices before the row loop (avoids borrow conflicts)
    let auto_inc_cols: Vec<usize> = table
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.auto_increment)
        .map(|(i, _)| i)
        .collect();

    let explicit_cols: Option<Vec<usize>> = if !ins.columns.is_empty() {
        Some(
            ins.columns
                .iter()
                .map(|col_name| {
                    let name = object_name_str(col_name);
                    table.col_idx(&name).ok_or_else(|| EngineError::ColumnNotFoundInTable {
                        name: name.clone(),
                        table: table_name.clone(),
                    })
                })
                .collect::<Result<Vec<usize>, EngineError>>()?,
        )
    } else {
        None
    };

    let mut inserted = 0usize;
    let mut inserted_rows: Vec<Vec<DbValue>> = Vec::new();
    for row_exprs in &rows {
        let col_indices: &[usize] = match &explicit_cols {
            Some(indices) => indices.as_slice(),
            None => &(0..table.col_count()).collect::<Vec<_>>(),
        };

        if row_exprs.len() != col_indices.len() {
            return Err(EngineError::Exec(format!(
                "Expected {} values, got {}",
                col_indices.len(),
                row_exprs.len()
            )));
        }

        let mut full_row: Vec<DbValue> = (0..table.col_count()).map(|_| DbValue::Null).collect();
        for (j, expr) in row_exprs.iter().enumerate() {
            let col_idx = col_indices[j];
            full_row[col_idx] = eval_literal_expr(expr)?;
        }

        // Apply AUTO_INCREMENT: fill NULL auto-inc columns with next sequence value
        for ci in &auto_inc_cols {
            if matches!(full_row[*ci], DbValue::Null) {
                full_row[*ci] = DbValue::Int(table.next_auto_inc);
                table.next_auto_inc += 1;
            }
        }

        // Evaluate CHECK constraints
        for check_expr in &table.check_constraints {
            let check_val = eval_expr(check_expr, &full_row, &table.col_index)
                .map_err(|e| EngineError::Exec(format!("CHECK constraint error: {}", e)))?;
            if !is_truthy(&check_val) {
                return Err(EngineError::Exec("CHECK constraint failed for row".to_string()));
            }
        }

        // Validate FOREIGN KEY constraints using pre-collected pk_sets
        for (local_col, ref_pks) in &fk_lookups {
            if let Some(&col_idx) = table.col_index.get(local_col) {
                let val = &full_row[col_idx];
                if !matches!(val, DbValue::Null) {
                    let pk_str = val.to_string().to_lowercase();
                    if !ref_pks.contains(&pk_str) {
                        return Err(EngineError::Exec(format!(
                            "FOREIGN KEY constraint: '{}' value '{}' not found in referenced table",
                            local_col, pk_str
                        )));
                    }
                }
            }
        }

        let result = table.insert(full_row.clone());
        match result {
            Ok(()) => {
                inserted += 1;
                inserted_rows.push(full_row);
            }
            // INSERT OR IGNORE: skip conflicting row silently
            // INSERT OR IGNORE / INSERT IGNORE: skip conflicting row silently
            Err(ref e)
                if (matches!(on_conflict, Some(SqliteOnConflict::Ignore)) || ins.ignore)
                    && matches!(e, EngineError::DuplicateKey(_)) =>
            {
                // ponytail: silently skip — row already exists
            }
            // INSERT OR ROLLBACK: rollback transaction on conflict
            Err(e)
                if matches!(on_conflict, Some(SqliteOnConflict::Rollback))
                    && matches!(e, EngineError::DuplicateKey(_)) =>
            {
                let _ = db.rollback();
                return Err(e);
            }
            // INSERT OR REPLACE / INSERT OR REPLACE: delete and re-insert
            Err(ref e) if is_replace && matches!(e, EngineError::DuplicateKey(_)) => {
                // REPLACE: overwrite the existing row in place (O(1))
                table.replace_by_pk(full_row.clone())?;
                inserted_rows.push(full_row);
                inserted += 1;
            }
            Err(ref e) if matches!(e, EngineError::DuplicateKey(_)) && ins.on.is_some() => {
                // UPSERT: ON CONFLICT DO UPDATE or ON CONFLICT DO NOTHING
                match &ins.on {
                    Some(OnInsert::OnConflict(oc)) => {
                        match &oc.action {
                            sqlparser::ast::OnConflictAction::DoNothing => {
                                // Skip conflicting row silently
                            }
                            sqlparser::ast::OnConflictAction::DoUpdate(du) => {
                                // Apply SET assignments to the existing row
                                if let Some(pk_col) = table.columns.iter().position(|c| c.primary_key) {
                                    let pk_val = &full_row[pk_col];
                                    if let Some(row_idx) = table.find_by_pk(pk_val) {
                                        for assign in &du.assignments {
                                            if let sqlparser::ast::AssignmentTarget::ColumnName(name) = &assign.target {
                                                let col_name = name.to_string().to_lowercase();
                                                if let Some(&ci) = table.col_index.get(&col_name) {
                                                    // Build col_map with EXCLUDED pseudo-columns
                                                    let mut upsert_map = table.col_index.clone();
                                                    for (col, &idx) in &table.col_index {
                                                        upsert_map.insert(format!("excluded.{}", col), idx);
                                                    }
                                                    // Use full_row (the proposed new values) for EXCLUDED references
                                                    let upsert_row = &full_row;
                                                    let new_val = eval_expr(&assign.value, upsert_row, &upsert_map)
                                                        .map_err(|_| {
                                                            EngineError::Exec(format!(
                                                                "UPSERT: invalid expr for '{}'",
                                                                col_name
                                                            ))
                                                        })?;
                                                    // update_cell maintains pk_set,
                                                    // pk_row_index, unique_set, and
                                                    // secondary indices for this column
                                                    table.update_cell(row_idx, ci, new_val);
                                                }
                                            }
                                        }
                                        inserted += 1;
                                    }
                                }
                            }
                        }
                    }
                    _ => return Err(EngineError::DuplicateKey(e.to_string())),
                }
            }
            Err(e) => return Err(EngineError::Exec(e.to_string())),
        }
    }

    // Track last_insert_rowid and changes
    let last_pk = db.get_table(&table_name).ok().and_then(|t| {
        let pk_idx = t.columns.iter().position(|c| c.primary_key)?;
        inserted_rows
            .last()
            .and_then(|row| row.get(pk_idx))
            .map(|v| v.to_string())
    });
    db.last_insert_rowid = last_pk;
    db.last_changes = inserted;
    LAST_INSERT_ROWID.with(|r| *r.borrow_mut() = db.last_insert_rowid.clone());
    LAST_CHANGES.with(|c| *c.borrow_mut() = inserted);

    if let Some(returning) = &returning {
        let table = db
            .get_table(&table_name)
            .map_err(|_| EngineError::TableNotFound(table_name.clone()))?;
        let ref_rows: Vec<&[DbValue]> = inserted_rows.iter().map(|r| r.as_slice()).collect();
        return Ok(format_projected_result(ref_rows, returning, &table.col_index, table));
    }

    let last_new = inserted_rows.last().map(|r| r.as_slice()).unwrap_or(&[]);
    fire_triggers(&table_name, "INSERT", db, last_new, &[]);
    Ok(format!("\"Inserted {} row(s)\"", inserted))
}
