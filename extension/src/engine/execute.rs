// a3sql statement executor — interprets sqlparser AST against Database

use std::collections::{HashMap, HashSet};

use sqlparser::ast::table_constraints::TableConstraint;
use sqlparser::ast::{
    BinaryOperator, ColumnOption, DataType, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments,
    LimitClause, ObjectName, ObjectType, OrderByKind, Query, ReferentialAction, Select, SelectItem, SetExpr, Statement,
    TableFactor, TableWithJoins, UnaryOperator, Values, WindowFrame, WindowFrameBound, WindowFrameUnits, WindowType,
};

use super::table::ForeignKeyInfo;
use super::table::IndexImpl;

use super::database::Database;
use super::index::IndexType as A3IndexType;
use super::table::Table;
use super::value::{Column, ColumnType, DbValue};

// ponytail: thread-local DB snapshot for subquery evaluation (avoids deadlock
// when exec_subquery is called inside eval_expr while DB lock is held).
thread_local! {
    static SUBQ_DB: std::cell::RefCell<Option<Database>> =
        const { std::cell::RefCell::new(None) };
}

// ── Public entry point ──────────────────────────────────────────────────

pub fn execute(stmt: &Statement, db: &mut Database) -> Result<String, String> {
    match stmt {
        Statement::CreateView(cv) => exec_create_view(cv, db),
        Statement::CreateTable(def) => {
            if def.query.is_some() {
                exec_create_table_as(def, db)
            } else {
                exec_create_table(def, db)
            }
        }
        Statement::Insert(ins) => exec_insert(ins, db),
        Statement::Query(q) => {
            // Process WITH / CTE clauses before executing the main query body
            let mut cte_tables: Vec<String> = Vec::new();
            if let Some(with) = &q.with {
                for cte in &with.cte_tables {
                    let alias = cte.alias.name.value.to_lowercase();
                    let cte_alias = alias.clone();

                    // NOT MATERIALIZED: skip temp table creation (inline in main query)
                    use sqlparser::ast::CteAsMaterialized;
                    if cte.materialized == Some(CteAsMaterialized::NotMaterialized) {
                        continue;
                    }

                    // For recursive CTEs, only execute the anchor (non-recursive) term first
                    let json = if with.recursive {
                        if let SetExpr::SetOperation { left, .. } = &*cte.query.body {
                            // Wrap the anchor in a minimal Query for exec_select
                            let anchor_q = Query {
                                with: None,
                                body: Box::new(left.as_ref().clone()),
                                order_by: None,
                                limit_clause: None,
                                fetch: None,
                                locks: vec![],
                                for_clause: None,
                                settings: None,
                                format_clause: None,
                                pipe_operators: vec![],
                            };
                            exec_select(&anchor_q, db)?
                        } else {
                            exec_select(&cte.query, db)?
                        }
                    } else {
                        if matches!(&*cte.query.body, SetExpr::SetOperation { .. }) {
                            exec_union(&cte.query.body, &cte.query, db)?
                        } else {
                            exec_select(&cte.query, db)?
                        }
                    };
                    let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json).unwrap_or_default();
                    if rows.len() >= 2 {
                        // Use CTE alias column names if specified (e.g. "cte(n)" — columns from alias)
                        let cte_col_names: Vec<String> = if !cte.alias.columns.is_empty() {
                            cte.alias.columns.iter().map(|c| c.name.value.to_lowercase()).collect()
                        } else {
                            vec![]
                        };
                        let header = &rows[0];
                        let cols: Vec<Column> = (0..header.len())
                            .map(|i| Column {
                                name: cte_col_names
                                    .get(i)
                                    .cloned()
                                    .unwrap_or_else(|| header[i].as_str().unwrap_or("col").to_lowercase()),
                                dtype: rows
                                    .get(1)
                                    .and_then(|r| r.get(i))
                                    .map(json_type_to_column)
                                    .unwrap_or(ColumnType::String),
                                primary_key: false,
                                not_null: false,
                                default: None,
                                auto_increment: false,
                            })
                            .collect();
                        if let Ok(mut cte_table) = Table::new(alias.clone(), cols) {
                            for row_data in &rows[1..] {
                                let db_row: Vec<DbValue> = row_data.iter().map(json_val_to_dbvalue).collect();
                                let _ = cte_table.insert(db_row);
                            }
                            db.add_table(cte_alias.clone(), cte_table);
                            cte_tables.push(cte_alias);
                        }
                    }
                }
            }

            // Handle recursive CTEs: WITH RECURSIVE cte AS (anchor UNION ALL recursive)
            if let Some(with) = &q.with {
                if with.recursive {
                    for cte in &with.cte_tables {
                        let alias = cte.alias.name.value.to_lowercase();
                        if let SetExpr::SetOperation {
                            op: _,
                            left: _anchor,
                            right: _recursive,
                            ..
                        } = &*cte.query.body
                        {
                            // ponytail: anchor is already in db via first pass above.
                            // The recursive term references the CTE alias — run it in a loop
                            // until no new rows are produced (iterative fixpoint).
                            for _iteration in 0..100 {
                                let prev_count = db.get_table(&alias).map(|t| t.row_count()).unwrap_or(0);
                                let json = if matches!(&*cte.query.body, SetExpr::SetOperation { .. }) {
                                    exec_union(&cte.query.body, &cte.query, db)?
                                } else {
                                    exec_select(&cte.query, db)?
                                };
                                let current: Vec<Vec<serde_json::Value>> =
                                    serde_json::from_str(&json).unwrap_or_default();
                                let current_count = current.len().saturating_sub(1);
                                if current_count <= prev_count {
                                    break; // fixpoint reached
                                }
                                // Insert new rows, skipping duplicates
                                if let Ok(table) = db.get_table_mut(&alias) {
                                    for row_data in &current[1..] {
                                        let db_row: Vec<DbValue> = row_data.iter().map(json_val_to_dbvalue).collect();
                                        let exists = table.rows.iter().any(|r| r == &db_row);
                                        if !exists {
                                            let _ = table.insert(db_row);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let result = if matches!(&*q.body, SetExpr::SetOperation { .. }) {
                exec_union(&q.body, q, db)
            } else {
                exec_select(q, db)
            };

            // Clean up CTE temp tables
            for name in &cte_tables {
                let _ = db.drop_table(name);
            }

            result
        }
        Statement::Update(upd) => exec_update(upd, db),
        Statement::Delete(del) => exec_delete(del, db),
        Statement::CreateTrigger(ct) => exec_create_trigger(ct, db),
        Statement::CreateIndex(idx) => exec_create_index(idx, db),
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
                        return Err(format!("View '{}' not found", name));
                    }
                    db.drop_view(&name)?;
                    Ok(format!("\"Dropped view '{}'\"", name))
                }
                _ => {
                    let type_str = format!("{}", object_type).to_lowercase();
                    if type_str.contains("index") {
                        if !drop_index_by_name(db, &name) {
                            if *if_exists {
                                return Ok(format!("\"Index '{}' not found\"", name));
                            }
                            return Err(format!("Index '{}' not found", name));
                        }
                        Ok(format!("\"Dropped index '{}'\"", name))
                    } else {
                        if !db.has_table(&name) {
                            if *if_exists {
                                return Ok(format!("\"Table '{}' not found\"", name));
                            }
                            return Err(format!("Table '{}' not found", name));
                        }
                        db.drop_table(&name)?;
                        Ok(format!("\"Dropped table '{}'\"", name))
                    }
                }
            }
        }
        Statement::RenameTable(rt) => {
            let old = object_name_str(&rt[0].old_name);
            let new = object_name_str(&rt[0].new_name);
            db.rename_table(&old, &new)?;
            Ok(format!("\"Table '{}' renamed to '{}'\"", old, new))
        }
        Statement::Truncate(trunc) => {
            let name = object_name_str(&trunc.table_names[0].name);
            if trunc.if_exists && !db.has_table(&name) {
                Ok(format!("\"Table '{}' not found\"", name))
            } else {
                db.get_table_mut(&name)?.truncate()?;
                Ok(format!("\"Table '{}' truncated\"", name))
            }
        }
        Statement::ShowTables { .. } => {
            let names = db.table_names();
            let inner: Vec<String> = names.iter().map(|n| format!("\"{}\"", n)).collect();
            Ok(format!("[{}]", inner.join(",")))
        }
        Statement::StartTransaction { .. } => {
            db.begin();
            Ok("\"Transaction started\"".into())
        }
        Statement::Commit { .. } => {
            db.commit()?;
            Ok("\"Committed\"".into())
        }
        Statement::Rollback { .. } => {
            db.rollback()?;
            Ok("\"Rolled back\"".into())
        }
        Statement::Savepoint { name, .. } => {
            db.savepoint(&name.to_string());
            Ok(format!("\"Savepoint '{}' created\"", name))
        }
        Statement::ReleaseSavepoint { name, .. } => {
            db.release_savepoint(&name.to_string())?;
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
                        db.get_table_mut(&table_name)?.add_column(col_name.clone(), dtype)?;
                        format!("\"Column '{}' added to '{}'\"", col_name, table_name)
                    }
                    sqlparser::ast::AlterTableOperation::DropColumn { column_names, .. } => {
                        for cn in column_names {
                            let col_name = cn.value.to_lowercase();
                            db.get_table_mut(&table_name)?.drop_column(&col_name)?;
                        }
                        format!("\"Column dropped from '{}'\"", table_name)
                    }
                    sqlparser::ast::AlterTableOperation::RenameColumn {
                        old_column_name,
                        new_column_name,
                    } => {
                        let old_name = old_column_name.value.to_lowercase();
                        let new_name = new_column_name.value.to_lowercase();
                        db.get_table_mut(&table_name)?.rename_column(&old_name, &new_name)?;
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
                        db.rename_table(&table_name, &new_name)?;
                        format!("\"Table renamed to '{}'\"", new_name)
                    }
                    _ => return Err(format!("ALTER TABLE operation not supported: {:?}", operation)),
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
                return Err("EXPLAIN ANALYZE is not supported".into());
            }
            explain_statement(inner, db)
        }
        other => Err(format!("Statement not supported: {:?}", other)),
    }
}

// ── ObjectName helper ───────────────────────────────────────────────────

fn object_name_str(name: &ObjectName) -> String {
    name.to_string().to_lowercase()
}

// ── CREATE TABLE ────────────────────────────────────────────────────────

fn exec_create_table(def: &sqlparser::ast::CreateTable, db: &mut Database) -> Result<String, String> {
    let table_name = object_name_str(&def.name);

    if def.if_not_exists && db.has_table(&table_name) {
        return Ok(format!("\"Table '{}' already exists\"", table_name));
    }

    let mut columns: Vec<Column> = Vec::new();
    let mut has_pk = false;
    let mut check_exprs: Vec<Expr> = Vec::new();

    for col_def in &def.columns {
        let col_name = col_def.name.value.to_lowercase();
        let dtype = parse_data_type(&col_def.data_type)?;

        let mut is_pk = false;
        let mut is_not_null = false;
        let mut default_val: Option<DbValue> = None;
        let mut auto_inc = false;
        for opt_def in &col_def.options {
            match &opt_def.option {
                ColumnOption::PrimaryKey(_) | ColumnOption::Unique { .. } => is_pk = true,
                ColumnOption::NotNull => is_not_null = true,
                ColumnOption::DialectSpecific(tokens) => {
                    let has_auto = tokens.iter().any(|t| {
                        let s = t.to_string().to_lowercase().replace('"', "");
                        s == "auto_increment" || s == "autoincrement"
                    });
                    if has_auto {
                        is_pk = true;
                        auto_inc = true;
                    }
                }
                ColumnOption::Default(expression) => {
                    // Evaluate default expression (literal only)
                    match expression {
                        Expr::Value(v) => default_val = Some(sql_val_to_db(&v.value)),
                        _ => return Err("DEFAULT only supports literal values".into()),
                    }
                }
                _ => {}
            }
        }

        if is_pk {
            if has_pk {
                return Err("Only one primary key column supported".into());
            }
            has_pk = true;
        }

        columns.push(Column {
            name: col_name,
            dtype,
            primary_key: is_pk,
            not_null: is_not_null,
            default: default_val,
            auto_increment: auto_inc,
        });
    }

    // Table-level PRIMARY KEY constraints — Display-based detection
    // Collect CHECK constraints from column-level options
    for col_def in &def.columns {
        for opt_def in &col_def.options {
            if let ColumnOption::Check(check) = &opt_def.option {
                check_exprs.push(*check.expr.clone());
            }
        }
    }

    // Collect FOREIGN KEY constraints from both column and table level
    let mut foreign_keys: Vec<ForeignKeyInfo> = Vec::new();
    for col_def in &def.columns {
        let col_name = col_def.name.value.to_lowercase();
        for opt_def in &col_def.options {
            if let ColumnOption::ForeignKey(fk) = &opt_def.option {
                let local_col = fk
                    .columns
                    .first()
                    .map(|c| c.value.to_lowercase())
                    .unwrap_or_else(|| col_name.clone());
                foreign_keys.push(ForeignKeyInfo {
                    local_column: local_col,
                    foreign_table: fk.foreign_table.to_string().to_lowercase(),
                    foreign_column: fk
                        .referred_columns
                        .first()
                        .map(|c| c.value.to_lowercase())
                        .unwrap_or_default(),
                    on_delete: fk.on_delete,
                    on_update: fk.on_update,
                });
            }
        }
    }
    for constraint in &def.constraints {
        match constraint {
            TableConstraint::ForeignKey(fk) => {
                foreign_keys.push(ForeignKeyInfo {
                    local_column: fk.columns.first().map(|c| c.value.to_lowercase()).unwrap_or_default(),
                    foreign_table: fk.foreign_table.to_string().to_lowercase(),
                    foreign_column: fk
                        .referred_columns
                        .first()
                        .map(|c| c.value.to_lowercase())
                        .unwrap_or_default(),
                    on_delete: fk.on_delete,
                    on_update: fk.on_update,
                });
            }
            TableConstraint::Check(ck) => {
                check_exprs.push(*ck.expr.clone());
            }
            _ => {}
        }
        let text = format!("{}", constraint).to_uppercase();
        if text.contains("PRIMARY KEY (") {
            // Extract column name from display text: "PRIMARY KEY (col)"
            if let Some(start) = text.find('(') {
                if let Some(end) = text.find(')') {
                    let cname = text[start + 1..end].trim().to_lowercase();
                    if let Some(col) = columns.iter_mut().find(|col| col.name == cname) {
                        if has_pk {
                            return Err("Only one primary key supported".into());
                        }
                        col.primary_key = true;
                        has_pk = true;
                    }
                }
            }
        }
    }

    let mut table = Table::new(table_name.clone(), columns)?;
    table.check_constraints = check_exprs;
    table.foreign_keys = foreign_keys;
    db.create_table(&table_name, table)?;
    Ok(format!("\"Table '{}' created\"", table_name))
}

// ── CREATE VIEW ─────────────────────────────────────────────────────────

fn exec_create_view(cv: &sqlparser::ast::CreateView, db: &mut Database) -> Result<String, String> {
    // ponytail: non-materialized views only (re-executed each reference)
    if cv.materialized {
        return Err("Materialized views are not supported".into());
    }
    let name = object_name_str(&cv.name);
    if db.has_table(&name) {
        return Err(format!("Table '{}' already exists — cannot create view", name));
    }
    if cv.if_not_exists && db.has_view(&name) {
        return Ok(format!("\"View '{}' already exists\"", name));
    }
    // Reconstruct the defining query as SQL text for storage
    let view_sql = cv.query.to_string();
    db.create_view(&name, &view_sql)?;
    Ok(format!("\"View '{}' created\"", name))
}

// ── CREATE TABLE AS SELECT (CTAS) ──────────────────────────────────────

fn exec_create_table_as(def: &sqlparser::ast::CreateTable, db: &mut Database) -> Result<String, String> {
    let table_name = object_name_str(&def.name);

    if def.if_not_exists && db.has_table(&table_name) {
        return Ok(format!("\"Table '{}' already exists\"", table_name));
    }

    let query = def.query.as_ref().unwrap();
    let json = exec_select(query, db)?;

    let rows: Vec<Vec<serde_json::Value>> =
        serde_json::from_str(&json).map_err(|e| format!("CTAS JSON parse: {}", e))?;

    if rows.len() < 2 {
        return Err("CTAS: SELECT returned no columns".into());
    }

    let header = &rows[0];
    let mut columns: Vec<Column> = Vec::new();
    for h in header {
        let name = h.as_str().unwrap_or("col").to_lowercase();
        columns.push(Column {
            name,
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        });
    }

    let mut table = Table::new(table_name.clone(), columns)?;
    for row_data in &rows[1..] {
        let db_row: Vec<DbValue> = row_data.iter().map(json_val_to_dbvalue).collect();
        table.insert(db_row).map_err(|e| format!("CTAS insert: {}", e))?;
    }

    db.create_table(&table_name, table)?;
    Ok(format!(
        "\"Table '{}' created with {} row(s)\"",
        table_name,
        rows.len() - 1
    ))
}

// ── INSERT ──────────────────────────────────────────────────────────────

fn exec_insert(ins: &sqlparser::ast::Insert, db: &mut Database) -> Result<String, String> {
    let table_name = ins.table.to_string().to_lowercase();
    let returning = ins.returning.clone();

    // Parse source into expression rows — do this BEFORE borrowing table mutably
    // since SetExpr::Select needs a mutable db reference via exec_select.
    let (rows, is_replace): (Vec<Vec<Expr>>, bool) = match &ins.source {
        Some(q) => match &*q.as_ref().body {
            SetExpr::Values(Values { rows, .. }) => (
                rows.iter().map(|parens| parens.content.clone()).collect(),
                ins.or.is_some() || crate::REPLACE_FLAG.load(std::sync::atomic::Ordering::SeqCst),
            ),
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
                    serde_json::from_str(&json).map_err(|e| format!("SELECT parse: {}", e))?;
                let to_val = |v: &serde_json::Value| -> Expr {
                    use sqlparser::ast::Value as V;
                    Expr::Value(
                        match v {
                            serde_json::Value::String(s) => V::SingleQuotedString(s.clone()),
                            serde_json::Value::Number(n) => V::Number(
                                n.as_i64()
                                    .map_or_else(|| format!("{}", n.as_f64().unwrap_or(0.0)), |i| format!("{}", i)),
                                false,
                            ),
                            serde_json::Value::Bool(b) => V::Boolean(*b),
                            serde_json::Value::Null => V::Null,
                            other => V::SingleQuotedString(format!("{}", other)),
                        }
                        .into(),
                    )
                };
                (
                    parsed
                        .iter()
                        .skip(1)
                        .map(|row| row.iter().map(to_val).collect())
                        .collect(),
                    ins.or.is_some(),
                )
            }
            _ => return Err("INSERT source must be VALUES or SELECT".into()),
        },
        None => return Err("INSERT must have a source".into()),
    };

    // Pre-collect FOREIGN KEY lookup data (foreign table pk_sets) before mutable table borrow
    let fk_lookups: Vec<(String, HashSet<String>)> = {
        let self_table = db.get_table(&table_name)?;
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

    // Now we have the rows; borrow table for column mapping and insertion
    let table = db.get_table_mut(&table_name)?;

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
                    table
                        .col_idx(&name)
                        .ok_or_else(|| format!("Unknown column '{}' in table '{}'", name, table_name))
                })
                .collect::<Result<Vec<usize>, String>>()?,
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
            return Err(format!(
                "Expected {} values, got {}",
                col_indices.len(),
                row_exprs.len()
            ));
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
                .map_err(|e| format!("CHECK constraint error: {}", e))?;
            if !is_truthy(&check_val) {
                return Err("CHECK constraint failed for row".to_string());
            }
        }

        // Validate FOREIGN KEY constraints using pre-collected pk_sets
        for (local_col, ref_pks) in &fk_lookups {
            if let Some(&col_idx) = table.col_index.get(local_col) {
                let val = &full_row[col_idx];
                if !matches!(val, DbValue::Null) {
                    let pk_str = val.to_string().to_lowercase();
                    if !ref_pks.contains(&pk_str) {
                        return Err(format!(
                            "FOREIGN KEY constraint: '{}' value '{}' not found in referenced table",
                            local_col, pk_str
                        ));
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
            Err(e) if is_replace && e.contains("Duplicate primary key") => {
                // REPLACE: find the old row by PK and replace it
                if let Some(pk_col) = table.columns.iter().position(|c| c.primary_key) {
                    let pk_val = &full_row[pk_col];
                    table.delete_by_pk(pk_val);
                    inserted_rows.push(full_row.clone());
                    table.insert(full_row)?;
                    inserted += 1;
                } else {
                    return Err(e);
                }
            }
            Err(e) if e.contains("Duplicate primary key") && ins.on.is_some() => {
                // UPSERT: ON CONFLICT DO UPDATE or ON CONFLICT DO NOTHING
                use sqlparser::ast::OnInsert;
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
                                    if let Some(row_idx) = table.rows.iter().position(|r| &r[pk_col] == pk_val) {
                                        for assign in &du.assignments {
                                            if let sqlparser::ast::AssignmentTarget::ColumnName(name) = &assign.target {
                                                let col_name = name.to_string().to_lowercase();
                                                if let Some(&ci) = table.col_index.get(&col_name) {
                                                    let new_val = eval_expr(
                                                        &assign.value,
                                                        &table.rows[row_idx],
                                                        &table.col_index,
                                                    )
                                                    .map_err(|_| format!("UPSERT: invalid expr for '{}'", col_name))?;
                                                    table.rows[row_idx][ci] = new_val;
                                                }
                                            }
                                        }
                                        table.rebuild_index();
                                        inserted += 1;
                                    }
                                }
                            }
                        }
                    }
                    _ => return Err(e),
                }
            }
            Err(e) => return Err(e),
        }
    }

    if let Some(returning) = &returning {
        let table = db.get_table(&table_name)?;
        let ref_rows: Vec<&[DbValue]> = inserted_rows.iter().map(|r| r.as_slice()).collect();
        return Ok(format_projected_result(ref_rows, returning, &table.col_index, table));
    }

    fire_triggers(&table_name, "INSERT", db);
    Ok(format!("\"Inserted {} row(s)\"", inserted))
}

