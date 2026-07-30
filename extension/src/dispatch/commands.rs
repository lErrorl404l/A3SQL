// a3sql — Custom command handlers (SAVE, LOAD, EXPORT, IMPORT, LISTEN, etc.)

use crate::engine;
use crate::engine::error::{error_response, ok_response, ErrorCode};
use crate::engine::execute;
use crate::ffi::LISTENER;
use crate::parser::parse_sql;
use crate::server;

mod io;
pub(super) use io::{handle_dump_sql, handle_export, handle_export_to_file, handle_import, handle_load, handle_save};

// ── Live patch ─────────────────────────────────────────────────────────────

/// Handle `live_patch` commands: list, query, or insert a patch rule.
pub(super) fn handle_live_patch(db: &mut engine::Database, trimmed: &str, args: &[&str]) -> String {
    // Parse args from trimmed input when args is empty (TCP mode)
    let lp_args: Vec<&str> = if args.is_empty() {
        let after_prefix = trimmed
            .strip_prefix("live_patch ")
            .or_else(|| trimmed.strip_prefix("LIVE_PATCH "))
            .unwrap_or("");
        if after_prefix.is_empty() {
            vec![""]
        } else {
            // Split by space for simple args; query mode handled separately
            after_prefix.split(' ').collect()
        }
    } else {
        args.to_vec()
    };
    let first_arg = lp_args.first().copied().unwrap_or("");

    // ponytail: table creation is idempotent via IF NOT EXISTS
    let create_sql = "CREATE TABLE IF NOT EXISTS patch_rules (id INTEGER AUTO_INCREMENT, name TEXT NOT NULL PRIMARY KEY, active INTEGER DEFAULT 1, priority INTEGER DEFAULT 0, match_type TEXT NOT NULL DEFAULT 'exact', match_value TEXT DEFAULT '', target_type TEXT NOT NULL, property TEXT NOT NULL, operator TEXT DEFAULT 'set', value TEXT NOT NULL, created_at TEXT DEFAULT '')";
    if let Err(e) = execute::parse_and_exec(create_sql, db) {
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
            match execute::execute(&stmts[0], db) {
                Ok(result) => ok_response(&result),
                Err(e) => error_response(ErrorCode::Exec, &e.to_string()),
            }
        }
        "query" => {
            // In TCP mode, reconstruct SQL from remaining raw input
            let sql = if args.is_empty() {
                trimmed
                    .strip_prefix("live_patch query ")
                    .or_else(|| trimmed.strip_prefix("LIVE_PATCH QUERY "))
                    .unwrap_or("")
            } else {
                lp_args.get(1).copied().unwrap_or("")
            };
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
                match execute::execute(stmt, db) {
                    Ok(r) => last = r,
                    Err(e) => return error_response(ErrorCode::Exec, &e.to_string()),
                }
            }
            ok_response(&last)
        }
        _ => {
            let target_type = first_arg;
            let property = lp_args.get(1).copied().unwrap_or("");
            let value = lp_args.get(2).copied().unwrap_or("");

            if target_type.is_empty() {
                return error_response(ErrorCode::Exec, "target_type is required");
            }
            if property.is_empty() {
                return error_response(ErrorCode::Exec, "property is required");
            }
            if value.is_empty() {
                return error_response(ErrorCode::Exec, "value is required");
            }

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            let secs = now.as_secs();
            let nanos = now.subsec_nanos();
            let name = format!("live_patch_{}_{:09}", secs, nanos);
            let insert_sql = format!(
                "INSERT INTO patch_rules (name, active, priority, match_type, match_value, target_type, property, operator, value, created_at) VALUES ('{}', 1, 0, 'exact', '', '{}', '{}', 'set', '{}', '{}')",
                name.replace('\'', "''"),
                target_type.replace('\'', "''"),
                property.replace('\'', "''"),
                value.replace('\'', "''"),
                secs,
            );
            if let Err(e) = execute::parse_and_exec(&insert_sql, db) {
                return error_response(ErrorCode::Exec, &e.to_string());
            }

            let row_id = db.last_insert_rowid.as_deref().unwrap_or("unknown");
            ok_response(&format!("\"Patch rule inserted with id {}\"", row_id))
        }
    }
}

// ── Cursor operations ─────────────────────────────────────────────────────

