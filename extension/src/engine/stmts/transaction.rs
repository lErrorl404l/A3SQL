// Transaction and runtime config statements

//! Transaction and SET statement execution.
//! Handles BEGIN, COMMIT, ROLLBACK, SAVEPOINT, RELEASE SAVEPOINT.

use super::super::database::Database;
use crate::engine::error::EngineError;
use sqlparser::ast::Set;

/// Execute SET statement — store runtime config.
pub(crate) fn exec_set(set: &Set, db: &mut Database) -> Result<String, EngineError> {
    match set {
        Set::SingleAssignment { variable, values, .. } => {
            let key = variable.to_string().to_lowercase();
            let val = values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
            db.set_config(&key, &val);
            Ok(format!("\"SET {} = {}\"", variable, val))
        }
        Set::MultipleAssignments { assignments } => {
            for assign in assignments {
                let key = assign.name.to_string().to_lowercase();
                db.set_config(&key, &assign.value.to_string());
            }
            Ok("\"SET (multiple)\"".into())
        }
        _ => Ok("\"SET (stored)\"".into()),
    }
}