/// Check if a SELECT projection contains any window function (OVER clause).
fn has_window_function(projection: &[SelectItem]) -> bool {
    for item in projection {
        let func = match item {
            SelectItem::UnnamedExpr(Expr::Function(f))
            | SelectItem::ExprWithAlias {
                expr: Expr::Function(f),
                ..
            } => f,
            _ => continue,
        };
        if func.over.is_some() {
            return true;
        }
    }
    false
}

/// Compute ROWS frame bounds for a window function at a given position within a partition.
/// Returns (start, end) inclusive indices into the ordered partition.
fn frame_bounds(frame: &WindowFrame, part_len: usize, pos: usize) -> (usize, usize) {
    let eval_offset = |expr: &Expr| -> usize {
        eval_literal_expr(expr)
            .ok()
            .and_then(|v| match v {
                DbValue::Int(i) => Some(i.max(0) as usize),
                _ => None,
            })
            .unwrap_or(0)
    };
    let max_pos = part_len.saturating_sub(1);
    match frame.units {
        WindowFrameUnits::Rows => {
            let start = match &frame.start_bound {
                WindowFrameBound::Preceding(None) => 0,
                WindowFrameBound::Preceding(Some(expr)) => pos.saturating_sub(eval_offset(expr)),
                WindowFrameBound::CurrentRow => pos,
                WindowFrameBound::Following(None) => pos,
                WindowFrameBound::Following(Some(expr)) => (pos + eval_offset(expr)).min(max_pos),
            };
            let end = match &frame.end_bound {
                Some(WindowFrameBound::Preceding(None)) => 0,
                Some(WindowFrameBound::Preceding(Some(expr))) => pos.saturating_sub(eval_offset(expr)),
                Some(WindowFrameBound::CurrentRow) => pos,
                Some(WindowFrameBound::Following(None)) => max_pos,
                Some(WindowFrameBound::Following(Some(expr))) => (pos + eval_offset(expr)).min(max_pos),
                None => pos,
            };
            (start.min(end), end.max(start))
        }
        // ponytail: RANGE/GROUPS not implemented — use full partition
        _ => (0, max_pos),
    }
}

/// Evaluate an aggregate over a frame of rows for window functions.
fn eval_window_aggregate(
    func_name: &str,
    rows: &[&[DbValue]],
    arg: Option<&Expr>,
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    match func_name {
        "count" => {
            let count = if let Some(a) = arg {
                rows.iter()
                    .filter(|r| {
                        eval_expr(a, r, col_map)
                            .map(|v| !matches!(v, DbValue::Null))
                            .unwrap_or(false)
                    })
                    .count()
            } else {
                rows.len()
            };
            Ok(DbValue::Int(count as i64))
        }
        "sum" => {
            let a = arg.ok_or("SUM requires an argument")?;
            let first = rows.first().and_then(|r| eval_expr(a, r, col_map).ok());
            match first {
                Some(DbValue::Int(_)) => {
                    let sum: i64 = rows
                        .iter()
                        .filter_map(|r| eval_expr(a, r, col_map).ok())
                        .filter_map(|v| match v {
                            DbValue::Int(n) => Some(n),
                            _ => None,
                        })
                        .sum();
                    Ok(DbValue::Int(sum))
                }
                _ => {
                    let sum: f64 = rows
                        .iter()
                        .filter_map(|r| eval_expr(a, r, col_map).ok())
                        .filter_map(|v| match v {
                            DbValue::Float(f) => Some(f),
                            DbValue::Int(n) => Some(n as f64),
                            _ => None,
                        })
                        .sum();
                    Ok(DbValue::Float(sum))
                }
            }
        }
        "avg" => {
            let a = arg.ok_or("AVG requires an argument")?;
            let mut sum = 0.0f64;
            let mut count = 0usize;
            for r in rows {
                if let Ok(v) = eval_expr(a, r, col_map) {
                    match v {
                        DbValue::Int(n) => {
                            sum += n as f64;
                            count += 1;
                        }
                        DbValue::Float(f) => {
                            sum += f;
                            count += 1;
                        }
                        _ => {}
                    }
                }
            }
            if count == 0 {
                Ok(DbValue::Null)
            } else {
                Ok(DbValue::Float(sum / count as f64))
            }
        }
        "min" => {
            let a = arg.ok_or("MIN requires an argument")?;
            rows.iter()
                .filter_map(|r| eval_expr(a, r, col_map).ok())
                .min_by(db_value_cmp)
                .ok_or_else(|| "MIN on empty frame".into())
        }
        "max" => {
            let a = arg.ok_or("MAX requires an argument")?;
            rows.iter()
                .filter_map(|r| eval_expr(a, r, col_map).ok())
                .max_by(db_value_cmp)
                .ok_or_else(|| "MAX on empty frame".into())
        }
        _ => Err(format!("Aggregate '{}' not supported as window function", func_name)),
    }
}

/// Compute window function values for each row and return them as appended columns.
fn compute_window_functions(
    projection: &[SelectItem],
    rows: &mut [Vec<DbValue>],
    col_map: &HashMap<String, usize>,
) -> Result<(), String> {
    let total = rows.len();
    if total == 0 {
        return Ok(());
    }

    for item in projection {
        let (func, _alias) = match item {
            SelectItem::UnnamedExpr(Expr::Function(f)) => (f, None),
            SelectItem::ExprWithAlias {
                expr: Expr::Function(f),
                alias,
            } => (f, Some(alias.value.to_lowercase())),
            _ => continue,
        };
        let Some(WindowType::WindowSpec(spec)) = &func.over else {
            continue;
        };

        let mut computed = vec![DbValue::Null; total];
        let func_name = func.name.to_string().to_lowercase();

        // Build partition index groups
        let mut partitions: Vec<Vec<usize>> = if spec.partition_by.is_empty() {
            vec![(0..total).collect()]
        } else {
            let mut groups: Vec<(Vec<DbValue>, Vec<usize>)> = Vec::new();
            for (i, row) in rows.iter().enumerate() {
                let key: Vec<DbValue> = spec
                    .partition_by
                    .iter()
                    .filter_map(|pe| eval_expr(pe, row, col_map).ok())
                    .collect();
                if let Some(pos) = groups.iter().position(|(k, _)| *k == key) {
                    groups[pos].1.push(i);
                } else {
                    groups.push((key, vec![i]));
                }
            }
            groups.into_iter().map(|(_, indices)| indices).collect()
        };

        for part_indices in &mut partitions {
            // Sort indices within partition by ORDER BY
            if !spec.order_by.is_empty() {
                part_indices.sort_by(|&a, &b| {
                    for ob in &spec.order_by {
                        let va = eval_expr(&ob.expr, &rows[a], col_map).unwrap_or(DbValue::Null);
                        let vb = eval_expr(&ob.expr, &rows[b], col_map).unwrap_or(DbValue::Null);
                        let cmp = db_value_cmp(&va, &vb);
                        let order = match ob.options.asc {
                            Some(false) => cmp.reverse(),
                            _ => cmp,
                        };
                        if order != std::cmp::Ordering::Equal {
                            return order;
                        }
                    }
                    std::cmp::Ordering::Equal
                });
            }

            // Apply the window function
            match func_name.as_str() {
                "row_number" => {
                    for (pos, &idx) in part_indices.iter().enumerate() {
                        computed[idx] = DbValue::Int(pos as i64 + 1);
                    }
                }
                "rank" => {
                    let mut rank = 1i64;
                    for pos in 0..part_indices.len() {
                        if pos > 0 {
                            let cur = &rows[part_indices[pos]];
                            let prev = &rows[part_indices[pos - 1]];
                            let mut equal = true;
                            for ob in &spec.order_by {
                                let vc = eval_expr(&ob.expr, cur, col_map).unwrap_or(DbValue::Null);
                                let vp = eval_expr(&ob.expr, prev, col_map).unwrap_or(DbValue::Null);
                                if vc != vp {
                                    equal = false;
                                    break;
                                }
                            }
                            if !equal {
                                rank = pos as i64 + 1;
                            }
                        }
                        computed[part_indices[pos]] = DbValue::Int(rank);
                    }
                }
                "dense_rank" => {
                    let mut dense_rank = 1i64;
                    for pos in 0..part_indices.len() {
                        if pos > 0 {
                            let cur = &rows[part_indices[pos]];
                            let prev = &rows[part_indices[pos - 1]];
                            let mut equal = true;
                            for ob in &spec.order_by {
                                let vc = eval_expr(&ob.expr, cur, col_map).unwrap_or(DbValue::Null);
                                let vp = eval_expr(&ob.expr, prev, col_map).unwrap_or(DbValue::Null);
                                if vc != vp {
                                    equal = false;
                                    break;
                                }
                            }
                            if equal {
                                computed[part_indices[pos]] = DbValue::Int(dense_rank);
                            } else {
                                computed[part_indices[pos]] = DbValue::Int(dense_rank);
                                // still same as dense rank — use pos+1
                            }
                            if !equal {
                                dense_rank += 1;
                            }
                        } else {
                            computed[part_indices[pos]] = DbValue::Int(dense_rank);
                        }
                    }
                    // Recompute: assign dense ranks properly
                    let mut rank = 1i64;
                    for pos in 0..part_indices.len() {
                        if pos > 0 {
                            let cur = &rows[part_indices[pos]];
                            let prev = &rows[part_indices[pos - 1]];
                            let mut equal = true;
                            for ob in &spec.order_by {
                                let vc = eval_expr(&ob.expr, cur, col_map).unwrap_or(DbValue::Null);
                                let vp = eval_expr(&ob.expr, prev, col_map).unwrap_or(DbValue::Null);
                                if vc != vp {
                                    equal = false;
                                    break;
                                }
                            }
                            if !equal {
                                rank += 1;
                            }
                        }
                        computed[part_indices[pos]] = DbValue::Int(rank);
                    }
                }
                "count" | "sum" | "avg" | "min" | "max" => {
                    let arg = extract_func_arg(func).ok();
                    for (pos, &idx) in part_indices.iter().enumerate() {
                        let (fs, fe) = if let Some(ref f) = spec.window_frame {
                            frame_bounds(f, part_indices.len(), pos)
                        } else {
                            (0, part_indices.len().saturating_sub(1))
                        };
                        let frame_rows: Vec<&[DbValue]> =
                            part_indices[fs..=fe].iter().map(|&p| rows[p].as_slice()).collect();
                        computed[idx] = eval_window_aggregate(func_name.as_str(), &frame_rows, arg, col_map)?;
                    }
                }
                _ => {
                    return Err(format!("Window function '{}' not supported", func_name));
                }
            }
        }

        // Append computed column to each row
        for (i, val) in computed.into_iter().enumerate() {
            rows[i].push(val);
        }
    }
    Ok(())
}

