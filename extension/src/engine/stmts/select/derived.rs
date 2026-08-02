// Derived tables in FROM — (SELECT ...) AS d materialised as temp tables

//! FROM derived tables — executed once against the current db and registered
//! as `__a3sql_derived_N` temp tables so the JOIN flat-column machinery
//! (col_map, NATURAL/USING position lookup) works unchanged.

use std::collections::{HashMap, HashSet};

use sqlparser::ast::{Query, SelectItem, SetExpr, TableAlias, TableFactor};

use crate::engine::error::EngineError;
use crate::engine::execute::select::exec_select;
use crate::engine::functions::builtin::json_type_to_column;
use crate::engine::functions::eval::{has_outer_refs, subquery_table_names};
use crate::engine::prelude::*;

/// Resolve a FROM/join table factor to (name, alias, table). Handles plain
/// tables (materialising views) and derived tables (`FROM (SELECT ...) d`).
///
/// A derived table is executed once against the current db — FROM subqueries
/// are never correlated, so no row context is needed — and registered as a
/// temp table whose name is recorded in `temp_tables` for the caller to drop
/// when done.
pub(crate) fn resolve_any_factor(
    factor: &TableFactor,
    db: &mut Database,
    derived_ctr: &mut usize,
    temp_tables: &mut Vec<String>,
) -> Result<(String, Option<String>, Table), EngineError> {
    match factor {
        TableFactor::Table { name, alias, .. } => {
            let tname = object_name_str(name);
            if !db.has_table(&tname) && db.has_view(&tname) {
                materialize_view(&tname, db)?;
            }
            let table = db.get_table(&tname).map_err(EngineError::Exec)?.clone();
            let a = alias.as_ref().map(|a| a.name.value.to_lowercase());
            Ok((tname, a, table))
        }
        TableFactor::Derived {
            lateral,
            subquery,
            alias,
            ..
        } => {
            if *lateral {
                return Err(EngineError::Exec("LATERAL derived tables not supported".into()));
            }
            if derived_is_correlated(subquery) {
                return Err(EngineError::Exec("correlated subquery in FROM not supported".into()));
            }
            let result_str = exec_select(subquery, db)?;
            *derived_ctr += 1;
            let temp_name = format!("__a3sql_derived_{}", derived_ctr);
            let table = subquery_json_to_table(&result_str, alias.as_ref(), &temp_name)?;
            db.add_table(temp_name.clone(), table.clone());
            temp_tables.push(temp_name.clone());
            let a = alias.as_ref().map(|a| a.name.value.to_lowercase());
            Ok((temp_name, a, table))
        }
        _ => Err(EngineError::Exec(
            "Only simple table references or derived tables supported in FROM".into(),
        )),
    }
}

/// A FROM derived table has no outer row context, so it can never be
/// decorrelated per-row. Reject references to tables outside the subquery's
/// own FROM (qualified outer refs in projection/WHERE) with a clear error.
fn derived_is_correlated(query: &Query) -> bool {
    let own = subquery_table_names(query);
    let empty_cols: HashSet<String> = HashSet::new();
    let empty_map: HashMap<String, usize> = HashMap::new();
    if let SetExpr::Select(s) = &*query.body {
        let sel = s
            .selection
            .as_ref()
            .is_some_and(|e| has_outer_refs(e, &own, &empty_cols, &empty_map));
        let proj = s.projection.iter().any(|item| match item {
            SelectItem::UnnamedExpr(e) | SelectItem::ExprWithAlias { expr: e, .. } => {
                has_outer_refs(e, &own, &empty_cols, &empty_map)
            }
            _ => false,
        });
        sel || proj
    } else {
        false
    }
}

/// Materialise a derived table from exec_select's JSON result
/// (`[[header],[row1],...]`). Column names come from the alias column list
/// (`d(x, y)`) when given, else from the result header with any table
/// qualifier stripped (`a.x` → `x`). Types default to String, inferred from
/// the first data row when present (same pattern as materialize_view / CTEs).
fn subquery_json_to_table(result_str: &str, alias: Option<&TableAlias>, name: &str) -> Result<Table, EngineError> {
    let rows: Vec<Vec<serde_json::Value>> = serde_json::from_str(result_str)
        .map_err(|e| EngineError::Exec(format!("Derived table result parse: {}", e)))?;
    let header = rows
        .first()
        .ok_or_else(|| EngineError::Exec("Derived table returned no header".into()))?;
    let alias_cols: Vec<String> = alias
        .map(|a| a.columns.iter().map(|c| c.name.value.to_lowercase()).collect())
        .unwrap_or_default();
    let cols: Vec<Column> = header
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let raw = h.as_str().unwrap_or("col").to_lowercase();
            Column {
                name: alias_cols
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| raw.rsplit('.').next().unwrap_or(&raw).to_string()),
                dtype: rows
                    .get(1)
                    .and_then(|r| r.get(i))
                    .map(json_type_to_column)
                    .unwrap_or(ColumnType::String),
                primary_key: false,
                not_null: false,
                default: None,
                default_expr: None,
                auto_increment: false,
                unique: false,
            }
        })
        .collect();
    let mut table = Table::new(name.to_string(), cols).map_err(EngineError::Exec)?;
    for row_data in rows.iter().skip(1) {
        let db_row: Vec<DbValue> = row_data.iter().map(json_val_to_dbvalue).collect();
        let _ = table.insert(db_row);
    }
    Ok(table)
}
