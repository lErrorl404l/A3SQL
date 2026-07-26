// a3sql serialization — CSV format

//! CSV serialization — comma-separated values for spreadsheet interoperability.

use super::super::database::Database;
use super::super::table::Table;
use super::super::value::{Column, ColumnType, DbValue};

/// Export a table as CSV (header row + data rows).
pub(crate) fn export_csv(table: &Table) -> String {
    let header: String = table
        .columns
        .iter()
        .map(|c| csv_quote(&c.name))
        .collect::<Vec<_>>()
        .join(",");

    let rows: String = table
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|v| csv_quote(&value_to_display(v)))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n");

    if rows.is_empty() {
        header
    } else {
        format!("{}\n{}", header, rows)
    }
}

/// Import a table from CSV string.
pub(crate) fn import_csv(table_name: &str, csv_str: &str, db: &mut Database) -> Result<(), String> {
    let lines: Vec<&str> = csv_str.lines().collect();
    if lines.is_empty() {
        return Err("Empty CSV".into());
    }

    // Parse header
    let headers = parse_csv_line(lines[0]);
    let columns: Vec<Column> = headers
        .iter()
        .map(|h| Column {
            name: h.to_lowercase(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        })
        .collect();

    let col_count = columns.len();
    let mut table = Table::new(table_name.into(), columns)?;

    // Parse data rows
    for line in &lines[1..] {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields = parse_csv_line(trimmed);
        if fields.len() != col_count {
            return Err(format!("CSV row has {} fields, expected {}", fields.len(), col_count));
        }
        let row: Vec<DbValue> = fields.into_iter().map(DbValue::String).collect();
        table.insert(row)?;
    }

    db.create_table(table_name, table)
}

fn csv_quote(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Parse a single CSV line into fields (used in tests).
pub(crate) fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars();

    while let Some(c) = chars.next() {
        match c {
            '"' if !in_quotes => in_quotes = true,
            '"' => {
                // Check for escaped quote ""
                let next = chars.clone().next();
                if next == Some('"') {
                    current.push('"');
                    chars.next(); // skip second "
                } else {
                    in_quotes = false;
                }
            }
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// Get a display string for a DbValue (for CSV export).
fn value_to_display(v: &DbValue) -> String {
    match v {
        DbValue::Null => String::new(),
        DbValue::Bool(b) => b.to_string(),
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => s.clone(),
        DbValue::Strings(arr) => arr.join(";"),
        DbValue::Floats(arr) => arr.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(";"),
    }
}
