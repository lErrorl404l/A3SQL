// Schema introspection: DESCRIBE TABLE, SHOW CREATE TABLE — extracted from execute.rs

//! Table inspection — DESCRIBE TABLE, SHOW CREATE TABLE.

use crate::engine::database::Database;
use crate::engine::error::EngineError;

// ── DESCRIBE TABLE ────────────────────────────────────────────────────

/// DESCRIBE table — returns a JSON array of column definitions.
pub(crate) fn describe_table(db: &Database, table_name: &str) -> Result<String, EngineError> {
    let table = db
        .get_table(table_name)
        .map_err(|_| EngineError::TableNotFound(table_name.into()))?;
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

// ── SHOW CREATE TABLE ──────────────────────────────────────────────────

/// SHOW CREATE TABLE — returns a CREATE TABLE SQL statement.
pub(crate) fn show_create_table(db: &Database, table_name: &str) -> Result<String, EngineError> {
    let table = db
        .get_table(table_name)
        .map_err(|_| EngineError::TableNotFound(table_name.into()))?;
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
    serde_json::to_string(&sql).map_err(|e| EngineError::Exec(format!("{}", e)))
}
