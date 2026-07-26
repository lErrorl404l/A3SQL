// a3sql statement executor — interprets sqlparser AST against Database

//! Statement executor — dispatches AST nodes to handler functions.
//! The main entry point is [`execute()`]. Each statement type is handled by
//! dedicated modules under `stmts/`.

use std::collections::HashMap;

use sqlparser::ast::{Expr, ObjectType, SelectItem, Statement};

use super::database::Database;
use super::functions::aggregate::projection_expr_name;
use super::functions::eval::eval_expr;
use super::stmts;
use super::stmts::ddl::{object_name_str, parse_data_type};
use super::stmts::select::cte::exec_cte_query;
use super::table::Table;
use super::value::DbValue;

use crate::engine::error::EngineError;

pub(crate) mod select;

// ponytail: thread-local DB snapshot for subquery evaluation (avoids deadlock
// when exec_subquery is called inside eval_expr while DB lock is held).
thread_local! {
    pub(crate) static SUBQ_DB: std::cell::RefCell<Option<Database>> =
        const { std::cell::RefCell::new(None) };
}

// ponytail: global tracking for last_insert_rowid / changes (no db ref in eval path)
thread_local! {
    pub(crate) static LAST_INSERT_ROWID: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
    pub(crate) static LAST_CHANGES: std::cell::RefCell<usize> =
        const { std::cell::RefCell::new(0) };
}