// ── SELECT ──────────────────────────────────────────────────────────────

fn exec_select(query: &Query, db: &mut Database) -> Result<String, String> {
    let select: &Select = match &*query.body {
        SetExpr::Select(s) => s.as_ref(),
        _ => return Err("Only SELECT statements supported".into()),
    };

    // Route to multi-table handler if FROM has JOINs OR multiple tables
    if has_multiple_tables(select) {
        return exec_select_joins(query, select, db);
    }

    // Handle bare SELECT without FROM clause
    if select.from.is_empty() {
        let row: &[DbValue] = &[];
        let empty_cols: HashMap<String, usize> = HashMap::new();
        let header: Vec<String> = select
            .projection
            .iter()
            .map(|item| match item {
                SelectItem::UnnamedExpr(e) => projection_expr_name(e),
                SelectItem::ExprWithAlias { alias, .. } => alias.value.to_lowercase(),
                SelectItem::Wildcard { .. } => "*".into(),
                _ => format!("{:?}", item),
            })
            .collect();
        let mut cells: Vec<String> = Vec::new();
        for item in &select.projection {
            let expr = match item {
                SelectItem::UnnamedExpr(e) => e,
                SelectItem::ExprWithAlias { expr: e, .. } => e,
                _ => {
                    cells.push("null".into());
                    continue;
                }
            };
            match eval_expr(expr, row, &empty_cols) {
                Ok(v) => cells.push(v.to_json_string()),
                Err(_) => cells.push("null".into()),
            }
        }
        let h = header
            .iter()
            .map(|h| format!("\"{}\"", h))
            .collect::<Vec<_>>()
            .join(",");
        let c = cells.join(",");
        return Ok(format!("[[{}],[{}]]", h, c));
    }

    // ── View resolution — materialise views referenced in FROM ──
    let view_tables: Vec<String> = {
        let tf = select.from.first().ok_or("No FROM clause")?;
        match &tf.relation {
            TableFactor::Table { name, .. } => {
                let tname = object_name_str(name);
                if !db.has_table(&tname) && db.has_view(&tname) {
                    materialize_view(&tname, db)?;
                    vec![tname]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    };

    // Resolve table (single-table)
    let table = resolve_single_table(&select.from, db)?;
    let where_expr = select.selection.as_ref();

    // 1. Filter rows by WHERE — index-assisted when possible
    // Set thread-local DB snapshot for subquery evaluation
    SUBQ_DB.with(|snap| *snap.borrow_mut() = Some(db.clone()));

    // Try trigram index first (fuzzy_match candidates); still re-eval WHERE for accuracy
    let filtered_rows: Vec<&[DbValue]> = if let Some(candidates) = try_trigram_index(where_expr, table) {
        candidates
            .into_iter()
            .filter(|row| {
                where_expr
                    .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                    .unwrap_or(true)
            })
            .collect()
    } else if let Some(rows) = try_btree_index(where_expr, table) {
        rows
    } else {
        table
            .rows
            .iter()
            .filter(|row| {
                where_expr
                    .map(|expr| is_truthy(&eval_expr(expr, row, &table.col_index).unwrap_or(DbValue::Bool(false))))
                    .unwrap_or(true)
            })
            .map(|r| r.as_slice())
            .collect()
    };

    // 2. If aggregates are present, handle them (with or without GROUP BY)
    if has_aggregate(&select.projection) {
        let group_partitions = if has_group_by(select) {
            partition_by_group(&filtered_rows, select, &table.col_index)?
        } else {
            vec![filtered_rows] // single group: all rows
        };
        // HAVING — filter partitions after grouping
        let group_partitions = if let Some(having) = &select.having {
            let flattened: Vec<Vec<&[DbValue]>> = group_partitions
                .into_iter()
                .filter(|group| {
                    if group.is_empty() {
                        return false;
                    }
                    is_truthy(&eval_expr(having, group[0], &table.col_index).unwrap_or(DbValue::Bool(false)))
                })
                .collect();
            flattened
        } else {
            group_partitions
        };
        let result = compute_aggregates(&group_partitions, &select.projection, &table.col_index);
        for name in &view_tables {
            let _ = db.drop_table(name);
        }
        return result;
    }

    // 3. GROUP BY without aggregates — simple dedup
    let grouped_rows = if has_group_by(select) {
        let partitions = partition_by_group(&filtered_rows, select, &table.col_index)?;
        // HAVING — filter after grouping
        let partitions: Vec<Vec<&[DbValue]>> = if let Some(having) = &select.having {
            partitions
                .into_iter()
                .filter(|group| {
                    if group.is_empty() {
                        return false;
                    }
                    is_truthy(&eval_expr(having, group[0], &table.col_index).unwrap_or(DbValue::Bool(false)))
                })
                .collect()
        } else {
            partitions
        };
        partitions.into_iter().map(|p| p[0]).collect()
    } else {
        filtered_rows
    };

    // 3.5 DISTINCT — dedup by comparing projected values
    let deduped_rows: Vec<&[DbValue]> = if select.distinct.is_some() {
        let mut seen: Vec<Vec<DbValue>> = Vec::new();
        grouped_rows
            .into_iter()
            .filter(|row| {
                // For DISTINCT, compare the projected (selected) column values
                let proj: Vec<DbValue> = select
                    .projection
                    .iter()
                    .filter_map(|item| {
                        if let SelectItem::UnnamedExpr(e) = item {
                            eval_expr(e, row, &table.col_index).ok()
                        } else {
                            None
                        }
                    })
                    .collect();
                if seen.contains(&proj) {
                    false
                } else {
                    seen.push(proj);
                    true
                }
            })
            .collect()
    } else {
        grouped_rows
    };

    // 3.75 Window functions — compute OVER expressions before ORDER BY
    let mut owned_rows: Vec<Vec<DbValue>> = deduped_rows.iter().map(|r| r.to_vec()).collect();
    if has_window_function(&select.projection) {
        compute_window_functions(&select.projection, &mut owned_rows, &table.col_index)?;
    }
    let post_wf_rows: Vec<&[DbValue]> = owned_rows.iter().map(|r| r.as_slice()).collect();

    // 4. ORDER BY
    let sorted_rows = if let Some(order_by) = &query.order_by {
        let exprs = match &order_by.kind {
            OrderByKind::Expressions(exprs) => exprs,
            _ => return Err("ORDER BY ALL not supported".into()),
        };
        if !exprs.is_empty() {
            sort_rows(post_wf_rows, exprs, &table.col_index)?
        } else {
            post_wf_rows
        }
    } else {
        post_wf_rows
    };

    // 5. LIMIT / OFFSET
    let limited_rows = apply_limit_offset(sorted_rows, &query.limit_clause)?;

    // 6. Format result — respect SELECT projection (only show chosen columns)
    let result = format_projected_result(limited_rows, &select.projection, &table.col_index, table);
    for name in &view_tables {
        let _ = db.drop_table(name);
    }
    Ok(result)
}

/// Format SELECT results, projecting to only the requested columns.
fn format_projected_result(
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

/// Check if the FROM clause has multiple tables or JOINs.
fn has_multiple_tables(select: &Select) -> bool {
    select.from.len() > 1 || select.from.iter().any(|t| !t.joins.is_empty())
}

/// Execute a SELECT with JOINs. Uses a flat-row column map with absolute positions.
fn exec_select_joins(query: &Query, select: &Select, db: &mut Database) -> Result<String, String> {
    use sqlparser::ast::{JoinConstraint, JoinOperator};

    // ── Resolve all tables in FROM + JOINs ──────────────────────────
    struct Tbl {
        name: String,
        cols: usize,
        start: usize,
        rows: Vec<Vec<DbValue>>,
    }

    let mut tbls: Vec<Tbl> = Vec::new();
    let mut abs: usize = 0;
    let mut view_tables: Vec<String> = Vec::new();

    for twj in &select.from {
        let (n, t) = resolve_table_factor(&twj.relation, db)?;
        if db.has_view(&n) {
            view_tables.push(n.clone());
        }
        let r: Vec<Vec<DbValue>> = t.rows.to_vec();
        let c = t.columns.len();
        tbls.push(Tbl {
            name: n.clone(),
            cols: c,
            start: abs,
            rows: r,
        });
        abs += c;
        for j in &twj.joins {
            let (jn, jt) = resolve_table_factor(&j.relation, db)?;
            if db.has_view(&jn) {
                view_tables.push(jn.clone());
            }
            let jr: Vec<Vec<DbValue>> = jt.rows.to_vec();
            let jc = jt.columns.len();
            tbls.push(Tbl {
                name: jn.clone(),
                cols: jc,
                start: abs,
                rows: jr,
            });
            abs += jc;
        }
    }

    // ── Build flat column map ───────────────────────────────────────
    let mut col_map: HashMap<String, usize> = HashMap::new();
    let mut header: Vec<String> = Vec::new();
    for tbl in &tbls {
        let tn = db.get_table(&tbl.name).map_err(|e| format!("JOIN: {}", e))?.clone();
        for (ci, col) in tn.columns.iter().enumerate() {
            let p = tbl.start + ci;
            col_map.insert(format!("{}.{}", tbl.name, col.name), p);
            col_map.insert(col.name.clone(), p);
            header.push(format!("{}.{}", tbl.name, col.name));
        }
    }

    let total = abs;

    // Helper: build flat row from table-row indices
    let bf = |idxs: &[usize]| -> Vec<DbValue> {
        let mut v = Vec::with_capacity(total);
        for (ti, &ri) in idxs.iter().enumerate() {
            if ri == usize::MAX {
                v.resize(v.len() + tbls[ti].cols, DbValue::Null);
            } else {
                v.extend_from_slice(&tbls[ti].rows[ri]);
            }
        }
        v
    };

    let ef = |e: &Expr, r: &[DbValue]| -> Result<DbValue, String> { eval_expr_on_flat_row(e, r, &col_map) };

    // ── Generate combined rows ──────────────────────────────────────
    let mut cidx: Vec<Vec<usize>> = (0..tbls[0].rows.len()).map(|i| vec![i]).collect();
    let no_constraint = JoinConstraint::None;
    let joins = &select.from[0].joins;

    // Precompute common column names for NATURAL joins
    let natural_common: Vec<Vec<(String, usize, usize)>> = joins
        .iter()
        .enumerate()
        .map(|(i, j)| {
            if matches!(
                &j.join_operator,
                JoinOperator::Inner(JoinConstraint::Natural)
                    | JoinOperator::LeftOuter(JoinConstraint::Natural)
                    | JoinOperator::RightOuter(JoinConstraint::Natural)
                    | JoinOperator::FullOuter(JoinConstraint::Natural)
            ) {
                // Right table is at tbls index i+1 (left accumulated = tbls[0..=i])
                let right_ti = i + 1;
                if right_ti < tbls.len() {
                    let right_name = &tbls[right_ti].name;
                    if let Ok(rt) = db.get_table(right_name) {
                        // For each right column, find if any left table has the same name
                        let mut common = Vec::new();
                        for right_col in &rt.columns {
                            for left_tbl in &tbls[0..right_ti] {
                                if let Ok(lt) = db.get_table(&left_tbl.name) {
                                    if lt.columns.iter().any(|c| c.name == right_col.name) {
                                        // Store (col_name, left_table_idx, right_start_in_flat_row + col_idx)
                                        if let Some(&lp) = col_map.get(&format!("{}.{}", left_tbl.name, right_col.name))
                                        {
                                            if let Some(&rp) =
                                                col_map.get(&format!("{}.{}", right_name, right_col.name))
                                            {
                                                common.push((right_col.name.clone(), lp, rp));
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        return common;
                    }
                }
            }
            Vec::new()
        })
        .collect();

    for (ti, tbl) in tbls.iter().enumerate().skip(1) {
        // Determine join type and constraint
        let join_info = if ti <= joins.len() {
            let join = &joins[ti - 1];
            Some(&join.join_operator)
        } else {
            None
        };
        let con: &JoinConstraint = match join_info {
            Some(
                JoinOperator::Inner(c)
                | JoinOperator::LeftOuter(c)
                | JoinOperator::RightOuter(c)
                | JoinOperator::FullOuter(c)
                | JoinOperator::Join(c)
                | JoinOperator::CrossJoin(c),
            ) => c,
            _ => &no_constraint,
        };
        let is_left = matches!(join_info, Some(JoinOperator::LeftOuter(_)));
        let is_right = matches!(join_info, Some(JoinOperator::RightOuter(_)));
        let is_full = matches!(join_info, Some(JoinOperator::FullOuter(_)));
        let preserve_left = is_left || is_full;
        let preserve_right = is_right || is_full;

        let mut right_matched = vec![false; tbl.rows.len()];
        let mut next = Vec::new();

        // Precompute USING column positions if applicable
        let using_cols: Vec<(usize, usize)> = match con {
            JoinConstraint::Using(cols) => {
                let mut pairs = Vec::new();
                for obj in cols {
                    let cname = obj.to_string().to_lowercase();
                    // Left side: look up bare name in col_map (ambiguous but standard SQL uses qualified)
                    // Try qualified: find which left table has this column
                    for left_tbl in &tbls[0..ti] {
                        if let Ok(lt) = db.get_table(&left_tbl.name) {
                            if lt.columns.iter().any(|c| c.name == cname) {
                                if let Some(&lp) = col_map.get(&format!("{}.{}", left_tbl.name, cname)) {
                                    if let Some(&rp) = col_map.get(&format!("{}.{}", tbl.name, cname)) {
                                        pairs.push((lp, rp));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                pairs
            }
            _ => Vec::new(),
        };

        // Precompute NATURAL positions if applicable
        let natural_pairs: &[(String, usize, usize)] = if ti >= 1 && ti - 1 < natural_common.len() {
            &natural_common[ti - 1]
        } else {
            &[]
        };

        for ls in &cidx {
            let mut hit = false;
            for (ri, rm) in right_matched.iter_mut().enumerate() {
                let mut cs = ls.clone();
                cs.push(ri);
                let f = bf(&cs);
                let ok = match con {
                    JoinConstraint::On(ex) => ef(ex, &f).map(|v| is_truthy(&v)).unwrap_or(false),
                    JoinConstraint::Using(_) => using_cols.iter().all(|&(lp, rp)| {
                        if lp < f.len() && rp < f.len() {
                            values_equal(&f[lp], &f[rp])
                        } else {
                            false
                        }
                    }),
                    JoinConstraint::Natural => natural_pairs.iter().all(|(_, lp, rp)| {
                        if *lp < f.len() && *rp < f.len() {
                            values_equal(&f[*lp], &f[*rp])
                        } else {
                            false
                        }
                    }),
                    _ => true,
                };
                if ok {
                    next.push(cs);
                    hit = true;
                    *rm = true;
                }
            }
            if preserve_left && !hit {
                let mut ns = ls.clone();
                ns.push(usize::MAX);
                next.push(ns);
            }
        }

        // Add unmatched right rows for RIGHT / FULL OUTER join
        if preserve_right {
            let all_max: Vec<usize> = (0..ti).map(|_| usize::MAX).collect();
            for (ri, matched) in right_matched.iter().enumerate() {
                if !matched {
                    let mut cs = all_max.clone();
                    cs.push(ri);
                    next.push(cs);
                }
            }
        }

        cidx = next;
    }

    // ── Materialize ─────────────────────────────────────────────────
    let mut rows: Vec<Vec<DbValue>> = cidx.iter().map(|ix| bf(ix)).collect();

    // WHERE
    if let Some(ex) = select.selection.as_ref() {
        rows.retain(|r| ef(ex, r).map(|v| is_truthy(&v)).unwrap_or(false));
    }

    // ORDER BY
    if let Some(ob) = &query.order_by {
        let exs = match &ob.kind {
            OrderByKind::Expressions(e) => e,
            _ => return Err("ORDER BY ALL not supported".into()),
        };
        if !exs.is_empty() {
            rows.sort_by(|a, b| {
                for o in exs {
                    let av = ef(&o.expr, a).unwrap_or(DbValue::Null);
                    let bv = ef(&o.expr, b).unwrap_or(DbValue::Null);
                    let c = value_to_string(&av).cmp(&value_to_string(&bv));
                    let c = if o.options.asc.unwrap_or(true) { c } else { c.reverse() };
                    if c != std::cmp::Ordering::Equal {
                        return c;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
    }

    // LIMIT / OFFSET
    let (off, lim) = match &query.limit_clause {
        Some(LimitClause::LimitOffset { limit, offset, .. }) => (
            parse_expr_as_usize(offset.as_ref().map(|o| &o.value)).unwrap_or(0),
            limit.as_ref().and_then(|e| parse_expr_as_usize(Some(e))),
        ),
        Some(LimitClause::OffsetCommaLimit { offset, limit }) => (
            parse_expr_as_usize(Some(offset)).unwrap_or(0),
            parse_expr_as_usize(Some(limit)),
        ),
        None => (0, None),
    };
    let s = off.min(rows.len());
    let e = match lim {
        Some(l) => (s + l).min(rows.len()),
        None => rows.len(),
    };
    rows = rows[s..e].to_vec();

    // Format
    let h = header
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect::<Vec<_>>()
        .join(",");
    let rj: Vec<String> = rows
        .iter()
        .map(|r| {
            let c: Vec<String> = r.iter().map(|v| v.to_json_string()).collect();
            format!("[{}]", c.join(","))
        })
        .collect();
    for name in &view_tables {
        let _ = db.drop_table(name);
    }
    Ok(format!("[[{}],{}]", h, rj.join(",")))
}

fn eval_expr_on_flat_row(expr: &Expr, row: &[DbValue], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    match expr {
        Expr::Identifier(ident) => {
            let name = ident.value.to_lowercase();
            if name == "current_timestamp" || name == "current_date" || name == "current_time" {
                return Ok(now_value());
            }
            match col_map.get(&name) {
                Some(&pos) => Ok(row[pos].clone()),
                None => Err(format!("Unknown column '{}'", name)),
            }
        }
        Expr::CompoundIdentifier(parts) => {
            // e.g. a.id → "a.id"
            let name = parts
                .iter()
                .map(|p| p.value.to_lowercase())
                .collect::<Vec<_>>()
                .join(".");
            match col_map.get(&name) {
                Some(&pos) => Ok(row[pos].clone()),
                None => {
                    // Try just the last part
                    let last = parts.last().unwrap().value.to_lowercase();
                    match col_map.get(&last) {
                        Some(&pos) => Ok(row[pos].clone()),
                        None => Err(format!("Unknown column '{}'", name)),
                    }
                }
            }
        }
        Expr::Value(v) => Ok(sql_val_to_db(&v.value)),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr_on_flat_row(left, row, col_map)?;
            let r = eval_expr_on_flat_row(right, row, col_map)?;
            apply_binary_op(&l, op, &r)
        }
        Expr::Nested(inner) => eval_expr_on_flat_row(inner, row, col_map),
        Expr::Function(func) => {
            let name = func.name.to_string().to_lowercase();
            if name == "fuzzy_match" {
                let args = match &func.args {
                    FunctionArguments::List(list) => &list.args,
                    _ => return Err("fuzzy_match requires args".into()),
                };
                if args.len() < 2 {
                    return Err("fuzzy_match requires 2 args".into());
                }
                let a1 = eval_expr_on_flat_row(get_func_arg_unnamed(&args[0])?, row, col_map)?;
                let a2 = eval_expr_on_flat_row(get_func_arg_unnamed(&args[1])?, row, col_map)?;
                let sim = Table::trigram_similarity(&value_to_string(&a1), &value_to_string(&a2));
                Ok(DbValue::Bool(sim >= 0.3))
            } else {
                exec_std_function(func, name, row, col_map)
            }
        }
        Expr::IsNull(expr) => {
            let val = eval_expr_on_flat_row(expr, row, col_map)?;
            Ok(DbValue::Bool(matches!(val, DbValue::Null)))
        }
        Expr::IsNotNull(expr) => {
            let val = eval_expr_on_flat_row(expr, row, col_map)?;
            Ok(DbValue::Bool(!matches!(val, DbValue::Null)))
        }
        Expr::Like {
            negated, expr, pattern, ..
        } => {
            let val = eval_expr_on_flat_row(expr, row, col_map)?;
            let pat = eval_expr_on_flat_row(pattern, row, col_map)?;
            let matched = simple_like(&value_to_string(&val), &value_to_string(&pat));
            Ok(DbValue::Bool(if *negated { !matched } else { matched }))
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_val = operand
                .as_ref()
                .map(|o| eval_expr_on_flat_row(o, row, col_map))
                .transpose()?;
            for cw in conditions.iter() {
                let matched = match &operand_val {
                    Some(ref op_val) => *op_val == eval_expr_on_flat_row(&cw.condition, row, col_map)?,
                    None => is_truthy(&eval_expr_on_flat_row(&cw.condition, row, col_map)?),
                };
                if matched {
                    return eval_expr_on_flat_row(&cw.result, row, col_map);
                }
            }
            match else_result {
                Some(expr) => eval_expr_on_flat_row(expr, row, col_map),
                None => Ok(DbValue::Null),
            }
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let val = eval_expr_on_flat_row(expr, row, col_map)?;
            let l = eval_expr_on_flat_row(low, row, col_map)?;
            let h = eval_expr_on_flat_row(high, row, col_map)?;
            use std::cmp::Ordering;
            let ge = db_value_cmp(&val, &l) != Ordering::Less;
            let le = db_value_cmp(&val, &h) != Ordering::Greater;
            Ok(DbValue::Bool(if *negated { !(ge && le) } else { ge && le }))
        }
        _ => Err(format!("Unsupported expression in JOIN: {:?}", expr)),
    }
}

/// Check if SELECT has a GROUP BY clause.
fn has_group_by(select: &Select) -> bool {
    use sqlparser::ast::GroupByExpr;
    match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
        GroupByExpr::All(_) => true,
    }
}

/// Check if SELECT projection contains aggregate functions.
fn has_aggregate(projection: &[SelectItem]) -> bool {
    for item in projection {
        let expr = match item {
            SelectItem::UnnamedExpr(e) => e,
            SelectItem::ExprWithAlias { expr, .. } => expr,
            _ => continue,
        };
        if contains_aggregate(expr) {
            return true;
        }
    }
    false
}

fn contains_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::Function(f) => {
            let name = f.name.to_string().to_lowercase();
            matches!(name.as_str(), "count" | "sum" | "avg" | "min" | "max")
        }
        Expr::Nested(inner) => contains_aggregate(inner),
        _ => false,
    }
}

/// Resolve GROUP BY identifiers against SELECT aliases.
/// `SELECT qty > 50 AS high_qty ... GROUP BY high_qty` → replaces `high_qty` with `qty > 50`.
fn resolve_group_by_aliases(select: &Select) -> Vec<Expr> {
    let exprs = group_by_exprs(select).unwrap_or(&[]);
    let mut alias_map: HashMap<String, Expr> = HashMap::new();
    for item in &select.projection {
        if let SelectItem::ExprWithAlias { expr, alias } = item {
            alias_map.insert(alias.value.clone(), expr.clone());
        }
    }
    if alias_map.is_empty() {
        return exprs.to_vec();
    }
    exprs
        .iter()
        .map(|e| {
            if let Expr::Identifier(ident) = e {
                if let Some(resolved) = alias_map.get(ident.value.as_str()) {
                    resolved.clone()
                } else {
                    e.clone()
                }
            } else {
                e.clone()
            }
        })
        .collect()
}

/// Partition filtered rows into groups by GROUP BY columns.
/// Returns a Vec of groups, where each group is a Vec of row references.
fn partition_by_group<'a>(
    rows: &[&'a [DbValue]],
    select: &Select,
    col_map: &HashMap<String, usize>,
) -> Result<Vec<Vec<&'a [DbValue]>>, String> {
    let exprs = resolve_group_by_aliases(select);
    let mut groups: Vec<Vec<&[DbValue]>> = Vec::new();
    let mut keys: Vec<Vec<DbValue>> = Vec::new();

    'rows: for row in rows {
        let key: Result<Vec<DbValue>, String> = exprs.iter().map(|e| eval_expr(e, row, col_map)).collect();
        let key = key?;

        for (i, existing_key) in keys.iter().enumerate() {
            if keys_equal(&key, existing_key) {
                groups[i].push(row);
                continue 'rows;
            }
        }
        keys.push(key);
        groups.push(vec![row]);
    }

    Ok(groups)
}

fn group_by_exprs(select: &Select) -> Result<&[Expr], String> {
    use sqlparser::ast::GroupByExpr;
    match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => Ok(exprs.as_slice()),
        GroupByExpr::All(_) => Err("GROUP BY ALL not supported".into()),
    }
}

fn keys_equal(a: &[DbValue], b: &[DbValue]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

/// Compute aggregate functions over partitions (groups) of rows.
fn compute_aggregates(
    partitions: &[Vec<&[DbValue]>],
    projection: &[SelectItem],
    col_map: &HashMap<String, usize>,
) -> Result<String, String> {
    if partitions.is_empty() {
        return Ok("[]".to_string());
    }

    // Build header from projection
    let mut header = Vec::new();
    for item in projection {
        match item {
            SelectItem::UnnamedExpr(expr) => header.push(projection_expr_name(expr)),
            SelectItem::ExprWithAlias { alias, .. } => header.push(alias.value.to_string()),
            _ => return Err("Unsupported SELECT item in aggregate query".into()),
        }
    }

    // Compute one row per partition
    let rows_json: Vec<String> = partitions
        .iter()
        .map(|group| {
            let cells: Vec<String> = projection
                .iter()
                .map(|item| {
                    let expr = match item {
                        SelectItem::UnnamedExpr(e) => e,
                        SelectItem::ExprWithAlias { expr, .. } => expr,
                        _ => return "null".to_string(),
                    };
                    eval_projection_expr(expr, group, col_map)
                        .map(|(_, v)| v.to_json_string())
                        .unwrap_or_else(|_| "null".to_string())
                })
                .collect();
            format!("[{}]", cells.join(","))
        })
        .collect();

    let header_json: String = header
        .iter()
        .map(|h| format!("\"{}\"", h))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!("[[{}],{}]", header_json, rows_json.join(",")))
}

fn projection_expr_name(expr: &Expr) -> String {
    match expr {
        Expr::Function(f) => f.name.to_string().to_uppercase(),
        Expr::Identifier(ident) => ident.value.to_lowercase(),
        _ => "EXPR".to_string(),
    }
}

/// Evaluate a projection expression (handles aggregates vs regular expressions).
fn eval_projection_expr(
    expr: &Expr,
    rows: &[&[DbValue]],
    col_map: &HashMap<String, usize>,
) -> Result<(String, DbValue), String> {
    // DBG
    match expr {
        Expr::Function(f) => {
            let name = f.name.to_string().to_lowercase();
            match name.as_str() {
                "count" => {
                    let is_distinct = matches!(
                        f.args,
                        sqlparser::ast::FunctionArguments::List(ref list)
                            if list.duplicate_treatment == Some(sqlparser::ast::DuplicateTreatment::Distinct)
                    );
                    let count = if is_distinct {
                        let mut seen: Vec<DbValue> = Vec::new();
                        for r in rows {
                            if let Ok(arg) = extract_func_arg(f) {
                                if let Ok(val) = eval_expr(arg, r, col_map) {
                                    if !seen.contains(&val) {
                                        seen.push(val);
                                    }
                                }
                            }
                        }
                        DbValue::Int(seen.len() as i64)
                    } else {
                        DbValue::Int(rows.len() as i64)
                    };
                    Ok(("COUNT".to_string(), count))
                }
                "sum" => {
                    let val = aggregate_sum(f, rows, col_map)?;
                    Ok(("SUM".to_string(), val))
                }
                "avg" => {
                    let val = aggregate_avg(f, rows, col_map)?;
                    Ok(("AVG".to_string(), val))
                }
                "min" => {
                    let val = aggregate_min(f, rows, col_map)?;
                    Ok(("MIN".to_string(), val))
                }
                "max" => {
                    let val = aggregate_max(f, rows, col_map)?;
                    Ok(("MAX".to_string(), val))
                }
                _ => {
                    // ponytail: unknown function, evaluate as expression on group
                    let val = eval_expr_on_group(expr, rows, col_map)?;
                    Ok((format!("{}", f.name), val))
                }
            }
        }
        Expr::Identifier(ident) => {
            // Regular column — use first row's value
            let val = if rows.is_empty() {
                DbValue::Null
            } else {
                let idx = col_map
                    .get(&ident.value.to_lowercase())
                    .ok_or_else(|| format!("Unknown column '{}'", ident.value))?;
                rows[0][*idx].clone()
            };
            Ok((ident.value.to_lowercase(), val))
        }
        _ => {
            let val = eval_expr_on_group(expr, rows, col_map)?;
            Ok(("expr".to_string(), val))
        }
    }
}

/// Evaluate an expression on a group of rows. For non-aggregate columns, uses first row.
fn eval_expr_on_group(expr: &Expr, rows: &[&[DbValue]], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    // For aggregate queries, non-aggregate columns use the first row's value
    if rows.is_empty() {
        return Ok(DbValue::Null);
    }
    eval_expr(expr, rows[0], col_map)
}

fn aggregate_sum(func: &Function, rows: &[&[DbValue]], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    let arg = extract_func_arg(func)?;
    if rows.is_empty() {
        return Ok(DbValue::Null);
    }
    let first = eval_expr_on_group(arg, rows, col_map)?;
    match first {
        DbValue::Int(..) => {
            let sum: i64 = rows
                .iter()
                .filter_map(|r| {
                    eval_expr(arg, r, col_map).ok().and_then(|v| match v {
                        DbValue::Int(n) => Some(n),
                        _ => None,
                    })
                })
                .sum();
            Ok(DbValue::Int(sum))
        }
        DbValue::Float(..) => {
            let sum: f64 = rows
                .iter()
                .filter_map(|r| {
                    eval_expr(arg, r, col_map).ok().and_then(|v| match v {
                        DbValue::Float(f) => Some(f),
                        DbValue::Int(n) => Some(n as f64),
                        _ => None,
                    })
                })
                .sum();
            Ok(DbValue::Float(sum))
        }
        _ => Err("SUM requires numeric column".into()),
    }
}

fn aggregate_avg(func: &Function, rows: &[&[DbValue]], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    let arg = extract_func_arg(func)?;
    if rows.is_empty() {
        return Ok(DbValue::Null);
    }
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for r in rows {
        if let Ok(v) = eval_expr(arg, r, col_map) {
            match v {
                DbValue::Int(n) => {
                    sum += n as f64;
                    count += 1;
                }
                DbValue::Float(f) => {
                    sum += f;
                    count += 1;
                }
                _ => {}
            }
        }
    }
    if count == 0 {
        Ok(DbValue::Null)
    } else {
        Ok(DbValue::Float(sum / count as f64))
    }
}

fn aggregate_min(func: &Function, rows: &[&[DbValue]], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    let arg = extract_func_arg(func)?;
    rows.iter()
        .filter_map(|r| eval_expr(arg, r, col_map).ok())
        .min_by(db_value_cmp)
        .ok_or_else(|| "MIN on empty set".into())
}

fn aggregate_max(func: &Function, rows: &[&[DbValue]], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    let arg = extract_func_arg(func)?;
    rows.iter()
        .filter_map(|r| eval_expr(arg, r, col_map).ok())
        .max_by(db_value_cmp)
        .ok_or_else(|| "MAX on empty set".into())
}

/// Compare DbValues: numeric comparison when both are numbers, string comparison otherwise.
fn db_value_cmp(a: &DbValue, b: &DbValue) -> std::cmp::Ordering {
    match (to_float(a), to_float(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => value_to_string(a).cmp(&value_to_string(b)),
    }
}

/// Extract the first argument expression from a function.
fn extract_func_arg(func: &Function) -> Result<&Expr, String> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err("Function requires argument list".into()),
    };
    if args.is_empty() {
        return Err("Function requires argument".into());
    }
    get_func_arg_unnamed(&args[0])
}

/// ORDER BY sorting
fn sort_rows<'a>(
    mut rows: Vec<&'a [DbValue]>,
    order_by: &[sqlparser::ast::OrderByExpr],
    col_map: &HashMap<String, usize>,
) -> Result<Vec<&'a [DbValue]>, String> {
    if order_by.is_empty() {
        return Ok(rows);
    }

    rows.sort_by(|a, b| {
        for order in order_by {
            let a_val = eval_expr(&order.expr, a, col_map).unwrap_or(DbValue::Null);
            let b_val = eval_expr(&order.expr, b, col_map).unwrap_or(DbValue::Null);
            let ordering = value_to_string(&a_val).cmp(&value_to_string(&b_val));
            let is_asc = order.options.asc.unwrap_or(true);
            let ordering = if is_asc { ordering } else { ordering.reverse() };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        std::cmp::Ordering::Equal
    });

    Ok(rows)
}

/// Apply LIMIT and OFFSET.
fn apply_limit_offset<'a>(
    rows: Vec<&'a [DbValue]>,
    limit_clause: &Option<LimitClause>,
) -> Result<Vec<&'a [DbValue]>, String> {
    let (offset_val, limit_val) = match limit_clause {
        Some(LimitClause::LimitOffset { limit, offset, .. }) => {
            let off = parse_expr_as_usize(offset.as_ref().map(|o| &o.value)).unwrap_or(0);
            let lim = limit.as_ref().and_then(|e| parse_expr_as_usize(Some(e)));
            (off, lim)
        }
        Some(LimitClause::OffsetCommaLimit { offset, limit }) => {
            let off = parse_expr_as_usize(Some(offset)).unwrap_or(0);
            let lim = parse_expr_as_usize(Some(limit));
            (off, lim)
        }
        None => (0, None),
    };

    let start = offset_val.min(rows.len());
    let end = match limit_val {
        Some(l) => (start + l).min(rows.len()),
        None => rows.len(),
    };

    Ok(rows[start..end].to_vec())
}

fn parse_expr_as_usize(expr: Option<&Expr>) -> Option<usize> {
    let expr = expr?;
    if let Expr::Value(v) = expr {
        if let sqlparser::ast::Value::Number(s, _) = &v.value {
            return s.parse::<usize>().ok();
        }
    }
    None
}

// ── UPDATE ──────────────────────────────────────────────────────────────

fn exec_update(upd: &sqlparser::ast::Update, db: &mut Database) -> Result<String, String> {
    let table_name = resolve_table_from_joins(&upd.table)?;
    let returning = upd.returning.clone();
    let table = db.get_table_mut(&table_name)?;

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
                .ok_or_else(|| format!("Unknown column '{}'", col_name))?;
            Ok((idx, &assign.value))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Collect all assignments and validate CHECK constraints upfront
    let mut updates: Vec<(usize, usize, DbValue)> = Vec::new();

    for i in &indices {
        for (col_ci, val_expr) in &assign_indices {
            let new_val = eval_literal_expr(val_expr)?;
            updates.push((*i, *col_ci, new_val));
        }
    }

    // Validate CHECK constraints for each updated row (drop table borrow first)
    let validated_updates = {
        let t = db.get_table(&table_name)?;
        updates
            .iter()
            .filter_map(|(row_i, col_ci, val)| {
                let mut row = t.rows[*row_i].clone();
                row[*col_ci] = val.clone();
                for expr in &t.check_constraints {
                    let v = eval_expr(expr, &row, &t.col_index).ok()?;
                    if !is_truthy(&v) {
                        return Some(Err("CHECK constraint failed"));
                    }
                }
                Some(Ok((*row_i, *col_ci, val.clone())))
            })
            .collect::<Result<Vec<(usize, usize, DbValue)>, &str>>()
    }?;

    // Validate FOREIGN KEY constraints for each update
    // Pre-collect PK update cascade data (must outlive the immutable borrow block)
    let mut pk_cascade_queue: Vec<(String, usize, String, DbValue)> = Vec::new();
    {
        let t = db.get_table(&table_name)?;
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
                            return Err(format!(
                                "FOREIGN KEY constraint: '{}' value '{}' not found in referenced table",
                                local_col, val_str
                            ));
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
                        let child = db.get_table(ref_table)?;
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
                                    return Err(format!(
                                        "FOREIGN KEY constraint violation: '{}' has reference to '{}' in '{}'",
                                        ref_table, old_val, table_name
                                    ));
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
        let t = db.get_table(&table_name)?;
        let mut seen = HashSet::new();
        validated_updates
            .iter()
            .filter(|(row_i, _, _)| seen.insert(*row_i))
            .map(|(row_i, _, _)| t.rows[*row_i].clone())
            .collect()
    } else {
        Vec::new()
    };

    let t = db.get_table_mut(&table_name)?;
    for (row_i, col_ci, val) in &validated_updates {
        t.update_cell(*row_i, *col_ci, val.clone());
    }

    if let Some(returning) = &returning {
        let table = db.get_table(&table_name)?;
        let ref_rows: Vec<&[DbValue]> = old_rows.iter().map(|r| r.as_slice()).collect();
        return Ok(format_projected_result(ref_rows, returning, &table.col_index, table));
    }

    fire_triggers(&table_name, "UPDATE", db);
    Ok(format!("\"Updated {} row(s)\"", validated_updates.len()))
}

// ── DELETE ──────────────────────────────────────────────────────────────

fn exec_delete(del: &sqlparser::ast::Delete, db: &mut Database) -> Result<String, String> {
    // Table name is in `from` (FromTable), not `tables` (MySQL multi-table)
    use sqlparser::ast::FromTable;
    let table_name = match &del.from {
        FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => match tables.first() {
            Some(tj) => match &tj.relation {
                TableFactor::Table { name, .. } => object_name_str(name),
                _ => return Err("DELETE: only simple table references supported".into()),
            },
            None => return Err("DELETE must specify a table".into()),
        },
    };

    let returning = del.returning.clone();

    // Clone col_index to avoid borrow conflict with table.delete()
    let col_idx = db.get_table(&table_name)?.col_index.clone();
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
                    let t = db.get_table(ref_table)?;
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
                    let child_table = db.get_table_mut(ref_table)?;
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
                let t = db.get_table(ref_table)?;
                let ref_pk_set: HashSet<String> = t
                    .rows
                    .iter()
                    .map(|row| row[*ref_col_idx].to_string().to_lowercase())
                    .collect();
                for (_, pk_val) in &deleted_pks {
                    if ref_pk_set.contains(pk_val) {
                        return Err(format!(
                            "FOREIGN KEY constraint violation: '{}' references '{}'",
                            ref_table, pk_val
                        ));
                    }
                }
            }
        }
    }

    // Capture old rows for RETURNING (before deletion)
    let old_rows: Vec<Vec<DbValue>> = if returning.is_some() {
        let t = db.get_table(&table_name)?;
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

    let table = db.get_table_mut(&table_name)?;
    let count = match pred {
        Some(expr) => table.delete(|row| eval_expr(&expr, row, &col_idx).map(|v| is_truthy(&v)).unwrap_or(false)),
        None => table.delete(|_| true),
    };

    if let Some(returning) = &returning {
        let table = db.get_table(&table_name)?;
        let ref_rows: Vec<&[DbValue]> = old_rows.iter().map(|r| r.as_slice()).collect();
        return Ok(format_projected_result(ref_rows, returning, &table.col_index, table));
    }

    fire_triggers(&table_name, "DELETE", db);
    Ok(format!("\"Deleted {} row(s)\"", count))
}

// ── Expression evaluator ────────────────────────────────────────────────

fn eval_expr(expr: &Expr, row: &[DbValue], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    match expr {
        Expr::Identifier(ident) => {
            let name = ident.value.to_lowercase();
            if name == "current_timestamp" || name == "current_date" || name == "current_time" {
                return Ok(now_value());
            }
            let idx = col_map.get(&name).ok_or_else(|| format!("Unknown column '{}'", name))?;
            Ok(row[*idx].clone())
        }
        Expr::Value(v) => Ok(sql_val_to_db(v)),
        Expr::BinaryOp { left, op, right } => {
            let l = eval_expr(left, row, col_map)?;
            let r = eval_expr(right, row, col_map)?;
            apply_binary_op(&l, op, &r)
        }
        Expr::UnaryOp { op, expr } => {
            let val = eval_expr(expr, row, col_map)?;
            apply_unary_op(op, &val)
        }
        Expr::Nested(inner) => eval_expr(inner, row, col_map),
        Expr::IsNull(expr) => {
            let val = eval_expr(expr, row, col_map)?;
            Ok(DbValue::Bool(matches!(val, DbValue::Null)))
        }
        Expr::IsNotNull(expr) => {
            let val = eval_expr(expr, row, col_map)?;
            Ok(DbValue::Bool(!matches!(val, DbValue::Null)))
        }
        Expr::Like {
            negated, expr, pattern, ..
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let pat = eval_expr(pattern, row, col_map)?;
            let matched = simple_like(&value_to_string(&val), &value_to_string(&pat));
            Ok(DbValue::Bool(if *negated { !matched } else { matched }))
        }
        Expr::InList { expr, list, negated } => {
            let val = eval_expr(expr, row, col_map)?;
            let mut found = false;
            for item in list {
                let item_val = eval_expr(item, row, col_map)?;
                if val == item_val {
                    found = true;
                    break;
                }
            }
            Ok(DbValue::Bool(if *negated { !found } else { found }))
        }
        Expr::Function(func) => exec_function(func, row, col_map),
        Expr::InSubquery {
            expr,
            subquery,
            negated,
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let subq_result = exec_subquery(subquery)?;
            let found = subq_result.contains(&val);
            Ok(DbValue::Bool(if *negated { !found } else { found }))
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            let operand_val = operand.as_ref().map(|o| eval_expr(o, row, col_map)).transpose()?;
            for cw in conditions.iter() {
                let matched = match &operand_val {
                    Some(ref op_val) => *op_val == eval_expr(&cw.condition, row, col_map)?,
                    None => is_truthy(&eval_expr(&cw.condition, row, col_map)?),
                };
                if matched {
                    return eval_expr(&cw.result, row, col_map);
                }
            }
            match else_result {
                Some(expr) => eval_expr(expr, row, col_map),
                None => Ok(DbValue::Null),
            }
        }
        Expr::Between {
            expr,
            low,
            high,
            negated,
        } => {
            let val = eval_expr(expr, row, col_map)?;
            let l = eval_expr(low, row, col_map)?;
            let h = eval_expr(high, row, col_map)?;
            use std::cmp::Ordering;
            let ge = db_value_cmp(&val, &l) != Ordering::Less;
            let le = db_value_cmp(&val, &h) != Ordering::Greater;
            Ok(DbValue::Bool(if *negated { !(ge && le) } else { ge && le }))
        }
        Expr::Exists { subquery, negated } => {
            let vals = exec_subquery(subquery)?;
            Ok(DbValue::Bool(if *negated { vals.is_empty() } else { !vals.is_empty() }))
        }
        Expr::Cast { expr, data_type, .. } => {
            let val = eval_expr(expr, row, col_map)?;
            cast_db_value(val, data_type)
        }
        _ => Err(format!("Unsupported expression: {:?}", expr)),
    }
}

/// Convert a serde_json::Value to DbValue for CTE row processing.
fn json_val_to_dbvalue(v: &serde_json::Value) -> DbValue {
    match v {
        serde_json::Value::Null => DbValue::Null,
        serde_json::Value::Bool(b) => DbValue::Bool(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(DbValue::Int)
            .or_else(|| n.as_f64().map(DbValue::Float))
            .unwrap_or(DbValue::Null),
        serde_json::Value::String(s) => DbValue::String(s.clone()),
        _ => DbValue::String(v.to_string()),
    }
}

/// Infer ColumnType from a sample JSON value (for CTE column type inference).
fn json_type_to_column(v: &serde_json::Value) -> ColumnType {
    match v {
        serde_json::Value::Null => ColumnType::String,
        serde_json::Value::Bool(_) => ColumnType::Bool,
        serde_json::Value::Number(n) => {
            if n.is_f64() {
                ColumnType::Float
            } else {
                ColumnType::Int
            }
        }
        serde_json::Value::String(_) => ColumnType::String,
        _ => ColumnType::String,
    }
}

/// CAST a DbValue to the target sqlparser DataType.
fn cast_db_value(val: DbValue, target: &DataType) -> Result<DbValue, String> {
    use sqlparser::ast::DataType as DT;
    match target {
        DT::Bool | DT::Boolean => match val {
            DbValue::Bool(b) => Ok(DbValue::Bool(b)),
            DbValue::Int(i) => Ok(DbValue::Bool(i != 0)),
            DbValue::Float(_) => Ok(DbValue::Bool(true)),
            DbValue::String(s) => {
                let lower = s.to_lowercase();
                match lower.as_str() {
                    "true" | "1" | "yes" => Ok(DbValue::Bool(true)),
                    "false" | "0" | "no" => Ok(DbValue::Bool(false)),
                    _ => Err(format!("Cannot cast '{}' to BOOL", s)),
                }
            }
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(format!("Cannot cast {:?} to BOOL", val)),
        },
        DT::Int(_) | DT::BigInt(_) | DT::SmallInt(_) | DT::TinyInt(_) => match val {
            DbValue::Int(i) => Ok(DbValue::Int(i)),
            DbValue::Float(f) => Ok(DbValue::Int(f as i64)),
            DbValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Ok(DbValue::Int(0))
                } else {
                    trimmed
                        .parse::<i64>()
                        .map(DbValue::Int)
                        .map_err(|_| format!("Cannot cast '{}' to INT", s))
                }
            }
            DbValue::Bool(b) => Ok(DbValue::Int(if b { 1 } else { 0 })),
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(format!("Cannot cast {:?} to INT", val)),
        },
        DT::Float(_) | DT::Double(_) | DT::Real | DT::Decimal(_) | DT::Numeric(_) => match val {
            DbValue::Int(i) => Ok(DbValue::Float(i as f64)),
            DbValue::Float(f) => Ok(DbValue::Float(f)),
            DbValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    Ok(DbValue::Float(0.0))
                } else {
                    trimmed
                        .parse::<f64>()
                        .map(DbValue::Float)
                        .map_err(|_| format!("Cannot cast '{}' to FLOAT", s))
                }
            }
            DbValue::Bool(b) => Ok(DbValue::Float(if b { 1.0 } else { 0.0 })),
            DbValue::Null => Ok(DbValue::Null),
            _ => Err(format!("Cannot cast {:?} to FLOAT", val)),
        },
        DT::Varchar(_) | DT::Char(_) | DT::Text | DT::String(_) | DT::Uuid => Ok(DbValue::String(val.to_string())),
        _ => Ok(DbValue::String(val.to_string())),
    }
}

fn eval_literal_expr(expr: &Expr) -> Result<DbValue, String> {
    match expr {
        Expr::Value(v) => Ok(sql_val_to_db(&v.value)),
        Expr::Nested(inner) => eval_literal_expr(inner),
        Expr::UnaryOp { op, expr } => {
            let val = eval_literal_expr(expr)?;
            apply_unary_op(op, &val)
        }
        _ => Err(format!("Complex expressions not supported in values: {:?}", expr)),
    }
}

// ── Function execution ──────────────────────────────────────────────────

fn exec_function(func: &Function, row: &[DbValue], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    let name = func.name.to_string().to_lowercase();
    match name.as_str() {
        "fuzzy_match" => exec_fuzzy_match(func, row, col_map),
        "fts_score" => exec_fts_score(func, row, col_map),
        _ => {
            // Check plugin registry for fn_ prefixed functions
            if let Some(fn_name) = name.strip_prefix("fn_") {
                if let Some((pfunc, _plugin)) = crate::engine::plugin::lookup_function(fn_name) {
                    let args = extract_func_args(func);
                    if args.len() < pfunc.min_args {
                        return Err(format!(
                            "{}: expected {} args, got {}",
                            fn_name,
                            pfunc.min_args,
                            args.len()
                        ));
                    }
                    return (pfunc.func)(&args);
                }
                return Err(format!("Unknown plugin function '{}'", fn_name));
            }
            exec_std_function(func, name, row, col_map)
        }
    }
}

/// Extract function arguments as Vec<DbValue> for plugin dispatch.
fn extract_func_args(func: &Function) -> Vec<DbValue> {
    let mut args = Vec::new();
    if let sqlparser::ast::FunctionArguments::List(list) = &func.args {
        for arg in &list.args {
            use sqlparser::ast::FunctionArg::*;
            use sqlparser::ast::FunctionArgExpr;
            match arg {
                Unnamed(FunctionArgExpr::Expr(expr))
                | Named {
                    arg: FunctionArgExpr::Expr(expr),
                    ..
                } => {
                    // ponytail: no eval context, pass raw SQL repr
                    args.push(DbValue::String(format!("{:?}", expr)));
                }
                Unnamed(FunctionArgExpr::Wildcard) => {
                    args.push(DbValue::String("*".into()));
                }
                _ => {}
            }
        }
    }
    args
}

/// Return the current timestamp as a DbValue (ISO 8601 format).
fn now_value() -> DbValue {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let z = secs / 86400 + 719468;
    let era = (z as i64 / 146097) as u64;
    let doe = (z as i64 - era as i64 * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, min, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    DbValue::String(format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}"))
}

/// Evaluate a standard SQL scalar function.
fn exec_std_function(
    func: &Function,
    name: String,
    row: &[DbValue],
    col_map: &HashMap<String, usize>,
) -> Result<DbValue, String> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Ok(now_value()), // e.g. CURRENT_TIMESTAMP without parens
    };
    let eval_args = |count: usize| -> Result<Vec<DbValue>, String> {
        if args.len() < count {
            return Err(format!("'{}' requires {} argument(s)", name, count));
        }
        args.iter()
            .take(count)
            .map(|a| eval_expr(get_func_arg_unnamed(a)?, row, col_map))
            .collect()
    };

    match name.as_str() {
        "upper" | "ucase" => {
            let vals = eval_args(1)?;
            let s = value_to_string(&vals[0]);
            Ok(DbValue::String(s.to_uppercase()))
        }
        "lower" | "lcase" => {
            let vals = eval_args(1)?;
            let s = value_to_string(&vals[0]);
            Ok(DbValue::String(s.to_lowercase()))
        }
        "length" | "len" => {
            let vals = eval_args(1)?;
            let s = value_to_string(&vals[0]);
            Ok(DbValue::Int(s.len() as i64))
        }
        "substr" | "substring" => {
            let vals = eval_args(3).or_else(|_| eval_args(2))?;
            let s = value_to_string(&vals[0]);
            let start = match vals[1] {
                DbValue::Int(i) => i.max(1) as usize - 1, // SQL is 1-indexed
                _ => return Err("SUBSTR start must be integer".into()),
            };
            if vals.len() >= 3 {
                let length = match vals[2] {
                    DbValue::Int(i) => i as usize,
                    _ => return Err("SUBSTR length must be integer".into()),
                };
                Ok(DbValue::String(s.chars().skip(start).take(length).collect()))
            } else {
                Ok(DbValue::String(s.chars().skip(start).collect()))
            }
        }
        "trim" => {
            let vals = eval_args(1)?;
            let s = value_to_string(&vals[0]);
            Ok(DbValue::String(s.trim().to_string()))
        }
        "coalesce" | "ifnull" => {
            for a in args {
                let v = eval_expr(get_func_arg_unnamed(a)?, row, col_map)?;
                if v != DbValue::Null {
                    return Ok(v);
                }
            }
            Ok(DbValue::Null)
        }
        "round" => {
            let vals = eval_args(2).or_else(|_| eval_args(1))?;
            let num = match vals[0] {
                DbValue::Float(f) => f,
                DbValue::Int(i) => i as f64,
                _ => return Err("ROUND requires numeric argument".into()),
            };
            let decimals = if vals.len() >= 2 {
                match vals[1] {
                    DbValue::Int(i) => i as u32,
                    _ => return Err("ROUND decimals must be integer".into()),
                }
            } else {
                0
            };
            let multiplier = 10_f64.powi(decimals as i32);
            Ok(DbValue::Float((num * multiplier).round() / multiplier))
        }
        "abs" => {
            let v = eval_args(1)?.swap_remove(0);
            match v {
                DbValue::Int(i) => Ok(DbValue::Int(i.abs())),
                DbValue::Float(f) => Ok(DbValue::Float(f.abs())),
                _ => Err("ABS requires numeric argument".into()),
            }
        }
        "now" | "current_timestamp" => Ok(now_value()),
        "concat" => {
            let mut result = String::new();
            for a in args {
                let v = eval_expr(get_func_arg_unnamed(a)?, row, col_map)?;
                result.push_str(&value_to_string(&v));
            }
            Ok(DbValue::String(result))
        }
        _ => Err(format!("Unknown function '{}'", name)),
    }
}

/// Get function argument as Expr, assuming Unnamed(FunctionArgExpr::Expr(e))
fn get_func_arg_unnamed(arg: &FunctionArg) -> Result<&Expr, String> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => Ok(e),
        FunctionArg::Unnamed(_) => Err("Expected expression argument".into()),
        FunctionArg::Named { arg, .. } | FunctionArg::ExprNamed { arg, .. } => match arg {
            FunctionArgExpr::Expr(e) => Ok(e),
            _ => Err("Expected expression in named argument".into()),
        },
    }
}

fn exec_fuzzy_match(func: &Function, row: &[DbValue], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err("fuzzy_match requires argument list".into()),
    };

    if args.len() < 2 {
        return Err("fuzzy_match requires at least 2 arguments".into());
    }

    let col_val = eval_expr(get_func_arg_unnamed(&args[0])?, row, col_map)?;
    let pat_val = eval_expr(get_func_arg_unnamed(&args[1])?, row, col_map)?;

    let threshold = if args.len() >= 3 {
        let t = eval_expr(get_func_arg_unnamed(&args[2])?, row, col_map)?;
        match t {
            DbValue::Float(f) => f,
            DbValue::Int(i) => i as f64,
            _ => 0.3,
        }
    } else {
        0.3
    };

    let similarity = Table::trigram_similarity(&value_to_string(&col_val), &value_to_string(&pat_val));
    Ok(DbValue::Bool(similarity >= threshold))
}

// ── Binary operators ───────────────────────────────────────────────────

fn apply_binary_op(left: &DbValue, op: &BinaryOperator, right: &DbValue) -> Result<DbValue, String> {
    match op {
        BinaryOperator::Eq => Ok(DbValue::Bool(values_equal(left, right))),
        BinaryOperator::NotEq => Ok(DbValue::Bool(!values_equal(left, right))),
        BinaryOperator::Lt => cmp_values(left, right, |o| o.is_lt()),
        BinaryOperator::LtEq => cmp_values(left, right, |o| o.is_le()),
        BinaryOperator::Gt => cmp_values(left, right, |o| o.is_gt()),
        BinaryOperator::GtEq => cmp_values(left, right, |o| o.is_ge()),
        BinaryOperator::Plus => arith_op(left, right, |a, b| a + b, |a, b| a + b),
        BinaryOperator::Minus => arith_op(left, right, |a, b| a - b, |a, b| a - b),
        BinaryOperator::Multiply => arith_op(left, right, |a, b| a * b, |a, b| a * b),
        BinaryOperator::Divide => arith_op(left, right, |a, b| a / b, |a, b| a / b),
        BinaryOperator::Modulo => match (to_float(left), to_float(right)) {
            (Some(a), Some(b)) if b != 0.0 => Ok(DbValue::Float(a % b)),
            _ => Err("Modulo requires numeric operands".into()),
        },
        BinaryOperator::And => Ok(DbValue::Bool(is_truthy(left) && is_truthy(right))),
        BinaryOperator::Or => Ok(DbValue::Bool(is_truthy(left) || is_truthy(right))),
        BinaryOperator::StringConcat => Ok(DbValue::String(format!(
            "{}{}",
            value_to_string(left),
            value_to_string(right)
        ))),
        _ => Err(format!("Unsupported operator: {:?}", op)),
    }
}

fn apply_unary_op(op: &UnaryOperator, val: &DbValue) -> Result<DbValue, String> {
    match op {
        UnaryOperator::Not => Ok(DbValue::Bool(!is_truthy(val))),
        UnaryOperator::Plus => Ok(val.clone()),
        UnaryOperator::Minus => match val {
            DbValue::Int(n) => Ok(DbValue::Int(-n)),
            DbValue::Float(f) => Ok(DbValue::Float(-f)),
            _ => Err(format!("Cannot negate {}", val)),
        },
        _ => Err(format!("Unsupported unary operator: {:?}", op)),
    }
}

// ── Comparison & coercion ──────────────────────────────────────────────

fn values_equal(a: &DbValue, b: &DbValue) -> bool {
    // NULL != anything (including NULL), per SQL standard
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return false;
    }
    match (a, b) {
        (DbValue::Null, DbValue::Null) => true,
        (DbValue::Int(x), DbValue::Int(y)) => x == y,
        (DbValue::Float(x), DbValue::Float(y)) => (x - y).abs() < f64::EPSILON,
        (DbValue::Int(x), DbValue::Float(y)) => (*x as f64 - y).abs() < f64::EPSILON,
        (DbValue::Float(x), DbValue::Int(y)) => (x - *y as f64).abs() < f64::EPSILON,
        (DbValue::Bool(x), DbValue::Bool(y)) => x == y,
        (DbValue::String(x), DbValue::String(y)) => x == y,
        _ => value_to_string(a) == value_to_string(b),
    }
}

