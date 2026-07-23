// a3db — Arma 3 Database Engine
// C ABI: RVExtension, RVExtensionArgs, RVExtensionVersion
//
// Build targets:
//   Linux:   x86_64-unknown-linux-gnu, i686-unknown-linux-gnu
//   Windows: x86_64-pc-windows-gnu,     i686-pc-windows-gnu
// Windows x86 (32-bit) needs a .def file or link args for decorated exports:
//   _RVExtensionVersion@8, _RVExtension@12, _RVExtensionArgs@20

#![allow(non_snake_case)]
// ponytail: unused items kept for phased implementation
#![allow(dead_code)]

mod engine;
mod parser;

use engine::error::{error_response, ok_response, A3dbError, ErrorCode};
use engine::execute as engine_execute;
use parser::parse_sql;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::{LazyLock, Mutex};

/// Global database instance (single-threaded, mutex-protected).
static DB: LazyLock<Mutex<engine::Database>> =
    LazyLock::new(|| Mutex::new(engine::Database::new()));

/// Optional pointer to the SQF callback function registered by the engine.
static CALLBACK: LazyLock<Mutex<Option<unsafe extern "C" fn(i32, *mut std::os::raw::c_char)>>> =
    LazyLock::new(|| Mutex::new(None));

// ── ABI ─────────────────────────────────────────────────────────────────────

/// Output buffer size from Arma engine. Currently 10240 bytes.
const OUTPUT_BUF_SIZE: u32 = 10240;

/// Version string — max 32 bytes including null terminator.
const VERSION: &[u8] = b"a3db 0.1.0\0";

/// Called by engine on extension load.
///
/// # Safety
/// `output` must be a valid, writable buffer of at least `output_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn RVExtensionVersion(output: *mut c_char, output_size: u32) {
    let len = (output_size as usize).min(VERSION.len());
    std::ptr::copy_nonoverlapping(VERSION.as_ptr(), output as *mut u8, len);
}

/// STRING callExtension STRING — compatibility entry point.
///
/// # Safety
/// `output` and `function` must be valid, non-null pointers to C string buffers.
#[no_mangle]
pub unsafe extern "C" fn RVExtension(
    output: *mut c_char,
    output_size: u32,
    function: *const c_char,
) {
    if output.is_null() || function.is_null() {
        return;
    }

    let input = match CStr::from_ptr(function).to_str() {
        Ok(s) => s,
        Err(_) => {
            write_output(output, output_size, "[-1,\"ERROR\",\"INVALID_UTF8\"]");
            return;
        }
    };

    let result = dispatch(input, &[]);
    write_output(output, output_size, &result);
}

/// STRING callExtension ARRAY — main entry point.
/// Returns 0 on success, -1 on error (extension return code).
///
/// # Safety
/// All pointer arguments must be valid, non-null pointers from the Arma engine.
#[no_mangle]
pub unsafe extern "C" fn RVExtensionArgs(
    output: *mut c_char,
    output_size: u32,
    function: *const c_char,
    argv: *const *const c_char,
    argc: u32,
) -> i32 {
    if output.is_null() || function.is_null() {
        return -1;
    }

    let input = match CStr::from_ptr(function).to_str() {
        Ok(s) => s,
        Err(_) => {
            write_output(output, output_size, "[-1,\"ERROR\",\"INVALID_UTF8\"]");
            return -1;
        }
    };

    let mut args: Vec<&str> = Vec::new();
    if !argv.is_null() {
        for i in 0..argc as isize {
            let ptr = *argv.offset(i);
            if !ptr.is_null() {
                if let Ok(s) = CStr::from_ptr(ptr).to_str() {
                    args.push(s);
                }
            }
        }
    }

    let result = dispatch(input, &args);
    write_output(output, output_size, &result);
    0
}

// ── Callback registration ──────────────────────────────────────────────────

/// Register a callback function that the extension can call back into SQF.
/// Arma calls this automatically when the extension exports the symbol.
///
/// # Safety
/// `callbackProc` must be a valid function pointer provided by the Arma engine.
#[no_mangle]
pub unsafe extern "C" fn RVExtensionRegisterCallback(
    callbackProc: Option<unsafe extern "C" fn(i32, *mut c_char)>,
) {
    let mut cb = CALLBACK.lock().unwrap();
    *cb = callbackProc;
}

