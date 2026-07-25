use super::super::database::Database;
use super::super::table::Table;
use super::super::value::{Column, ColumnType, DbValue};
use sqlparser::ast::{
    Analyze, CopySource, CopyTarget, DataType, Function, Ident, Merge, MergeAction, MergeClauseKind, MergeInsertExpr,
    MergeUpdateExpr, ObjectName, ObjectNamePart, SequenceOptions, ShowCreateObject, ShowStatementOptions,
    VacuumStatement,
};

fn object_name_str(name: &ObjectName) -> String {
    name.0
        .iter()
        .filter_map(|p| match p {
            ObjectNamePart::Identifier(i) => Some(i.value.to_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

pub(crate) fn exec_show_columns(so: &ShowStatementOptions, db: &Database) -> Result<String, String> {
    let tn = so
        .show_in
        .as_ref()
        .and_then(|si| si.parent_name.as_ref())
        .map(|n| object_name_str(n))
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
    drop(src);
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
            if let Ok(DbValue::Bool(true)) = super::super::execute::eval_expr(&merge.on, &combined, &cmap) {
                is_matched = true;
                for cl in &merge.clauses {
                    if matches!(cl.clause_kind, MergeClauseKind::Matched) {
                        match &cl.action {
                            MergeAction::Update(MergeUpdateExpr { assignments, .. }) => {
                                for a in assignments {
                                    if let sqlparser::ast::AssignmentTarget::ColumnName(n) = &a.target {
                                        let cn = n.to_string().to_lowercase();
                                        if let Some(&ci) = tgt.col_index.get(&cn) {
                                            if let Ok(v) = super::super::execute::eval_expr(
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

pub(crate) fn exec_vacuum(v: &VacuumStatement, db: &mut Database) -> Result<String, String> {
    let tables: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    for tn in tables {
        if let Ok(mut t) = db.get_table_mut(&tn) {
            t.rebuild_index();
        }
    }
    if v.reindex {
        Ok("\"REINDEX complete\"".into())
    } else {
        Ok("\"VACUUM complete\"".into())
    }
}

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

pub(crate) fn exec_create_sequence(
    name: &ObjectName,
    ifne: bool,
    _opts: &[SequenceOptions],
    _dt: Option<&DataType>,
    db: &mut Database,
) -> Result<String, String> {
    let sn = object_name_str(name);
    if ifne && db.has_table(&sn) {
        return Ok(format!("\"Sequence '{}' exists\"", sn));
    }
    let cols = vec![Column {
        name: "val".into(),
        dtype: ColumnType::Int,
        primary_key: false,
        not_null: false,
        default: Some(DbValue::Int(0)),
        auto_increment: false,
    }];
    let mut table = Table::new(format!("__seq_{}", sn), cols).map_err(|e| format!("CREATE SEQUENCE: {}", e))?;
    let _ = table.insert(vec![DbValue::Int(0)]);
    db.add_table(format!("__seq_{}", sn), table);
    Ok(format!("\"Sequence '{}' created\"", sn))
}

pub(crate) fn exec_comment_on(
    _ot: &str,
    on: &ObjectName,
    comment: Option<&str>,
    db: &mut Database,
) -> Result<String, String> {
    db.set_config(&format!("comment_{}", object_name_str(on)), comment.unwrap_or(""));
    Ok("\"COMMENT (stored)\"".into())
}

pub(crate) fn exec_call(func: &Function, db: &mut Database) -> Result<String, String> {
    let empty = Vec::new();
    let empty_map = std::collections::HashMap::new();
    match super::super::execute::exec_function(func, &empty, &empty_map) {
        Ok(val) => Ok(format!("\"CALL returned: {}\"", val)),
        Err(e) => Err(format!("CALL error: {}", e)),
    }
}

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

pub(crate) fn exec_create_virtual_table(
    name: &ObjectName,
    if_not_exists: bool,
    module_name: &Ident,
    module_args: &[Ident],
    db: &mut Database,
) -> Result<String, String> {
    let tn = object_name_str(name);
    if if_not_exists && db.has_table(&tn) {
        return Ok(format!("\"Table '{}' exists\"", tn));
    }
    if !["fts3", "fts4", "fts5"].contains(&module_name.value.to_lowercase().as_str()) {
        return Err(format!("Virtual table module '{}' not supported", module_name));
    }
    let cols: Vec<Column> = module_args
        .iter()
        .map(|a| Column {
            name: a.value.to_lowercase(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        })
        .collect();
    if cols.is_empty() {
        return Err("ftsX requires columns".into());
    }
    let mut table = Table::new(tn.clone(), cols).map_err(|e| format!("CREATE VIRTUAL TABLE: {}", e))?;
    for (i, cn) in module_args.iter().enumerate() {
        let _ = table.create_index(
            &format!("fts_trgm_{}", cn.value.to_lowercase()),
            &cn.value.to_lowercase(),
            super::super::index::IndexType::Trigram,
        );
    }
    db.add_table(tn.clone(), table);
    Ok(format!("\"Virtual table '{}' created (FTS trigram)\"", tn))
}
