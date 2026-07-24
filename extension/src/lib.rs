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

// ponytail: external TCP listener — global lock on a single listener
static LISTENER: LazyLock<Mutex<Option<std::net::TcpListener>>> =
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
    if trimmed == "listen" {
        return handle_listen(args);
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
    let response = if results.is_empty() {
        ok_response("\"OK\"")
    } else if results.len() == 1 {
        ok_response(&results[0])
    } else {
        ok_response(&format!("[{}]", results.join(",")))
    };

    // Guard: Arma output buffer is ~10KB. If the response exceeds it, the
    // engine would silently truncate. Return an error instead so the caller
    // knows to use LIMIT/OFFSET for pagination.
    if response.len() > (OUTPUT_BUF_SIZE.saturating_sub(64)) as usize {
        return error_response(
            ErrorCode::Internal,
            "Result exceeds output buffer (10KB). Use LIMIT/OFFSET to paginate.",
        );
    }

    response
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

// ── External TCP connector ─────────────────────────────────────────────

/// Start a TCP server on 127.0.0.1:<port> that accepts SQL queries.
/// Each connection: read one line, execute via dispatch(), write result, close.
/// Allows external tools (scripts, dashboards) to query the in-game database
/// while the game is running.
fn handle_listen(args: &[&str]) -> String {
    let port: u16 = args.first().and_then(|s| s.parse().ok()).unwrap_or(33306);

    // If already listening, just return current status
    if LISTENER.lock().unwrap().is_some() {
        return ok_response(&format!("\"Already listening on 127.0.0.1:{}\"", port));
    }

    let addr = format!("127.0.0.1:{}", port);
    let listener = match std::net::TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => return error_response(ErrorCode::Io, &format!("Bind failed: {}", e)),
    };

    let addr_clone = addr.clone();
    *LISTENER.lock().unwrap() = Some(listener.try_clone().unwrap_or_else(|_| panic!("clone")));

    std::thread::spawn(move || {
        // ponytail: one-query-per-connection, no keep-alive or threading
        #[allow(clippy::significant_drop_in_scrutinee)]
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    use std::io::{BufRead, BufReader, Write};
                    let mut line = String::new();
                    let mut reader = BufReader::new(&stream);
                    if reader.read_line(&mut line).is_ok() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            let result = dispatch(trimmed, &[]);
                            let _ = writeln!(stream, "{}", result);
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    ok_response(&format!("\"Listening on {}\"", addr_clone))
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

    #[test]
    fn abi_full_lifecycle() {
        // Simulates SQF: "a3db" callExtension "CREATE TABLE ..."
        let r = dispatch(
            "CREATE TABLE abi_life (id STRING PRIMARY KEY, name STRING, qty INT)",
            &[],
        );
        assert!(r.contains("\"OK\""), "create: {}", r);

        let r = dispatch("INSERT INTO abi_life VALUES ('a1', 'widget', 10)", &[]);
        assert!(r.contains("\"OK\""), "insert: {}", r);
        let r = dispatch("INSERT INTO abi_life VALUES ('a2', 'gadget', 20)", &[]);
        assert!(r.contains("\"OK\""), "insert2: {}", r);

        let r = dispatch("SELECT * FROM abi_life", &[]);
        assert!(r.contains("widget"), "select: {}", r);
        assert!(r.contains("gadget"), "select: {}", r);

        let r = dispatch("SELECT name FROM abi_life WHERE qty = 10", &[]);
        assert!(r.contains("widget"), "where: {}", r);
        assert!(!r.contains("gadget"), "where neg: {}", r);

        let r = dispatch("UPDATE abi_life SET qty = 15 WHERE id = 'a1'", &[]);
        assert!(r.contains("\"OK\""), "update: {}", r);
        let r = dispatch("SELECT qty FROM abi_life WHERE id = 'a1'", &[]);
        assert!(r.contains("15"), "update verify: {}", r);

        let r = dispatch("DELETE FROM abi_life WHERE id = 'a2'", &[]);
        assert!(r.contains("\"OK\""), "delete: {}", r);
        let r = dispatch("SELECT COUNT(*) FROM abi_life", &[]);
        assert!(r.contains("1"), "count after delete: {}", r);
    }

    #[test]
    fn abi_fuzzy_match() {
        let r = dispatch(
            "CREATE TABLE abi_fuzzy (id STRING PRIMARY KEY, val STRING)",
            &[],
        );
        if !r.contains("\"OK\"") && !r.contains("already exists") {
            panic!("create: {}", r);
        }
        let r = dispatch("INSERT INTO abi_fuzzy VALUES ('f1_m4', 'rhs_m4a1')", &[]);
        assert!(r.contains("\"OK\""), "insert1: {}", r);
        let r = dispatch("INSERT INTO abi_fuzzy VALUES ('f2_m4', 'rhs_m4_gl')", &[]);
        assert!(r.contains("\"OK\""), "insert2: {}", r);
        dispatch(
            "INSERT INTO abi_fuzzy VALUES ('f3_other', 'other_thing')",
            &[],
        );

        // 'rhs_m4' has Jaccard ~0.5 with 'rhs_m4a1' and 'rhs_m4_gl' — above 0.3 threshold
        let r = dispatch("SELECT id FROM abi_fuzzy WHERE val %% 'rhs_m4'", &[]);
        assert!(r.contains("f1_m4"), "fuzzy match 1: {}", r);
        assert!(r.contains("f2_m4"), "fuzzy match 2: {}", r);
        assert!(!r.contains("f3_other"), "fuzzy no match: {}", r);

        // Fuzzy with CONCAT on LHS
        let r = dispatch(
            "SELECT * FROM abi_fuzzy WHERE CONCAT(val, 'X') %% 'hell'",
            &[],
        );
        assert!(r.contains("\"OK\""), "fuzzy concat: {}", r);
    }

    #[test]
    fn abi_transactions() {
        dispatch("CREATE TABLE abi_txn (k STRING PRIMARY KEY, v INT)", &[]);

        let r = dispatch("BEGIN", &[]);
        assert!(r.contains("\"OK\""));
        dispatch("INSERT INTO abi_txn VALUES ('a', 1)", &[]);
        let r = dispatch("ROLLBACK", &[]);
        assert!(r.contains("\"OK\""));
        let r = dispatch("SELECT COUNT(*) FROM abi_txn", &[]);
        assert!(r.contains("0"), "rollback: {}", r);

        dispatch("BEGIN", &[]);
        dispatch("INSERT INTO abi_txn VALUES ('a', 1)", &[]);
        let r = dispatch("COMMIT", &[]);
        assert!(r.contains("\"OK\""));
        let r = dispatch("SELECT COUNT(*) FROM abi_txn", &[]);
        assert!(r.contains("1"), "commit: {}", r);

        // Nested savepoint
        dispatch("BEGIN", &[]);
        dispatch("INSERT INTO abi_txn VALUES ('b', 2)", &[]);
        dispatch("SAVEPOINT sp1", &[]);
        dispatch("INSERT INTO abi_txn VALUES ('c', 3)", &[]);
        dispatch("ROLLBACK TO SAVEPOINT sp1", &[]);
        let r = dispatch("SELECT COUNT(*) FROM abi_txn", &[]);
        assert!(r.contains("2"), "savepoint: {}", r);
        dispatch("RELEASE SAVEPOINT sp1", &[]);
        dispatch("COMMIT", &[]);
        let r = dispatch("SELECT COUNT(*) FROM abi_txn", &[]);
        assert!(r.contains("2"), "final commit: {}", r);
    }

    #[test]
    fn abi_save_load() {
        let tmp = std::env::temp_dir().join("a3db_abi_save_test.bin");
        let path = tmp.to_string_lossy().to_string();

        dispatch("CREATE TABLE abi_sl (k STRING PRIMARY KEY, v INT)", &[]);
        dispatch("INSERT INTO abi_sl VALUES ('x', 42)", &[]);
        dispatch("INSERT INTO abi_sl VALUES ('y', 99)", &[]);

        let r = dispatch("save", &[&path]);
        assert!(r.contains("\"OK\""), "save: {}", r);

        // Verify the binary file has correct magic and content
        let saved = std::fs::read(&tmp).unwrap();
        assert_eq!(&saved[0..4], b"A3DB", "magic bytes");
        assert!(saved.len() > 100, "file size");

        // NOTE: LOAD is not tested via dispatch because it calls db.clear()
        // which would destroy the global DB shared by all parallel tests.
        // Binary round-trip is tested at the engine layer in serialize::tests.

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn abi_export_import_json_roundtrip() {
        // JSON export via dispatch("export json <table>", &[])
        dispatch(
            "CREATE TABLE abi_ej (k STRING PRIMARY KEY, val STRING)",
            &[],
        );
        dispatch("INSERT INTO abi_ej VALUES ('a', 'alpha')", &[]);
        dispatch("INSERT INTO abi_ej VALUES ('b', 'beta')", &[]);

        let r = dispatch("export json abi_ej", &[]);
        assert!(r.contains("\"OK\""), "export: {}", r);
        assert!(r.contains("alpha"), "export data: {}", r);

        // CSV export
        let r = dispatch("export csv abi_ej", &[]);
        assert!(r.contains("\"OK\""), "csv export: {}", r);
        assert!(r.contains("alpha"), "csv data: {}", r);

        // SQL dump
        let r = dispatch("dump_sql", &[]);
        assert!(r.contains("\"OK\""), "dump_sql: {}", r);
    }

    #[test]
    fn abi_multi_statement() {
        let r = dispatch(
            "CREATE TABLE abi_ms (id STRING PRIMARY KEY, val STRING); \
             INSERT INTO abi_ms VALUES ('a', 'first'); \
             INSERT INTO abi_ms VALUES ('b', 'second')",
            &[],
        );
        assert!(r.contains("\"OK\""), "multi create+insert: {}", r);

        let r = dispatch("SELECT * FROM abi_ms", &[]);
        assert!(r.contains("first"), "multi select: {}", r);
        assert!(r.contains("second"), "multi select: {}", r);
    }

    #[test]
    fn abi_errors() {
        let r = dispatch("SELECT * FROM abi_nonexistent", &[]);
        assert!(r.contains("ERR_EXEC"), "bad table: {}", r);

        let r = dispatch("NOT VALID SQL HERE", &[]);
        assert!(r.contains("ERR_PARSE"), "bad sql: {}", r);
    }

    #[test]
    fn abi_index_equality() {
        dispatch("CREATE TABLE abi_idx (k STRING PRIMARY KEY, v INT)", &[]);
        dispatch("CREATE INDEX abi_idx_v ON abi_idx (v)", &[]);

        for i in 0..10 {
            let sql = format!("INSERT INTO abi_idx VALUES ('k{}', {})", i, i * 10);
            dispatch(&sql, &[]);
        }

        // Equality lookup on indexed column — uses BTreeIndex
        let r = dispatch("SELECT k FROM abi_idx WHERE v = 30", &[]);
        assert!(r.contains("k3"), "btree lookup: {}", r);
        assert!(!r.contains("k0"), "btree no match: {}", r);
        assert!(!r.contains("k5"), "btree no match: {}", r);
    }

    #[test]
    fn abi_create_with_defaults() {
        // SHOW TABLES
        let r = dispatch("SHOW TABLES", &[]);
        assert!(r.contains("\"OK\""), "show tables: {}", r);
    }

    #[test]
    fn abi_order_by_limit() {
        dispatch("CREATE TABLE abi_ol (k STRING PRIMARY KEY, v INT)", &[]);
        for i in (0..5).rev() {
            let sql = format!("INSERT INTO abi_ol VALUES ('k{}', {})", i, i);
            dispatch(&sql, &[]);
        }

        let r = dispatch("SELECT k FROM abi_ol ORDER BY v ASC", &[]);
        assert!(r.contains("k0"), "order: {}", r);

        let r = dispatch("SELECT k FROM abi_ol ORDER BY v DESC LIMIT 2", &[]);
        assert!(r.contains("k4"), "desc limit: {}", r);
        assert!(r.contains("k3"), "desc limit: {}", r);
        assert!(!r.contains("k2"), "desc limit exceed: {}", r);
    }

    #[test]
    fn abi_aggregates() {
        dispatch(
            "CREATE TABLE abi_ag (k STRING PRIMARY KEY, grp STRING, v INT)",
            &[],
        );
        dispatch("INSERT INTO abi_ag VALUES ('a', 'x', 10)", &[]);
        dispatch("INSERT INTO abi_ag VALUES ('b', 'x', 20)", &[]);
        dispatch("INSERT INTO abi_ag VALUES ('c', 'y', 30)", &[]);

        let r = dispatch(
            "SELECT COUNT(*), SUM(v), AVG(v), MIN(v), MAX(v) FROM abi_ag",
            &[],
        );
        assert!(r.contains("3"), "count: {}", r);
        assert!(r.contains("60"), "sum: {}", r);
        assert!(r.contains("20"), "avg: {}", r);
        assert!(r.contains("10"), "min: {}", r);
        assert!(r.contains("30"), "max: {}", r);
    }

    #[test]
    fn abi_null_arithmetic() {
        dispatch("CREATE TABLE abi_null (k STRING PRIMARY KEY, v INT)", &[]);
        dispatch("INSERT INTO abi_null VALUES ('a', NULL)", &[]);
        dispatch("INSERT INTO abi_null VALUES ('b', 5)", &[]);

        let r = dispatch("SELECT * FROM abi_null WHERE v IS NULL", &[]);
        assert!(r.contains("a"), "null check: {}", r);
    }

    #[test]
    fn abi_insert_select_with_index_after_delete() {
        // Index maintenance after DELETE (bug regression)
        dispatch("CREATE TABLE abi_id (k STRING PRIMARY KEY, v INT)", &[]);
        dispatch("CREATE INDEX abi_id_v ON abi_id (v)", &[]);
        dispatch("INSERT INTO abi_id VALUES ('a', 10)", &[]);
        dispatch("INSERT INTO abi_id VALUES ('b', 20)", &[]);
        dispatch("DELETE FROM abi_id WHERE k = 'a'", &[]);
        dispatch("INSERT INTO abi_id VALUES ('c', 10)", &[]);
        dispatch("INSERT INTO abi_id VALUES ('d', 30)", &[]);

        // BTreeIndex on v should return exact results after delete+reinsert
        let r = dispatch("SELECT k FROM abi_id WHERE v = 10", &[]);
        assert!(r.contains("c"), "index after delete: {}", r);
        assert!(!r.contains("a"), "deleted row: {}", r);
    }

    #[test]
    fn abi_update_with_index() {
        // Index maintenance after UPDATE (bug regression)
        dispatch("CREATE TABLE abi_ui (k STRING PRIMARY KEY, v INT)", &[]);
        dispatch("CREATE INDEX abi_ui_v ON abi_ui (v)", &[]);
        dispatch("INSERT INTO abi_ui VALUES ('a', 10)", &[]);
        dispatch("INSERT INTO abi_ui VALUES ('b', 20)", &[]);
        dispatch("UPDATE abi_ui SET v = 50 WHERE k = 'a'", &[]);

        let r = dispatch("SELECT k FROM abi_ui WHERE v = 50", &[]);
        assert!(r.contains("a"), "index after update: {}", r);

        let r = dispatch("SELECT k FROM abi_ui WHERE v = 10", &[]);
        assert!(!r.contains("a"), "old value gone: {}", r);
    }

    #[test]
    fn abi_like_operator() {
        dispatch(
            "CREATE TABLE abi_lk (k STRING PRIMARY KEY, val STRING)",
            &[],
        );
        dispatch("INSERT INTO abi_lk VALUES ('a', 'hello')", &[]);
        dispatch("INSERT INTO abi_lk VALUES ('b', 'help')", &[]);
        dispatch("INSERT INTO abi_lk VALUES ('c', 'world')", &[]);

        let r = dispatch("SELECT k FROM abi_lk WHERE val LIKE 'hel%'", &[]);
        assert!(r.contains("a"), "like a: {}", r);
        assert!(r.contains("b"), "like b: {}", r);
        assert!(!r.contains("c"), "like no c: {}", r);
    }

    #[test]
    fn dispatch_large_result_truncation_guard() {
        // Insert enough rows to exceed the 10KB output buffer
        dispatch(
            "CREATE TABLE buf_test (id STRING PRIMARY KEY, data STRING)",
            &[],
        );
        let big_str = "x".repeat(500);
        for i in 0..25 {
            let sql = format!(
                "INSERT INTO buf_test VALUES ('k{i}', '{big_payload}')",
                i = i,
                big_payload = big_str
            );
            dispatch(&sql, &[]);
        }
        // SELECT all rows should trigger the overflow guard
        let result = dispatch("SELECT * FROM buf_test", &[]);
        assert!(
            result.contains("ERR_INTERNAL"),
            "expected overflow error, got: {}",
            result
        );
        assert!(
            result.contains("LIMIT/OFFSET"),
            "expected pagination hint, got: {}",
            result
        );
    }
}