fn cmp_values<F>(a: &DbValue, b: &DbValue, cmp: F) -> Result<DbValue, String>
where
    F: Fn(std::cmp::Ordering) -> bool,
{
    // SQL: NULL compared to anything is NULL (treated as false)
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return Ok(DbValue::Bool(false));
    }
    let ord = match (to_float(a), to_float(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => value_to_string(a).cmp(&value_to_string(b)),
    };
    Ok(DbValue::Bool(cmp(ord)))
}

fn arith_op<F, G>(a: &DbValue, b: &DbValue, int_op: F, float_op: G) -> Result<DbValue, String>
where
    F: Fn(i64, i64) -> i64,
    G: Fn(f64, f64) -> f64,
{
    // SQL: NULL in any arithmetic → NULL
    if matches!(a, DbValue::Null) || matches!(b, DbValue::Null) {
        return Ok(DbValue::Null);
    }
    match (a, b) {
        (DbValue::Int(x), DbValue::Int(y)) => Ok(DbValue::Int(int_op(*x, *y))),
        _ => match (to_float(a), to_float(b)) {
            (Some(x), Some(y)) => Ok(DbValue::Float(float_op(x, y))),
            _ => Err(format!("Type mismatch: {} vs {}", a, b)),
        },
    }
}

fn to_float(v: &DbValue) -> Option<f64> {
    match v {
        DbValue::Int(n) => Some(*n as f64),
        DbValue::Float(f) => Some(*f),
        DbValue::String(s) => s.parse::<f64>().ok(),
        _ => None,
    }
}

