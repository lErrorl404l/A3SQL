// a3sql — Command dispatch and SQL execution orchestration

//! Command dispatch — routes `callExtension` input to SQL execution or
//! custom commands (SAVE/LOAD/EXPORT/IMPORT/LISTEN/PLUGINS).

mod commands;
mod sql;

#[allow(unused_imports)]
pub(crate) use sql::{split_sql, substitute_params};

use crate::engine;
use crate::engine::error::{error_response, ok_response, ErrorCode};
use crate::ffi::{CREDENTIALS, REMOTE};

// ── Dispatch ────────────────────────────────────────────────────────────────

/// Backward-compatible dispatch that acquires the global DB lock internally.
///
/// Prefer `dispatch_inner` for new code — it takes an explicit `&mut Database`
/// handle so callers control lock duration.
pub fn dispatch(input: &str, args: &[&str]) -> String {
    let mut db = crate::ffi::DB.lock().unwrap();
    dispatch_inner(&mut db, input, args)
}

/// Core dispatch: SQL execution + custom commands with explicit DB handle.
///
/// Handles:
///   - Custom commands (version, export, import, save, load, dump_sql)
///   - Multi-statement SQL (statements separated by `;`)
///   - Each parsed statement executed against the engine
///
/// When the `auth` feature is enabled and configured, every query must carry a
/// `SIGNED <hex_sig> <payload>` prefix. Unsigned queries are rejected with
/// `ERR_AUTH`.
pub(crate) fn dispatch_inner(db: &mut engine::Database, input: &str, args: &[&str]) -> String {
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
        db.clear();
        return ok_response("\"OK\"");
    }
    if lowered == "version" {
        return ok_response(concat!("\"a3sql ", env!("CARGO_PKG_VERSION"), "\""));
    }
    if lowered == "dump_sql" || lowered == "export_sql" {
        return commands::handle_dump_sql(db);
    }
    if lowered.starts_with("export_to_file") {
        return commands::handle_export_to_file(db, trimmed, args);
    }
    if lowered.starts_with("export") || lowered.starts_with("import") {
        let result = if lowered.starts_with("export") {
            commands::handle_export(db, trimmed, args)
        } else {
            commands::handle_import(db, trimmed, args)
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
        return commands::handle_save(db, &save_args);
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
        return commands::handle_load(db, &load_args);
    }
    if lowered == "stop_listen" || lowered == "stop" {
        return commands::handle_stop_listen();
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
        return commands::handle_listen(&listen_args);
    }

    if lowered.starts_with("live_patch") {
        return commands::handle_live_patch(db, trimmed, args);
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
            return match engine::stmts::ddl::describe_table(db, name) {
                Ok(data) => ok_response(&data),
                Err(e) => error_response(ErrorCode::Exec, &e.to_string()),
            };
        }
    }
    if let Some(rest) = lowered.strip_prefix("show create table ") {
        let name = rest.trim();
        if !name.is_empty() {
            return match engine::stmts::ddl::show_create_table(db, name) {
                Ok(data) => ok_response(&data),
                Err(e) => error_response(ErrorCode::Exec, &e.to_string()),
            };
        }
    }

    // ── Query cursor — iterate large result sets ───────────────────────
    if lowered.strip_prefix("cursor create ").is_some() {
        return commands::handle_cursor_create(db, trimmed);
    }
    if lowered.strip_prefix("cursor fetch ").is_some() {
        return commands::handle_cursor_fetch(db, trimmed);
    }
    if lowered.starts_with("cursor drop ") || lowered == "cursor drop" {
        return commands::handle_cursor_drop(db, trimmed);
    }

    // ── Prepared statements ────────────────────────────────────────────
    if let Some(rest) = lowered.strip_prefix("prepare ") {
        return commands::handle_prepare(db, rest);
    }
    if let Some(rest) = lowered.strip_prefix("execute_prepared ") {
        return commands::handle_execute_prepared(db, rest, args);
    }

    // ── Remote forwarding — if connected to a remote server, forward ──
    // Rewrite standalone REINDEX to VACUUM REINDEX (sqlparser only parses VACUUM REINDEX)
    if lowered == "reindex" || lowered.starts_with("reindex ") {
        let reindex_sql = if lowered == "reindex" {
            "VACUUM REINDEX".to_string()
        } else {
            format!("VACUUM REINDEX {}", &trimmed[8..])
        };
        let statements = sql::split_sql(&reindex_sql);
        let result = sql::exec_sql_statements(db, &statements, args);
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
    let response = sql::exec_sql_statements(db, &sql::split_sql(trimmed), args);

    // Guard: Arma output buffer is ~10KB. If the response exceeds it, the
    // engine would silently truncate. Return an error instead so the caller
    // knows to use LIMIT/OFFSET for pagination.
    if response.len() > (20480u32.saturating_sub(64)) as usize {
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
