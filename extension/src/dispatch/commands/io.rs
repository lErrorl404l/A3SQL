use crate::engine;
use crate::engine::error::{error_response, ok_response, ErrorCode};

// ── Import/Export handlers ──────────────────────────────────────────────

pub(crate) fn handle_export(db: &engine::Database, input: &str, _args: &[&str]) -> String {
    let parts: Vec<&str> = input.splitn(3, |c: char| c.is_whitespace()).collect();
    let format_str = parts.get(1).unwrap_or(&"json");
    let table_name = parts.get(2).map(|s| s.trim());

    let format: engine::serialize::Format = match format_str.parse() {
        Ok(f) => f,
        Err(e) => return error_response(ErrorCode::Exec, &e),
    };

    match format {
        engine::serialize::Format::Sql => {
            let sql = engine::serialize::export_sql(db);
            let encoded = ::serde_json::to_string(&sql).unwrap_or_else(|_| "\"\"".into());
            ok_response(&encoded)
        }
        engine::serialize::Format::Binary => {
            let bytes = engine::serialize::export_binary(db);
            let hex = engine::serialize::hex_encode(&bytes);
            ok_response(&format!("\"{}\"", hex))
        }
        engine::serialize::Format::Csv => {
            let name = match table_name {
                Some(n) if !n.is_empty() => n,
                _ => return error_response(ErrorCode::Exec, "Usage: export <format> <table>"),
            };
            match db.get_table(name) {
                Ok(table) => {
                    let data = engine::serialize::export(format, table, db);
                    let encoded = ::serde_json::to_string(&data).unwrap_or_else(|_| "\"\"".into());
                    ok_response(&encoded)
                }
                Err(e) => error_response(ErrorCode::Table, &e),
            }
        }
        _ => {
            let name = match table_name {
                Some(n) if !n.is_empty() => n,
                _ => return error_response(ErrorCode::Exec, "Usage: export <format> <table>"),
            };
            match db.get_table(name) {
                Ok(table) => {
                    let data = engine::serialize::export(format, table, db);
                    ok_response(&data)
                }
                Err(e) => error_response(ErrorCode::Table, &e),
            }
        }
    }
}

pub(crate) fn handle_import(db: &mut engine::Database, input: &str, args: &[&str]) -> String {
    let parts: Vec<&str> = input.splitn(3, |c: char| c.is_whitespace()).collect();
    let format_str = parts.get(1).unwrap_or(&"json");
    let table_name = parts.get(2).map(|s| s.trim()).unwrap_or("");

    if table_name.is_empty() {
        return error_response(ErrorCode::Exec, "Usage: import <format> <table>");
    }

    let format: engine::serialize::Format = match format_str.parse() {
        Ok(f) => f,
        Err(e) => return error_response(ErrorCode::Exec, &e),
    };

    let data = args.first().unwrap_or(&"");
    if data.is_empty() {
        return error_response(ErrorCode::Exec, "No data provided");
    }
    match engine::serialize::import(format, table_name, data, db) {
        Ok(()) => ok_response(&format!("\"Imported into '{}'\"", table_name)),
        Err(e) => error_response(ErrorCode::Exec, &e),
    }
}

pub(crate) fn handle_dump_sql(db: &engine::Database) -> String {
    let sql = engine::serialize::export_sql(db);
    let encoded = ::serde_json::to_string(&sql).unwrap_or_else(|_| format!("\"{}\"", ""));
    ok_response(&encoded)
}

pub(crate) fn handle_save(db: &engine::Database, args: &[&str]) -> String {
    let filename = args.first().unwrap_or(&"a3sql.bin");
    let bytes = engine::serialize::export_binary(db);
    match std::fs::write(filename, bytes) {
        Ok(()) => ok_response(&format!("\"Saved to '{}'\"", filename)),
        Err(e) => error_response(ErrorCode::Io, &format!("Save failed: {}", e)),
    }
}

pub(crate) fn handle_load(db: &mut engine::Database, args: &[&str]) -> String {
    let filename = args.first().unwrap_or(&"a3sql.bin");
    match std::fs::read(filename) {
        Ok(bytes) => {
            db.clear();
            match engine::serialize::import_binary(&bytes, db) {
                Ok(()) => ok_response(&format!("\"Loaded from '{}'\"", filename)),
                Err(e) => error_response(ErrorCode::Exec, &format!("Load failed: {}", e)),
            }
        }
        Err(e) => error_response(ErrorCode::Io, &format!("Read failed: {}", e)),
    }
}

// ── Export to file ─────────────────────────────────────────────────────

/// Write table data or SQL dump directly to a file on disk.
pub(crate) fn handle_export_to_file(db: &engine::Database, trimmed: &str, args: &[&str]) -> String {
    let parts: Vec<&str> = trimmed.splitn(4, |c: char| c.is_whitespace()).collect();
    let format_str = parts.get(1).copied().unwrap_or("json");
    let has_table = parts.len() > 2 && !parts[2].is_empty();
    let table_name = if has_table { parts.get(2) } else { None };
    let cmd_path = if has_table { parts.get(3) } else { parts.get(2) };
    let file_path: String = match cmd_path.or_else(|| args.first()) {
        Some(p) => p.to_string(),
        None => {
            if format_str == "sql" {
                "a3sql_export.sql".into()
            } else if let Some(t) = table_name {
                format!("{}.{}", t, format_str)
            } else {
                "a3sql_export.txt".into()
            }
        }
    };

    let format: engine::serialize::Format = match format_str.parse() {
        Ok(f) => f,
        Err(e) => return error_response(ErrorCode::Exec, &e),
    };

    let data = match format {
        engine::serialize::Format::Sql => engine::serialize::export_sql(db),
        engine::serialize::Format::Binary => engine::serialize::hex_encode(&engine::serialize::export_binary(db)),
        _ => {
            let name = match table_name {
                Some(n) if !n.is_empty() => n,
                _ => return error_response(ErrorCode::Exec, "Table name required for json/csv export"),
            };
            let table = match db.get_table(name) {
                Ok(t) => t,
                Err(e) => return error_response(ErrorCode::Table, &e),
            };
            engine::serialize::export(format, table, db)
        }
    };

    let path_display = file_path.clone();
    if let Some(parent) = std::path::Path::new(&file_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&file_path, &data) {
        Ok(()) => ok_response(&format!("\"Exported to '{}'\"", path_display)),
        Err(e) => error_response(ErrorCode::Io, &format!("Write failed: {}", e)),
    }
}
