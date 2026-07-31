// CTE execution — WITH clauses, recursive CTEs, SELECT INTO TABLE
// Extracted from execute.rs

//! CTE execution — WITH clauses, recursive CTEs, SELECT INTO TABLE.

use sqlparser::ast::{Query, SetExpr};

use super::super::super::execute::select::exec_select;
use super::exec_union;
use crate::engine::error::EngineError;
use crate::engine::prelude::*;

/// Execute a Query that may have WITH/CTE clauses, recursive CTEs,
/// and/or SELECT INTO TABLE.
pub(crate) fn exec_cte_query(query: &Query, db: &mut Database) -> Result<String, EngineError> {
    // Process WITH / CTE clauses before executing the main query body
    let mut cte_tables: Vec<String> = Vec::new();
    if let Some(with) = &query.with {
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
            } else if matches!(&*cte.query.body, SetExpr::SetOperation { .. }) {
                exec_union(&cte.query.body, &cte.query, db)?
            } else {
                exec_select(&cte.query, db)?
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
                        unique: false,
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
    if let Some(with) = &query.with {
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
                        let current: Vec<Vec<serde_json::Value>> = serde_json::from_str(&json).unwrap_or_default();
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

    let result = if matches!(&*query.body, SetExpr::SetOperation { .. }) {
        exec_union(&query.body, query, db)
    } else {
        exec_select(query, db)
    };

    // Clean up CTE temp tables
    for name in &cte_tables {
        let _ = db.drop_table(name);
    }

    // SELECT INTO TABLE — create table from query results
    if let Ok(json) = result.as_deref() {
        if let SetExpr::Select(select) = &*query.body {
            if let Some(ref into) = select.into {
                let target_name = into.name.to_string().to_lowercase();
                if !into.table {
                    // non-table INTO (e.g. INTO OUTFILE) not supported
                } else if let Ok(rows) = serde_json::from_str::<Vec<Vec<serde_json::Value>>>(json) {
                    if rows.len() >= 2 {
                        let header = &rows[0];
                        let cols: Vec<Column> = header
                            .iter()
                            .enumerate()
                            .map(|(i, h)| {
                                let dtype = rows[1]
                                    .get(i)
                                    .map(|v| match v {
                                        serde_json::Value::Number(n) => {
                                            if n.is_f64() {
                                                ColumnType::Float
                                            } else {
                                                ColumnType::Int
                                            }
                                        }
                                        serde_json::Value::Bool(_) => ColumnType::Bool,
                                        _ => ColumnType::String,
                                    })
                                    .unwrap_or(ColumnType::String);
                                Column {
                                    name: h.as_str().unwrap_or(&format!("col{}", i)).to_lowercase(),
                                    dtype,
                                    primary_key: false,
                                    not_null: false,
                                    default: None,
                                    auto_increment: false,
                                    unique: false,
                                }
                            })
                            .collect();
                        if let Ok(mut table) = Table::new(target_name.clone(), cols) {
                            for row_data in &rows[1..] {
                                let db_row: Vec<DbValue> = row_data.iter().map(json_val_to_dbvalue).collect();
                                let _ = table.insert(db_row);
                            }
                            db.add_table(target_name.clone(), table);
                        }
                    }
                }
            }
        }
    }

    result
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
