//! Prepared statement support — stored SQL templates with parameter counts.

/// A stored SQL template with a parameter count (used by prepared statements).
#[derive(Debug, Clone)]
pub(crate) struct PreparedStmt {
    /// SQL template with $1, $2, ... placeholders.
    pub sql: String,
    /// Expected number of arguments.
    #[allow(dead_code, reason = "prepared statement feature not yet wired")]
    pub arg_count: usize,
}

impl super::Database {
    /// Store a prepared SQL template.
    pub fn prepare(&mut self, name: &str, sql: &str, arg_count: usize) {
        self.prepared.insert(
            name.to_lowercase(),
            PreparedStmt {
                sql: sql.to_string(),
                arg_count,
            },
        );
    }

    /// Remove a prepared statement.
    #[allow(dead_code, reason = "prepared statement feature not yet wired")]
    pub fn drop_prepared(&mut self, name: &str) -> Result<(), String> {
        self.prepared
            .remove(&name.to_lowercase())
            .map(|_| ())
            .ok_or_else(|| format!("Prepared statement '{}' not found", name))
    }
}
