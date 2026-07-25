use super::super::database::Database;
use super::super::index::IndexType as A3IndexType;
use super::super::table::{ForeignKeyInfo, Table};
use super::super::value::{Column, ColumnType, DbValue};
use sqlparser::ast::table_constraints::TableConstraint;
use sqlparser::ast::{
    Analyze, ColumnOption, CopySource, CopyTarget, CreateIndex, CreateTrigger, DataType, Expr, Function, Ident, Merge,
    MergeAction, MergeClauseKind, MergeUpdateExpr, ObjectName, ObjectNamePart, SequenceOptions, ShowCreateObject,
    ShowStatementOptions, TriggerEvent, TriggerPeriod, VacuumStatement,
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

pub(crate) fn exec_call(func: &Function, _db: &mut Database) -> Result<String, String> {
    let empty = Vec::new();
    let empty_map = std::collections::HashMap::new();
    match super::super::functions::eval::exec_function(func, &empty, &empty_map) {
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
    for (_i, cn) in module_args.iter().enumerate() {
        let _ = table.create_index(
            &format!("fts_trgm_{}", cn.value.to_lowercase()),
            &cn.value.to_lowercase(),
            super::super::index::IndexType::Trigram,
        );
    }
    db.add_table(tn.clone(), table);
    Ok(format!("\"Virtual table '{}' created (FTS trigram)\"", tn))
}

// ── Helper functions ────────────────────────────────────────────────────

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

// ── CREATE TABLE ────────────────────────────────────────────────────────

pub(crate) fn exec_create_table(def: &sqlparser::ast::CreateTable, db: &mut Database) -> Result<String, String> {
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

pub(crate) fn exec_create_view(cv: &sqlparser::ast::CreateView, db: &mut Database) -> Result<String, String> {
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

pub(crate) fn exec_create_table_as(def: &sqlparser::ast::CreateTable, db: &mut Database) -> Result<String, String> {
    let table_name = object_name_str(&def.name);

    if def.if_not_exists && db.has_table(&table_name) {
        return Ok(format!("\"Table '{}' already exists\"", table_name));
    }

    let query = def.query.as_ref().unwrap();
    let json = super::super::stmts::select::exec_select(query, db)?;

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

// ── CREATE INDEX ───────────────────────────────────────────────────────────

/// Create an index on a table column. Supports BTREE and TRIGRAM index types.
pub(crate) fn exec_create_index(idx: &CreateIndex, db: &mut Database) -> Result<String, String> {
    let index_name = match &idx.name {
        Some(name) => super::super::execute::object_name_str(name),
        None => return Err("CREATE INDEX requires a name".into()),
    };
    let table_name = super::super::execute::object_name_str(&idx.table_name);

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
        "\"Index '{}' on '{}' ({}) created\"",
        index_name, table_name, column
    ))
}

// ── CREATE TRIGGER ──────────────────────────────────────────────────────────

/// Create a trigger that fires on INSERT/UPDATE/DELETE.
pub(crate) fn exec_create_trigger(ct: &CreateTrigger, db: &mut Database) -> Result<String, String> {
    let table_name = ct.table_name.to_string().to_lowercase();
    let trigger_name = ct.name.to_string().to_lowercase();

    let event_str = match ct.events.first() {
        Some(TriggerEvent::Insert) => "INSERT",
        Some(TriggerEvent::Update(_)) => "UPDATE",
        Some(TriggerEvent::Delete) => "DELETE",
        _ => return Err("Unsupported trigger event".into()),
    };
    let timing_str = match ct.period.as_ref() {
        Some(p) => match p {
            TriggerPeriod::Before => "BEFORE",
            TriggerPeriod::After => "AFTER",
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
    table.triggers.push(crate::engine::trigger::TriggerInfo {
        name: trigger_name.clone(),
        timing: timing_str.to_string(),
        event: event_str.to_string(),
        body: body.clone(),
    });

    Ok(format!("\"Trigger '{}' created on '{}'\"", trigger_name, table_name))
}
