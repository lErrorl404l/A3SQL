// DDL statements: CREATE, DROP, ALTER, SHOW, MERGE, VIRTUAL TABLE
// ponytail: modular DDL handlers

use super::super::database::Database;
use super::super::index::IndexType;
use super::super::table::Table;
use super::super::value::{Column, ColumnType, DbValue};
use sqlparser::ast::{
    Ident, Merge, MergeAction, MergeClauseKind, MergeInsertExpr, MergeUpdateExpr, ObjectName, ObjectNamePart,
    ShowCreateObject, ShowStatementOptions,
};

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

/// MERGE INTO target USING source ON condition WHEN MATCHED THEN UPDATE/DELETE ...
pub(crate) fn exec_merge(merge: &Merge, db: &mut Database) -> Result<String, String> {
    let target_name =
        resolve_table_name(&merge.table).ok_or_else(|| "MERGE: could not resolve target table".to_string())?;
    let source_name =
        resolve_table_name(&merge.source).ok_or_else(|| "MERGE: could not resolve source table".to_string())?;
    let source = db.get_table(&source_name)?;
    let src_rows: Vec<Vec<DbValue>> = source.rows.clone();
    let src_cols: Vec<String> = source.columns.iter().map(|c| c.name.clone()).collect();
    drop(source);

    let mut matched = 0u64;
    let mut inserted = 0u64;
    let target = db.get_table_mut(&target_name)?;
    let tgt_cols: Vec<String> = target.columns.iter().map(|c| c.name.clone()).collect();

    for src_row in &src_rows {
        let mut is_matched = false;
        // ponytail: simple row-by-row evaluation of ON condition
        for tgt_row in 0..target.rows.len() {
            let combined: Vec<DbValue> = src_row.iter().chain(target.rows[tgt_row].iter()).cloned().collect();
            let combined_map: std::collections::HashMap<String, usize> = src_cols
                .iter()
                .chain(tgt_cols.iter())
                .enumerate()
                .map(|(i, n)| (n.clone(), i))
                .collect();
            if let Ok(DbValue::Bool(true)) = super::super::execute::eval_expr(&merge.on, &combined, &combined_map) {
                is_matched = true;
                for clause in &merge.clauses {
                    if matches!(clause.clause_kind, MergeClauseKind::Matched) {
                        if let Some(pred) = &clause.predicate {
                            if !matches!(
                                super::super::execute::eval_expr(pred, &combined, &combined_map),
                                Ok(DbValue::Bool(true))
                            ) {
                                continue;
                            }
                        }
                        match &clause.action {
                            MergeAction::Update(MergeUpdateExpr { assignments, .. }) => {
                                for assign in assignments {
                                    if let sqlparser::ast::AssignmentTarget::ColumnName(name) = &assign.target {
                                        let col_name = name.to_string().to_lowercase();
                                        if let Some(&ci) = target.col_index.get(&col_name) {
                                            if let Ok(val) = super::super::execute::eval_expr(
                                                &assign.value,
                                                &target.rows[tgt_row],
                                                &target.col_index,
                                            ) {
                                                target.rows[tgt_row][ci] = val;
                                            }
                                        }
                                    }
                                }
                                matched += 1;
                            }
                            MergeAction::Delete { .. } => {
                                let row_to_delete = target.rows[tgt_row].clone();
                                target.delete(|r| *r == row_to_delete);
                                matched += 1;
                            }
                            _ => {}
                        }
                    }
                }
                break;
            }
        }
        if !is_matched {
            for clause in &merge.clauses {
                if matches!(clause.clause_kind, MergeClauseKind::NotMatched) {
                    if let MergeAction::Insert(MergeInsertExpr { columns: _, .. }) = &clause.action {
                        let mut row: Vec<DbValue> = Vec::new();
                        for tc in &tgt_cols {
                            if let Some(si) = src_cols.iter().position(|s| s == tc) {
                                row.push(src_row[si].clone());
                            } else {
                                row.push(DbValue::Null);
                            }
                        }
                        let _ = target.insert(row);
                        inserted += 1;
                    }
                }
            }
        }
    }
    Ok(format!("\"MERGE: {} matched, {} inserted\"", matched, inserted))
}

/// CREATE VIRTUAL TABLE ... USING fts3/fts4/fts5
pub(crate) fn exec_create_virtual_table(
    name: &ObjectName,
    if_not_exists: bool,
    module_name: &Ident,
    module_args: &[Ident],
    db: &mut Database,
) -> Result<String, String> {
    let table_name = object_name_str(name);
    if if_not_exists && db.has_table(&table_name) {
        return Ok(format!("\"Table '{}' already exists\"", table_name));
    }
    let module = module_name.value.to_lowercase();
    match module.as_str() {
        "fts3" | "fts4" | "fts5" => {
            let col_names: Vec<String> = module_args.iter().map(|a| a.value.to_lowercase()).collect();
            if col_names.is_empty() {
                return Err("CREATE VIRTUAL TABLE ... USING ftsX requires columns".into());
            }
            let cols: Vec<Column> = col_names
                .iter()
                .map(|n| Column {
                    name: n.clone(),
                    dtype: ColumnType::String,
                    primary_key: false,
                    not_null: false,
                    default: None,
                    auto_increment: false,
                })
                .collect();
            let mut table = Table::new(table_name.clone(), cols)?;
            // Add trigram index on each text column
            for (i, col_name) in col_names.iter().enumerate() {
                let _ = table.create_index(&format!("fts_trgm_{}", col_name), col_name, IndexType::Trigram);
            }
            db.add_table(table_name.clone(), table);
            Ok(format!("\"Virtual table '{}' created (FTS via trigram)\"", table_name))
        }
        _ => Err(format!(
            "Virtual table module '{}' not supported (try fts3/fts4/fts5)",
            module
        )),
    }
}

fn resolve_table_name(tf: &sqlparser::ast::TableFactor) -> Option<String> {
    match tf {
        sqlparser::ast::TableFactor::Table { name, .. } => Some(object_name_str(name)),
        _ => None,
    }
}
