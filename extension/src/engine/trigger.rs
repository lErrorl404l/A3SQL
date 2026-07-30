// Trigger types and execution

//! Trigger execution — fires BEFORE/AFTER INSERT/UPDATE/DELETE triggers.
//! Each trigger is a SQL body executed against the current row context.
//!
//! # Recursion guard
//! A thread-local depth counter prevents infinite trigger recursion.
//! When a trigger body performs INSERT/UPDATE/DELETE, those operations
//! would re-enter `fire_triggers`. The counter skips execution above
//! `MAX_DEPTH`, preventing stack overflow.

use std::cell::Cell;

use super::database::Database;
use crate::engine::value::DbValue;

/// Maximum trigger recursion depth before execution is silently skipped.
const MAX_DEPTH: u32 = 16;

thread_local! {
    static TRIGGER_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Trigger definition stored on a Table.
#[derive(Debug, Clone)]
pub(crate) struct TriggerInfo {
    pub name: String,
    pub timing: String, // "BEFORE" | "AFTER"
    pub event: String,  // "INSERT" | "UPDATE" | "DELETE"
    pub body: String,
}

impl TriggerInfo {
    #[allow(dead_code, reason = "trigger matching not yet wired into executor")]
    pub fn matches(&self, event: &str, timing: &str) -> bool {
        self.event == event && self.timing == timing
    }
}

/// Format a DbValue as a SQL literal (type-correct) for trigger body substitution.
fn sv(v: &DbValue) -> String {
    match v {
        DbValue::Null => "NULL".into(),
        DbValue::Bool(b) => format!("{}", if *b { 1 } else { 0 }),
        DbValue::Int(n) => format!("{}", n),
        DbValue::Float(f) => format!("{}", f),
        DbValue::String(s) => format!("'{}'", s),
        DbValue::Strings(v) => serde_json::to_string(v).unwrap_or_default(),
        DbValue::Floats(v) => serde_json::to_string(v).unwrap_or_default(),
    }
}

/// Fire AFTER triggers for a given table and event.
///
/// `new_row` / `old_row` provide the affected row for NEW.col / OLD.col substitution.
/// Pass `&[]` if no row context is available (the prefixes will be stripped silently).
///
/// Returns immediately (no-op) if the trigger recursion limit has been reached.
/// This prevents infinite loops when a trigger body performs INSERT/UPDATE/DELETE
/// that would fire other triggers.
pub(crate) fn fire_triggers(
    table_name: &str,
    event: &str,
    db: &mut Database,
    new_row: &[DbValue],
    old_row: &[DbValue],
) {
    // Guard: skip if we've recursed too deep
    let prev = TRIGGER_DEPTH.with(|d| {
        let depth = d.get();
        if depth >= MAX_DEPTH {
            return None;
        }
        d.set(depth + 1);
        Some(depth)
    });
    let prev = match prev {
        Some(p) => p,
        None => return,
    };

    // Fire triggers only on the named table — not all tables.
    // This prevents a trigger on table A from firing when a different trigger's
    // body modifies table B (which would cause cross-table re-entrancy).
    if let Ok(t) = db.get_table(table_name) {
        let triggers: Vec<TriggerInfo> = t
            .triggers
            .iter()
            .filter(|tr| tr.event == event && tr.timing == "AFTER")
            .cloned()
            .collect();
        let _ = t;
        let cols = t.columns.clone();
        for tr in triggers {
            let mut body = tr.body.clone();
            for (ci, col) in cols.iter().enumerate() {
                let col_name = col.name.clone();
                if ci < new_row.len() {
                    let new_val = sv(&new_row[ci]);
                    for prefix in ["NEW.", "new.", "New."] {
                        body = body.replace(&format!("{}{}", prefix, col_name), &new_val);
                    }
                }
                if ci < old_row.len() {
                    let old_val = sv(&old_row[ci]);
                    for prefix in ["OLD.", "old.", "Old."] {
                        body = body.replace(&format!("{}{}", prefix, col_name), &old_val);
                    }
                }
            }
            // Fallback: strip remaining OLD./NEW. prefixes for bare column names
            body = body
                .replace("OLD.", "")
                .replace("old.", "")
                .replace("NEW.", "")
                .replace("new.", "");
            if let Err(e) = crate::engine::execute::parse_and_exec(&body, db) {
                eprintln!("Trigger '{}' error: {}", tr.name, e);
            }
        }
    }

    TRIGGER_DEPTH.with(|d| d.set(prev));
}

/// Fire BEFORE triggers for a given table and event.
///
/// `new_row` / `old_row` provide the affected row for NEW.col / OLD.col substitution.
/// Pass `&[]` if no row context is available.
///
/// Returns an error if any BEFORE trigger fails (e.g., via RAISE(ABORT, ...)).
/// Unlike `fire_triggers` (AFTER), BEFORE triggers can abort the operation.
pub(crate) fn fire_triggers_before(
    table_name: &str,
    event: &str,
    db: &mut Database,
    new_row: &[DbValue],
    old_row: &[DbValue],
) -> Result<(), String> {
    let prev = TRIGGER_DEPTH.with(|d| {
        let depth = d.get();
        if depth >= MAX_DEPTH {
            return None;
        }
        d.set(depth + 1);
        Some(depth)
    });
    let prev = match prev {
        Some(p) => p,
        None => return Ok(()),
    };

    if let Ok(t) = db.get_table(table_name) {
        let triggers: Vec<TriggerInfo> = t
            .triggers
            .iter()
            .filter(|tr| tr.event == event && tr.timing == "BEFORE")
            .cloned()
            .collect();
        let cols = t.columns.clone();
        for tr in triggers {
            let mut body = tr.body.clone();
            for (ci, col) in cols.iter().enumerate() {
                let col_name = col.name.clone();
                if ci < new_row.len() {
                    let new_val = sv(&new_row[ci]);
                    for prefix in ["NEW.", "new.", "New."] {
                        body = body.replace(&format!("{}{}", prefix, col_name), &new_val);
                    }
                }
                if ci < old_row.len() {
                    let old_val = sv(&old_row[ci]);
                    for prefix in ["OLD.", "old.", "Old."] {
                        body = body.replace(&format!("{}{}", prefix, col_name), &old_val);
                    }
                }
            }
            body = body
                .replace("OLD.", "")
                .replace("old.", "")
                .replace("NEW.", "")
                .replace("new.", "");
            crate::engine::functions::builtin::RAISE_ABORTED.with(|f| f.set(false));
            if let Err(e) = crate::engine::execute::parse_and_exec(&body, db) {
                TRIGGER_DEPTH.with(|d| d.set(prev));
                return Err(format!("BEFORE trigger '{}' error: {}", tr.name, e));
            }
            if crate::engine::functions::builtin::RAISE_ABORTED.with(|f| f.replace(false)) {
                TRIGGER_DEPTH.with(|d| d.set(prev));
                return Err(format!("BEFORE trigger '{}' aborted", tr.name));
            }
        }
    }

    TRIGGER_DEPTH.with(|d| d.set(prev));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::database::Database;
    use crate::engine::execute::parse_and_exec;
    use crate::engine::table::Table;
    use crate::engine::value::{Column, ColumnType};

    /// Build a Database with:
    ///   a (id STRING PRIMARY KEY)
    ///   log (msg STRING)
    ///   trigger t AFTER INSERT ON a → INSERT INTO log VALUES ('x')
    fn make_trigger_test_db() -> Database {
        let mut db = Database::new();
        let cols_a = vec![Column {
            name: "id".into(),
            dtype: ColumnType::String,
            primary_key: true,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        db.create_table("a", Table::new("a".into(), cols_a).unwrap()).unwrap();

        let cols_log = vec![Column {
            name: "msg".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        db.create_table("log", Table::new("log".into(), cols_log).unwrap())
            .unwrap();

        // Manually add the trigger since CREATE TRIGGER parsing may be fragile
        let t = db.get_table_mut("a").unwrap();
        t.triggers.push(TriggerInfo {
            name: "t".into(),
            timing: "AFTER".into(),
            event: "INSERT".into(),
            body: "INSERT INTO log VALUES ('x')".into(),
        });
        let _ = t;
        db
    }

    #[test]
    fn trigger_fires_once_simple() {
        let mut db = make_trigger_test_db();
        fire_triggers("a", "INSERT", &mut db, &[], &[]);
        let log = db.get_table("log").unwrap();
        assert_eq!(log.row_count(), 1, "trigger should have inserted one row");
        // DbValue::to_string() wraps strings in quotes, so check via debug or raw access
        assert_eq!(format!("{:?}", log.rows[0][0]), "String(\"x\")");
    }

    #[test]
    fn trigger_recursion_guard_does_not_stack_overflow() {
        let mut db = make_trigger_test_db();
        // Insert into 'a' — the trigger inserts into 'log', which has no trigger.
        // So this is a shallow depth=1 test that the basic path works.
        parse_and_exec("INSERT INTO a VALUES ('1')", &mut db).unwrap();
        let log = db.get_table("log").unwrap();
        assert_eq!(log.row_count(), 1);
        let a = db.get_table("a").unwrap();
        assert_eq!(a.row_count(), 1);
    }

    #[test]
    fn trigger_recursion_deep_is_capped() {
        // Chain: insert into a → trigger t inserts into log → another trigger on log → ...
        // We test that the depth counter prevents infinite recursion by adding a
        // self-referencing trigger (insert into the same table the trigger is on).
        let mut db = Database::new();
        let cols = vec![Column {
            name: "id".into(),
            dtype: ColumnType::String,
            primary_key: true,
            not_null: false,
            default: None,
            auto_increment: false,
        }];
        db.create_table("self", Table::new("self".into(), cols).unwrap())
            .unwrap();
        let t = db.get_table_mut("self").unwrap();
        // Trigger on 'self' that inserts into 'self' — would be infinite without guard
        t.triggers.push(TriggerInfo {
            name: "loop".into(),
            timing: "AFTER".into(),
            event: "INSERT".into(),
            body: "INSERT INTO self VALUES ('recursed')".into(),
        });
        let _ = t;

        // This should NOT stack overflow
        parse_and_exec("INSERT INTO self VALUES ('first')", &mut db).unwrap();

        // The depth limit maxes at 16, so we should see at most ~16 more rows
        let self_t = db.get_table("self").unwrap();
        assert!(self_t.row_count() >= 1, "at least the original row");
        assert!(
            self_t.row_count() <= MAX_DEPTH as usize + 1,
            "should be capped at MAX_DEPTH+1, got {}",
            self_t.row_count()
        );
    }
}
