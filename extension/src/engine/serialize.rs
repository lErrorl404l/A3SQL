// a3db serialization — JSON, CSV, SQL, and Binary formats

use super::database::Database;
use super::table::Table;
use super::value::{Column, ColumnType, DbValue};

/// Supported serialization formats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
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
// JSON
// ═══════════════════════════════════════════════════════════════════════════

/// Export a table as JSON.
pub fn export_json(table: &Table) -> String {
    let cols_json: Vec<String> = table
        .columns
        .iter()
        .map(|c| {
            format!(
                r#"{{"name":"{}","type":"{}","primary_key":{}}}"#,
                c.name, c.dtype, c.primary_key
            )
        })
        .collect();

    let rows_json: Vec<String> = table
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<String> = row.iter().map(|v| v.to_json_string()).collect();
            format!("[{}]", cells.join(","))
        })
        .collect();

    format!(
        r#"{{"name":"{}","columns":[{}],"rows":[{}]}}"#,
        table.name,
        cols_json.join(","),
        rows_json.join(",")
    )
}

/// Export full database as JSON.
pub fn export_json_db(db: &Database) -> String {
    let tables: Vec<String> = db
        .table_names()
        .iter()
        .filter_map(|name| db.get_table(name).ok())
        .map(export_json)
        .collect();
    format!(r#"{{"tables":[{}]}}"#, tables.join(","))
}

/// Import a table from JSON data.
pub fn import_json(table_name: &str, json_str: &str, db: &mut Database) -> Result<(), String> {
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;

    let obj = parsed.as_object().ok_or("Expected JSON object")?;

    // Parse columns
    let columns = match obj.get("columns") {
        Some(serde_json::Value::Array(arr)) => {
            let mut cols = Vec::new();
            for col_val in arr {
                let col_obj = col_val.as_object().ok_or("Invalid column definition")?;
                let name = col_obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Column missing 'name'")?
                    .to_string();
                let type_str = col_obj
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("String");
                let primary_key = col_obj
                    .get("primary_key")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let dtype = match type_str.to_lowercase().as_str() {
                    "bool" | "boolean" => ColumnType::Bool,
                    "int" | "integer" => ColumnType::Int,
                    "float" | "double" => ColumnType::Float,
                    "string" | "text" => ColumnType::String,
                    "strings" => ColumnType::Strings,
                    "floats" => ColumnType::Floats,
                    _ => ColumnType::String,
                };
                cols.push(Column {
                    name,
                    dtype,
                    primary_key,
                });
            }
            cols
        }
        _ => return Err("Missing or invalid 'columns' array".into()),
    };

    let mut table = Table::new(table_name.into(), columns).map_err(|e| format!("Schema: {}", e))?;

    // Parse rows
    if let Some(serde_json::Value::Array(rows)) = obj.get("rows") {
        for row_val in rows {
            let cells = row_val.as_array().ok_or("Row must be an array")?;
            let mut db_row = Vec::with_capacity(cells.len());
            for (i, cell) in cells.iter().enumerate() {
                let col_type = &table.columns[i].dtype;
                db_row.push(json_to_dbvalue(cell, col_type));
            }
            table
                .insert(db_row)
                .map_err(|e| format!("Row insert: {}", e))?;
        }
    }

    db.create_table(table_name, table)
}

fn json_to_dbvalue(v: &serde_json::Value, expected: &ColumnType) -> DbValue {
    match (v, expected) {
        (serde_json::Value::Null, _) => DbValue::Null,
        (serde_json::Value::Bool(b), _) => DbValue::Bool(*b),
        (serde_json::Value::Number(n), ColumnType::Int) => {
            n.as_i64().map(DbValue::Int).unwrap_or(DbValue::Null)
        }
        (serde_json::Value::Number(n), ColumnType::Float) => {
            n.as_f64().map(DbValue::Float).unwrap_or(DbValue::Null)
        }
        (serde_json::Value::Number(n), _) => n
            .as_f64()
            .map(|f| {
                if f.fract() == 0.0 {
                    DbValue::Int(f as i64)
                } else {
                    DbValue::Float(f)
                }
            })
            .unwrap_or(DbValue::Null),
        (serde_json::Value::String(s), ColumnType::Strings) => DbValue::Strings(vec![s.clone()]),
        (serde_json::Value::String(s), _) => DbValue::String(s.clone()),
        (serde_json::Value::Array(arr), ColumnType::Strings) => {
            let strs: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            DbValue::Strings(strs)
        }
        (serde_json::Value::Array(arr), ColumnType::Floats) => {
            let flts: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
            DbValue::Floats(flts)
        }
        (serde_json::Value::Array(arr), _) => {
            // Mixed array — try to parse
            let strs: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if !strs.is_empty() {
                DbValue::Strings(strs)
            } else {
                DbValue::String(format!("{:?}", arr))
            }
        }
        _ => DbValue::String(format!("{}", v)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CSV
// ═══════════════════════════════════════════════════════════════════════════

/// Export a table as CSV (header row + data rows).
pub fn export_csv(table: &Table) -> String {
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
pub fn import_csv(table_name: &str, csv_str: &str, db: &mut Database) -> Result<(), String> {
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
            return Err(format!(
                "CSV row has {} fields, expected {}",
                fields.len(),
                col_count
            ));
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

fn parse_csv_line(line: &str) -> Vec<String> {
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

// ═══════════════════════════════════════════════════════════════════════════
// SQL dump
// ═══════════════════════════════════════════════════════════════════════════

/// Export full database as SQL statements.
pub fn export_sql(db: &Database) -> String {
    let mut out = String::new();
    out.push_str("-- a3db SQL dump\n\n");

    for name in db.table_names() {
        let table = match db.get_table(name) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // CREATE TABLE
        let col_defs: Vec<String> = table
            .columns
            .iter()
            .map(|c| {
                let type_str = format!("{}", c.dtype);
                let pk = if c.primary_key { " PRIMARY KEY" } else { "" };
                format!("{} {}{}", c.name, type_str, pk)
            })
            .collect();
        out.push_str(&format!(
            "CREATE TABLE {} ({});\n",
            table.name,
            col_defs.join(", ")
        ));

        // INSERT rows
        for row in &table.rows {
            let vals: Vec<String> = row.iter().map(sql_value).collect();
            out.push_str(&format!(
                "INSERT INTO {} VALUES ({});\n",
                table.name,
                vals.join(", ")
            ));
        }
        out.push('\n');
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
            let inner: Vec<String> = arr
                .iter()
                .map(|s| format!("'{}'", s.replace('\'', "''")))
                .collect();
            format!("ARRAY[{}]", inner.join(","))
        }
        DbValue::Floats(arr) => {
            let inner: Vec<String> = arr.iter().map(|f| f.to_string()).collect();
            format!("ARRAY[{}]", inner.join(","))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Binary format
// ═══════════════════════════════════════════════════════════════════════════
//
// Format:
//   [4 bytes] magic "A3DB"
//   [1 byte]  version (0x01)
//   [4 bytes] table count (u32 LE)
//   for each table:
//     [4 bytes] name length (u32 LE) + UTF-8 bytes
//     [4 bytes] column count (u32 LE)
//     for each column:
//       [4 bytes] name length + UTF-8 bytes
//       [1 byte]  type tag
//       [1 byte]  primary_key flag
//     [4 bytes] row count (u32 LE)
//     for each row:
//       for each column:
//         [1 byte] value tag
//         value data

const BINARY_MAGIC: &[u8; 4] = b"A3DB";
const BINARY_VERSION: u8 = 0x01;

#[repr(u8)]
enum BinTag {
    Null = 0,
    Bool = 1,
    Int = 2,
    Float = 3,
    String = 4,
    Strings = 5,
    Floats = 6,
}

/// Export full database as binary.
pub fn export_binary(db: &Database) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(BINARY_MAGIC);
    buf.push(BINARY_VERSION);

    let names = db.table_names();
    let table_count = names.len() as u32;
    buf.extend_from_slice(&table_count.to_le_bytes());

    for name in names {
        let table = match db.get_table(name) {
            Ok(t) => t,
            Err(_) => continue,
        };
        write_bin_table(&mut buf, table);
    }

    buf
}

fn write_bin_table(buf: &mut Vec<u8>, table: &Table) {
    // Name
    write_bin_str(buf, &table.name);
    // Columns
    let col_count = table.columns.len() as u32;
    buf.extend_from_slice(&col_count.to_le_bytes());
    for col in &table.columns {
        write_bin_str(buf, &col.name);
        buf.push(col_type_tag(&col.dtype));
        buf.push(if col.primary_key { 1 } else { 0 });
    }
    // Rows
    let row_count = table.rows.len() as u32;
    buf.extend_from_slice(&row_count.to_le_bytes());
    for row in &table.rows {
        for val in row {
            write_bin_value(buf, val);
        }
    }
}

fn write_bin_str(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = bytes.len() as u32;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(bytes);
}

fn col_type_tag(dtype: &ColumnType) -> u8 {
    match dtype {
        ColumnType::Bool => 0,
        ColumnType::Int => 1,
        ColumnType::Float => 2,
        ColumnType::String => 3,
        ColumnType::Strings => 4,
        ColumnType::Floats => 5,
    }
}

fn dtype_from_tag(tag: u8) -> Result<ColumnType, String> {
    match tag {
        0 => Ok(ColumnType::Bool),
        1 => Ok(ColumnType::Int),
        2 => Ok(ColumnType::Float),
        3 => Ok(ColumnType::String),
        4 => Ok(ColumnType::Strings),
        5 => Ok(ColumnType::Floats),
        _ => Err(format!("Unknown column type tag: {}", tag)),
    }
}

fn write_bin_value(buf: &mut Vec<u8>, val: &DbValue) {
    match val {
        DbValue::Null => buf.push(BinTag::Null as u8),
        DbValue::Bool(b) => {
            buf.push(BinTag::Bool as u8);
            buf.push(if *b { 1 } else { 0 });
        }
        DbValue::Int(n) => {
            buf.push(BinTag::Int as u8);
            buf.extend_from_slice(&n.to_le_bytes());
        }
        DbValue::Float(f) => {
            buf.push(BinTag::Float as u8);
            buf.extend_from_slice(&f.to_bits().to_le_bytes());
        }
        DbValue::String(s) => {
            buf.push(BinTag::String as u8);
            write_bin_str(buf, s);
        }
        DbValue::Strings(arr) => {
            buf.push(BinTag::Strings as u8);
            let count = arr.len() as u32;
            buf.extend_from_slice(&count.to_le_bytes());
            for s in arr {
                write_bin_str(buf, s);
            }
        }
        DbValue::Floats(arr) => {
            buf.push(BinTag::Floats as u8);
            let count = arr.len() as u32;
            buf.extend_from_slice(&count.to_le_bytes());
            for f in arr {
                buf.extend_from_slice(&f.to_bits().to_le_bytes());
            }
        }
    }
}

/// Import database from binary.
pub fn import_binary(data: &[u8], db: &mut Database) -> Result<(), String> {
    if data.len() < 5 {
        return Err("Binary data too short".into());
    }
    if &data[0..4] != BINARY_MAGIC {
        return Err("Invalid binary magic".into());
    }
    if data[4] != BINARY_VERSION {
        return Err(format!("Unsupported binary version {}", data[4]));
    }

    let mut pos = 5usize;
    if pos + 4 > data.len() {
        return Err("Truncated binary data".into());
    }
    let table_count = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;

    for _ in 0..table_count {
        pos = read_bin_table(data, pos, db)?;
    }

    Ok(())
}

fn read_bin_table(data: &[u8], mut pos: usize, db: &mut Database) -> Result<usize, String> {
    // Name
    let (name, new_pos) = read_bin_str(data, pos)?;
    pos = new_pos;

    // Columns
    if pos + 4 > data.len() {
        return Err("Truncated binary: column count".into());
    }
    let col_count = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;

    let mut columns = Vec::with_capacity(col_count);
    for _ in 0..col_count {
        let (col_name, new_pos) = read_bin_str(data, pos)?;
        pos = new_pos;
        if pos + 2 > data.len() {
            return Err("Truncated binary: column def".into());
        }
        let dtype = dtype_from_tag(data[pos])?;
        let primary_key = data[pos + 1] != 0;
        pos += 2;
        columns.push(Column {
            name: col_name,
            dtype,
            primary_key,
        });
    }

    let mut table = Table::new(name.clone(), columns)?;

    // Rows
    if pos + 4 > data.len() {
        return Err("Truncated binary: row count".into());
    }
    let row_count = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;

    for _ in 0..row_count {
        let mut row = Vec::with_capacity(table.col_count());
        for _ in 0..table.col_count() {
            let (val, new_pos) = read_bin_value(data, pos)?;
            row.push(val);
            pos = new_pos;
        }
        table
            .insert(row)
            .map_err(|e| format!("Binary import: {}", e))?;
    }

    db.create_table(&name, table)?;
    Ok(pos)
}

fn read_bin_str(data: &[u8], pos: usize) -> Result<(String, usize), String> {
    if pos + 4 > data.len() {
        return Err("Truncated binary: string length".into());
    }
    let len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
    let start = pos + 4;
    if start + len > data.len() {
        return Err("Truncated binary: string data".into());
    }
    let s = std::str::from_utf8(&data[start..start + len])
        .map_err(|_| "Invalid UTF-8 in binary".to_string())?;
    Ok((s.to_string(), start + len))
}

fn read_bin_value(data: &[u8], pos: usize) -> Result<(DbValue, usize), String> {
    if pos >= data.len() {
        return Err("Truncated binary: value tag".into());
    }
    match data[pos] {
        t if t == BinTag::Null as u8 => Ok((DbValue::Null, pos + 1)),
        t if t == BinTag::Bool as u8 => {
            if pos + 2 > data.len() {
                return Err("Truncated binary: bool".into());
            }
            Ok((DbValue::Bool(data[pos + 1] != 0), pos + 2))
        }
        t if t == BinTag::Int as u8 => {
            if pos + 9 > data.len() {
                return Err("Truncated binary: int".into());
            }
            let n = i64::from_le_bytes(data[pos + 1..pos + 9].try_into().unwrap());
            Ok((DbValue::Int(n), pos + 9))
        }
        t if t == BinTag::Float as u8 => {
            if pos + 9 > data.len() {
                return Err("Truncated binary: float".into());
            }
            let bits = u64::from_le_bytes(data[pos + 1..pos + 9].try_into().unwrap());
            Ok((DbValue::Float(f64::from_bits(bits)), pos + 9))
        }
        t if t == BinTag::String as u8 => {
            let (s, new_pos) = read_bin_str(data, pos + 1)?;
            Ok((DbValue::String(s), new_pos))
        }
        t if t == BinTag::Strings as u8 => {
            let mut p = pos + 1;
            if p + 4 > data.len() {
                return Err("Truncated binary: strings count".into());
            }
            let count = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
            p += 4;
            let mut arr = Vec::with_capacity(count);
            for _ in 0..count {
                let (s, new_p) = read_bin_str(data, p)?;
                arr.push(s);
                p = new_p;
            }
            Ok((DbValue::Strings(arr), p))
        }
        t if t == BinTag::Floats as u8 => {
            let mut p = pos + 1;
            if p + 4 > data.len() {
                return Err("Truncated binary: floats count".into());
            }
            let count = u32::from_le_bytes(data[p..p + 4].try_into().unwrap()) as usize;
            p += 4;
            let mut arr = Vec::with_capacity(count);
            for _ in 0..count {
                if p + 8 > data.len() {
                    return Err("Truncated binary: float value".into());
                }
                let bits = u64::from_le_bytes(data[p..p + 8].try_into().unwrap());
                arr.push(f64::from_bits(bits));
                p += 8;
            }
            Ok((DbValue::Floats(arr), p))
        }
        t => Err(format!("Unknown binary value tag: {}", t)),
    }
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
        DbValue::Floats(arr) => arr
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(";"),
    }
}

// ── Convenience ──────────────────────────────────────────────────────────

/// Export in the given format.
pub fn export(format: Format, table: &Table, db: &Database) -> String {
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

pub fn hex_encode(bytes: &[u8]) -> String {
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
pub fn import(
    format: Format,
    table_name: &str,
    data: &str,
    db: &mut Database,
) -> Result<(), String> {
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
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::Int,
                primary_key: false,
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
        let json = export_json_db(&db); // full DB JSON
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
        let fields = parse_csv_line(r#"a,"b,c",d"#);
        assert_eq!(fields, vec!["a", "b,c", "d"]);
    }

    #[test]
    fn csv_parse_escaped_quote() {
        let fields = parse_csv_line(r#"a,"b""c",d"#);
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
