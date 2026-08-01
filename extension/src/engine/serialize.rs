// a3sql serialization — JSON, CSV, SQL, and Binary formats

//! Serialization — import/export tables in JSON, CSV, SQL, and Binary formats.

use super::database::Database;
use super::table::Table;
use super::value::DbValue;

mod binary;
mod csv;
mod json;

pub(crate) use binary::{export_binary, import_binary};
pub(crate) use csv::{export_csv, import_csv};
pub(crate) use json::{export_json, import_json};

/// Supported serialization formats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Format {
    Json,
    Csv,
    Sql,
    Binary,
}

impl std::str::FromStr for Format {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Format::Json),
            "csv" => Ok(Format::Csv),
            "sql" => Ok(Format::Sql),
            "bin" | "binary" => Ok(Format::Binary),
            _ => Err(format!("Unknown format '{}'", s)),
        }
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Json => write!(f, "JSON"),
            Format::Csv => write!(f, "CSV"),
            Format::Sql => write!(f, "SQL"),
            Format::Binary => write!(f, "BINARY"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SQL dump
// ═══════════════════════════════════════════════════════════════════════════

/// Export full database as SQL statements including schema (tables, views, indexes).
pub(crate) fn export_sql(db: &Database) -> String {
    let mut out = String::new();
    out.push_str("-- a3sql SQL dump\n\n");

    // ── Tables ──────────────────────────────────────────────────────────
    for name in db.table_names() {
        let table = match db.get_table(name) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // CREATE TABLE with constraints
        let col_defs: Vec<String> = table
            .columns
            .iter()
            .map(|c| {
                let mut parts: Vec<String> = Vec::new();
                parts.push(format!("{} {}", c.name, c.dtype));
                if c.primary_key {
                    parts.push("PRIMARY KEY".into());
                }
                if c.auto_increment {
                    parts.push("AUTO_INCREMENT".into());
                }
                if c.not_null {
                    parts.push("NOT NULL".into());
                }
                if let Some(ref def) = c.default {
                    parts.push(format!("DEFAULT {}", def));
                }
                parts.join(" ")
            })
            .collect();
        let mut fk_parts: Vec<String> = Vec::new();
        for fk in &table.foreign_keys {
            fk_parts.push(format!(
                "FOREIGN KEY ({}) REFERENCES {} ({})",
                fk.local_column, fk.foreign_table, fk.foreign_column
            ));
        }
        let all_parts: Vec<String> = col_defs.into_iter().chain(fk_parts).collect();
        out.push_str(&format!("CREATE TABLE {} ({});\n", table.name, all_parts.join(", ")));

        // CREATE INDEX for each secondary index
        for (meta, _) in &table.indices {
            out.push_str(&format!(
                "CREATE {} INDEX {} ON {} ({});\n",
                meta.index_type, meta.name, meta.table, meta.column
            ));
        }

        // INSERT rows
        for row in &table.rows {
            let vals: Vec<String> = row.iter().map(sql_value).collect();
            out.push_str(&format!("INSERT INTO {} VALUES ({});\n", table.name, vals.join(", ")));
        }
        out.push('\n');
    }

    // ── Views ───────────────────────────────────────────────────────────
    for vname in db.view_names() {
        if let Some(sql) = db.get_view(vname) {
            out.push_str(&format!("CREATE VIEW {} AS {};\n\n", vname, sql));
        }
    }

    out
}

/// Format a DbValue for SQL output.
fn sql_value(v: &DbValue) -> String {
    match v {
        DbValue::Null => "NULL".into(),
        DbValue::Bool(b) => {
            if *b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        DbValue::Strings(arr) => {
            let inner: Vec<String> = arr.iter().map(|s| format!("'{}'", s.replace('\'', "''"))).collect();
            format!("ARRAY[{}]", inner.join(","))
        }
        DbValue::Floats(arr) => {
            let inner: Vec<String> = arr.iter().map(|f| f.to_string()).collect();
            format!("ARRAY[{}]", inner.join(","))
        }
    }
}

// ── Convenience ──────────────────────────────────────────────────────────

/// Export in the given format.
pub(crate) fn export(format: Format, table: &Table, db: &Database) -> String {
    match format {
        Format::Json => export_json(table),
        Format::Csv => export_csv(table),
        Format::Sql => export_sql(db),
        Format::Binary => {
            // Binary returns raw bytes, encode as hex for string transport
            let bytes = export_binary(db);
            hex_encode(&bytes)
        }
    }
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    let hex = hex.trim();
    if !hex.len().is_multiple_of(2) {
        return Err("Hex string length must be even".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| format!("Hex decode: {}", e)))
        .collect()
}

/// Import in the given format.
pub(crate) fn import(format: Format, table_name: &str, data: &str, db: &mut Database) -> Result<(), String> {
    match format {
        Format::Json => import_json(table_name, data, db),
        Format::Csv => import_csv(table_name, data, db),
        Format::Sql => Err("SQL format import not yet supported (use JSON or CSV)".into()),
        Format::Binary => {
            let bytes = hex_decode(data)?;
            import_binary(&bytes, db)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::value::*;

    fn make_db() -> Database {
        let mut db = Database::new();
        let cols = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::String,
                primary_key: true,
                not_null: false,
                default: None,
                default_expr: None,
                auto_increment: false,
                unique: false,
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                default_expr: None,
                auto_increment: false,
                unique: false,
            },
        ];
        let mut table = Table::new("items".into(), cols).unwrap();
        table
            .insert(vec![DbValue::String("a".into()), DbValue::Int(10)])
            .unwrap();
        table
            .insert(vec![DbValue::String("b".into()), DbValue::Int(20)])
            .unwrap();
        db.create_table("items", table).unwrap();
        db
    }

    #[test]
    fn json_roundtrip() {
        let db = make_db();
        let table = db.get_table("items").unwrap();
        let json = export_json(table); // single-table JSON
        let mut db2 = Database::new();
        import_json("items", &json, &mut db2).unwrap();
        assert!(db2.has_table("items"));
        let t = db2.get_table("items").unwrap();
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.columns[0].name, "id");
    }

    #[test]
    fn json_db_roundtrip() {
        let db = make_db();
        let json = super::json::export_json_db(&db); // full DB JSON
        // Parse and verify structure
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("tables").is_some());
    }

    #[test]
    fn csv_roundtrip() {
        let db = make_db();
        let table = db.get_table("items").unwrap();
        let csv = export_csv(table);
        let mut db2 = Database::new();
        import_csv("items", &csv, &mut db2).unwrap();
        let t = db2.get_table("items").unwrap();
        assert_eq!(t.rows.len(), 2);
    }

    #[test]
    fn sql_dump() {
        let db = make_db();
        let sql = export_sql(&db);
        assert!(sql.contains("CREATE TABLE"));
        assert!(sql.contains("INSERT INTO"));
        assert!(sql.contains("items"));
    }

    #[test]
    fn binary_roundtrip() {
        let db = make_db();
        let bytes = export_binary(&db);
        let mut db2 = Database::new();
        import_binary(&bytes, &mut db2).unwrap();
        assert!(db2.has_table("items"));
        let t = db2.get_table("items").unwrap();
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.columns[0].name, "id");
    }

    #[test]
    fn csv_parse() {
        let fields = csv::parse_csv_line(r#"a,"b,c",d"#);
        assert_eq!(fields, vec!["a", "b,c", "d"]);
    }

    #[test]
    fn csv_parse_escaped_quote() {
        let fields = csv::parse_csv_line(r#"a,"b""c",d"#);
        assert_eq!(fields, vec!["a", r#"b"c"#, "d"]);
    }

    #[test]
    fn json_table_export() {
        let db = make_db();
        let table = db.get_table("items").unwrap();
        let json = export_json(table);
        assert!(json.contains("items"));
        assert!(json.contains("id"));
        assert!(json.contains("STRING"));
    }

    #[test]
    fn format_from_str() {
        assert_eq!("json".parse::<Format>().unwrap(), Format::Json);
        assert_eq!("CSV".parse::<Format>().unwrap(), Format::Csv);
        assert_eq!("SQL".parse::<Format>().unwrap(), Format::Sql);
        assert_eq!("binary".parse::<Format>().unwrap(), Format::Binary);
        assert!("unknown".parse::<Format>().is_err());
    }
}
