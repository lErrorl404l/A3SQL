// a3sql serialization — Binary format
//
// Format:
//   [4 bytes] magic "A3SQL"
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

//! Binary serialization — compact binary format for save/load (`.bin` files).

use super::super::database::Database;
use super::super::table::Table;
use super::super::value::{Column, ColumnType, DbValue};

const BINARY_MAGIC: &[u8; 4] = b"A3SQ";
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
pub(crate) fn export_binary(db: &Database) -> Vec<u8> {
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
pub(crate) fn import_binary(data: &[u8], db: &mut Database) -> Result<(), String> {
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
    let table_count = u32::from_le_bytes(
        data[pos..pos + 4]
            .try_into()
            .map_err(|_| "truncated binary data: table count".to_string())?,
    ) as usize;
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
            not_null: false,
            default: None,
            auto_increment: false,
            unique: false,
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
        table.insert(row).map_err(|e| format!("Binary import: {}", e))?;
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
    let s = std::str::from_utf8(&data[start..start + len]).map_err(|_| "Invalid UTF-8 in binary".to_string())?;
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
