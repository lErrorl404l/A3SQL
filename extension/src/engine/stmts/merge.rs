// MERGE statement execution
//
// Supports RETURNING clause via MSSQL-style OUTPUT / PostgreSQL-style RETURNING
// (sqlparser normalises both into OutputClause on the Merge struct).

//! MERGE (UPSERT) execution — WHEN MATCHED / WHEN NOT MATCHED branches,
//! RETURNING clause support.

use sqlparser::ast::{Merge, MergeAction, MergeClauseKind, MergeUpdateExpr, OutputClause};

use crate::engine::database::Database;
use crate::engine::error::EngineError;
use crate::engine::execute::format_projected_result;
use crate::engine::value::DbValue;

pub(crate) fn exec_merge(merge: &Merge, db: &mut Database) -> Result<String, EngineError> {
    let target = match &merge.table {
        sqlparser::ast::TableFactor::Table { name, .. } => Some(super::ddl::object_name_str(name)),
        _ => None,
    }
    .ok_or_else(|| EngineError::Exec("MERGE: no target".to_string()))?;
    let source = match &merge.source {
        sqlparser::ast::TableFactor::Table { name, .. } => Some(super::ddl::object_name_str(name)),
        _ => None,
    }
    .ok_or_else(|| EngineError::Exec("MERGE: no source".to_string()))?;

    let return_select = match &merge.output {
        Some(OutputClause::Returning { select_items, .. }) => Some(select_items.clone()),
        _ => None,
    };

    let src = db
        .get_table(&source)
        .map_err(|_| EngineError::TableNotFound(source.clone()))?;
    let src_rows = src.rows.clone();
    let src_cols: Vec<String> = src.columns.iter().map(|c| c.name.clone()).collect();
    let _ = src;
    let mut matched = 0u64;
    let mut inserted = 0u64;
    let mut affected_rows: Vec<Vec<DbValue>> = Vec::new();
    let tgt = db
        .get_table_mut(&target)
        .map_err(|_| EngineError::TableNotFound(target.clone()))?;
    let tgt_cols: Vec<String> = tgt.columns.iter().map(|c| c.name.clone()).collect();
    for sr in &src_rows {
        let mut is_matched = false;
        let mut ri = 0;
        while ri < tgt.rows.len() {
            let combined: Vec<DbValue> = sr.iter().chain(tgt.rows[ri].iter()).cloned().collect();
            let cmap: std::collections::HashMap<String, usize> = src_cols
                .iter()
                .chain(tgt_cols.iter())
                .enumerate()
                .map(|(i, n)| (n.clone(), i))
                .collect();
            if let Ok(DbValue::Bool(true)) = crate::engine::functions::eval::eval_expr(&merge.on, &combined, &cmap) {
                is_matched = true;
                for cl in &merge.clauses {
                    if matches!(cl.clause_kind, MergeClauseKind::Matched) {
                        match &cl.action {
                            MergeAction::Update(MergeUpdateExpr { assignments, .. }) => {
                                for a in assignments {
                                    if let sqlparser::ast::AssignmentTarget::ColumnName(n) = &a.target {
                                        let cn = n.to_string().to_lowercase();
                                        if let Some(&ci) = tgt.col_index.get(&cn) {
                                            if let Ok(v) = crate::engine::functions::eval::eval_expr(
                                                &a.value,
                                                &tgt.rows[ri],
                                                &tgt.col_index,
                                            ) {
                                                tgt.rows[ri][ci] = v;
                                            }
                                        }
                                    }
                                }
                                if return_select.is_some() {
                                    affected_rows.push(tgt.rows[ri].clone());
                                }
                                matched += 1;
                            }
                            MergeAction::Delete { .. } => {
                                let rd = tgt.rows[ri].clone();
                                if return_select.is_some() {
                                    affected_rows.push(rd.clone());
                                }
                                tgt.delete(|r| *r == rd);
                                // After deletion, rows shift — don't increment ri
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                break;
            }
            ri += 1;
        }
        if !is_matched {
            for cl in &merge.clauses {
                if matches!(cl.clause_kind, MergeClauseKind::NotMatched) {
                    if let MergeAction::Insert(_) = &cl.action {
                        let mut row = Vec::new();
                        for tc in &tgt_cols {
                            if let Some(si) = src_cols.iter().position(|s| s == tc) {
                                row.push(sr[si].clone());
                            } else {
                                row.push(DbValue::Null);
                            }
                        }
                        let _ = tgt.insert(row.clone());
                        if return_select.is_some() {
                            affected_rows.push(row);
                        }
                        inserted += 1;
                    }
                }
            }
        }
    }

    if let Some(select_items) = return_select {
        let table = db
            .get_table(&target)
            .map_err(|_| EngineError::TableNotFound(target.clone()))?;
        let ref_rows: Vec<&[DbValue]> = affected_rows.iter().map(|r| r.as_slice()).collect();
        return Ok(format_projected_result(
            ref_rows,
            &select_items,
            &table.col_index,
            table,
        ));
    }

    Ok(format!("\"MERGE: {} matched, {} inserted\"", matched, inserted))
}
