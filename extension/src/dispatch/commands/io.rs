use std::path::Path;

use crate::config::CONFIG;
use crate::engine;
use crate::engine::error::{error_response, ok_response, ErrorCode};

// ── Path validation ─────────────────────────────────────────────────────

/// Validate and resolve `filename` against the configured `data_dir`.
/// Rejects absolute paths and paths containing `..` or `~`.
/// Creates the data directory if it does not exist.
/// Returns an error response string on failure.
fn safe_data_path(filename: &str) -> Result<std::path::PathBuf, String> {
    // ponytail: global data_dir lock; per-account dirs if multi-tenant needed
    let p = Path::new(filename);
    // `has_root()` catches Windows root-relative paths like `/foo` or `\foo`
    // that `is_absolute()` misses (no drive prefix) — those still escape the
    // data dir when joined. On Unix the two are equivalent.
    if p.is_absolute() || p.has_root() {
        return Err(error_response(ErrorCode::Io, "Absolute paths are not allowed"));
    }
    for component in p.components() {
        let s = component.as_os_str().to_str().unwrap_or("");
        if s == ".." {
            return Err(error_response(ErrorCode::Io, "Path must not contain '..'"));
        }
        if s == "~" {
            return Err(error_response(ErrorCode::Io, "Path must not contain '~'"));
        }
    }
    let data_dir = CONFIG.data_dir().to_path_buf();
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| error_response(ErrorCode::Io, &format!("Cannot create data dir: {}", e)))?;
    }
    let resolved = data_dir.join(filename);
    // Reject extension of already-resolved path that tries to escape via
    // intermediate symlinks outside data_dir (canonicalize only when the
    // resolved path already exists; if it doesn't exist yet (writing), the
    // check is best-effort).
    if resolved.exists() {
        let canonical = resolved
            .canonicalize()
            .map_err(|_| error_response(ErrorCode::Io, "Cannot resolve path"))?;
        if !canonical.starts_with(data_dir.canonicalize().unwrap_or(data_dir.clone())) {
            return Err(error_response(ErrorCode::Io, "Path escapes data directory"));
        }
    }
    Ok(resolved)
}

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
    let filename = args.first().copied().unwrap_or("a3sql.bin");
    if filename.is_empty() {
        return error_response(ErrorCode::Io, "Filename must not be empty");
    }
    let path = match safe_data_path(filename) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let bytes = engine::serialize::export_binary(db);
    match std::fs::write(&path, bytes) {
        Ok(()) => ok_response(&format!("\"Saved to '{}/{}'\"", CONFIG.data_dir().display(), filename)),
        Err(e) => error_response(ErrorCode::Io, &format!("Save failed: {}", e)),
    }
}

pub(crate) fn handle_load(db: &mut engine::Database, args: &[&str]) -> String {
    let filename = args.first().copied().unwrap_or("a3sql.bin");
    if filename.is_empty() {
        return error_response(ErrorCode::Io, "Filename must not be empty");
    }
    let path = match safe_data_path(filename) {
        Ok(p) => p,
        Err(e) => return e,
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            db.clear();
            match engine::serialize::import_binary(&bytes, db) {
                Ok(()) => ok_response(&format!(
                    "\"Loaded from '{}/{}'\"",
                    CONFIG.data_dir().display(),
                    filename
                )),
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
    let filename: String = match cmd_path.or_else(|| args.first()) {
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

    if filename.is_empty() {
        return error_response(ErrorCode::Io, "Filename must not be empty");
    }

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

    let path = match safe_data_path(&filename) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let path_display = path.to_string_lossy().to_string();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, &data) {
        Ok(()) => ok_response(&format!("\"Exported to '{}'\"", path_display)),
        Err(e) => error_response(ErrorCode::Io, &format!("Write failed: {}", e)),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn err_msg(r: Result<std::path::PathBuf, String>) -> String {
        match r {
            Ok(_) => "OK".to_string(),
            Err(e) => e,
        }
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(err_msg(safe_data_path("/etc/passwd")).contains("Absolute paths"));
        #[cfg(windows)]
        assert!(err_msg(safe_data_path("C:\\windows\\system32\\evil.dll")).contains("Absolute paths"));
    }

    #[test]
    fn rejects_root_relative_paths() {
        // Windows treats `/foo` / `\foo` as root-relative (root, no drive
        // prefix) — `is_absolute()` alone misses these, so the data dir can
        // be escaped. `has_root()` catches them on both platforms.
        assert!(err_msg(safe_data_path("/tmp/escape.sql")).contains("Absolute paths"));
        #[cfg(windows)]
        assert!(err_msg(safe_data_path("\\tmp\\escape.sql")).contains("Absolute paths"));
    }

    #[test]
    fn rejects_parent_dir_and_tilde() {
        assert!(err_msg(safe_data_path("../evil.sql")).contains("'..'"));
        assert!(err_msg(safe_data_path("sub/../../evil.sql")).contains("'..'"));
        assert!(err_msg(safe_data_path("~/evil.sql")).contains("'~'"));
    }

    #[test]
    fn accepts_relative_paths() {
        let r = safe_data_path("a3sql.bin");
        assert!(r.is_ok(), "expected OK, got {}", err_msg(r));
        let r = safe_data_path("subdir/a3sql.bin");
        assert!(r.is_ok(), "expected OK, got {}", err_msg(r));
        // cleanup side effects from the accepted cases
        let _ = std::fs::remove_dir_all(CONFIG.data_dir());
    }
}
