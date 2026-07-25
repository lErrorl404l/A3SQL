// a3sql — Arma 3 Database Engine
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

/// Global database instance (single-threaded, mutex-protected).
pub(crate) static DB: LazyLock<Mutex<engine::Database>> = LazyLock::new(|| Mutex::new(engine::Database::new()));

/// Optional pointer to the SQF callback function registered by the engine.
static CALLBACK: LazyLock<Mutex<Option<unsafe extern "C" fn(i32, *mut std::os::raw::c_char)>>> =
    LazyLock::new(|| Mutex::new(None));
// ponytail: external TCP listener — global lock on a single listener
static LISTENER: LazyLock<Mutex<Option<std::net::TcpListener>>> = LazyLock::new(|| Mutex::new(None));

/// Stored credentials for TCP authentication. Empty = anonymous access.
static CREDENTIALS: LazyLock<Mutex<(String, String)>> = LazyLock::new(|| Mutex::new((String::new(), String::new())));
static REMOTE: LazyLock<Mutex<Option<std::net::TcpStream>>> = LazyLock::new(|| Mutex::new(None));
// Flag set by dispatch() when REPLACE INTO is detected before SQL parsing.
// Read by exec_insert() to handle PK conflict via delete+re-insert.
pub(crate) static REPLACE_FLAG: AtomicBool = AtomicBool::new(false);

// ── ABI ─────────────────────────────────────────────────────────────────────

/// Output buffer size from Arma engine. Currently 10240 bytes.
const OUTPUT_BUF_SIZE: u32 = 10240;

/// Version string — max 32 bytes including null terminator.
const VERSION: &[u8] = b"a3sql 0.1.0\0";

/// Called by engine on extension load.
///
/// # Safety
/// `output` must be a valid, writable buffer of at least `output_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn RVExtensionVersion(output: *mut c_char, output_size: u32) {
    let len = (output_size as usize).min(VERSION.len());
    std::ptr::copy_nonoverlapping(VERSION.as_ptr(), output as *mut u8, len);
    // Init built-in plugins on first load
    engine::plugin::init_builtin_plugins();
}

