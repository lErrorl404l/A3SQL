// Miscellaneous DDL/DML statements: MERGE, VACUUM, COPY, COMMENT, CALL, ANALYZE, SHOW COLUMNS, SHOW CREATE, DROP TRIGGER

use super::object_name_str;
use crate::engine::database::Database;
use crate::engine::value::DbValue;
use sqlparser::ast::{
    Analyze, CopySource, CopyTarget, Function, Merge, MergeAction, MergeClauseKind, MergeUpdateExpr, ObjectName,
    ShowCreateObject, ShowStatementOptions, VacuumStatement,
};

// ── SHOW COLUMNS ────────────────────────────────────────────────────────

pub(crate) fn exec_show_columns(so: &ShowStatementOptions, db: &Database) -> Result<String, String> {
    let tn = so
        .show_in
        .as_ref()
        .and_then(|si| si.parent_name.as_ref())
        .map(object_name_str)
        .ok_or_else(|| "SHOW COLUMNS requires FROM".to_string())?;
    let t = db.get_table(&tn)?;
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

pub(crate) fn exec_show_create(ot: &ShowCreateObject, on: &ObjectName, db: &Database) -> Result<String, String> {
    let name = object_name_str(on);
    match ot {
        ShowCreateObject::Table => {
            let t = db.get_table(&name)?;
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
        _ => Err("SHOW CREATE only supports TABLE".into()),
    }
}

// ── DROP TRIGGER ────────────────────────────────────────────────────────

pub(crate) fn exec_drop_trigger(
    tn: &ObjectName,
    table: Option<&ObjectName>,
    db: &mut Database,
) -> Result<String, String> {
    let name = object_name_str(tn);
    let names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    let target_names = if let Some(t) = table {
        vec![object_name_str(t)]
    } else {
        names
    };
    for tn2 in &target_names {
        if let Ok(t) = db.get_table_mut(tn2) {
            if t.triggers.iter().any(|tr| tr.name == name) {
                t.triggers.retain(|tr| tr.name != name);
                return Ok(format!("\"Trigger '{}' dropped\"", name));
            }
        }
    }
    Err(format!("Trigger '{}' not found", name))
}

// ── MERGE ───────────────────────────────────────────────────────────────

pub(crate) fn exec_merge(merge: &Merge, db: &mut Database) -> Result<String, String> {
    let target = match &merge.table {
        sqlparser::ast::TableFactor::Table { name, .. } => Some(object_name_str(name)),
        _ => None,
    }
    .ok_or_else(|| "MERGE: no target".to_string())?;
    let source = match &merge.source {
        sqlparser::ast::TableFactor::Table { name, .. } => Some(object_name_str(name)),
        _ => None,
    }
    .ok_or_else(|| "MERGE: no source".to_string())?;
    let src = db.get_table(&source)?;
    let src_rows = src.rows.clone();
    let src_cols: Vec<String> = src.columns.iter().map(|c| c.name.clone()).collect();
    let _ = src;
    let mut matched = 0u64;
    let mut inserted = 0u64;
    let tgt = db.get_table_mut(&target)?;
    let tgt_cols: Vec<String> = tgt.columns.iter().map(|c| c.name.clone()).collect();
    for sr in &src_rows {
        let mut is_matched = false;
        for ri in 0..tgt.rows.len() {
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
                                matched += 1;
                            }
                            MergeAction::Delete { .. } => {
                                let rd = tgt.rows[ri].clone();
                                tgt.delete(|r| *r == rd);
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
                        let _ = tgt.insert(row);
                        inserted += 1;
                    }
                }
            }
        }
    }
    Ok(format!("\"MERGE: {} matched, {} inserted\"", matched, inserted))
}

// ── VACUUM ──────────────────────────────────────────────────────────────

pub(crate) fn exec_vacuum(v: &VacuumStatement, db: &mut Database) -> Result<String, String> {
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
    _target: &CopyTarget,
    db: &mut Database,
) -> Result<String, String> {
    if !to {
        return Err("COPY FROM not supported".into());
    }
    match source {
        CopySource::Table { table_name, .. } => {
            let t = db.get_table(&object_name_str(table_name))?;
            Ok(format!("\"COPY: {} rows\"", t.row_count()))
        }
        _ => Err("COPY only supports table source".into()),
    }
}

// ── COMMENT ON ──────────────────────────────────────────────────────────

pub(crate) fn exec_comment_on(
    _ot: &str,
    on: &ObjectName,
    comment: Option<&str>,
    db: &mut Database,
) -> Result<String, String> {
    db.set_config(&format!("comment_{}", object_name_str(on)), comment.unwrap_or(""));
    Ok("\"COMMENT (stored)\"".into())
}

// ── CALL ────────────────────────────────────────────────────────────────

pub(crate) fn exec_call(func: &Function, _db: &mut Database) -> Result<String, String> {
    let empty = Vec::new();
    let empty_map = std::collections::HashMap::new();
    match crate::engine::functions::eval::exec_function(func, &empty, &empty_map) {
        Ok(val) => Ok(format!("\"CALL returned: {}\"", val)),
        Err(e) => Err(format!("CALL error: {}", e)),
    }
}

// ── ANALYZE ─────────────────────────────────────────────────────────────

pub(crate) fn exec_analyze(a: &Analyze, db: &mut Database) -> Result<String, String> {
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
