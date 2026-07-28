// a3sql — Command dispatch and SQL execution orchestration

//! Command dispatch — routes `callExtension` input to SQL execution or
//! custom commands (SAVE/LOAD/EXPORT/IMPORT/LISTEN/PLUGINS).

use crate::engine;
use crate::engine::error::{error_response, ok_response, A3sqlError, ErrorCode};
use crate::engine::execute;
use crate::ffi::{CREDENTIALS, DB, LISTENER, OUTPUT_BUF_SIZE, REMOTE};
use crate::parser::parse_sql;
use crate::server;

// ── Dispatch ────────────────────────────────────────────────────────────────

/// Main dispatch: SQL execution + custom commands.
///
/// Handles:
///   - Custom commands (version, export, import, save, load, dump_sql)
///   - Multi-statement SQL (statements separated by `;`)
///   - Each parsed statement executed against the engine
///
/// When the `auth` feature is enabled and configured, every query must carry a
/// `SIGNED <hex_sig> <payload>` prefix. Unsigned queries are rejected with
/// `ERR_AUTH`.
pub fn dispatch(input: &str, args: &[&str]) -> String {
    // Lazy init built-in plugins (also called on RVExtensionVersion for release)
    static PLUGIN_INIT: std::sync::Once = std::sync::Once::new();
    PLUGIN_INIT.call_once(|| {
        engine::plugin::init_builtin_plugins();
    });

    let trimmed = input.trim();

    // ── Auth verification (if enabled) ─────────────────────────────────
    // Returns the verified payload (without `SIGNED <sig>`) when auth
    // passes, or the original input when auth is disabled / not configured.
    let trimmed = match verify_auth(trimmed) {
        Ok(t) => t,
        Err(e) => return e,
    };

    // ── Custom commands (handled before SQL parsing) ──────────────────
    // Normalise to lowercase for case-insensitive command matching
    let lowered = trimmed.to_lowercase();
    if lowered == "ping" {
        return ok_response("\"PONG\"");
    }
    if lowered == "reset" {
        DB.lock().unwrap().clear();
        return ok_response("\"OK\"");
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

    if lowered.starts_with("live_patch") {
        let first_arg = args.first().copied().unwrap_or("");
        let mut db = DB.lock().unwrap();

        // ponytail: table creation is idempotent via IF NOT EXISTS
        let create_sql = "CREATE TABLE IF NOT EXISTS patch_rules (id INTEGER PRIMARY KEY, name TEXT NOT NULL, active INTEGER DEFAULT 1, priority INTEGER DEFAULT 0, match_type TEXT NOT NULL DEFAULT 'exact', match_value TEXT DEFAULT '', target_type TEXT NOT NULL, property TEXT NOT NULL, operator TEXT DEFAULT 'set', value TEXT NOT NULL, created_at TEXT DEFAULT '')";
        if let Err(e) = execute::parse_and_exec(create_sql, &mut db) {
            return error_response(ErrorCode::Exec, &e.to_string());
        }

        match first_arg {
            "list" => {
                let sql = "SELECT * FROM patch_rules ORDER BY priority";
                let stmts = match parse_sql(sql) {
                    Ok(s) => s,
                    Err(e) => return error_response(ErrorCode::Parse, &e.to_string()),
                };
                if stmts.is_empty() {
                    return error_response(ErrorCode::Exec, "no statements");
                }
                match execute::execute(&stmts[0], &mut db) {
                    Ok(result) => return ok_response(&result),
                    Err(e) => return error_response(ErrorCode::Exec, &e.to_string()),
                }
            }
            "query" => {
                let sql = args.get(1).copied().unwrap_or("");
                if sql.is_empty() {
                    return error_response(ErrorCode::Exec, "SQL required for query mode");
                }
                let stmts = match parse_sql(sql) {
                    Ok(s) => s,
                    Err(e) => return error_response(ErrorCode::Parse, &e.to_string()),
                };
                if stmts.is_empty() {
                    return error_response(ErrorCode::Exec, "no statements in query");
                }
                // Execute all statements, return last result
                let mut last = String::from("\"OK\"");
                for stmt in &stmts {
                    match execute::execute(stmt, &mut db) {
                        Ok(r) => last = r,
                        Err(e) => return error_response(ErrorCode::Exec, &e.to_string()),
                    }
                }
                return ok_response(&last);
            }
            _ => {
                let target_type = first_arg;
                let property = args.get(1).copied().unwrap_or("");
                let value = args.get(2).copied().unwrap_or("");

                if target_type.is_empty() {
                    return error_response(ErrorCode::Exec, "target_type is required");
                }
                if property.is_empty() {
                    return error_response(ErrorCode::Exec, "property is required");
                }
                if value.is_empty() {
                    return error_response(ErrorCode::Exec, "value is required");
                }

                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let name = format!("live_patch_{}", secs);
                let insert_sql = format!(
                    "INSERT INTO patch_rules (name, active, priority, match_type, match_value, target_type, property, operator, value, created_at) VALUES ('{}', 1, 0, 'exact', '', '{}', '{}', 'set', '{}', '{}')",
                    name.replace('\'', "''"),
                    target_type.replace('\'', "''"),
                    property.replace('\'', "''"),
                    value.replace('\'', "''"),
                    secs,
                );
                if let Err(e) = execute::parse_and_exec(&insert_sql, &mut db) {
                    return error_response(ErrorCode::Exec, &e.to_string());
                }

                let row_id = db.last_insert_rowid.as_deref().unwrap_or("unknown");
                return ok_response(&format!("\"Patch rule inserted with id {}\"", row_id));
            }
        }
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
        let parts: Vec<&str> = trimmed.splitn(4, |c: char| c.is_whitespace()).collect();
        let name = parts.get(1).unwrap_or(&"unknown");
        let argc: usize = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let body = parts.get(3).unwrap_or(&"");
        engine::plugin::register_sqf_function(name, argc, body);
        return ok_response(&format!("\"Function '{}' registered with {} args\"", name, argc));
    }
    if lowered.starts_with("plugin_dir ") {
        let dir = trimmed.trim_start_matches("plugin_dir ").trim();
        let loaded = engine::plugin::load_plugin_dir(dir);
        let json = serde_json::to_string(&loaded).unwrap_or_else(|_| "[]".into());
        return ok_response(&json);
    }

    // Handle DESCRIBE table / SHOW CREATE TABLE before SQL parsing (case-insensitive)
    if let Some(rest) = lowered.strip_prefix("describe ") {
        let name = rest.trim();
        if !name.is_empty() {
            let db = DB.lock().unwrap();
            return match engine::stmts::ddl::describe_table(&db, name) {
                Ok(data) => ok_response(&data),
                Err(e) => error_response(ErrorCode::Exec, &e.to_string()),
            };
        }
    }
    if let Some(rest) = lowered.strip_prefix("show create table ") {
        let name = rest.trim();
        if !name.is_empty() {
            let db = DB.lock().unwrap();
            return match engine::stmts::ddl::show_create_table(&db, name) {
                Ok(data) => ok_response(&data),
                Err(e) => error_response(ErrorCode::Exec, &e.to_string()),
            };
        }
    }

    // ── Query cursor — iterate large result sets ───────────────────────
    // cursor create <name> <sql>    — create a cursor over a query
    // cursor fetch <name> [limit]   — fetch next page of results
    // cursor drop <name>            — close a cursor
    if let Some(_rest) = lowered.strip_prefix("cursor create ") {
        let parts: Vec<&str> = trimmed[14..].trim().splitn(2, |c: char| c.is_whitespace()).collect();
        if parts.len() >= 2 {
            let cur_name = parts[0];
            let sql = parts[1..].join(" ").trim().to_string();
            let mut db = DB.lock().unwrap();
            db.create_cursor(cur_name, &sql, 100);
            return ok_response(&format!("\"Cursor '{}' created\"", cur_name));
        }
        return error_response(ErrorCode::Exec, "Usage: cursor create <name> <query>");
    }
    if let Some(_rest) = lowered.strip_prefix("cursor fetch ") {
        let parts: Vec<&str> = trimmed[13..].trim().splitn(2, |c: char| c.is_whitespace()).collect();
        let cur_name = parts[0];
        let limit: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
        let (state, db) = {
            let mut db = DB.lock().unwrap();
            let cursor = db.cursors.get(cur_name).cloned();
            match cursor {
                Some(mut c) => {
                    c.offset += limit;
                    db.cursors.insert(cur_name.to_string(), c.clone());
                    (c.clone(), db)
                }
                None => return error_response(ErrorCode::Exec, &format!("Cursor '{}' not found", cur_name)),
            }
        };
        // Execute the paginated query
        drop(state);
        let sql = format!(
            "{} LIMIT {} OFFSET {}",
            db.cursors.get(cur_name).map(|c| &c.sql).unwrap_or(&String::new()),
            limit,
            db.cursors.get(cur_name).map(|c| c.offset - limit).unwrap_or(0),
        );
        drop(db);
        let result = dispatch(&sql, &[]);
        return result;
    }
    if lowered.starts_with("cursor drop ") || lowered == "cursor drop" {
        let name = trimmed[12..].trim();
        let mut db = DB.lock().unwrap();
        match db.drop_cursor(name) {
            Ok(()) => ok_response(&format!("\"Cursor '{}' dropped\"", name)),
            Err(e) => error_response(ErrorCode::Exec, &e),
        };
        return ok_response("\"OK\"");
    }

    // ── Prepared statements ────────────────────────────────────────────
    // prepare <name> <sql>                          — store SQL template
    // execute_prepared <name> [arg1 arg2 ...]        — run stored SQL with args
    if let Some(rest) = lowered.strip_prefix("prepare ") {
        let parts: Vec<&str> = rest.trim().splitn(2, |c: char| c.is_whitespace()).collect::<Vec<_>>();
        if parts.len() >= 2 {
            let stmt_name = parts[0];
            let sql = parts[1..].join(" ");
            // Count $1 .. $n args
            let arg_count = sql
                .match_indices('$')
                .filter(|&(idx, _)| sql.as_bytes().get(idx + 1).copied().unwrap_or(0).is_ascii_digit())
                .count();
            let mut db = DB.lock().unwrap();
            db.prepare(stmt_name, &sql, arg_count);
            return ok_response(&format!("\"Prepared '{}'\"", stmt_name));
        }
        return error_response(ErrorCode::Exec, "Usage: prepare <name> <sql>");
    }
    if let Some(rest) = lowered.strip_prefix("execute_prepared ") {
        let parts: Vec<&str> = rest.trim().splitn(2, |c: char| c.is_whitespace()).collect::<Vec<_>>();
        if parts.is_empty() {
            return error_response(ErrorCode::Exec, "Usage: execute_prepared <name> [args...]");
        }
        let stmt_name = parts[0];
        let rest_args = parts.get(1).map(|s| s.trim()).unwrap_or("");
        let extra_args: Vec<&str> = if rest_args.is_empty() {
            vec![]
        } else {
            rest_args.split_whitespace().collect()
        };
        let all_args: Vec<&str> = args.iter().chain(extra_args.iter()).copied().collect();
        let sql;
        let prepared;
        {
            let db = DB.lock().unwrap();
            prepared = db.prepared.get(stmt_name).cloned();
        }
        match prepared {
            Some(stmt) => {
                sql = stmt.sql;
                // Use substitute_params to fill placeholders, then execute
                let filled = crate::dispatch::substitute_params(&sql, &all_args);
                return dispatch(&filled, &[]);
            }
            None => {
                return error_response(
                    ErrorCode::Exec,
                    &format!("Prepared statement '{}' not found", stmt_name),
                )
            }
        }
    }

    // ── Remote forwarding — if connected to a remote server, forward ──
    // Rewrite standalone REINDEX to VACUUM REINDEX (sqlparser only parses VACUUM REINDEX)
    if lowered == "reindex" || lowered.starts_with("reindex ") {
        let reindex_sql = if lowered == "reindex" {
            "VACUUM REINDEX".to_string()
        } else {
            format!("VACUUM REINDEX {}", &trimmed[8..])
        };
        let statements = split_sql(&reindex_sql);
        let result = exec_sql_statements(&statements, args);
        return result;
    }

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

// ── Auth verification ──────────────────────────────────────────────────

/// If the `auth` feature is enabled and configured, verify the Ed25519
/// signature prefix on the input. Returns the unsigned payload on success
/// or an error response string on failure.
///
/// When auth is disabled (feature off or `auth_required = false` in config),
/// returns the original input unchanged.
#[cfg(feature = "auth")]
fn verify_auth(input: &str) -> Result<&str, String> {
    use crate::engine::error::{error_response, ErrorCode};

    if !crate::config::CONFIG.auth_enabled() {
        return Ok(input);
    }
    let pubkey = crate::config::CONFIG
        .public_key_bytes()
        .ok_or_else(|| error_response(ErrorCode::Auth, "No public key configured in a3sql.toml"))?;
    let (sig_hex, payload) = crate::auth::parse_signed_input(input)
        .ok_or_else(|| error_response(ErrorCode::Auth, "Missing signature. Format: SIGNED <hex_sig> <query>"))?;
    if !crate::auth::verify_signature(&pubkey, payload, sig_hex) {
        return Err(error_response(ErrorCode::Auth, "Signature verification failed"));
    }
    Ok(payload)
}

/// No‑op when auth feature is disabled — passes everything through.
#[cfg(not(feature = "auth"))]
fn verify_auth(input: &str) -> Result<&str, String> {
    Ok(input)
}

/// Split SQL by semicolons, respecting string literals.
pub(crate) fn split_sql(sql: &str) -> Vec<String> {
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
pub(crate) fn substitute_params(sql: &str, args: &[&str]) -> String {
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
        // Handle CREATE TRIGGER manually (GenericDialect parser doesn't handle BEGIN...END)
        // Also handle trigger body split across statements by split_sql (; inside BEGIN...END)
        let lowered_sql = sql.trim().to_lowercase();
        if lowered_sql.starts_with("create trigger") || lowered_sql.starts_with("create or replace trigger") {
            // Collect all statements until we find END (trigger body spans multiple ;-split parts)
            let mut trigger_sql = sql.clone();
            // Check if this trigger body has been split by ; in the body
            if !trigger_sql.trim().to_lowercase().ends_with("end") {
                let mut remaining = statements
                    .iter()
                    .skip_while(|s| s.as_str() != sql.as_str())
                    .skip(1)
                    .cloned()
                    .collect::<Vec<String>>();
                // Find END in subsequent statements
                while let Some(next_stmt) = remaining.first().cloned() {
                    trigger_sql.push(';');
                    trigger_sql.push_str(&next_stmt);
                    if next_stmt.trim().to_lowercase().ends_with("end")
                        || next_stmt.trim().to_lowercase().ends_with("end;")
                    {
                        break;
                    }
                    remaining.remove(0);
                }
            }
            match handle_create_trigger(&trigger_sql, &mut db) {
                Ok(r) => results.push(r),
                Err(e) => {
                    let err = A3sqlError::new(ErrorCode::Exec, &e);
                    return err.to_response();
                }
            }
            // Skip remaining parts of the trigger body that were merged
            continue;
        }

        // Skip standalone END statements that were part of a trigger body
        let _trimmed_sql = sql.trim();
        if lowered_sql == "end" || lowered_sql == "end;" {
            continue;
        }

        match parse_sql(&sql) {
            Ok(stmts) => {
                for stmt in &stmts {
                    // Set COPY STDIN data from callExtension args before execution
                    if !args.is_empty() && matches!(stmt, sqlparser::ast::Statement::Copy { to: false, .. }) {
                        execute::COPY_STDIN.with(|s| {
                            *s.borrow_mut() = Some(args[0].to_string());
                        });
                    }
                    match execute::execute(stmt, &mut db) {
                        Ok(data) => results.push(data),
                        Err(e) => {
                            let err = A3sqlError::new(ErrorCode::Exec, e.to_string());
                            return err.to_response();
                        }
                    }
                }
            }
            Err(e) => {
                let err = A3sqlError::new(ErrorCode::Parse, format!("{}", e));
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

// ── Trigger handling ─────────────────────────────────────────────────────

/// Manually parse and execute CREATE TRIGGER (bypasses sqlparser which doesn't handle BEGIN...END).
fn handle_create_trigger(sql: &str, db: &mut crate::engine::database::Database) -> Result<String, String> {
    let s = sql.trim();
    let _lower = s.to_lowercase();
    // Parse: CREATE TRIGGER name AFTER|BEFORE event ON table [FOR EACH ROW] BEGIN body END
    let rest = s
        .strip_prefix("CREATE TRIGGER ")
        .or_else(|| s.strip_prefix("create trigger "))
        .ok_or_else(|| "CREATE TRIGGER syntax error".to_string())?;

    // Trigger name (first word)
    let (name, rest) = rest
        .split_once(|c: char| c.is_whitespace())
        .ok_or_else(|| "CREATE TRIGGER: expected trigger name".to_string())?;
    let name = name.trim().to_lowercase();
    let rest = rest.trim();

    // Timing: BEFORE or AFTER
    let (timing, rest) = if rest.to_lowercase().starts_with("before") {
        ("BEFORE", &rest["before".len()..])
    } else if rest.to_lowercase().starts_with("after") {
        ("AFTER", &rest["after".len()..])
    } else {
        return Err("CREATE TRIGGER: expected BEFORE or AFTER".to_string());
    };
    let rest = rest.trim();

    // Event: INSERT, UPDATE, DELETE (or OR-combination, take first)
    let event = if rest.to_lowercase().starts_with("insert") {
        &rest[.."insert".len()]
    } else if rest.to_lowercase().starts_with("update") {
        &rest[.."update".len()]
    } else if rest.to_lowercase().starts_with("delete") {
        &rest[.."delete".len()]
    } else {
        return Err("CREATE TRIGGER: expected INSERT, UPDATE, or DELETE".to_string());
    };
    let event = event.to_uppercase();
    let rest = rest[event.len()..].trim();

    // ON
    if !rest.to_lowercase().starts_with("on") {
        return Err("CREATE TRIGGER: expected ON".to_string());
    }
    let rest = rest["on".len()..].trim();

    // Table name (until FOR EACH ROW, BEGIN, or end)
    let table_end = rest
        .to_lowercase()
        .find(" for each row")
        .or_else(|| {
            rest.to_lowercase()
                .find(" for each")
                .or_else(|| rest.to_lowercase().find(" begin"))
        })
        .unwrap_or(rest.len());
    let table_name = rest[..table_end].trim().to_lowercase();
    let rest = rest[table_end..].trim();

    // Skip FOR EACH ROW if present
    let rest = if rest.to_lowercase().starts_with("for each row") {
        rest["for each row".len()..].trim()
    } else if rest.to_lowercase().starts_with("for each") {
        rest["for each".len()..].trim()
    } else {
        rest
    };

    // Check for WHEN condition (skip it - ponytail: not supported)
    let rest = if rest.to_lowercase().starts_with("when") {
        // Find BEGIN after WHEN
        let begin_pos = rest
            .to_lowercase()
            .find(" begin")
            .ok_or_else(|| "CREATE TRIGGER: expected BEGIN after WHEN".to_string())?;
        &rest[begin_pos..]
    } else {
        rest
    };

    // BEGIN ... END body — track nesting
    let body_start_idx = rest
        .to_lowercase()
        .find("begin")
        .ok_or_else(|| "CREATE TRIGGER: expected BEGIN".to_string())?;
    let body_start = &rest[body_start_idx + "begin".len()..];
    let lower_body = body_start.to_lowercase();
    let mut depth = 1i32;
    let mut end_idx = 0usize;
    let bytes = lower_body.as_bytes();
    let mut i = 0;
    while i < bytes.len() && depth > 0 {
        if bytes[i..].starts_with(b"begin")
            && bytes
                .get(i + 5)
                .is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'_')
        {
            depth += 1;
            i += 5;
        } else if bytes[i..].starts_with(b"end")
            && bytes
                .get(i + 3)
                .is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'_')
        {
            depth -= 1;
            if depth == 0 {
                end_idx = i;
                break;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    if depth != 0 {
        return Err("CREATE TRIGGER: expected END".to_string());
    }
    let body_sql = body_start[..end_idx].trim().trim_end_matches(';').trim();

    // Store the trigger
    let table = db.get_table_mut(&table_name)?;
    table.triggers.push(engine::trigger::TriggerInfo {
        name: name.clone(),
        timing: timing.to_string(),
        event: event.clone(),
        body: body_sql.to_string(),
    });

    Ok(format!("\"Trigger '{}' created on '{}'\"", name, table_name))
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

// ── TCP listener handlers ──────────────────────────────────────────────

fn handle_stop_listen() -> String {
    *LISTENER.lock().unwrap() = None;
    ok_response("\"Listener stopped\"")
}

fn handle_listen(args: &[&str]) -> String {
    // Stop any existing listener first
    *LISTENER.lock().unwrap() = None;

    let port: u16 = args.first().and_then(|s| s.parse().ok()).unwrap_or(33306);
    match server::start_server("127.0.0.1", port, None) {
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
