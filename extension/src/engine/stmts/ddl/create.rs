// CREATE statements: TABLE, VIEW, TABLE AS SELECT, SEQUENCE, INDEX, TRIGGER, VIRTUAL TABLE

use super::{json_val_to_dbvalue, object_name_str, parse_data_type, sql_val_to_db};
use crate::engine::database::Database;
use crate::engine::index::IndexType as A3IndexType;
use crate::engine::table::{ForeignKeyInfo, Table};
use crate::engine::value::{Column, ColumnType, DbValue};
use sqlparser::ast::table_constraints::TableConstraint;
use sqlparser::ast::{
    ColumnOption, CreateIndex, CreateTrigger, DataType, Expr, Ident, ObjectName, SequenceOptions, TriggerEvent,
    TriggerPeriod,
};

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
                ColumnOption::Default(expression) => match expression {
                    Expr::Value(v) => default_val = Some(sql_val_to_db(&v.value)),
                    _ => return Err("DEFAULT only supports literal values".into()),
                },
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
    let json = super::super::select::exec_select(query, db)?;

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

// ── CREATE SEQUENCE ─────────────────────────────────────────────────────

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

// ── CREATE INDEX ───────────────────────────────────────────────────────────

pub(crate) fn exec_create_index(idx: &CreateIndex, db: &mut Database) -> Result<String, String> {
    let index_name = match &idx.name {
        Some(name) => crate::engine::execute::object_name_str(name),
        None => return Err("CREATE INDEX requires a name".into()),
    };
    let table_name = crate::engine::execute::object_name_str(&idx.table_name);

    if idx.if_not_exists {
        let table = db.get_table(&table_name)?;
        if table.has_index(&index_name) {
            return Ok(format!("\"Index '{}' already exists\"", index_name));
        }
    }

    use sqlparser::ast::IndexType as SqlIdx;
    let index_type = match &idx.using {
        None | Some(SqlIdx::BTree) => A3IndexType::BTree,
        Some(SqlIdx::GIN) => A3IndexType::Trigram,
        Some(SqlIdx::Custom(id)) if id.value.to_uppercase() == "TRIGRAM" => A3IndexType::Trigram,
        Some(other) => return Err(format!("Unsupported index type: {}", other)),
    };

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

// ── CREATE VIRTUAL TABLE ───────────────────────────────────────────────────

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
            crate::engine::index::IndexType::Trigram,
        );
    }
    db.add_table(tn.clone(), table);
    Ok(format!("\"Virtual table '{}' created (FTS trigram)\"", tn))
}