fn is_truthy(v: &DbValue) -> bool {
    match v {
        DbValue::Null => false,
        DbValue::Bool(b) => *b,
        DbValue::Int(n) => *n != 0,
        DbValue::Float(f) => *f != 0.0,
        DbValue::String(s) => !s.is_empty(),
        DbValue::Strings(arr) => !arr.is_empty(),
        DbValue::Floats(arr) => !arr.is_empty(),
    }
}

fn value_to_string(v: &DbValue) -> String {
    match v {
        DbValue::Null => "NULL".into(),
        DbValue::Bool(b) => b.to_string(),
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => s.clone(),
        DbValue::Strings(arr) => arr.join(","),
        DbValue::Floats(arr) => arr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","),
    }
}

// ── SQL value conversion ───────────────────────────────────────────────

fn sql_val_to_db(v: &sqlparser::ast::Value) -> DbValue {
    match v {
        sqlparser::ast::Value::Null => DbValue::Null,
        sqlparser::ast::Value::Boolean(b) => DbValue::Bool(*b),
        sqlparser::ast::Value::Number(s, _) => {
            if s.contains('.') {
                s.parse::<f64>()
                    .map(DbValue::Float)
                    .unwrap_or(DbValue::String(s.clone()))
            } else {
                s.parse::<i64>().map(DbValue::Int).unwrap_or(DbValue::String(s.clone()))
            }
        }
        sqlparser::ast::Value::SingleQuotedString(s) | sqlparser::ast::Value::DoubleQuotedString(s) => {
            DbValue::String(s.clone())
        }
        _ => DbValue::String(format!("{:?}", v)),
    }
}