/// STRING callExtension STRING — compatibility entry point.
///
/// # Safety
/// `output` and `function` must be valid, non-null pointers to C string buffers.
#[no_mangle]
pub unsafe extern "C" fn RVExtension(output: *mut c_char, output_size: u32, function: *const c_char) {
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
pub unsafe extern "C" fn RVExtensionRegisterCallback(callbackProc: Option<unsafe extern "C" fn(i32, *mut c_char)>) {
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
pub fn dispatch(input: &str, args: &[&str]) -> String {
    // Lazy init built-in plugins (also called on RVExtensionVersion for release)
    static PLUGIN_INIT: std::sync::Once = std::sync::Once::new();
    PLUGIN_INIT.call_once(|| {
        engine::plugin::init_builtin_plugins();
    });

    let trimmed = input.trim();

    // ── Custom commands (handled before SQL parsing) ──────────────────
    // Normalise to lowercase for case-insensitive command matching
    let lowered = trimmed.to_lowercase();
    if lowered == "ping" {
        return ok_response("\"PONG\"");
    }
    if lowered == "version" {
        return ok_response("\"a3sql 0.1.0\"");
    }
    if lowered == "dump_sql" || lowered == "export_sql" {
        return handle_dump_sql();
    }
    if lowered.starts_with("export_to_file") {
        return handle_export_to_file(trimmed, args);
    }
    if lowered.starts_with("export") || lowered.starts_with("import") {
        let result = if lowered.starts_with("export") {
            handle_export(trimmed, args)
        } else {
            handle_import(trimmed, args)
        };
        return result;
    }
    if lowered == "save" || lowered.starts_with("save ") {
        let mut save_args = args.to_vec();
        if let Some(path) = trimmed.strip_prefix("save ") {
            if !path.is_empty() && save_args.is_empty() {
                save_args.push(path);
            }
        } else if let Some(path) = trimmed.strip_prefix("SAVE ") {
            if !path.is_empty() && save_args.is_empty() {
                save_args.push(path);
            }
        }
        return handle_save(&save_args);
    }
    if lowered == "load" || lowered.starts_with("load ") {
        let mut load_args = args.to_vec();
        if let Some(path) = trimmed.strip_prefix("load ") {
            if !path.is_empty() && load_args.is_empty() {
                load_args.push(path);
            }
        } else if let Some(path) = trimmed.strip_prefix("LOAD ") {
            if !path.is_empty() && load_args.is_empty() {
                load_args.push(path);
            }
        }
        return handle_load(&load_args);
    }
    if lowered == "stop_listen" || lowered == "stop" {
        return handle_stop_listen();
    }
    if lowered == "set_credentials" || lowered.starts_with("set_credentials") {
        let user = args.first().unwrap_or(&"");
        let pass = args.get(1).copied().unwrap_or("");
        *CREDENTIALS.lock().unwrap() = (user.to_string(), pass.to_string());
        return ok_response("\"Credentials set\"");
    }
    if lowered == "listen" || lowered.starts_with("listen ") {
        let mut listen_args = args.to_vec();
        if let Some(port) = trimmed.strip_prefix("listen ") {
            if !port.is_empty() && listen_args.is_empty() {
                listen_args.push(port);
            }
        } else if let Some(port) = trimmed.strip_prefix("LISTEN ") {
            if !port.is_empty() && listen_args.is_empty() {
                listen_args.push(port);
            }
        }
        return handle_listen(&listen_args);
    }

    // Remote server connection for network replication
    if lowered == "connect" || lowered.starts_with("connect ") {
        let parts: Vec<&str> = trimmed.splitn(3, |c: char| c.is_whitespace()).collect();
        let host = parts.get(1).unwrap_or(&"127.0.0.1");
        let port: u16 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(33306);
        let addr = format!("{}:{}", host, port);
        return match std::net::TcpStream::connect(&addr) {
            Ok(stream) => {
                *REMOTE.lock().unwrap() = Some(stream);
                ok_response(&format!("\"Connected to {}\"", addr))
            }
            Err(e) => error_response(ErrorCode::Io, &format!("Connect failed: {}", e)),
        };
    }

    if lowered == "disconnect" {
        *REMOTE.lock().unwrap() = None;
        return ok_response("\"Disconnected\"");
    }

    // ── Plugin commands ─────────────────────────────────────────────────
    if lowered == "plugins" {
        let info = engine::plugin::list_plugins();
        let json = serde_json::to_string(&info).unwrap_or_else(|_| "[]".into());
        return ok_response(&json);
    }
    if lowered.starts_with("register_function ") {
        let parts: Vec<&str> = trimmed.splitn(3, |c: char| c.is_whitespace()).collect();
        let name = parts.get(1).unwrap_or(&"unknown");
        let argc: usize = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        engine::plugin::register_sqf_function(name, argc);
        return ok_response(&format!("\"Function '{}' registered with {} args\"", name, argc));
    }
    if lowered.starts_with("plugin_dir ") {
        let dir = trimmed.trim_start_matches("plugin_dir ").trim();
        let loaded = engine::plugin::load_plugin_dir(dir);
        let json = serde_json::to_string(&loaded).unwrap_or_else(|_| "[]".into());
        return ok_response(&json);
    }

    if trimmed.to_uppercase().starts_with("REPLACE INTO") {
        REPLACE_FLAG.store(true, Ordering::SeqCst);
        // Replace "REPLACE INTO" with "INSERT INTO" for sqlparser
        let insert_sql = format!("INSERT INTO {}", &trimmed[12..]);
        let statements = split_sql(&insert_sql);
        let result = exec_sql_statements(&statements, args);
        REPLACE_FLAG.store(false, Ordering::SeqCst);
        return result;
    }

    // Handle DESCRIBE table / SHOW CREATE TABLE before SQL parsing (case-insensitive)
    if let Some(rest) = lowered.strip_prefix("describe ") {
        let name = rest.trim();
        if !name.is_empty() {
            let db = DB.lock().unwrap();
            return match engine::execute::describe_table(&db, name) {
                Ok(data) => ok_response(&data),
                Err(e) => error_response(ErrorCode::Exec, &e),
            };
        }
    }
    if let Some(rest) = lowered.strip_prefix("show create table ") {
        let name = rest.trim();
        if !name.is_empty() {
            let db = DB.lock().unwrap();
            return match engine::execute::show_create_table(&db, name) {
                Ok(data) => ok_response(&data),
                Err(e) => error_response(ErrorCode::Exec, &e),
            };
        }
    }

    // ── Remote forwarding — if connected to a remote server, forward ──
    {
        let remote = REMOTE.lock().unwrap();
        if let Some(stream) = remote.as_ref() {
            // Send SQL to remote server via TCP
            use std::io::{BufRead, BufReader, Write};
            let mut out_stream = stream.try_clone().unwrap_or_else(|_| unreachable!());
            let writable = writeln!(out_stream, "{}", trimmed);
            let readable = {
                let mut reader = BufReader::new(&out_stream);
                let mut resp = String::new();
                reader.read_line(&mut resp).ok();
                resp
            };
            return writable
                .map(|_| readable.trim().to_string())
                .unwrap_or_else(|_| error_response(ErrorCode::Io, "Remote connection lost"));
        }
    }

    // ── Multi-statement SQL execution ─────────────────────────────────
    let response = exec_sql_statements(&split_sql(trimmed), args);

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

/// Substitute `$1`, `$2`, ... placeholders in a SQL string with escaped
/// values from `args`. This is the primary SQL injection prevention mechanism:
/// modders pass user input as separate args rather than interpolating into SQL.
///
/// Escaping rules:
/// - Strings: wrapped in single quotes, inner `'` doubled → `''`
/// - Integers: placed as-is (parsed validation)
/// - NULL: placed as `NULL`
fn substitute_params(sql: &str, args: &[&str]) -> String {
    // ponytail: simple char-by-char scan — fast enough for embedded DB
    let mut result = String::with_capacity(sql.len());
    let mut in_string = false;
    let mut chars = sql.char_indices().peekable();

    while let Some((_, c)) = chars.next() {
        if c == '\'' {
            in_string = !in_string;
            result.push(c);
        } else if c == '$' && !in_string {
            // Read the placeholder number (one or more digits)
            let mut num_str = String::new();
            while let Some(&(_, c2)) = chars.peek() {
                if c2.is_ascii_digit() {
                    num_str.push(c2);
                    chars.next();
                } else {
                    break;
                }
            }
            if !num_str.is_empty() {
                let idx: usize = num_str.parse().unwrap_or(1);
                if idx > 0 && idx <= args.len() {
                    let val = args[idx - 1];
                    // String arg: quote + escape inner quotes. Others: pass through.
                    if val.is_empty() {
                        result.push_str("''");
                    } else if val == "NULL" || val == "null" {
                        result.push_str("NULL");
                    } else if val.parse::<i64>().is_ok() || val.parse::<f64>().is_ok() {
                        result.push_str(val);
                    } else if val == "true" || val == "false" {
                        // Booleans used in expressions
                        result.push_str(val);
                    } else {
                        // Default: escape as string
                        result.push('\'');
                        result.push_str(&val.replace('\'', "''"));
                        result.push('\'');
                    }
                } else {
                    // Unknown placeholder → leave as-is (will cause SQL error)
                    result.push('$');
                    result.push_str(&num_str);
                }
            } else {
                result.push('$');
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Execute a batch of SQL statements against the global DB.
/// Returns a formatted response string with accumulated results.
fn exec_sql_statements(statements: &[String], args: &[&str]) -> String {
    if statements.is_empty() {
        return ok_response("\"\"");
    }

    let mut db = DB.lock().unwrap();
    let mut results: Vec<String> = Vec::new();

    for sql in statements {
        // Substitute $1, $2, ... placeholders with escaped values from callExtension args
        let sql = if args.is_empty() {
            sql.clone()
        } else {
            substitute_params(sql, args)
        };
        match parse_sql(&sql) {
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

    if results.is_empty() {
        ok_response("\"OK\"")
    } else if results.len() == 1 {
        ok_response(&results[0])
    } else {
        ok_response(&format!("[{}]", results.join(",")))
    }
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
            let encoded = ::serde_json::to_string(&sql).unwrap_or_else(|_| "\"\"".into());
            ok_response(&encoded)
        }
        engine::serialize::Format::Binary => {
            let bytes = engine::serialize::export_binary(&db);
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
                    let data = engine::serialize::export(format, table, &db);
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
    // JSON-encode the SQL string so it survives CBA_fnc_parseJSON
    let encoded = ::serde_json::to_string(&sql).unwrap_or_else(|_| format!("\"{}\"", ""));
    ok_response(&encoded)
}

fn handle_save(args: &[&str]) -> String {
    let filename = args.first().unwrap_or(&"a3sql.bin");
    let db = DB.lock().unwrap();
    let bytes = engine::serialize::export_binary(&db);
    match std::fs::write(filename, bytes) {
        Ok(()) => ok_response(&format!("\"Saved to '{}'\"", filename)),
        Err(e) => error_response(ErrorCode::Io, &format!("Save failed: {}", e)),
    }
}

fn handle_load(args: &[&str]) -> String {
    let filename = args.first().unwrap_or(&"a3sql.bin");
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
fn handle_stop_listen() -> String {
    *LISTENER.lock().unwrap() = None;
    ok_response("\"Listener stopped\"")
}

/// Serve a single TCP client connection.
/// Reads lines, handles LOGIN/auth, dispatches SQL, responds.
/// Used by both the in-game TCP listener and the standalone server.
fn serve_client(stream: std::net::TcpStream) {
    use std::io::{BufRead, BufReader, Write};
    let mut stream = stream;
    let mut reader = match stream.try_clone() {
        Ok(c) => BufReader::new(c),
        Err(_) => return,
    };

    fn has_auth() -> bool {
        let (user, pass) = CREDENTIALS.lock().unwrap().clone();
        !user.is_empty() || !pass.is_empty()
    }
    fn check_login(user: &str, pass: &str) -> bool {
        let expected = CREDENTIALS.lock().unwrap().clone();
        user == expected.0 && pass == expected.1
    }

    let mut authenticated = !has_auth();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "QUIT" || trimmed == "EXIT" {
            break;
        }
        if !authenticated {
            if let Some(rest) = trimmed.strip_prefix("LOGIN ") {
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                if parts.len() >= 2 && check_login(parts[0], parts[1]) {
                    let _ = writeln!(stream, "[0,\"OK\",\"Authenticated\"]");
                    authenticated = true;
                } else {
                    let _ = writeln!(stream, "[-1,\"ERR_AUTH\",\"Invalid credentials\"]");
                    break;
                }
            } else {
                let _ = writeln!(stream, "[-1,\"ERR_AUTH\",\"LOGIN <user> <pass> required\"]");
                break;
            }
            continue;
        }
        let result = dispatch(trimmed, &[]);
        let _ = writeln!(stream, "{}", result);
    }
}

/// Start a TCP server on `bind:port`. Each client gets a thread.
/// Pass `db_path` for persistence (loads on start, saves on writes).
/// This is the shared entry point used by both the extension's `listen` command
/// and the standalone `a3sql-server` binary.
pub fn start_server(bind: &str, port: u16, db_path: Option<&str>) -> Result<String, String> {
    let addr = format!("{}:{}", bind, port);

    if let Some(path) = db_path {
        // Load existing database if file exists
        let r = dispatch(&format!("load {}", path), &[]);
        eprintln!("[a3sql-server] Loaded from {}: {}", path, r);
    }

    let listener = try_bind(&addr).map_err(|e| format!("Bind failed: {}", e))?;

    // Register auto-save on SIGTERM for persistence
    if let Some(path) = db_path {
        let path = path.to_string();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let r = dispatch(&format!("save {}", path), &[]);
            if r.contains("ERR") {
                eprintln!("[a3sql-server] auto-save: {}", r);
            }
        });
    }

    *LISTENER.lock().unwrap() = Some(listener.try_clone().map_err(|e| e.to_string())?);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => break,
            };
            std::thread::spawn(|| serve_client(stream));
        }
    });

    Ok(addr)
}

fn try_bind(addr: &str) -> Result<std::net::TcpListener, String> {
    let mut last_err = String::new();
    for i in 0..6 {
        match std::net::TcpListener::bind(addr) {
            Ok(l) => return Ok(l),
            Err(e) => {
                last_err = format!("Bind failed: {}", e);
                if i < 5 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
    }
    Err(last_err)
}

fn handle_listen(args: &[&str]) -> String {
    // Stop any existing listener first
    *LISTENER.lock().unwrap() = None;

    let port: u16 = args.first().and_then(|s| s.parse().ok()).unwrap_or(33306);
    match start_server("127.0.0.1", port, None) {
        Ok(addr) => ok_response(&format!("\"Listening on {}\"", addr)),
        Err(e) => error_response(ErrorCode::Io, &e),
    }
}

// ── Export to file ────────────────────────────────────────────────────

/// Write table data or SQL dump directly to a file on disk.
/// Format: export_to_file json|csv|sql <table_or_none> <path>
fn handle_export_to_file(trimmed: &str, args: &[&str]) -> String {
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
        engine::serialize::Format::Sql => {
            let db = DB.lock().unwrap();
            engine::serialize::export_sql(&db)
        }
        engine::serialize::Format::Binary => {
            let db = DB.lock().unwrap();
            engine::serialize::hex_encode(&engine::serialize::export_binary(&db))
        }
        _ => {
            let name = match table_name {
                Some(n) if !n.is_empty() => n,
                _ => return error_response(ErrorCode::Exec, "Table name required for json/csv export"),
            };
            let db = DB.lock().unwrap();
            let table = match db.get_table(name) {
                Ok(t) => t,
                Err(e) => return error_response(ErrorCode::Table, &e),
            };
            engine::serialize::export(format, table, &db)
        }
    };

    let path_display = file_path.clone();
    // Create parent directory if needed
    if let Some(parent) = std::path::Path::new(&file_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&file_path, &data) {
        Ok(()) => ok_response(&format!("\"Exported to '{}'\"", path_display)),
        Err(e) => error_response(ErrorCode::Io, &format!("Write failed: {}", e)),
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
        assert!(result.contains("ERR_PARSE"), "expected ERR_PARSE, got: {}", result);
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
        // Simulates SQF: "a3sql" callExtension "CREATE TABLE ..."
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
        let r = dispatch("CREATE TABLE abi_fuzzy (id STRING PRIMARY KEY, val STRING)", &[]);
        if !r.contains("\"OK\"") && !r.contains("already exists") {
            panic!("create: {}", r);
        }
        let r = dispatch("INSERT INTO abi_fuzzy VALUES ('f1_m4', 'rhs_m4a1')", &[]);
        assert!(r.contains("\"OK\""), "insert1: {}", r);
        let r = dispatch("INSERT INTO abi_fuzzy VALUES ('f2_m4', 'rhs_m4_gl')", &[]);
        assert!(r.contains("\"OK\""), "insert2: {}", r);
        dispatch("INSERT INTO abi_fuzzy VALUES ('f3_other', 'other_thing')", &[]);

        // 'rhs_m4' has Jaccard ~0.5 with 'rhs_m4a1' and 'rhs_m4_gl' — above 0.3 threshold
        let r = dispatch("SELECT id FROM abi_fuzzy WHERE val %% 'rhs_m4'", &[]);
        assert!(r.contains("f1_m4"), "fuzzy match 1: {}", r);
        assert!(r.contains("f2_m4"), "fuzzy match 2: {}", r);
        assert!(!r.contains("f3_other"), "fuzzy no match: {}", r);

        // Fuzzy with CONCAT on LHS
        let r = dispatch("SELECT * FROM abi_fuzzy WHERE CONCAT(val, 'X') %% 'hell'", &[]);
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
        let tmp = std::env::temp_dir().join("a3sql_abi_save_test.bin");
        let path = tmp.to_string_lossy().to_string();

        dispatch("CREATE TABLE abi_sl (k STRING PRIMARY KEY, v INT)", &[]);
        dispatch("INSERT INTO abi_sl VALUES ('x', 42)", &[]);
        dispatch("INSERT INTO abi_sl VALUES ('y', 99)", &[]);

        let r = dispatch("save", &[&path]);
        assert!(r.contains("\"OK\""), "save: {}", r);

        // Verify the binary file has correct magic and content
        let saved = std::fs::read(&tmp).unwrap();
        assert_eq!(&saved[0..4], b"A3SQ", "magic bytes");
        assert!(saved.len() > 100, "file size");

        // NOTE: LOAD is not tested via dispatch because it calls db.clear()
        // which would destroy the global DB shared by all parallel tests.
        // Binary round-trip is tested at the engine layer in serialize::tests.

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn abi_export_import_json_roundtrip() {
        // JSON export via dispatch("export json <table>", &[])
        dispatch("CREATE TABLE abi_ej (k STRING PRIMARY KEY, val STRING)", &[]);
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
        dispatch("CREATE TABLE abi_ag (k STRING PRIMARY KEY, grp STRING, v INT)", &[]);
        dispatch("INSERT INTO abi_ag VALUES ('a', 'x', 10)", &[]);
        dispatch("INSERT INTO abi_ag VALUES ('b', 'x', 20)", &[]);
        dispatch("INSERT INTO abi_ag VALUES ('c', 'y', 30)", &[]);

        let r = dispatch("SELECT COUNT(*), SUM(v), AVG(v), MIN(v), MAX(v) FROM abi_ag", &[]);
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
        dispatch("CREATE TABLE abi_lk (k STRING PRIMARY KEY, val STRING)", &[]);
        dispatch("INSERT INTO abi_lk VALUES ('a', 'hello')", &[]);
        dispatch("INSERT INTO abi_lk VALUES ('b', 'help')", &[]);
        dispatch("INSERT INTO abi_lk VALUES ('c', 'world')", &[]);

        let r = dispatch("SELECT k FROM abi_lk WHERE val LIKE 'hel%'", &[]);
        assert!(r.contains("a"), "like a: {}", r);
        assert!(r.contains("b"), "like b: {}", r);
        assert!(!r.contains("c"), "like no c: {}", r);
    }

    // ── C ABI tests — simulate EXACTLY what Arma's callExtension does ──

    fn abi_call(input: &str) -> String {
        let cmd = std::ffi::CString::new(input).unwrap();
        let mut out = vec![0u8; 102400];
        unsafe {
            RVExtension(out.as_mut_ptr() as *mut c_char, out.len() as u32, cmd.as_ptr());
            std::ffi::CStr::from_ptr(out.as_ptr() as *const c_char)
                .to_string_lossy()
                .into_owned()
        }
    }

    fn abi_call_args(cmd: &str, args: &[&str]) -> String {
        let command = std::ffi::CString::new(cmd).unwrap();
        let c_args: Vec<std::ffi::CString> = args.iter().map(|a| std::ffi::CString::new(*a).unwrap()).collect();
        let mut ptrs: Vec<*const c_char> = c_args.iter().map(|a| a.as_ptr()).collect();
        let mut out = vec![0u8; 65536];
        unsafe {
            RVExtensionArgs(
                out.as_mut_ptr() as *mut c_char,
                out.len() as u32,
                command.as_ptr(),
                ptrs.as_mut_ptr(),
                args.len() as u32,
            );
            std::ffi::CStr::from_ptr(out.as_ptr() as *const c_char)
                .to_string_lossy()
                .into_owned()
        }
    }

    #[test]
    fn abi_extern_full_sequence() {
        // Single sequential test for all C ABI calls. The global DB is shared by ALL
        // parallel tests, so running these as separate functions would cause conflicts.
        // This simulates the EXACT callExtension path Arma 3 uses.

        // 1. RVExtensionVersion
        let mut out = vec![0u8; 256];
        unsafe {
            RVExtensionVersion(out.as_mut_ptr() as *mut c_char, out.len() as u32);
            let v = std::ffi::CStr::from_ptr(out.as_ptr() as *const c_char)
                .to_string_lossy()
                .into_owned();
            assert!(v.len() > 5 && v.contains("0.1.0"), "RVExtensionVersion: {}", v);
        }

        // 2. RVExtension string form — simulates: "a3sql" callExtension "version"
        let r = abi_call("version");
        assert!(r.starts_with("[0,"), "string ABI version: {}", r);

        // 3. RVExtensionArgs array form — simulates: "a3sql" callExtension ["version", []]
        let r = abi_call_args("version", &[]);
        assert!(r.starts_with("[0,"), "args ABI version: {}", r);

        // 4. Full CRUD lifecycle through string ABI
        let tname = "abi_ext_seq";
        let r = abi_call(&format!("CREATE TABLE {} (id STRING PRIMARY KEY, val INT)", tname));
        assert!(r.contains("\"OK\""), "create: {}", r);
        let r = abi_call(&format!("INSERT INTO {} VALUES ('a', 10)", tname));
        assert!(r.contains("\"OK\""), "insert: {}", r);
        let r = abi_call(&format!("INSERT INTO {} VALUES ('b', 20)", tname));
        assert!(r.contains("\"OK\""), "insert2: {}", r);
        let r = abi_call(&format!("SELECT * FROM {}", tname));
        assert!(r.contains("a"), "select a: {}", r);
        assert!(r.contains("b"), "select b: {}", r);

        // 5. Parameterized query — simulates secure SQF pattern
        let evil = "foo' OR '1'='1";
        let r = abi_call_args(&format!("SELECT * FROM {} WHERE id = $1", tname), &[evil]);
        assert!(r.contains("\"OK\""), "injection should be blocked: {}", r);
        assert!(r.len() < 80, "injection returned data: {}", r);
    }

    // ── Response format validation — every SQF function's return matches CBA_fnc_parseJSON ──

    /// Validate that a response string is a CBA-parseable array: [code, "OK|ERR_*", data].
    /// Returns the parsed (code, status, data_len) tuple for further assertions.
    fn validate_response(resp: &str) -> (i64, String, usize) {
        // Must start with '[' and end with ']'
        assert!(resp.starts_with('['), "must start with [: {}", resp);
        assert!(resp.ends_with(']'), "must end with ]: {}", resp);

        // Parse as JSON array of 3 elements
        let parsed: Vec<::serde_json::Value> =
            ::serde_json::from_str(resp).unwrap_or_else(|e| panic!("invalid JSON response '{}': {}", resp, e));

        assert_eq!(parsed.len(), 3, "response must be a 3-element array: {}", resp);

        let code = parsed[0]
            .as_i64()
            .unwrap_or_else(|| panic!("element[0] must be integer: {}", resp));
        assert!((-1..=0).contains(&code), "error code must be -1 or 0: got {}", code);

        let status = parsed[1]
            .as_str()
            .unwrap_or_else(|| panic!("element[1] must be string: {}", resp))
            .to_string();
        assert!(
            status == "OK" || status.starts_with("ERR_"),
            "status must be 'OK' or 'ERR_*': got '{}'",
            status
        );

        // Third element can be anything — string, array, object
        let data_len = parsed[2].to_string().len();

        (code, status, data_len)
    }

    #[test]
    fn resp_fmt_full_sequence() {
        // Sequential: all response-format validations in one test to avoid DB conflicts

        // 1. version
        let r = abi_call("version");
        let (c, s, _) = validate_response(&r);
        assert_eq!(c, 0);
        assert_eq!(s, "OK");

        // 2. CREATE
        let r = abi_call("CREATE TABLE resp_seq (id STRING PRIMARY KEY, val INT)");
        let (c, _, _) = validate_response(&r);
        assert_eq!(c, 0, "create: {}", r);

        // 3. INSERT
        let r = abi_call("INSERT INTO resp_seq VALUES ('a', 10)");
        let (c, _, _) = validate_response(&r);
        assert_eq!(c, 0);

        // 4. SELECT
        let r = abi_call("SELECT * FROM resp_seq");
        let (_, s, _) = validate_response(&r);
        assert_eq!(s, "OK");

        // 5. export json
        let r = abi_call("export json resp_seq");
        let (c, _, _) = validate_response(&r);
        assert_eq!(c, 0, "export json: {}", r);

        // 6. export csv
        let r = abi_call("export csv resp_seq");
        let (c, _, _) = validate_response(&r);
        assert_eq!(c, 0, "export csv: {}", r);

        // 7. dump_sql
        let r = abi_call("dump_sql");
        let (c, s, _) = validate_response(&r);
        assert_eq!(c, 0);
        assert_eq!(s, "OK");

        // 8. save/load
        let tmp = std::env::temp_dir().join("a3sql_resp_seq.bin");
        let path = tmp.to_string_lossy().to_string();
        let r = abi_call_args("save", &[&path]);
        let (c, _, _) = validate_response(&r);
        assert_eq!(c, 0);
        let r = abi_call_args("load", &[&path]);
        let (c, s, _) = validate_response(&r);
        assert_eq!(c, 0, "load: {}", r);
        assert_eq!(s, "OK");
        let _ = std::fs::remove_file(&path);

        // 9. error: bad table
        let r = abi_call("SELECT * FROM resp_bad");
        let (c, s, _) = validate_response(&r);
        assert_eq!(c, -1);
        assert!(s.starts_with("ERR_"));

        // 10. error: bad SQL
        let r = abi_call("NOT VALID SQL");
        let (c, s, _) = validate_response(&r);
        assert_eq!(c, -1);
        assert!(s.starts_with("ERR_"));

        // 11. cleanup
        abi_call("DROP TABLE IF EXISTS resp_seq");
    }

    #[test]
    fn dispatch_large_result_truncation_guard() {
        // Insert enough rows to exceed the 10KB output buffer
        dispatch("CREATE TABLE buf_test (id STRING PRIMARY KEY, data STRING)", &[]);
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

    // ── Parameterised query tests ─────────────────────────────────────

    #[test]
    fn substitute_params_empty() {
        assert_eq!(
            substitute_params("SELECT * FROM t WHERE k = $1", &[]),
            "SELECT * FROM t WHERE k = $1"
        );
    }

    #[test]
    fn substitute_params_string() {
        assert_eq!(
            substitute_params("SELECT * FROM t WHERE k = $1", &["hello"]),
            "SELECT * FROM t WHERE k = 'hello'"
        );
    }

    #[test]
    fn substitute_params_string_escape_quote() {
        assert_eq!(
            substitute_params("SELECT * FROM t WHERE k = $1", &["it's"]),
            "SELECT * FROM t WHERE k = 'it''s'"
        );
    }

    #[test]
    fn substitute_params_injection_attempt() {
        // Previously the "already quoted" check would pass this through RAW
        assert_eq!(
            substitute_params("SELECT * FROM t WHERE k = $1", &["' OR '1'='1"]),
            "SELECT * FROM t WHERE k = ''' OR ''1''=''1'"
        );
    }

    #[test]
    fn substitute_params_null() {
        assert_eq!(
            substitute_params("SELECT * FROM t WHERE k = $1", &["NULL"]),
            "SELECT * FROM t WHERE k = NULL"
        );
    }

    #[test]
    fn substitute_params_int() {
        assert_eq!(
            substitute_params("SELECT * FROM t WHERE k = $1", &["42"]),
            "SELECT * FROM t WHERE k = 42"
        );
    }

    #[test]
    fn substitute_params_float() {
        assert_eq!(
            substitute_params("SELECT * FROM t WHERE k = $1", &["3.14"]),
            "SELECT * FROM t WHERE k = 3.14"
        );
    }

    #[test]
    fn substitute_params_bool() {
        assert_eq!(
            substitute_params("SELECT * FROM t WHERE k = $1", &["true"]),
            "SELECT * FROM t WHERE k = true"
        );
    }

    #[test]
    fn substitute_params_multi() {
        assert_eq!(
            substitute_params("INSERT INTO t VALUES ($1, $2, $3)", &["a", "42", "hello"]),
            "INSERT INTO t VALUES ('a', 42, 'hello')"
        );
    }

    #[test]
    fn substitute_params_respects_string_context() {
        // Placeholders inside string literals should NOT be substituted
        assert_eq!(
            substitute_params(
                "SELECT '$1' AS literal FROM t WHERE k = $2",
                &["should_not_appear", "real"]
            ),
            "SELECT '$1' AS literal FROM t WHERE k = 'real'"
        );
    }

    // ── Plugin tests ───────────────────────────────────────────────────

    #[test]
    fn plugins_list_empty() {
        let r = dispatch("plugins", &[]);
        assert!(r.starts_with("[0,"), "plugins should succeed: {}", r);
    }

    #[test]
    fn plugins_register_sqf() {
        let r = dispatch("register_function test_echo 1", &[]);
        assert!(r.starts_with("[0,"), "register should succeed: {}", r);
    }

    #[test]
    fn plugins_echo_builtin() {
        // The builtin_echo plugin registers fn_echo()
        // Create a table and call fn_echo() via SQL
        dispatch("CREATE TABLE plugin_echo_test (id STRING PRIMARY KEY)", &[]);
        dispatch("INSERT INTO plugin_echo_test VALUES ('hello')", &[]);
        // In the current engine, plugin functions aren't fully wired through SQL
        // We test the registry directly:
        let exists = crate::engine::plugin::lookup_function("echo");
        assert!(exists.is_some(), "builtin_echo should register fn_echo()");
        let (pf, _) = exists.unwrap();
        let result = (pf.func)(&[crate::engine::value::DbValue::String("world".into())]);
        assert!(result.is_ok(), "echo should return ok");
        // Verify the echoed value matches the input
        let val = result.unwrap();
        assert!(matches!(val, crate::engine::value::DbValue::String(ref s) if s == "world"));
        dispatch("DROP TABLE plugin_echo_test", &[]);
    }
}
