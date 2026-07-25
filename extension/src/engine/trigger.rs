// Trigger types and execution

use crate::engine::database::Database;

/// Trigger definition stored on a Table.
#[derive(Debug, Clone)]
pub struct TriggerInfo {
    pub name: String,
    pub timing: String, // "BEFORE" | "AFTER"
    pub event: String,  // "INSERT" | "UPDATE" | "DELETE"
    pub body: String,
}

impl TriggerInfo {
    pub fn matches(&self, event: &str, timing: &str) -> bool {
        self.event == event && self.timing == timing
    }
}

/// Fire AFTER triggers for a given table and event.
pub fn fire_triggers(_table_name: &str, event: &str, db: &mut Database) {
    let names: Vec<String> = db.table_names().iter().map(|s| s.to_string()).collect();
    for tn in names {
        if let Ok(t) = db.get_table(&tn) {
            let triggers: Vec<TriggerInfo> = t
                .triggers
                .iter()
                .filter(|tr| tr.event == event && tr.timing == "AFTER")
                .cloned()
                .collect();
            drop(t);
            for tr in triggers {
                let body = tr.body.replace("OLD.", "").replace("NEW.", "");
                if let Err(e) = crate::engine::execute::parse_and_exec(&body, db) {
                    eprintln!("Trigger '{}' error: {}", tr.name, e);
                }
            }
        }
    }
}
