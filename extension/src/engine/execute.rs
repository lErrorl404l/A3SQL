// a3sql statement executor — interprets sqlparser AST against Database

//! Statement executor — dispatches AST nodes to handler functions.
//! The main entry point is [`execute()`]. Each statement type is handled by
//! dedicated modules under `stmts/` or `execute/`.

use sqlparser::ast::{ObjectType, Statement};

use super::database::Database;
use super::stmts;
use super::stmts::ddl::{object_name_str, parse_data_type};
use super::stmts::select::cte::exec_cte_query;

use crate::engine::error::EngineError;

pub(crate) mod select;
pub(crate) mod util;

// Re-export shared utilities for backward compatibility with existing imports.
use util::drop_index_by_name;
pub(crate) use util::{format_projected_result, parse_and_exec, COPY_STDIN, LAST_CHANGES, LAST_INSERT_ROWID, SUBQ_DB};

// ── Public entry point ──────────────────────────────────────────────────

pub(crate) fn execute(stmt: &Statement, db: &mut Database) -> Result<String, EngineError> {
    match stmt {
        Statement::CreateView(cv) => stmts::ddl::exec_create_view(cv, db),
        Statement::CreateTable(def) => stmts::ddl::exec_create_table(def, db),
        Statement::Insert(ins) => stmts::insert::exec_insert(ins, db),
        Statement::Query(q) => exec_cte_query(q, db),
        Statement::Update(upd) => stmts::update::exec_update(upd, db),
        Statement::Delete(del) => stmts::delete::exec_delete(del, db),
        Statement::CreateTrigger(ct) => stmts::ddl::exec_create_trigger(ct, db),
        Statement::CreateIndex(idx) => stmts::ddl::exec_create_index(idx, db),
        Statement::Drop {
            names,
            object_type,
            if_exists,
            ..
        } => {
            let name = object_name_str(&names[0]);
            match object_type {
                ObjectType::View => {
                    if !db.has_view(&name) {
                        if *if_exists {
                            return Ok(format!("\"View '{}' not found\"", name));
                        }
                        return Err(EngineError::ViewNotFound(name.to_string()));
                    }
                    db.drop_view(&name).map_err(EngineError::Exec)?;
                    Ok(format!("\"Dropped view '{}'\"", name))
                }
                _ => {
                    let type_str = format!("{}", object_type).to_lowercase();
                    if type_str.contains("index") {
                        if !drop_index_by_name(db, &name) {
                            if *if_exists {
                                return Ok(format!("\"Index '{}' not found\"", name));
                            }
                            return Err(EngineError::IndexNotFound(name.to_string()));
                        }
                        Ok(format!("\"Dropped index '{}'\"", name))
                    } else {
                        if !db.has_table(&name) {
                            if *if_exists {
                                return Ok(format!("\"Table '{}' not found\"", name));
                            }
                            return Err(EngineError::TableNotFound(name.to_string()));
                        }
                        db.drop_table(&name).map_err(EngineError::Exec)?;
                        Ok(format!("\"Dropped table '{}'\"", name))
                    }
                }
            }
        }
        Statement::RenameTable(rt) => {
            let old = object_name_str(&rt[0].old_name);
            let new = object_name_str(&rt[0].new_name);
            db.rename_table(&old, &new).map_err(EngineError::Exec)?;
            Ok(format!("\"Table '{}' renamed to '{}'\"", old, new))
        }
        Statement::Truncate(trunc) => {
            let name = object_name_str(&trunc.table_names[0].name);
            if trunc.if_exists && !db.has_table(&name) {
                Ok(format!("\"Table '{}' not found\"", name))
            } else {
                db.get_table_mut(&name).map_err(EngineError::Exec)?.truncate()?;
                Ok(format!("\"Table '{}' truncated\"", name))
            }
        }
        Statement::Set(set) => stmts::transaction::exec_set(set, db),
        Statement::Pragma { name, value, is_eq: _ } => {
            // ponytail: PRAGMA stored in config, no actual behavior change
            if let Some(v) = value {
                db.set_config(&object_name_str(name), &v.to_string());
            }
            Ok(format!(
                "\"PRAGMA {} = {:?}\"",
                object_name_str(name),
                value.as_ref().map(|v| v.to_string()).unwrap_or_default()
            ))
        }
        Statement::ShowColumns { show_options, .. } => stmts::ddl::exec_show_columns(show_options, db),
        Statement::ShowCreate { obj_type, obj_name } => stmts::ddl::exec_show_create(obj_type, obj_name, db),
        Statement::DropTrigger(dt) => stmts::ddl::exec_drop_trigger(&dt.trigger_name, dt.table_name.as_ref(), db),
        Statement::AttachDatabase {
            schema_name,
            database_file_name,
            database: _,
        } => {
            db.set_config(&format!("attach_{}", schema_name), &database_file_name.to_string());
            Ok(format!("\"Attached '{}' as '{}'\"", database_file_name, schema_name))
        }
        Statement::Merge(merge) => stmts::merge::exec_merge(merge, db),
        Statement::CreateVirtualTable {
            name,
            if_not_exists,
            module_name,
            module_args,
        } => stmts::ddl::exec_create_virtual_table(name, *if_not_exists, module_name, module_args, db),
        Statement::ShowTables { .. } => {
            let names = db.table_names();
            let inner: Vec<String> = names.iter().map(|n| format!("\"{}\"", n)).collect();
            Ok(format!("[{}]", inner.join(",")))
        }
        Statement::ShowVariables { .. } | Statement::ShowStatus { .. } => {
            let vars: Vec<String> = db
                .config
                .keys()
                .map(|k| format!("\"{} = {}\"", k, db.config.get(k).unwrap_or(&String::new())))
                .collect();
            Ok(format!("[{}]", vars.join(",")))
        }
        Statement::StartTransaction { .. } => {
            db.begin();
            Ok("\"Transaction started\"".into())
        }
        Statement::Commit { .. } => {
            db.commit().map_err(EngineError::Exec)?;
            Ok("\"Committed\"".into())
        }
        Statement::Rollback { .. } => {
            db.rollback().map_err(EngineError::Exec)?;
            Ok("\"Rolled back\"".into())
        }
        Statement::Savepoint { name, .. } => {
            db.savepoint(&name.to_string());
            Ok(format!("\"Savepoint '{}' created\"", name))
        }
        Statement::ReleaseSavepoint { name, .. } => {
            db.release_savepoint(&name.to_string()).map_err(EngineError::Exec)?;
            Ok(format!("\"Savepoint '{}' released\"", name))
        }
        Statement::AlterTable(at) => {
            let table_name = object_name_str(&at.name);
            let mut results = Vec::new();
            for operation in &at.operations {
                let result = match operation {
                    sqlparser::ast::AlterTableOperation::AddColumn { column_def, .. } => {
                        let col_name = column_def.name.value.to_lowercase();
                        let dtype = parse_data_type(&column_def.data_type)?;
                        db.get_table_mut(&table_name)
                            .map_err(EngineError::Exec)?
                            .add_column(col_name.clone(), dtype)?;
                        format!("\"Column '{}' added to '{}'\"", col_name, table_name)
                    }
                    sqlparser::ast::AlterTableOperation::DropColumn { column_names, .. } => {
                        for cn in column_names {
                            let col_name = cn.value.to_lowercase();
                            db.get_table_mut(&table_name)
                                .map_err(EngineError::Exec)?
                                .drop_column(&col_name)?;
                        }
                        format!("\"Column dropped from '{}'\"", table_name)
                    }
                    sqlparser::ast::AlterTableOperation::RenameColumn {
                        old_column_name,
                        new_column_name,
                    } => {
                        let old_name = old_column_name.value.to_lowercase();
                        let new_name = new_column_name.value.to_lowercase();
                        db.get_table_mut(&table_name)
                            .map_err(EngineError::Exec)?
                            .rename_column(&old_name, &new_name)?;
                        format!("\"Column '{}' renamed to '{}'\"", old_name, new_name)
                    }
                    sqlparser::ast::AlterTableOperation::RenameTable {
                        table_name: new_name_info,
                    } => {
                        let new_name = match new_name_info {
                            sqlparser::ast::RenameTableNameKind::To(name)
                            | sqlparser::ast::RenameTableNameKind::As(name) => name.to_string(),
                        }
                        .to_lowercase();
                        db.rename_table(&table_name, &new_name).map_err(EngineError::Exec)?;
                        format!("\"Table renamed to '{}'\"", new_name)
                    }
                    _ => {
                        return Err(EngineError::Exec(format!(
                            "ALTER TABLE operation not supported: {:?}",
                            operation
                        )))
                    }
                };
                results.push(result);
            }
            Ok(format!("[{}]", results.join(",")))
        }
        Statement::Explain {
            statement: inner,
            analyze,
            ..
        } => {
            if *analyze {
                return Err(EngineError::Exec("EXPLAIN ANALYZE is not supported".into()));
            }
            stmts::explain::explain_statement(inner, db)
        }
        Statement::Vacuum(v) => stmts::ddl::exec_vacuum(v, db),
        Statement::Copy { source, to, target, .. } => stmts::ddl::exec_copy(source, *to, target, db),
        Statement::CreateSequence {
            name,
            if_not_exists,
            sequence_options,
            data_type,
            ..
        } => stmts::ddl::exec_create_sequence(
            name,
            *if_not_exists,
            sequence_options.as_slice(),
            data_type.as_ref(),
            db,
        ),
        Statement::Comment {
            object_type,
            object_name,
            comment,
            ..
        } => {
            let type_str = format!("{:?}", object_type);
            stmts::ddl::exec_comment_on(&type_str, object_name, comment.as_deref(), db)
        }
        Statement::Call(f) => stmts::ddl::exec_call(f, db),
        Statement::Analyze(a) => stmts::ddl::exec_analyze(a, db),
        other => Err(EngineError::Exec(format!("Statement not supported: {:?}", other))),
    }
}

#[cfg(test)]
pub(crate) mod tests;
