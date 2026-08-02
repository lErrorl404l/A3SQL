// a3sql — SQL helpers: splitting, substitution, execution

use crate::engine;
use crate::engine::error::{A3sqlError, ErrorCode, ok_response};
use crate::engine::execute;

// ── SQL splitting ──────────────────────────────────────────────────────────

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

// ── Parameter substitution ─────────────────────────────────────────────────

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

// ── Batch execution ────────────────────────────────────────────────────────

/// Execute a batch of SQL statements against the global DB.
/// Returns a formatted response string with accumulated results.
pub(super) fn exec_sql_statements(db: &mut engine::Database, statements: &[String], args: &[&str]) -> String {
    if statements.is_empty() {
        return ok_response("\"\"");
    }
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
            match handle_create_trigger(&trigger_sql, db) {
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

        match db.cached_parse(&sql) {
            Ok(stmts) => {
                for stmt in stmts.iter() {
                    // Set COPY STDIN data from callExtension args before execution
                    if !args.is_empty() && matches!(stmt, sqlparser::ast::Statement::Copy { to: false, .. }) {
                        let data = &args[0];
                        if data.len() > 1024 * 1024 {
                            let err = A3sqlError::new(ErrorCode::Io, "COPY FROM stdin data exceeds 1MB limit");
                            return err.to_response();
                        }
                        if data.trim().is_empty() {
                            let err = A3sqlError::new(ErrorCode::Io, "COPY FROM stdin: empty data");
                            return err.to_response();
                        }
                        execute::COPY_STDIN.with(|s| {
                            *s.borrow_mut() = Some(data.to_string());
                        });
                    }
                    match execute::execute(stmt, db) {
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

// ── Trigger handling ───────────────────────────────────────────────────────

/// Manually parse and execute CREATE TRIGGER (bypasses sqlparser which doesn't handle BEGIN...END).
fn handle_create_trigger(sql: &str, db: &mut crate::engine::database::Database) -> Result<String, String> {
    let s = sql.trim();
    let _lower = s.to_lowercase();
    // Parse: CREATE TRIGGER name AFTER|BEFORE event ON table [FOR EACH ROW] BEGIN body END
    let rest = s
        .strip_prefix("CREATE TRIGGER ")
        .or_else(|| s.strip_prefix("create trigger "))
        .or_else(|| s.strip_prefix("CREATE OR REPLACE TRIGGER "))
        .or_else(|| s.strip_prefix("create or replace trigger "))
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
    table.triggers.push(crate::engine::trigger::TriggerInfo {
        name: name.clone(),
        timing: timing.to_string(),
        event: event.clone(),
        body: body_sql.to_string(),
    });

    Ok(format!("\"Trigger '{}' created on '{}'\"", name, table_name))
}
