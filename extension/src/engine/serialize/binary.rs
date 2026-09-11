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
// v0x03: persists column flags (auto_increment, not_null, unique) and
// defaults, plus per-table next_auto_inc counter. v0x02 saves load fine
// (flags default to false, counter to 1). v0x01 saves rejected.
const BINARY_VERSION: u8 = 0x03;
const CHECKSUM_LEN: usize = 8;

/// FNV-1a 64-bit — fast, no deps, good enough to catch truncation/corruption.
fn checksum(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in data {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

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

/// Export full database as binary, with an FNV-1a checksum trailer so
/// truncation/corruption is detected on load.
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

    // Trailer: FNV-1a over everything written so far.
    buf.extend_from_slice(&checksum(&buf).to_le_bytes());
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
        // v0x02: primary_key flag
        buf.push(if col.primary_key { 1 } else { 0 });
        // v0x03: flag byte — bit 0=auto_inc, 1=not_null, 2=unique,
        //         3=has_default, 4=has_default_expr
        let mut flags: u8 = 0;
        if col.auto_increment {
            flags |= 1;
        }
        if col.not_null {
            flags |= 1 << 1;
        }
        if col.unique {
            flags |= 1 << 2;
        }
        if col.default.is_some() {
            flags |= 1 << 3;
        }
        if col.default_expr.is_some() {
            flags |= 1 << 4;
        }
        buf.push(flags);
        // Default value (if present)
        if let Some(ref def) = col.default {
            write_bin_value(buf, def);
        }
        // Default expression (if present) — serialized as string
        if let Some(ref expr) = col.default_expr {
            write_bin_str(buf, &format!("{}", expr));
        }
    }
    // v0x03: next_auto_inc counter
    buf.extend_from_slice(&table.next_auto_inc.to_le_bytes());
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

/// Import database from binary. Verifies the checksum trailer and gives an
/// actionable message for old-format saves.
pub(crate) fn import_binary(data: &[u8], db: &mut Database) -> Result<(), String> {
    if data.len() < 5 + CHECKSUM_LEN {
        return Err("Binary data too short".into());
    }
    if &data[0..4] != BINARY_MAGIC {
        return Err("Invalid binary magic".into());
    }
    let version = data[4];
    if version == 0x01 {
        return Err(
            "Binary version 0x01 is unsupported (pre-1.0). Re-save from a working a3sql instance to migrate.".into(),
        );
    }
    if version > BINARY_VERSION {
        return Err(format!(
            "Save file uses format v{:#x}, this build supports v{:#x} — upgrade a3sql to load it.",
            version, BINARY_VERSION
        ));
    }
    // version is 0x02 or 0x03 — both accepted; read_bin_table dispatches on version

    // Verify checksum over everything before the trailer.
    let payload_end = data.len() - CHECKSUM_LEN;
    let expected = u64::from_le_bytes(
        data[payload_end..]
            .try_into()
            .map_err(|_| "Truncated binary data: checksum".to_string())?,
    );
    if checksum(&data[..payload_end]) != expected {
        return Err("Checksum mismatch — save file is corrupt or truncated.".into());
    }

    let mut pos = 5usize;
    if pos + 4 > payload_end {
        return Err("Truncated binary data".into());
    }
    let table_count = u32::from_le_bytes(
        data[pos..pos + 4]
            .try_into()
            .map_err(|_| "truncated binary data: table count".to_string())?,
    ) as usize;
    pos += 4;

    for _ in 0..table_count {
        pos = read_bin_table(data, pos, db, version)?;
    }

    Ok(())
}

fn read_bin_table(data: &[u8], mut pos: usize, db: &mut Database, version: u8) -> Result<usize, String> {
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

        // v0x03: flag byte + optional defaults
        let (auto_increment, not_null, unique, default, default_expr) = if version >= 0x03 {
            if pos >= data.len() {
                return Err("Truncated binary: column flags".into());
            }
            let flags = data[pos];
            pos += 1;
            let auto_inc = flags & 1 != 0;
            let not_null = flags & (1 << 1) != 0;
            let unique = flags & (1 << 2) != 0;
            let has_default = flags & (1 << 3) != 0;
            let has_default_expr = flags & (1 << 4) != 0;
            let def = if has_default {
                let (v, p) = read_bin_value(data, pos)?;
                pos = p;
                Some(v)
            } else {
                None
            };
            let expr = if has_default_expr {
                let (s, p) = read_bin_str(data, pos)?;
                pos = p;
                // Parse the expression string back into an Expr
                Some(sqlparser::ast::Expr::Identifier(sqlparser::ast::Ident::new(s)))
            } else {
                None
            };
            (auto_inc, not_null, unique, def, expr)
        } else {
            // v0x02: no flags, defaults to false/None
            (false, false, false, None, None)
        };

        columns.push(Column {
            name: col_name,
            dtype,
            primary_key,
            not_null,
            default,
            default_expr,
            auto_increment,
            unique,
        });
    }

    let mut table = Table::new(name.clone(), columns)?;

    // v0x03: next_auto_inc counter
    if version >= 0x03 {
        if pos + 8 > data.len() {
            return Err("Truncated binary: next_auto_inc".into());
        }
        table.next_auto_inc = i64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
    }

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