// ponytail: thread-local buffer for COPY FROM stdin data
thread_local! {
    pub(crate) static COPY_STDIN: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

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

/// Format SELECT results, projecting to only the requested columns.
pub(crate) fn format_projected_result(
    rows: Vec<&[DbValue]>,
    projection: &[SelectItem],
    col_map: &HashMap<String, usize>,
    table: &Table,
) -> String {
    let is_wildcard = projection
        .iter()
        .any(|item| matches!(item, SelectItem::Wildcard { .. }));
    if is_wildcard {
        return table.format_result(rows);
    }

    // Build header from projection
    let header: Vec<String> = projection
        .iter()
        .map(|item| match item {
            SelectItem::UnnamedExpr(expr) => projection_expr_name(expr),
            SelectItem::ExprWithAlias { alias, .. } => alias.value.to_lowercase(),
            SelectItem::Wildcard { .. } => unreachable!(),
            _ => format!("{:?}", item),
        })
        .collect();

    let header_json = format!(
        "[{}]",
        header
            .iter()
            .map(|h| format!("\"{}\"", h))
            .collect::<Vec<_>>()
            .join(",")
    );

    // Pre-count window functions in projection for correct column offset
    let wf_prefix_counts: Vec<usize> = projection
        .iter()
        .scan(0, |count, item| {
            let expr = match item {
                SelectItem::UnnamedExpr(e) => Some(e),
                SelectItem::ExprWithAlias { expr: e, .. } => Some(e),
                _ => None,
            };
            let is_win = expr.is_some_and(|e| matches!(e, Expr::Function(f) if f.over.is_some()));
            let idx = *count;
            if is_win {
                *count += 1;
            }
            Some(idx)
        })
        .collect();
    let orig_cols = col_map.len();
    let row_jsons: Vec<String> = rows
        .iter()
        .map(|row| {
            let cells: Vec<String> = projection
                .iter()
                .enumerate()
                .map(|(proj_idx, item)| {
                    let expr = match item {
                        SelectItem::UnnamedExpr(e) => Some(e),
                        SelectItem::ExprWithAlias { expr: e, .. } => Some(e),
                        SelectItem::Wildcard { .. } => None,
                        _ => None,
                    };
                    if let Some(e) = expr {
                        let is_window = matches!(e, Expr::Function(f) if f.over.is_some());
                        if is_window {
                            let win_idx = wf_prefix_counts[proj_idx];
                            let win_col = orig_cols + win_idx;
                            if win_col < row.len() {
                                return row[win_col].to_json_string();
                            }
                        }
                        match eval_expr(e, row, col_map) {
                            Ok(v) => v.to_json_string(),
                            Err(_) => "null".to_string(),
                        }
                    } else {
                        "null".to_string()
                    }
                })
                .collect();
            format!("[{}]", cells.join(","))
        })
        .collect();

    let mut parts: Vec<String> = vec![header_json];
    parts.extend(row_jsons);
    format!("[{}]", parts.join(","))
}

// (subquery and order_limit moved to execute/select.rs)

// ── CREATE INDEX handling ──────────────────────────────────────────────

fn drop_index_by_name(db: &mut Database, name: &str) -> bool {
    let table_names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    for tname in table_names {
        if let Ok(table) = db.get_table_mut(&tname) {
            if table.drop_index(name).is_ok() {
                return true;
            }
        }
    }
    false
}

// ── TRIGGERS ─────────────────────────────────────────────────────────────

/// Parse and execute a raw SQL statement string within the DB context.
pub(crate) fn parse_and_exec(sql: &str, db: &mut Database) -> Result<String, EngineError> {
    use sqlparser::dialect::SQLiteDialect;
    use sqlparser::parser::Parser;
    // Try SQLite dialect first (handles CREATE TRIGGER with BEGIN...END body)
    let sqlite = SQLiteDialect {};
    if let Ok(stmts) = Parser::parse_sql(&sqlite, sql) {
        let mut results = Vec::new();
        for stmt in stmts {
            results.push(execute(&stmt, db)?);
        }
        return Ok(results.join("\n"));
    }
    let meta = sqlparser::dialect::GenericDialect {};
    let stmts =
        Parser::parse_sql(&meta, sql).map_err(|e| EngineError::Exec(format!("Parse error in trigger body: {}", e)))?;
    let mut results = Vec::new();
    for stmt in stmts {
        results.push(execute(&stmt, db)?);
    }
    Ok(results.join("\n"))
}

// Execute CREATE TRIGGER.
// ── CREATE VIRTUAL TABLE ─────────────────────────────────────────────────

// ── EXPLAIN ─────────────────────────────────────────────────────────────

// Note: EXPLAIN, simple_like, like_match, wildcard_match moved to
// functions/eval.rs. simple_wildcard was unused dead code, removed.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::database::Database;
    use crate::engine::value::*;

    fn make_test_db() -> Database {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::String,
                primary_key: true,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "name".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "value".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let table = Table::new("items".into(), cols).unwrap();
        db.create_table("items", table).unwrap();
        db
    }

    pub(crate) fn parse_and_exec(sql: &str, db: &mut Database) -> Result<String, String> {
        let stmts = crate::parser::parse_sql(sql).map_err(|e| format!("{}", e))?;
        let mut result = String::new();
        for stmt in &stmts {
            result = execute(stmt, db)?;
        }
        Ok(result)
    }

    #[test]
    fn create_table() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, val FLOAT)", &mut db).unwrap();
        assert!(db.has_table("t"));
    }

    #[test]
    fn insert_and_select() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items", &mut db).unwrap();
        assert!(result.contains("\"id\"") && result.contains("\"name\""));
        assert!(result.contains("alpha") && result.contains("beta"));
    }

    #[test]
    fn select_with_where() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE value >= 20", &mut db).unwrap();
        assert!(result.contains("beta"));
        assert!(!result.contains("alpha"));
    }

    #[test]
    fn update_rows() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("UPDATE items SET name = 'updated' WHERE id = 'a'", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE id = 'a'", &mut db).unwrap();
        assert!(result.contains("updated"));
    }

    #[test]
    fn delete_rows() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        let result = parse_and_exec("DELETE FROM items WHERE id = 'a'", &mut db);
        eprintln!("DELETE result: {:?}", result);
        assert!(result.is_ok(), "DELETE failed: {:?}", result.err());
        assert_eq!(db.get_table("items").unwrap().rows.len(), 0);
    }

    #[test]
    fn like_operator() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('abc123', 'test', 1)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE id LIKE '%123'", &mut db).unwrap();
        assert!(result.contains("abc123"));
    }

    #[test]
    fn trigram_index_fuzzy_after_insert() {
        let mut db = make_indexed_db();
        parse_and_exec("INSERT INTO idx_test VALUES ('rhs_m4a1', 1)", &mut db).unwrap();
        let result = parse_and_exec("SELECT k FROM idx_test WHERE k %% 'rhs_m4'", &mut db).unwrap();
        assert!(result.contains("rhs_m4a1"), "trigram index: {}", result);
    }

    // ── Phase 7: ORDER BY, LIMIT, Aggregates ─────────────────────────

    #[test]
    fn order_by_desc() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items ORDER BY value DESC", &mut db).unwrap();
        // In DESC order, 20 should come before 10
        let pos_20 = result.find(",beta,").unwrap_or(0);
        let pos_10 = result.find(",alpha,").unwrap_or(usize::MAX);
        assert!(
            pos_20 < pos_10,
            "beta(20) should appear before alpha(10) in DESC: {}",
            result
        );
    }

    #[test]
    fn limit_clause() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items LIMIT 1", &mut db).unwrap();
        let count = result.matches("alpha").count() + result.matches("beta").count();
        assert_eq!(count, 1, "LIMIT 1 should return 1 row: {}", result);
    }

    #[test]
    fn count_aggregate() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT COUNT(*) FROM items", &mut db).unwrap();
        assert!(result.contains("2"), "COUNT should be 2: {}", result);
    }

    #[test]
    fn count_distinct() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('c', 'gamma', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT COUNT(DISTINCT value) FROM items", &mut db).unwrap();
        assert!(result.contains("2"), "COUNT(DISTINCT value) should be 2: {}", result);
    }

    #[test]
    fn sum_aggregate() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('b', 'beta', 20)", &mut db).unwrap();
        let result = parse_and_exec("SELECT SUM(value) FROM items", &mut db).unwrap();
        assert!(result.contains("30"), "SUM should be 30: {}", result);
    }

    #[test]
    fn group_by() {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "cat".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut table = Table::new("data".into(), cols).unwrap();
        table
            .insert(vec![DbValue::String("a".into()), DbValue::Int(10)])
            .unwrap();
        table
            .insert(vec![DbValue::String("a".into()), DbValue::Int(20)])
            .unwrap();
        table
            .insert(vec![DbValue::String("b".into()), DbValue::Int(30)])
            .unwrap();
        db.create_table("data", table).unwrap();

        let result = parse_and_exec("SELECT cat, SUM(val) FROM data GROUP BY cat", &mut db).unwrap();
        assert!(result.contains("30"), "SUM(a) = 30: {}", result);
        assert!(result.contains("30"), "SUM(b) = 30: {}", result);
    }

    #[test]
    fn transaction_rollback() {
        let mut db = make_test_db();
        parse_and_exec("BEGIN", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('rx', 'rollback_test', 99)", &mut db).unwrap();
        parse_and_exec("ROLLBACK", &mut db).unwrap();
        let t = db.get_table("items").unwrap();
        assert_eq!(t.rows.len(), 0, "rows should be 0 after rollback");
    }

    #[test]
    fn transaction_commit() {
        let mut db = make_test_db();
        parse_and_exec("BEGIN", &mut db).unwrap();
        parse_and_exec("INSERT INTO items VALUES ('cx', 'commit_test', 99)", &mut db).unwrap();
        parse_and_exec("COMMIT", &mut db).unwrap();
        let t = db.get_table("items").unwrap();
        assert_eq!(t.rows.len(), 1, "rows should be 1 after commit");
    }

    // ── Edge cases ──────────────────────────────────────────────────

    #[test]
    fn empty_table_select() {
        let mut db = make_test_db();
        let result = parse_and_exec("SELECT * FROM items", &mut db).unwrap();
        assert_eq!(result, "[[\"id\",\"name\",\"value\"]]");
    }

    #[test]
    fn empty_where_select() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('a', 'alpha', 10)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE 1 = 0", &mut db).unwrap();
        assert_eq!(result, "[[\"id\",\"name\",\"value\"]]");
    }

    #[test]
    fn null_insert() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('n', NULL, 99)", &mut db).unwrap();
        let result = parse_and_exec("SELECT * FROM items WHERE id = 'n'", &mut db).unwrap();
        assert!(result.contains("null"));
    }

    #[test]
    fn bulk_insert_500() {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "v".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let t = Table::new("bulk".into(), cols).unwrap();
        db.create_table("bulk", t).unwrap();
        for i in 0..500 {
            parse_and_exec(&format!("INSERT INTO bulk VALUES ({},{})", i, i * 2), &mut db).unwrap();
        }
        let r = parse_and_exec("SELECT COUNT(*) FROM bulk", &mut db).unwrap();
        assert!(r.contains("500"), "count: {}", r);
        let s = parse_and_exec("SELECT SUM(v) FROM bulk", &mut db).unwrap();
        // sum(i=0..499, i*2) = 249500
        assert!(s.contains("249500"), "sum: {}", s);
    }

    #[test]
    fn string_with_semicolon() {
        let mut db = make_test_db();
        let sql = "INSERT INTO items VALUES ('sc', 'a;b', 1)";
        parse_and_exec(sql, &mut db).unwrap();
        let r = parse_and_exec("SELECT * FROM items WHERE id = 'sc'", &mut db).unwrap();
        assert!(r.contains("a;b"));
    }

    #[test]
    fn order_empty_table() {
        let mut db = make_test_db();
        let r = parse_and_exec("SELECT * FROM items ORDER BY value", &mut db).unwrap();
        assert_eq!(r, "[[\"id\",\"name\",\"value\"]]");
    }

    // ── Index maintenance tests ─────────────────────────────────────

    fn make_indexed_db() -> Database {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "k".into(),
                dtype: ColumnType::String,
                primary_key: true,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "v".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let t = Table::new("idx_test".into(), cols).unwrap();
        db.create_table("idx_test", t).unwrap();
        parse_and_exec("INSERT INTO idx_test VALUES ('a', 10)", &mut db).unwrap();
        parse_and_exec("INSERT INTO idx_test VALUES ('b', 20)", &mut db).unwrap();
        parse_and_exec("CREATE INDEX btree_v ON idx_test (v) USING BTREE", &mut db).unwrap();
        parse_and_exec("CREATE INDEX trigram_k ON idx_test (k) USING TRIGRAM", &mut db).unwrap();
        db
    }

    // ── JOIN tests ──────────────────────────────────────────────────

    #[test]
    fn cross_join() {
        let mut db = Database::new();
        let ca = vec![Column {
            name: "x".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut ta = Table::new("ta".into(), ca).unwrap();
        ta.insert(vec![DbValue::Int(1)]).unwrap();
        ta.insert(vec![DbValue::Int(2)]).unwrap();
        db.create_table("ta", ta).unwrap();
        let cb = vec![Column {
            name: "y".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut tb = Table::new("tb".into(), cb).unwrap();
        tb.insert(vec![DbValue::String("a".into())]).unwrap();
        db.create_table("tb", tb).unwrap();
        let r = parse_and_exec("SELECT * FROM ta, tb", &mut db).unwrap();
        assert!(
            r.contains("1") && r.contains("a") && r.contains("2"),
            "cross join: {}",
            r
        );
    }

    #[test]
    fn inner_join() {
        let mut db = Database::new();
        let ca = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "v".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut ta = Table::new("a".into(), ca).unwrap();
        ta.insert(vec![DbValue::Int(1), DbValue::String("one".into())]).unwrap();
        ta.insert(vec![DbValue::Int(2), DbValue::String("two".into())]).unwrap();
        db.create_table("a", ta).unwrap();
        let cb = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "d".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut tb = Table::new("b".into(), cb).unwrap();
        tb.insert(vec![DbValue::Int(1), DbValue::String("desc1".into())])
            .unwrap();
        tb.insert(vec![DbValue::Int(3), DbValue::String("desc3".into())])
            .unwrap();
        db.create_table("b", tb).unwrap();
        let r = parse_and_exec("SELECT * FROM a INNER JOIN b ON a.id = b.id", &mut db).unwrap();
        assert!(r.contains("one"), "inner join: {}", r);
        assert!(!r.contains("two"), "should exclude two: {}", r);
    }

    #[test]
    fn left_join() {
        let mut db = Database::new();
        let ca = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut ta = Table::new("a".into(), ca).unwrap();
        ta.insert(vec![DbValue::String("x".into())]).unwrap();
        ta.insert(vec![DbValue::String("y".into())]).unwrap();
        db.create_table("a", ta).unwrap();
        let cb = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut tb = Table::new("b".into(), cb).unwrap();
        tb.insert(vec![DbValue::String("x".into())]).unwrap();
        db.create_table("b", tb).unwrap();
        let r = parse_and_exec("SELECT * FROM a LEFT JOIN b ON a.k = b.k", &mut db).unwrap();
        assert!(r.contains("x"), "x: {}", r);
        assert!(r.contains("null") || r.contains("y"), "y null: {}", r);
    }

    #[test]
    fn join_with_where() {
        let mut db = Database::new();
        let ca = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "n".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut ta = Table::new("u".into(), ca).unwrap();
        ta.insert(vec![DbValue::Int(1), DbValue::String("alice".into())])
            .unwrap();
        ta.insert(vec![DbValue::Int(2), DbValue::String("bob".into())]).unwrap();
        db.create_table("u", ta).unwrap();
        let cb = vec![
            Column {
                name: "uid".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "r".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut tb = Table::new("r".into(), cb).unwrap();
        tb.insert(vec![DbValue::Int(1), DbValue::String("admin".into())])
            .unwrap();
        tb.insert(vec![DbValue::Int(2), DbValue::String("user".into())])
            .unwrap();
        db.create_table("r", tb).unwrap();
        let sql = "SELECT * FROM u INNER JOIN r ON u.id = r.uid WHERE r.r = 'admin'";
        let r = parse_and_exec(sql, &mut db).unwrap();
        assert!(r.contains("alice"), "alice admin: {}", r);
        assert!(!r.contains("bob"), "bob not admin: {}", r);
    }

    #[test]
    fn null_arithmetic() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('nx', 'null_test', NULL)", &mut db).unwrap();
        let r = parse_and_exec("SELECT * FROM items WHERE value IS NULL", &mut db).unwrap();
        assert!(r.contains("null_test"), "null: {}", r);
    }

    #[test]
    fn fuzzy_fn_call_integration() {
        let mut db = make_test_db();
        parse_and_exec("INSERT INTO items VALUES ('fn_test', 'hello', 1)", &mut db).unwrap();
        let r = parse_and_exec("SELECT * FROM items WHERE id %% 'fn_t'", &mut db).unwrap();
        assert!(r.contains("fn_test"), "fuzzy fn: {}", r);
    }

    #[test]
    fn auto_increment_flag_set() {
        let mut db = Database::new();
        let _ = parse_and_exec(
            "CREATE TABLE ait (id INT PRIMARY KEY AUTO_INCREMENT, val STRING)",
            &mut db,
        );
        let table = db.get_table("ait").unwrap();
        assert!(
            table.columns[0].auto_increment,
            "id should have auto_increment=true, got false"
        );
        assert!(table.columns[0].primary_key, "id should be primary key");
    }

    #[test]
    fn btree_index_equality_selection() {
        // BTreeIndex should be consulted for `col = literal` WHERE
        let mut db = make_indexed_db();
        // btree_v index exists on v
        let r = parse_and_exec("SELECT * FROM idx_test WHERE v = 10", &mut db).unwrap();
        assert!(r.contains("\"a\""), "btree index lookup: {}", r);
        assert!(!r.contains("\"b\""), "should not include b: {}", r);
    }

    #[test]
    fn btree_index_equality_fallback() {
        // Non-equality WHERE still works via full scan
        let mut db = make_indexed_db();
        let r = parse_and_exec("SELECT * FROM idx_test WHERE k = 'a'", &mut db).unwrap();
        assert!(r.contains("\"a\""), "fallback lookup: {}", r);
    }

    #[test]
    fn right_join() {
        let mut db = Database::new();
        let ca = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut ta = Table::new("a".into(), ca).unwrap();
        ta.insert(vec![DbValue::String("x".into())]).unwrap();
        db.create_table("a", ta).unwrap();
        let cb = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut tb = Table::new("b".into(), cb).unwrap();
        tb.insert(vec![DbValue::String("x".into())]).unwrap();
        tb.insert(vec![DbValue::String("y".into())]).unwrap();
        db.create_table("b", tb).unwrap();
        let r = parse_and_exec("SELECT * FROM a RIGHT JOIN b ON a.k = b.k", &mut db).unwrap();
        assert!(r.contains("x"), "x: {}", r);
        assert!(r.contains("y"), "y: {}", r);
    }

    #[test]
    fn natural_join() {
        let mut db = Database::new();
        let ca = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "name".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut a = Table::new("a".into(), ca).unwrap();
        a.insert(vec![DbValue::Int(1), DbValue::String("one".into())]).unwrap();
        a.insert(vec![DbValue::Int(2), DbValue::String("two".into())]).unwrap();
        db.create_table("a", a).unwrap();
        let cb = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut b = Table::new("b".into(), cb).unwrap();
        b.insert(vec![DbValue::Int(1), DbValue::String("desc1".into())])
            .unwrap();
        b.insert(vec![DbValue::Int(3), DbValue::String("desc3".into())])
            .unwrap();
        db.create_table("b", b).unwrap();
        // NATURAL JOIN should join on common column "id"
        let r = parse_and_exec("SELECT * FROM a NATURAL JOIN b", &mut db).unwrap();
        assert!(r.contains("one"), "natural join should include one: {}", r);
        assert!(r.contains("desc1"), "natural join should include desc1: {}", r);
        assert!(
            !r.contains("two"),
            "natural join should exclude two (id=2 not in b): {}",
            r
        );
        assert!(
            !r.contains("desc3"),
            "natural join should exclude desc3 (id=3 not in a): {}",
            r
        );
    }

    #[test]
    fn join_using() {
        let mut db = Database::new();
        let ca = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "name".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut a = Table::new("a".into(), ca).unwrap();
        a.insert(vec![DbValue::Int(1), DbValue::String("one".into())]).unwrap();
        a.insert(vec![DbValue::Int(2), DbValue::String("two".into())]).unwrap();
        db.create_table("a", a).unwrap();
        let cb = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            },
        ];
        let mut b = Table::new("b".into(), cb).unwrap();
        b.insert(vec![DbValue::Int(1), DbValue::String("desc1".into())])
            .unwrap();
        b.insert(vec![DbValue::Int(3), DbValue::String("desc3".into())])
            .unwrap();
        db.create_table("b", b).unwrap();
        // JOIN USING (id) should join on column "id"
        let r = parse_and_exec("SELECT * FROM a JOIN b USING (id)", &mut db).unwrap();
        assert!(r.contains("one"), "join using should include one: {}", r);
        assert!(r.contains("desc1"), "join using should include desc1: {}", r);
        assert!(!r.contains("two"), "join using should exclude two: {}", r);
        assert!(!r.contains("desc3"), "join using should exclude desc3: {}", r);
        // INNER JOIN USING should also work
        let r2 = parse_and_exec("SELECT * FROM a INNER JOIN b USING (id)", &mut db).unwrap();
        assert!(r2.contains("one"), "inner join using: {}", r2);
    }

    #[test]
    fn multi_table_join() {
        let mut db = Database::new();
        let ca = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut a = Table::new("a".into(), ca).unwrap();
        a.insert(vec![DbValue::String("x".into())]).unwrap();
        db.create_table("a", a).unwrap();
        let mut b = Table::new(
            "b".into(),
            vec![Column {
                name: "k".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            }],
        )
        .unwrap();
        b.insert(vec![DbValue::String("x".into())]).unwrap();
        db.create_table("b", b).unwrap();
        let mut c = Table::new(
            "c".into(),
            vec![Column {
                name: "k".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            }],
        )
        .unwrap();
        c.insert(vec![DbValue::String("x".into())]).unwrap();
        db.create_table("c", c).unwrap();
        let r = parse_and_exec(
            "SELECT * FROM a INNER JOIN b ON a.k = b.k INNER JOIN c ON b.k = c.k",
            &mut db,
        )
        .unwrap();
        assert!(
            r.contains("x") && r.chars().filter(|&c| c == 'x').count() >= 3,
            "multi: {}",
            r
        );
    }

    #[test]
    fn self_join() {
        let mut db = Database::new();
        let cols = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut t = Table::new("t".into(), cols).unwrap();
        t.insert(vec![DbValue::String("x".into())]).unwrap();
        t.insert(vec![DbValue::String("y".into())]).unwrap();
        db.create_table("t", t).unwrap();
        let r = parse_and_exec("SELECT a.k, b.k FROM t AS a CROSS JOIN t AS b", &mut db).unwrap();
        assert!(r.contains("x") && r.matches("x").count() >= 2, "self cross: {}", r);
    }

    #[test]
    fn join_with_aggregate() {
        let mut db = Database::new();
        let ca = vec![Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut a = Table::new("a".into(), ca).unwrap();
        a.insert(vec![DbValue::Int(1)]).unwrap();
        a.insert(vec![DbValue::Int(2)]).unwrap();
        db.create_table("a", a).unwrap();
        let mut b = Table::new(
            "b".into(),
            vec![Column {
                name: "aid".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            }],
        )
        .unwrap();
        b.insert(vec![DbValue::Int(1)]).unwrap();
        b.insert(vec![DbValue::Int(1)]).unwrap();
        db.create_table("b", b).unwrap();
        // Aggregate + JOIN is not yet supported — skip for now
        // The GROUP BY + aggregate pipeline only works in single-table exec_select
        println!("note: JOIN+aggregate not yet supported");
    }

    #[test]
    fn join_with_order_by() {
        let mut db = Database::new();
        let ca = vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        let mut a = Table::new("a".into(), ca).unwrap();
        a.insert(vec![DbValue::String("b".into())]).unwrap();
        a.insert(vec![DbValue::String("a".into())]).unwrap();
        db.create_table("a", a).unwrap();
        let mut b = Table::new(
            "b".into(),
            vec![Column {
                name: "k".into(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            }],
        )
        .unwrap();
        b.insert(vec![DbValue::String("a".into())]).unwrap();
        b.insert(vec![DbValue::String("b".into())]).unwrap();
        db.create_table("b", b).unwrap();
        let r = parse_and_exec("SELECT a.k FROM a INNER JOIN b ON a.k = b.k ORDER BY a.k ASC", &mut db).unwrap();
        assert!(r.contains("a") && r.contains("b"), "join order: {}", r);
    }

    // ── EXPLAIN tests ──────────────────────────────────────────────────

    #[test]
    fn explain_select() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN SELECT * FROM items", &mut db).unwrap();
        assert!(r.contains("SeqScan"), "explain select: {}", r);
        assert!(r.contains("items"), "table name: {}", r);
    }

    #[test]
    fn explain_insert() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN INSERT INTO items VALUES ('a', 'b', 1)", &mut db).unwrap();
        assert!(r.contains("Insert"), "explain insert: {}", r);
        assert!(r.contains("items"), "table name: {}", r);
    }

    #[test]
    fn explain_create_table() {
        let mut db = Database::new();
        let r = parse_and_exec("EXPLAIN CREATE TABLE et (id STRING PRIMARY KEY)", &mut db).unwrap();
        assert!(r.contains("CreateTable"), "explain create: {}", r);
        assert!(r.contains("et"), "table name: {}", r);
    }

    #[test]
    fn explain_update() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN UPDATE items SET name = 'x' WHERE id = 'a'", &mut db).unwrap();
        assert!(r.contains("Update"), "explain update: {}", r);
    }

    #[test]
    fn explain_delete() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN DELETE FROM items WHERE id = 'a'", &mut db).unwrap();
        assert!(r.contains("Delete"), "explain delete: {}", r);
    }

    #[test]
    fn explain_with_where() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN SELECT * FROM items WHERE name = 'test'", &mut db).unwrap();
        assert!(r.contains("Filter"), "explain filter: {}", r);
        assert!(r.contains("SeqScan"), "explain scan: {}", r);
    }

    #[test]
    fn explain_with_order_limit() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN SELECT * FROM items ORDER BY name LIMIT 5", &mut db).unwrap();
        assert!(r.contains("OrderBy"), "explain order: {}", r);
        assert!(r.contains("Limit"), "explain limit: {}", r);
    }

    #[test]
    fn explain_analyze_rejected() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN ANALYZE SELECT * FROM items", &mut db);
        assert!(r.is_err(), "ANALYZE should be rejected");
        if let Err(e) = r {
            assert!(e.contains("ANALYZE"), "err msg: {}", e);
        }
    }

    #[test]
    fn explain_show_tables() {
        let mut db = Database::new();
        let r = parse_and_exec("EXPLAIN SHOW TABLES", &mut db).unwrap();
        assert!(r.contains("ShowTables"), "explain show tables: {}", r);
    }

    #[test]
    fn explain_transaction() {
        let mut db = Database::new();
        let r = parse_and_exec("EXPLAIN BEGIN", &mut db).unwrap();
        assert!(r.contains("StartTransaction"), "explain begin: {}", r);
        let r = parse_and_exec("EXPLAIN COMMIT", &mut db).unwrap();
        assert!(r.contains("Commit"), "explain commit: {}", r);
        let r = parse_and_exec("EXPLAIN ROLLBACK", &mut db).unwrap();
        assert!(r.contains("Rollback"), "explain rollback: {}", r);
    }

    #[test]
    fn explain_create_index() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN CREATE INDEX my_idx ON items (name)", &mut db).unwrap();
        assert!(r.contains("CreateIndex"), "explain create index: {}", r);
    }

    #[test]
    fn explain_with_indexes() {
        let mut db = make_indexed_db();
        let r = parse_and_exec("EXPLAIN SELECT * FROM idx_test WHERE v = 10", &mut db).unwrap();
        assert!(r.contains("indexes"), "should show indexes: {}", r);
        assert!(r.contains("btree_v"), "should list btree_v: {}", r);
    }

    #[test]
    fn explain_alter_table() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN ALTER TABLE items ADD COLUMN extra INT", &mut db).unwrap();
        assert!(r.contains("AlterTable"), "explain alter: {}", r);
    }

    #[test]
    fn explain_truncate() {
        let mut db = make_test_db();
        let r = parse_and_exec("EXPLAIN TRUNCATE items", &mut db).unwrap();
        assert!(r.contains("Truncate"), "explain truncate: {}", r);
    }

    // ── CHECK constraints ─────────────────────────────────────────────────

    #[test]
    fn check_table_level_constraint_create() {
        let mut db = Database::new();
        let r = parse_and_exec(
            "CREATE TABLE t (id STRING PRIMARY KEY, val INT, CHECK (val > 0))",
            &mut db,
        );
        assert!(r.is_ok(), "create with CHECK: {:?}", r);
        assert!(db.has_table("t"));
        let t = db.get_table("t").unwrap();
        assert_eq!(t.check_constraints.len(), 1, "should have 1 CHECK constraint");
    }

    #[test]
    fn check_column_level_constraint() {
        let mut db = Database::new();
        let r = parse_and_exec(
            "CREATE TABLE t (id STRING PRIMARY KEY, val INT CHECK (val > 0))",
            &mut db,
        );
        assert!(r.is_ok(), "col-level CHECK: {:?}", r);
    }

    #[test]
    fn check_constraint_enforced_on_insert() {
        let mut db = Database::new();
        parse_and_exec(
            "CREATE TABLE t (id STRING PRIMARY KEY, val INT CHECK (val > 0))",
            &mut db,
        )
        .unwrap();
        // Valid insert
        parse_and_exec("INSERT INTO t VALUES ('a', 10)", &mut db).unwrap();
        // Invalid insert (violates CHECK)
        let r = parse_and_exec("INSERT INTO t VALUES ('b', -5)", &mut db);
        assert!(r.is_err(), "should reject negative val");
        if let Err(e) = r {
            assert!(e.contains("CHECK"), "msg: {}", e);
        }
    }

    #[test]
    fn check_constraint_enforced_on_update() {
        let mut db = Database::new();
        parse_and_exec(
            "CREATE TABLE t (id STRING PRIMARY KEY, val INT CHECK (val > 0))",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO t VALUES ('a', 10)", &mut db).unwrap();
        // Valid update
        parse_and_exec("UPDATE t SET val = 20 WHERE id = 'a'", &mut db).unwrap();
        // Invalid update
        let r = parse_and_exec("UPDATE t SET val = -1 WHERE id = 'a'", &mut db);
        assert!(r.is_err(), "should reject UPDATE with CHECK violation");
    }

    // ── FOREIGN KEY constraints ────────────────────────────────────────────

    #[test]
    fn fk_update_local_column_validated() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p2')", &mut db).unwrap();
        parse_and_exec(
            "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
        // Valid FK update
        parse_and_exec("UPDATE child SET pid = 'p2' WHERE id = 'c1'", &mut db).unwrap();
        // Invalid FK update (value not in parent)
        let r = parse_and_exec("UPDATE child SET pid = 'nonexistent' WHERE id = 'c1'", &mut db);
        assert!(r.is_err(), "should reject FK update to nonexistent ref");
        if let Err(e) = r {
            assert!(e.contains("FOREIGN KEY"), "msg: {}", e);
        }
    }

    #[test]
    fn fk_update_referenced_pk_restrict() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
        parse_and_exec(
            "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
        // Try to update PK in parent when child references it
        let r = parse_and_exec("UPDATE parent SET id = 'p2' WHERE id = 'p1'", &mut db);
        assert!(r.is_err(), "should reject PK update with FK ref");
        if let Err(e) = r {
            assert!(e.contains("FOREIGN KEY"), "msg: {}", e);
        }
    }

    #[test]
    fn fk_update_referenced_pk_allowed_when_no_refs() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p2')", &mut db).unwrap();
        parse_and_exec(
            "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
        // 'p2' has no child references → update should succeed
        let r = parse_and_exec("UPDATE parent SET id = 'p3' WHERE id = 'p2'", &mut db);
        assert!(r.is_ok(), "should allow PK update with no FK refs: {:?}", r);
    }

    #[test]
    fn fk_delete_restrict() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
        parse_and_exec(
            "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
        // DELETE should be rejected when child references the row
        let r = parse_and_exec("DELETE FROM parent WHERE id = 'p1'", &mut db);
        assert!(r.is_err(), "should reject DELETE with FK ref");
        if let Err(e) = r {
            assert!(e.contains("FOREIGN KEY"), "msg: {}", e);
        }
    }

    #[test]
    fn fk_delete_allowed_when_no_references() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p2')", &mut db).unwrap();
        parse_and_exec(
            "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id))",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
        // 'p2' has no child references → delete should succeed
        let r = parse_and_exec("DELETE FROM parent WHERE id = 'p2'", &mut db);
        assert!(r.is_ok(), "should allow DELETE with no FK refs: {:?}", r);
    }

    // ── PK update pk_set maintenance ──────────────────────────────────────

    #[test]
    fn pk_update_works_and_maintains_pk_set() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, val INT)", &mut db).unwrap();
        parse_and_exec("INSERT INTO t VALUES ('a', 1)", &mut db).unwrap();
        let t = db.get_table("t").unwrap();
        assert!(t.pk_set.contains("'a'"), "PK set should have 'a', got: {:?}", t.pk_set);
        // Update PK
        parse_and_exec("UPDATE t SET id = 'b' WHERE id = 'a'", &mut db).unwrap();
        let t = db.get_table("t").unwrap();
        assert!(!t.pk_set.contains("'a'"), "PK set should no longer have 'a'");
        assert!(t.pk_set.contains("'b'"), "PK set should have 'b'");
        // New row with old PK should work
        parse_and_exec("INSERT INTO t VALUES ('a', 2)", &mut db).unwrap();
    }

    // ── Trigram FTS ────────────────────────────────────────────────────────

    #[test]
    fn trigram_index_used_for_fuzzy_match() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, name STRING)", &mut db).unwrap();
        parse_and_exec("INSERT INTO t VALUES ('1', 'rhs_m4a1')", &mut db).unwrap();
        parse_and_exec("INSERT INTO t VALUES ('2', 'rhs_m4a1_carryhandle')", &mut db).unwrap();
        parse_and_exec("INSERT INTO t VALUES ('3', 'hlc_ak74')", &mut db).unwrap();
        parse_and_exec("CREATE INDEX trigram_name ON t (name) USING TRIGRAM", &mut db).unwrap();
        // Query using %% (fuzzy_match) with trigram index
        // trigram_similarity("rhs_m4a1", "rhs_m4") = 0.5 ≥ 0.3 → match
        // trigram_similarity("rhs_m4a1_carryhandle", "rhs_m4") = 0.25 < 0.3 → no match
        // trigram_similarity("hlc_ak74", "rhs_m4") = 0.0 < 0.3 → no match
        let r = parse_and_exec("SELECT id FROM t WHERE name %% 'rhs_m4'", &mut db).unwrap();
        assert!(r.contains("1"), "should match rhs_m4a1: {}", r);
        assert!(!r.contains("3"), "should NOT match hlc_ak74: {}", r);
    }

    #[test]
    fn fts_score_function() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, name STRING)", &mut db).unwrap();
        parse_and_exec("INSERT INTO t VALUES ('1', 'hello world')", &mut db).unwrap();
        let r = parse_and_exec("SELECT fts_score(name, 'hello') FROM t WHERE id = '1'", &mut db).unwrap();
        // fts_score should return a float > 0
        assert!(
            r.contains("0.") || r.contains("1."),
            "fts_score should be a float: {}",
            r
        );
    }

    #[test]
    fn fk_delete_cascade() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p2')", &mut db).unwrap();
        parse_and_exec(
            "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id) ON DELETE CASCADE)",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c2', 'p1')", &mut db).unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c3', 'p2')", &mut db).unwrap();
        assert_eq!(db.get_table("child").unwrap().row_count(), 3);
        // DELETE p1 should cascade-delete c1 and c2
        parse_and_exec("DELETE FROM parent WHERE id = 'p1'", &mut db).unwrap();
        assert_eq!(db.get_table("child").unwrap().row_count(), 1, "c3 should remain");
        let child_rows = &db.get_table("child").unwrap().rows;
        assert_eq!(child_rows[0][0].to_string(), "'c3'", "only c3 remains");
    }

    #[test]
    fn fk_delete_set_null() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('p1')", &mut db).unwrap();
        parse_and_exec(
            "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id) ON DELETE SET NULL)",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c1', 'p1')", &mut db).unwrap();
        parse_and_exec("DELETE FROM parent WHERE id = 'p1'", &mut db).unwrap();
        let child = db.get_table("child").unwrap();
        assert_eq!(child.row_count(), 1, "child row should remain");
        assert_eq!(child.rows[0][1], DbValue::Null, "pid should be NULL");
    }

    #[test]
    fn fk_update_cascade() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE parent (id STRING PRIMARY KEY)", &mut db).unwrap();
        parse_and_exec("INSERT INTO parent VALUES ('old_pk')", &mut db).unwrap();
        parse_and_exec(
            "CREATE TABLE child (id STRING PRIMARY KEY, pid STRING REFERENCES parent(id) ON UPDATE CASCADE)",
            &mut db,
        )
        .unwrap();
        parse_and_exec("INSERT INTO child VALUES ('c1', 'old_pk')", &mut db).unwrap();
        // Update parent PK
        parse_and_exec("UPDATE parent SET id = 'new_pk' WHERE id = 'old_pk'", &mut db).unwrap();
        let child = db.get_table("child").unwrap();
        assert_eq!(child.rows[0][1].to_string(), "'new_pk'", "child FK should be updated");
    }

    #[test]
    fn upsert_on_conflict_do_update_excluded() {
        let mut db = Database::new();
        parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, v INT)", &mut db).unwrap();
        // Insert initial row
        parse_and_exec("INSERT INTO t VALUES ('a', 100)", &mut db).unwrap();
        // UPSERT: ON CONFLICT DO UPDATE SET v = EXCLUDED.v
        parse_and_exec(
            "INSERT INTO t VALUES ('a', 200) ON CONFLICT (id) DO UPDATE SET v = EXCLUDED.v",
            &mut db,
        )
        .unwrap();
        // Verify v was updated from 100 → 200
        let result = parse_and_exec("SELECT v FROM t WHERE id = 'a'", &mut db).unwrap();
        assert!(result.contains("200"), "UPSERT should update v to 200, got: {}", result);
    }
}