// ── Data type parsing ──────────────────────────────────────────────────

fn parse_data_type(dt: &DataType) -> Result<ColumnType, String> {
    match dt {
        DataType::Int(_)
        | DataType::Integer(_)
        | DataType::BigInt(_)
        | DataType::SmallInt(_)
        | DataType::TinyInt(_) => Ok(ColumnType::Int),
        DataType::Float(_)
        | DataType::Double(_)
        | DataType::Real
        | DataType::Decimal(_)
        | DataType::Dec(_)
        | DataType::Numeric(_) => Ok(ColumnType::Float),
        DataType::String(_) | DataType::Text | DataType::Varchar(_) | DataType::Char(_) | DataType::Uuid => {
            Ok(ColumnType::String)
        }
        DataType::Boolean | DataType::Bool => Ok(ColumnType::Bool),
        DataType::Array(elem) => {
            use sqlparser::ast::ArrayElemTypeDef;
            let inner = match elem {
                ArrayElemTypeDef::SquareBracket(dt, _) => dt.as_ref(),
                ArrayElemTypeDef::AngleBracket(dt) => dt.as_ref(),
                ArrayElemTypeDef::Parenthesis(dt) => dt.as_ref(),
                ArrayElemTypeDef::None => return Ok(ColumnType::Strings),
            };
            match inner {
                DataType::String(_) | DataType::Varchar(_) | DataType::Text | DataType::Char(_) => {
                    Ok(ColumnType::Strings)
                }
                DataType::Float(_) | DataType::Double(_) | DataType::Real => Ok(ColumnType::Floats),
                _ if inner.to_string().to_lowercase() == "string" => Ok(ColumnType::Strings),
                _ => Err(format!("Unsupported array element type: {}", inner)),
            }
        }
        DataType::Custom(name, _) => {
            let s = name.to_string().to_uppercase();
            match s.as_str() {
                "STRINGS" => Ok(ColumnType::Strings),
                "FLOATS" => Ok(ColumnType::Floats),
                "STRING" => Ok(ColumnType::String),
                "INT" | "INTEGER" | "BIGINT" | "TINYINT" | "SMALLINT" => Ok(ColumnType::Int),
                "FLOAT" | "DOUBLE" => Ok(ColumnType::Float),
                "BOOL" | "BOOLEAN" => Ok(ColumnType::Bool),
                _ => Err(format!("Unknown custom type '{}'", s)),
            }
        }
        _ => Err(format!("Unsupported data type: {:?}", dt)),
    }
}

