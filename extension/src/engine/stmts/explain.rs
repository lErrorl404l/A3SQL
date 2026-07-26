// EXPLAIN plan generation — produces JSON query plan descriptions
// Extracted from execute.rs.

use serde_json::json;
use sqlparser::ast::{FromTable, GroupByExpr, LimitClause, OrderByKind, SetExpr, Statement, TableFactor};

use super::super::database::Database;

/// Generate an EXPLAIN plan description as a JSON array of plan nodes.
pub fn explain_statement(stmt: &Statement, db: &Database) -> Result<String, String> {
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
            // ponytail: inline resolve_table_from_joins to avoid depending on private fn in execute.rs
            let tname = match &upd.table.relation {
                TableFactor::Table { name, .. } => name.to_string().to_lowercase(),
                _ => String::new(),
            };
            let plan = json!({"type": "Update", "table": tname});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::Delete(del) => {
            let tname = match &del.from {
                FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => match tables.first() {
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

        Statement::Merge(merge) => {
            let tname = format!("{}", merge.table);
            let plan = json!({"type": "Merge", "target": tname, "clauses": merge.clauses.len()});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::AttachDatabase { schema_name, .. } => {
            let plan = json!({"type": "AttachDatabase", "schema": schema_name.to_string()});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        Statement::CreateVirtualTable { name, module_name, .. } => {
            let plan = json!(
                {"type": "CreateVirtualTable", "table": name.to_string(), "module": module_name.to_string()}
            );
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }

        // Fallback for unsupported statement types — show the SQL text
        other => {
            let plan = json!({"type": format!("{}", other)});
            serde_json::to_string(&[plan]).map_err(|e| e.to_string())
        }
    }
}