/// Create a cursor over a query for paginated iteration.
pub(super) fn handle_cursor_create(db: &mut engine::Database, trimmed: &str) -> String {
    let parts: Vec<&str> = trimmed[14..].trim().splitn(2, |c: char| c.is_whitespace()).collect();
    if parts.len() >= 2 {
        let cur_name = parts[0];
        let sql = parts[1..].join(" ").trim().to_string();
        db.create_cursor(cur_name, &sql, 100);
        return ok_response(&format!("\"Cursor '{}' created\"", cur_name));
    }
    error_response(ErrorCode::Exec, "Usage: cursor create <name> <query>")
}

/// Fetch the next page of results from a cursor.
pub(super) fn handle_cursor_fetch(db: &mut engine::Database, trimmed: &str) -> String {
    let parts: Vec<&str> = trimmed[13..].trim().splitn(2, |c: char| c.is_whitespace()).collect();
    let cur_name = parts[0];
    let limit: usize = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let cursor_info = {
        let cursor = db.cursors.get(cur_name).cloned();
        match cursor {
            Some(mut c) => {
                c.offset += limit;
                let sql = c.sql.clone();
                let offset = c.offset - limit;
                db.cursors.insert(cur_name.to_string(), c);
                (sql, offset)
            }
            None => return error_response(ErrorCode::Exec, &format!("Cursor '{}' not found", cur_name)),
        }
    };
    let sql = format!("{} LIMIT {} OFFSET {}", cursor_info.0, limit, cursor_info.1,);
    super::dispatch_inner(db, &sql, &[])
}

/// Drop (close) a named cursor.
pub(super) fn handle_cursor_drop(db: &mut engine::Database, trimmed: &str) -> String {
    let name = trimmed[12..].trim();
    match db.drop_cursor(name) {
        Ok(()) => ok_response(&format!("\"Cursor '{}' dropped\"", name)),
        Err(e) => error_response(ErrorCode::Exec, &e),
    }
}

// ── Prepared statements ───────────────────────────────────────────────────

/// Store a SQL template as a prepared statement with `$1..$N` placeholders.
/// `rest` is the lowercased text after `prepare ` (from `lowered.strip_prefix`),
/// matching the original dispatch behavior where SQL is stored lowercased.
pub(super) fn handle_prepare(db: &mut engine::Database, rest: &str) -> String {
    let parts: Vec<&str> = rest.trim().splitn(2, |c: char| c.is_whitespace()).collect::<Vec<_>>();
    if parts.len() >= 2 {
        let stmt_name = parts[0];
        let sql = parts[1..].join(" ");
        // Count $1 .. $n args
        let arg_count = sql
            .match_indices('$')
            .filter(|&(idx, _)| sql.as_bytes().get(idx + 1).copied().unwrap_or(0).is_ascii_digit())
            .count();
        db.prepare(stmt_name, &sql, arg_count);
        return ok_response(&format!("\"Prepared '{}'\"", stmt_name));
    }
    error_response(ErrorCode::Exec, "Usage: prepare <name> <sql>")
}

/// Execute a previously prepared statement with argument substitution.
/// `rest` is the lowercased text after `execute_prepared ` (from `lowered.strip_prefix`).
pub(super) fn handle_execute_prepared(db: &mut engine::Database, rest: &str, args: &[&str]) -> String {
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
    let prepared = db.prepared.get(stmt_name).cloned();
    match prepared {
        Some(stmt) => {
            let sql = stmt.sql;
            // Use substitute_params to fill placeholders, then execute
            let filled = crate::dispatch::substitute_params(&sql, &all_args);
            crate::dispatch::dispatch_inner(db, &filled, &[])
        }
        None => error_response(
            ErrorCode::Exec,
            &format!("Prepared statement '{}' not found", stmt_name),
        ),
    }
}

// ── TCP listener handlers ──────────────────────────────────────────────

pub(super) fn handle_stop_listen() -> String {
    *LISTENER.lock().unwrap() = None;
    ok_response("\"Listener stopped\"")
}

pub(super) fn handle_listen(args: &[&str]) -> String {
    // Stop any existing listener first
    *LISTENER.lock().unwrap() = None;

    let port: u16 = args.first().and_then(|s| s.parse().ok()).unwrap_or(33306);
    match server::start_server("127.0.0.1", port, None) {
        Ok(addr) => ok_response(&format!("\"Listening on {}\"", addr)),
        Err(e) => error_response(ErrorCode::Io, &e),
    }
}