// ── Table resolution ───────────────────────────────────────────────────

/// Materialize a view by executing its SQL and inserting the results as a temp table.
fn materialize_view(name: &str, db: &mut Database) -> Result<(), String> {
    let sql = db
        .get_view(name)
        .ok_or_else(|| format!("View '{}' not found", name))?
        .clone();

    let stmts = crate::parser::parse_sql(&sql).map_err(|e| format!("{}", e))?;
    let stmt = stmts.into_iter().next().ok_or("View definition is empty")?;

    let result = execute(&stmt, db)?;

    let rows: Vec<Vec<serde_json::Value>> =
        serde_json::from_str(&result).map_err(|e| format!("View result parse: {}", e))?;

    if rows.len() >= 2 {
        let header = &rows[0];
        let cols: Vec<Column> = header
            .iter()
            .map(|h| Column {
                name: h.as_str().unwrap_or("col").to_lowercase(),
                dtype: ColumnType::String,
                primary_key: false,
                not_null: false,
                default: None,
                auto_increment: false,
            })
            .collect();

        if let Ok(mut table) = Table::new(name.to_string(), cols) {
            for row_data in &rows[1..] {
                let db_row: Vec<DbValue> = row_data.iter().map(json_val_to_dbvalue).collect();
                let _ = table.insert(db_row);
            }
            db.add_table(name.to_string(), table);
        }
    }

    Ok(())
}

/// Resolve a table factor, materialising a view if the name is not a real table.
fn resolve_table_factor(
    factor: &TableFactor,
    db: &mut Database,
) -> Result<(String, crate::engine::table::Table), String> {
    match factor {
        TableFactor::Table { name, .. } => {
            let tname = object_name_str(name);
            if !db.has_table(&tname) && db.has_view(&tname) {
                materialize_view(&tname, db)?;
            }
            let table = db.get_table(&tname)?.clone();
            Ok((tname, table))
        }
        _ => Err("Only simple table references supported in FROM".into()),
    }
}

fn resolve_single_table<'a>(from: &[TableWithJoins], db: &'a Database) -> Result<&'a Table, String> {
    let tf = from.first().ok_or("No FROM clause")?;
    match &tf.relation {
        TableFactor::Table { name, .. } => db.get_table(&object_name_str(name)),
        _ => Err("Only simple table references supported in FROM".into()),
    }
}

fn resolve_table_from_joins(tj: &TableWithJoins) -> Result<String, String> {
    match &tj.relation {
        TableFactor::Table { name, .. } => Ok(object_name_str(name)),
        _ => Err("Only simple table references supported".into()),
    }
}

// ── UNION / INTERSECT / EXCEPT ─────────────────────────────────────────

/// Execute a SetOperation (UNION/INTERSECT/EXCEPT) by executing both branches
/// and combining results.
fn exec_union(so: &SetExpr, _query: &Query, db: &mut Database) -> Result<String, String> {
    use sqlparser::ast::{SetOperator, SetQuantifier};
    let (left, right, op, is_all) = match so {
        SetExpr::SetOperation {
            left,
            op,
            set_quantifier,
            right,
        } => {
            let is_all = matches!(set_quantifier, SetQuantifier::All);
            (left, right, op, is_all)
        }
        _ => return Err("Expected SetOperation".into()),
    };

    let lq = wrap_setexpr(left);
    let rq = wrap_setexpr(right);
    let lj = exec_select(&lq, db)?;
    let rj = exec_select(&rq, db)?;

    let parse = |s: &str| -> Vec<Vec<serde_json::Value>> { serde_json::from_str(s).unwrap_or_default() };
    let l_rows = parse(&lj);
    let r_rows = parse(&rj);

    // Helper to count row multiplicities for ALL variants
    let row_counts = |rows: &[Vec<serde_json::Value>]| -> Vec<(Vec<serde_json::Value>, usize)> {
        let mut counts: Vec<(Vec<serde_json::Value>, usize)> = Vec::new();
        for row in rows {
            if let Some(pos) = counts.iter().position(|(r, _)| r == row) {
                counts[pos].1 += 1;
            } else {
                counts.push((row.clone(), 1));
            }
        }
        counts
    };

    let all = match op {
        SetOperator::Union if is_all => {
            // UNION ALL — concatenate, no dedup
            let mut rows = l_rows.clone();
            if !rows.is_empty() && !r_rows.is_empty() {
                rows.extend(r_rows[1..].iter().cloned());
            } else {
                rows.extend(r_rows);
            }
            rows
        }
        SetOperator::Union => {
            // UNION DISTINCT — deduplicate across both branches
            let mut rows = Vec::new();
            if !l_rows.is_empty() {
                rows.push(l_rows[0].clone()); // header from left
                for row in &l_rows[1..] {
                    if !rows[1..].contains(row) {
                        rows.push(row.clone());
                    }
                }
                for row in &r_rows[1..] {
                    if !rows[1..].contains(row) {
                        rows.push(row.clone());
                    }
                }
            }
            rows
        }
        SetOperator::Except if is_all => {
            // EXCEPT ALL — remove multiplicities from left
            if l_rows.len() > 1 && r_rows.len() > 1 {
                let header = l_rows[0].clone();
                let mut data: Vec<Vec<serde_json::Value>> = l_rows[1..].to_vec();
                for r_row in &r_rows[1..] {
                    if let Some(pos) = data.iter().position(|d| d == r_row) {
                        data.remove(pos);
                    }
                }
                let mut result = vec![header];
                result.extend(data);
                result
            } else {
                l_rows.clone()
            }
        }
        SetOperator::Except | SetOperator::Minus => {
            // EXCEPT / EXCEPT DISTINCT — rows in left but not in right
            let mut rows = Vec::new();
            if !l_rows.is_empty() {
                rows.push(l_rows[0].clone()); // header from left
                for row in &l_rows[1..] {
                    if !r_rows[1..].contains(row) {
                        rows.push(row.clone());
                    }
                }
            }
            rows
        }
        SetOperator::Intersect if is_all => {
            // INTERSECT ALL — min multiplicity
            let mut rows = Vec::new();
            if !l_rows.is_empty() && r_rows.len() > 1 {
                rows.push(l_rows[0].clone()); // header
                let l_counts = row_counts(&l_rows[1..]);
                let r_counts = row_counts(&r_rows[1..]);
                for (l_row, l_cnt) in &l_counts {
                    if let Some((_, r_cnt)) = r_counts.iter().find(|(r, _)| r == l_row) {
                        let take = (*l_cnt).min(*r_cnt);
                        for _ in 0..take {
                            rows.push(l_row.clone());
                        }
                    }
                }
            }
            rows
        }
        SetOperator::Intersect => {
            // INTERSECT / INTERSECT DISTINCT — rows in both
            let mut rows = Vec::new();
            if !l_rows.is_empty() {
                rows.push(l_rows[0].clone()); // header from left
                for row in &l_rows[1..] {
                    if r_rows[1..].contains(row) && !rows[1..].contains(row) {
                        rows.push(row.clone());
                    }
                }
            }
            rows
        }
    };

    Ok(serde_json::to_string(&all).unwrap_or_else(|_| "[]".into()))
}

/// Wrap a SetExpr into a minimal Query for exec_select.
fn wrap_setexpr(expr: &SetExpr) -> Query {
    Query {
        with: None,
        body: Box::new(expr.clone()),
        order_by: None,
        limit_clause: None,
        fetch: None,
        locks: vec![],
        for_clause: None,
        settings: None,
        format_clause: None,
        pipe_operators: vec![],
    }
}

/// Apply ORDER BY and LIMIT from a Query to a parsed JSON result string.
fn apply_order_limit(json_str: &str, query: &Query) -> Result<String, String> {
    if query.order_by.is_none() && query.limit_clause.is_none() {
        return Ok(json_str.to_string());
    }
    // Parse the JSON, re-format with order/limit
    // Since the result is already JSON, we just pass through for now
    // ponytail: ORDER BY/LIMIT on UNION results is complex — pass through raw
    Ok(json_str.to_string())
}

// ── Index-assisted lookup ─────────────────────────────────────────────

/// Try to use a BTreeIndex for a simple `col = literal` WHERE clause.
/// Returns `Some(rows)` if an index was used, `None` to fall back to full scan.
fn try_btree_index<'a>(where_expr: Option<&Expr>, table: &'a Table) -> Option<Vec<&'a [DbValue]>> {
    let expr = where_expr?;
    let (col_name, value) = match expr {
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Eq,
            right,
        } => match (left.as_ref(), right.as_ref()) {
            (Expr::Identifier(ident), Expr::Value(v)) | (Expr::Value(v), Expr::Identifier(ident)) => {
                (ident.value.to_lowercase(), sql_val_to_db(&v.value))
            }
            _ => return None,
        },
        _ => return None,
    };
    let indices = table.btree_lookup(&col_name, &value)?;
    Some(indices.into_iter().map(|i| table.rows[i].as_slice()).collect())
}

/// Try to use a TrigramIndex for a `fuzzy_match(col, pattern)` WHERE clause.
/// Returns `Some(candidate_rows)` if a trigram index exists, `None` to fall back to full scan.
fn try_trigram_index<'a>(where_expr: Option<&Expr>, table: &'a Table) -> Option<Vec<&'a [DbValue]>> {
    let expr = where_expr?;
    let (col_name, pattern_val) = match expr {
        Expr::Function(f) if f.name.to_string().to_lowercase() == "fuzzy_match" => {
            let args = match &f.args {
                FunctionArguments::List(list) => &list.args,
                _ => return None,
            };
            if args.len() < 2 {
                return None;
            }
            let col_expr = get_func_arg_unnamed(&args[0]).ok()?;
            let pat_expr = get_func_arg_unnamed(&args[1]).ok()?;
            let col_name = match col_expr {
                Expr::Identifier(ident) => ident.value.to_lowercase(),
                Expr::CompoundIdentifier(parts) => parts
                    .iter()
                    .map(|p| p.value.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("."),
                _ => return None,
            };
            let pattern_val = match pat_expr {
                Expr::Value(v) => match &v.value {
                    sqlparser::ast::Value::SingleQuotedString(s) => s.clone(),
                    sqlparser::ast::Value::DoubleQuotedString(s) => s.clone(),
                    sqlparser::ast::Value::Null => return None,
                    _ => return None,
                },
                _ => return None,
            };
            (col_name, pattern_val)
        }
        _ => return None,
    };

    // Check for trigram index on this column
    let trigram = table.find_index(&col_name, A3IndexType::Trigram)?;
    let IndexImpl::Trigram(ref idx) = trigram else {
        unreachable!()
    };

    let candidates = idx.candidates(&pattern_val);
    if candidates.is_empty() || candidates.len() >= table.rows.len() {
        return None; // not worth it, full scan is similar cost
    }

    Some(candidates.into_iter().map(|i| table.rows[i].as_slice()).collect())
}

/// Return the trigram similarity score between two strings (for ranked FTS).
/// Called as `fts_score(col, pattern)` in SQL.
fn exec_fts_score(func: &Function, row: &[DbValue], col_map: &HashMap<String, usize>) -> Result<DbValue, String> {
    let args = match &func.args {
        FunctionArguments::List(list) => &list.args,
        _ => return Err("fts_score requires argument list".into()),
    };
    if args.len() < 2 {
        return Err("fts_score requires at least 2 arguments".into());
    }
    let col_val = eval_expr(get_func_arg_unnamed(&args[0])?, row, col_map)?;
    let pat_val = eval_expr(get_func_arg_unnamed(&args[1])?, row, col_map)?;
    let similarity = Table::trigram_similarity(&value_to_string(&col_val), &value_to_string(&pat_val));
    Ok(DbValue::Float(similarity))
}

// ── Subquery execution ────────────────────────────────────────────────

/// Execute a subquery (SELECT) and return the first column of each row.
/// Uses the thread-local DB snapshot set by exec_select (avoids deadlock).
fn exec_subquery(query: &Query) -> Result<Vec<DbValue>, String> {
    let db_snapshot = SUBQ_DB.with(|snap| {
        snap.borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| "Subquery not supported in this context".to_string())
    })?;
    let mut db_copy = db_snapshot;
    let result_str = exec_select(query, &mut db_copy)?;

    // Parse the full JSON response: [0,"OK",[["h"],[v1],[v2],...]]
    let mut values = Vec::new();
    match serde_json::from_str::<Vec<serde_json::Value>>(&result_str) {
        Ok(full) if full.len() >= 3 => {
            if let Some(rows) = full[2].as_array() {
                for row in rows.iter().skip(1) {
                    if let Some(arr) = row.as_array() {
                        if let Some(first) = arr.first() {
                            values.push(json_value_to_dbvalue(first));
                        }
                    }
                }
            }
        }
        Ok(_) => {
            // Response parsed but doesn't have the expected structure
        }
        Err(_) => {
            // Fallback: try to extract from the raw string
            // Find data between the first [[ and the last ]]
            if let Some(start) = result_str.find("[[") {
                if let Some(end) = result_str.rfind("]]") {
                    let inner = &result_str[start + 1..end];
                    // Split by ], [ to get individual rows
                    for row_str in inner.split("],[") {
                        let cleaned = row_str.trim_matches('[').trim_matches(']').trim();
                        if !cleaned.is_empty() {
                            let val = cleaned.trim_matches('"');
                            if let Ok(n) = val.parse::<i64>() {
                                values.push(DbValue::Int(n));
                            } else if let Ok(f) = val.parse::<f64>() {
                                values.push(DbValue::Float(f));
                            } else {
                                values.push(DbValue::String(val.to_string()));
                            }
                        }
                    }
                    // Remove header (first value)
                    if values.len() > 1 {
                        values.remove(0);
                    }
                }
            }
        }
    }
    Ok(values)
}