// ── Dispatch ────────────────────────────────────────────────────────────────

/// Main dispatch: SQL execution + custom commands.
///
/// Handles:
///   - Custom commands (version, export, import, save, load, dump_sql)
///   - Multi-statement SQL (statements separated by `;`)
///   - Each parsed statement executed against the engine
fn dispatch(input: &str, args: &[&str]) -> String {
    let trimmed = input.trim();

    // ── Custom commands (handled before SQL parsing) ──────────────────
    if trimmed == "version" {
        return ok_response("\"a3db 0.1.0\"");
    }
    if trimmed == "dump_sql" || trimmed == "export_sql" {
        return handle_dump_sql();
    }
    if trimmed.starts_with("export") || trimmed.starts_with("import") {
        let result = if trimmed.starts_with("export") {
            handle_export(trimmed, args)
        } else {
            handle_import(trimmed, args)
        };
        return result;
    }
    if trimmed == "save" {
        return handle_save(args);
    }
    if trimmed == "load" {
        return handle_load(args);
    }

    // ── Multi-statement SQL execution ─────────────────────────────────
    // Split by semicolons, executing each non-empty statement in order.
    // Results from SELECT-like statements are accumulated.
    let statements = split_sql(trimmed);
    if statements.is_empty() {
        return ok_response("\"\"");
    }

    let mut db = DB.lock().unwrap();
    let mut results: Vec<String> = Vec::new();

    for sql in &statements {
        match parse_sql(sql) {
            Ok(stmts) => {
                for stmt in &stmts {
                    match engine_execute(stmt, &mut db) {
                        Ok(data) => results.push(data),
                        Err(e) => {
                            let err = A3dbError::new(ErrorCode::Exec, &e);
                            return err.to_response();
                        }
                    }
                }
            }
            Err(e) => {
                let err = A3dbError::new(ErrorCode::Parse, format!("{}", e));
                return err.to_response();
            }
        }
    }

    // Format accumulated results
    if results.is_empty() {
        ok_response("\"OK\"")
    } else if results.len() == 1 {
        ok_response(&results[0])
    } else {
        ok_response(&format!("[{}]", results.join(",")))
    }
}

/// Split SQL by semicolons, respecting string literals.
fn split_sql(sql: &str) -> Vec<String> {
    let mut stmts = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut prev = ' ';

    for c in sql.chars() {
        if c == '\'' && prev != '\\' {
            in_string = !in_string;
        }
        if c == ';' && !in_string {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                stmts.push(trimmed.to_string());
            }
            current.clear();
        } else {
            current.push(c);
        }
        prev = c;
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        stmts.push(trimmed.to_string());
    }

    stmts
}

// ── Import/Export handlers ──────────────────────────────────────────────

fn handle_export(input: &str, _args: &[&str]) -> String {
    let parts: Vec<&str> = input.splitn(3, |c: char| c.is_whitespace()).collect();
    let format_str = parts.get(1).unwrap_or(&"json");
    let table_name = parts.get(2).map(|s| s.trim());

    let format: engine::serialize::Format = match format_str.parse() {
        Ok(f) => f,
        Err(e) => return error_response(ErrorCode::Exec, &e),
    };

    let db = DB.lock().unwrap();

    match format {
        engine::serialize::Format::Sql => {
            let sql = engine::serialize::export_sql(&db);
            ok_response(&sql)
        }
        engine::serialize::Format::Binary => {
            let bytes = engine::serialize::export_binary(&db);
            let hex = engine::serialize::hex_encode(&bytes);
            ok_response(&format!("\"{}\"", hex))
        }
        _ => {
            let name = match table_name {
                Some(n) if !n.is_empty() => n,
                _ => return error_response(ErrorCode::Exec, "Usage: export <format> <table>"),
            };
            match db.get_table(name) {
                Ok(table) => {
                    let data = engine::serialize::export(format, table, &db);
                    ok_response(&data)
                }
                Err(e) => error_response(ErrorCode::Table, &e),
            }
        }
    }
}