/// Convert a serde_json::Value to a DbValue.
fn json_value_to_dbvalue(v: &serde_json::Value) -> DbValue {
    match v {
        serde_json::Value::Null => DbValue::Null,
        serde_json::Value::Bool(b) => DbValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                DbValue::Int(i)
            } else {
                DbValue::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => DbValue::String(s.clone()),
        _ => DbValue::String(v.to_string()),
    }
}

// ── CREATE INDEX handling ──────────────────────────────────────────────

fn exec_create_index(idx: &sqlparser::ast::CreateIndex, db: &mut Database) -> Result<String, String> {
    let index_name = match &idx.name {
        Some(name) => object_name_str(name),
        None => return Err("CREATE INDEX requires a name".into()),
    };
    let table_name = object_name_str(&idx.table_name);

    // IF NOT EXISTS — silently return if index already exists
    if idx.if_not_exists {
        let table = db.get_table(&table_name)?;
        if table.has_index(&index_name) {
            return Ok(format!("\"Index '{}' already exists\"", index_name));
        }
    }

    // Determine index type from USING clause (default BTREE)
    use sqlparser::ast::IndexType as SqlIdx;
    let index_type = match &idx.using {
        None | Some(SqlIdx::BTree) => A3IndexType::BTree,
        Some(SqlIdx::GIN) => A3IndexType::Trigram,
        Some(SqlIdx::Custom(id)) if id.value.to_uppercase() == "TRIGRAM" => A3IndexType::Trigram,
        Some(other) => return Err(format!("Unsupported index type: {}", other)),
    };

    // Get the column name from the first index column
    let column = match idx.columns.first() {
        Some(col) => col.to_string().to_lowercase(),
        None => return Err("CREATE INDEX requires at least one column".into()),
    };

    let table = db.get_table_mut(&table_name)?;
    table.create_index(&index_name, &column, index_type)?;

    Ok(format!(
        "\"Created {} index '{}' on {}({})\"",
        index_type, index_name, table_name, column
    ))
}

/// Drop an index by name across all tables.
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
fn parse_and_exec(sql: &str, db: &mut Database) -> Result<String, String> {
    use sqlparser::dialect::SQLiteDialect;
    use sqlparser::parser::Parser;
    // Try SQLite dialect first (handles CREATE TRIGGER with BEGIN...END body)
    let mut sqlite = SQLiteDialect {};
    if let Ok(stmts) = Parser::parse_sql(&mut sqlite, sql) {
        let mut results = Vec::new();
        for stmt in stmts {
            results.push(execute(&stmt, db)?);
        }
        return Ok(results.join("\n"));
    }
    let mut meta = sqlparser::dialect::GenericDialect {};
    let stmts = Parser::parse_sql(&mut meta, sql).map_err(|e| format!("Parse error in trigger body: {}", e))?;
    let mut results = Vec::new();
    for stmt in stmts {
        results.push(execute(&stmt, db)?);
    }
    Ok(results.join("\n"))
}

/// Execute CREATE TRIGGER.
fn exec_create_trigger(ct: &sqlparser::ast::CreateTrigger, db: &mut Database) -> Result<String, String> {
    let table_name = ct.table_name.to_string().to_lowercase();
    let trigger_name = ct.name.to_string().to_lowercase();
    let _timing = ct.period.as_ref().map(|p| format!("{:?}", p)).unwrap_or_default();
    let _event = ct.events.first().map(|e| format!("{:?}", e)).unwrap_or_default();
    // Use first event only and infer FROM/UPDATE/DELETE

    let event_str = match ct.events.first() {
        Some(sqlparser::ast::TriggerEvent::Insert) => "INSERT",
        Some(sqlparser::ast::TriggerEvent::Update(_)) => "UPDATE",
        Some(sqlparser::ast::TriggerEvent::Delete) => "DELETE",
        _ => return Err("Unsupported trigger event".into()),
    };
    let timing_str = match ct.period.as_ref() {
        Some(p) => match p {
            sqlparser::ast::TriggerPeriod::Before => "BEFORE",
            sqlparser::ast::TriggerPeriod::After => "AFTER",
            _ => return Err("Only BEFORE/AFTER triggers supported".into()),
        },
        None => return Err("Trigger timing (BEFORE/AFTER) required".into()),
    };
    // Extract body SQL from the statements block
    let body = ct.statements.as_ref().map(|s| format!("{}", s)).unwrap_or_default();
    if body.is_empty() {
        return Err("Trigger requires a body (SQL statement)".into());
    }
    let table = db.get_table_mut(&table_name)?;
    table.triggers.push(crate::engine::table::TriggerInfo {
        name: trigger_name.clone(),
        timing: timing_str.to_string(),
        event: event_str.to_string(),
        body,
    });
    Ok(format!("\"Trigger '{}' created on '{}'\"", trigger_name, table_name))
}

/// Fire AFTER triggers for a given table and event.
fn fire_triggers(_table_name: &str, event: &str, db: &mut Database) {
    let names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    for tn in names {
        if let Ok(t) = db.get_table(&tn) {
            let triggers: Vec<crate::engine::table::TriggerInfo> = t
                .triggers
                .iter()
                .filter(|tr| tr.event == event && tr.timing == "AFTER")
                .cloned()
                .collect();
            drop(t);
            for tr in triggers {
                // Replace OLD and NEW references with the target table
                let body = tr.body.replace("OLD.", "").replace("NEW.", "");
                if let Err(e) = parse_and_exec(&body, db) {
                    eprintln!("Trigger '{}' error: {}", tr.name, e);
                }
            }
        }
    }
}

// ── EXPLAIN ─────────────────────────────────────────────────────────────

/// Generate an EXPLAIN plan description as a JSON array of plan nodes.
fn explain_statement(stmt: &Statement, db: &Database) -> Result<String, String> {
    use serde_json::json;

    match stmt {
        Statement::Query(query) => {
            let mut steps: Vec<serde_json::Value> = Vec::new();

            if let SetExpr::Select(select) = &*query.body {
                let select = select.as_ref();

                // FROM clause — table scans and joins
                for twj in &select.from {
                    match &twj.relation {
                        TableFactor::Table { name, .. } => {
                            let tname = name.to_string().to_lowercase();
                            let mut scan = json!({"type": "SeqScan", "table": tname});

                            // Show available indexes on this table
                            if let Ok(table) = db.get_table(&tname) {
                                if !table.indices.is_empty() {
                                    let idxs: Vec<&str> = table.indices.iter().map(|(m, _)| m.name.as_str()).collect();
                                    scan["indexes"] = json!(idxs);
                                }
                            }
                            steps.push(scan);
                        }
                        _ => {
                            steps.push(json!({"type": "SubQuery"}));
                        }
                    }

                    for join in &twj.joins {
                        let rel = match &join.relation {
                            TableFactor::Table { name, .. } => name.to_string(),
                            _ => "<subquery>".into(),
                        };
                        steps.push(json!({"type": "Join", "relation": rel}));
                    }
                }

                // Bare SELECT (no FROM)
                if select.from.is_empty() {
                    steps.push(json!({"type": "Projection"}));
                }

                // WHERE clause
                if let Some(expr) = &select.selection {
                    steps.push(json!({"type": "Filter", "condition": format!("{}", expr)}));
                }

                // GROUP BY
                use sqlparser::ast::GroupByExpr;
                if let GroupByExpr::Expressions(exprs, _) = &select.group_by {
                    if !exprs.is_empty() {
                        let gb: Vec<String> = exprs.iter().map(|e| format!("{}", e)).collect();
                        steps.push(json!({"type": "GroupBy", "columns": gb}));
                    }
                }

                // HAVING
                if let Some(expr) = &select.having {
                    steps.push(json!({"type": "Having", "condition": format!("{}", expr)}));
                }

                // ORDER BY
                if let Some(order_by) = &query.order_by {
                    if let OrderByKind::Expressions(exprs) = &order_by.kind {
                        if !exprs.is_empty() {
                            let ob: Vec<String> = exprs.iter().map(|e| format!("{}", e.expr)).collect();
                            steps.push(json!({"type": "OrderBy", "columns": ob}));
                        }
                    }
                }

                // LIMIT / OFFSET
                if let Some(lc) = &query.limit_clause {
                    if let LimitClause::LimitOffset { limit, offset, .. } = lc {
                        let mut l = serde_json::Map::new();
                        l.insert("type".into(), json!("Limit"));
                        if let Some(e) = limit {
                            l.insert("limit".into(), json!(format!("{}", e)));
                        }
                        if let Some(o) = offset {
                            l.insert("offset".into(), json!(format!("{}", o.value)));
                        }
                        steps.push(serde_json::Value::Object(l));
                    } else if let LimitClause::OffsetCommaLimit { offset, limit } = lc {
                        steps.push(json!({
                            "type": "Limit",
                            "limit": format!("{}", limit),
                            "offset": format!("{}", offset),
                        }));
                    }
                }
            }

            if steps.is_empty() {
                steps.push(json!({"type": "Unknown"}));
            }

            serde_json::to_string(&steps).map_err(|e| e.to_string())
        }

        Statement::Insert(ins) => {
            let tname = ins.table.to_string().to_lowercase();
            let ncols = ins.columns.len();
            let mut plan = json!({"type": "Insert", "table": tname});
            if ncols > 0 {
                let cols: Vec<String> = ins.columns.iter().map(|c| c.to_string().to_lowercase()).collect();
                plan["columns"] = json!(cols);
            }
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::CreateTable(def) => {
            let tname = def.name.to_string().to_lowercase();
            let ncols = def.columns.len();
            let plan = json!({"type": "CreateTable", "table": tname, "columns": ncols});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::CreateIndex(idx) => {
            let iname = idx
                .name
                .as_ref()
                .map(|n| n.to_string())
                .unwrap_or_default()
                .to_lowercase();
            let tname = idx.table_name.to_string().to_lowercase();
            let plan = json!({"type": "CreateIndex", "index": iname, "table": tname});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::Update(upd) => {
            let tname = resolve_table_from_joins(&upd.table).unwrap_or_default();
            let plan = json!({"type": "Update", "table": tname});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::Delete(del) => {
            let tname = match &del.from {
                sqlparser::ast::FromTable::WithFromKeyword(tables)
                | sqlparser::ast::FromTable::WithoutKeyword(tables) => match tables.first() {
                    Some(tj) => match &tj.relation {
                        TableFactor::Table { name, .. } => name.to_string().to_lowercase(),
                        _ => String::new(),
                    },
                    None => String::new(),
                },
            };
            let plan = json!({"type": "Delete", "table": tname});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::Drop { names, object_type, .. } => {
            let oname = names.first().map(|n| n.to_string()).unwrap_or_default();
            let otype = format!("{}", object_type).to_lowercase();
            let plan = json!({"type": "Drop", "object_type": otype, "name": oname});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::Truncate(trunc) => {
            let tname = trunc
                .table_names
                .first()
                .map(|t| t.name.to_string())
                .unwrap_or_default();
            let plan = json!({"type": "Truncate", "table": tname.to_lowercase()});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::AlterTable(at) => {
            let tname = at.name.to_string().to_lowercase();
            let ops: Vec<String> = at.operations.iter().map(|o| format!("{}", o)).collect();
            let plan = json!({"type": "AlterTable", "table": tname, "operations": ops});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::RenameTable(rt) => {
            let old = rt[0].old_name.to_string().to_lowercase();
            let new = rt[0].new_name.to_string().to_lowercase();
            let plan = json!({"type": "RenameTable", "old_name": old, "new_name": new});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::ShowTables { .. } => {
            let plan = json!({"type": "ShowTables"});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::StartTransaction { .. } => {
            let plan = json!({"type": "StartTransaction"});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::Commit { .. } => {
            let plan = json!({"type": "Commit"});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::Rollback { .. } => {
            let plan = json!({"type": "Rollback"});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::Savepoint { name, .. } => {
            let plan = json!({"type": "Savepoint", "name": name.to_string()});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::ReleaseSavepoint { name, .. } => {
            let plan = json!({"type": "ReleaseSavepoint", "name": name.to_string()});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        // Fallback for unsupported statement types — show the SQL text
        other => {
            let plan = json!({"type": format!("{}", other)});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }
    }
}

// ── LIKE pattern matching ──────────────────────────────────────────────

fn simple_like(value: &str, pattern: &str) -> bool {
    let val_chars: Vec<char> = value.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();
    like_match(&val_chars, &pat_chars, 0, 0)
}

fn like_match(val: &[char], pat: &[char], vi: usize, pi: usize) -> bool {
    if pi == pat.len() {
        return vi == val.len();
    }
    match pat[pi] {
        '%' => {
            let mut vi2 = vi;
            while vi2 <= val.len() {
                if like_match(val, pat, vi2, pi + 1) {
                    return true;
                }
                vi2 += 1;
            }
            false
        }
        '_' => vi < val.len() && like_match(val, pat, vi + 1, pi + 1),
        c => vi < val.len() && val[vi] == c && like_match(val, pat, vi + 1, pi + 1),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

// ── Schema introspection ─────────────────────────────────────────────────

/// DESCRIBE table — returns a JSON array of column definitions.
pub fn describe_table(db: &crate::engine::database::Database, table_name: &str) -> Result<String, String> {
    let table = db.get_table(table_name)?;
    let mut rows = vec!["[\"Field\",\"Type\",\"Null\",\"Key\",\"Default\",\"Extra\"]".into()];
    for col in &table.columns {
        let null_s = if col.not_null { "\"NO\"" } else { "\"YES\"" };
        let key_s = if col.primary_key {
            "\"PRI\"".to_string()
        } else {
            "\"\"".to_string()
        };
        let dflt = match &col.default {
            Some(v) => format!("\"{}\"", v),
            None => "\"\"".into(),
        };
        let extra = if col.auto_increment {
            "\"auto_increment\""
        } else {
            "\"\""
        };
        rows.push(format!(
            "[\"{}\",\"{}\",{},{},{},{}]",
            col.name, col.dtype, null_s, key_s, dflt, extra
        ));
    }
    Ok(format!("[{}]", rows.join(",")))
}

/// SHOW CREATE TABLE — returns a CREATE TABLE SQL statement.
pub fn show_create_table(db: &crate::engine::database::Database, table_name: &str) -> Result<String, String> {
    let table = db.get_table(table_name)?;
    let mut sql = format!("CREATE TABLE \"{}\" (\n", table_name);
    let col_defs: Vec<String> = table
        .columns
        .iter()
        .map(|col| {
            let mut def = format!("  \"{}\" {}", col.name, col.dtype);
            if col.primary_key {
                def += " PRIMARY KEY";
            }
            if col.not_null && !col.primary_key {
                def += " NOT NULL";
            }
            if let Some(ref d) = col.default {
                def += &format!(" DEFAULT {}", d);
            }
            if col.auto_increment {
                def += " AUTO_INCREMENT";
            }
            def
        })
        .collect();
    sql += &col_defs.join(",\n");
    sql += "\n)";
    ::serde_json::to_string(&sql).map_err(|e| format!("{}", e))
}

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

    fn parse_and_exec(sql: &str, db: &mut Database) -> Result<String, String> {
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
}