fn handle_import(input: &str, args: &[&str]) -> String {
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

    let mut db = DB.lock().unwrap();
    match engine::serialize::import(format, table_name, data, &mut db) {
        Ok(()) => ok_response(&format!("\"Imported into '{}'\"", table_name)),
        Err(e) => error_response(ErrorCode::Exec, &e),
    }
}

fn handle_dump_sql() -> String {
    let db = DB.lock().unwrap();
    let sql = engine::serialize::export_sql(&db);
    ok_response(&format!("\"{}\"", sql.replace('"', "\"\"")))
}

fn handle_save(args: &[&str]) -> String {
    let filename = args.first().unwrap_or(&"a3db.bin");
    let db = DB.lock().unwrap();
    let bytes = engine::serialize::export_binary(&db);
    match std::fs::write(filename, bytes) {
        Ok(()) => ok_response(&format!("\"Saved to '{}'\"", filename)),
        Err(e) => error_response(ErrorCode::Io, &format!("Save failed: {}", e)),
    }
}

fn handle_load(args: &[&str]) -> String {
    let filename = args.first().unwrap_or(&"a3db.bin");
    match std::fs::read(filename) {
        Ok(bytes) => {
            let mut db = DB.lock().unwrap();
            db.clear();
            match engine::serialize::import_binary(&bytes, &mut db) {
                Ok(()) => ok_response(&format!("\"Loaded from '{}'\"", filename)),
                Err(e) => error_response(ErrorCode::Exec, &format!("Load failed: {}", e)),
            }
        }
        Err(e) => error_response(ErrorCode::Io, &format!("Read failed: {}", e)),
    }
}

fn write_output(output: *mut c_char, output_size: u32, s: &str) {
    let bytes = s.as_bytes();
    let len = (output_size as usize - 1).min(bytes.len());
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), output as *mut u8, len);
        *output.add(len) = 0;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_create_table() {
        // Use unique name since global DB is shared across tests
        let result = dispatch("CREATE TABLE dispatch_test_t (id STRING PRIMARY KEY)", &[]);
        assert!(result.contains("\"OK\""));
    }

    #[test]
    fn dispatch_select() {
        dispatch("CREATE TABLE dispatch_test_s (id STRING PRIMARY KEY)", &[]);
        dispatch("INSERT INTO dispatch_test_s VALUES ('x')", &[]);
        let result = dispatch("SELECT * FROM dispatch_test_s", &[]);
        assert!(result.contains("\"OK\""), "SELECT failed: {}", result);
    }

    #[test]
    fn dispatch_fuzzy() {
        dispatch("CREATE TABLE dispatch_test_f (id STRING PRIMARY KEY)", &[]);
        dispatch("INSERT INTO dispatch_test_f VALUES ('rhs_m4a1')", &[]);
        let result = dispatch("SELECT * FROM dispatch_test_f WHERE id %% 'rhs_m4'", &[]);
        assert!(result.contains("rhs_m4a1"), "fuzzy match: {}", result);
    }

    #[test]
    fn dispatch_bad_sql() {
        let result = dispatch("NOT VALID SQL $$$", &[]);
        assert!(
            result.contains("ERR_PARSE"),
            "expected ERR_PARSE, got: {}",
            result
        );
    }

    #[test]
    fn dispatch_empty() {
        let result = dispatch("", &[]);
        assert!(result.contains("\"OK\""), "expected OK, got: {}", result);
    }

    #[test]
    fn dispatch_multi_statement() {
        let result = dispatch(
            "CREATE TABLE ms_test (id STRING PRIMARY KEY); INSERT INTO ms_test VALUES ('a')",
            &[],
        );
        assert!(result.contains("\"OK\""), "multi-statement: {}", result);
    }

    #[test]
    fn dispatch_split_sql() {
        let stmts = split_sql("SELECT 1; SELECT 2;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT 1");
        assert_eq!(stmts[1], "SELECT 2");
    }

    #[test]
    fn dispatch_split_sql_with_string() {
        let stmts = split_sql("INSERT INTO t VALUES ('a;b'); SELECT 1");
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn dispatch_version() {
        let result = dispatch("version", &[]);
        assert!(result.contains("0.1.0"));
    }
}
