//! Cursor management — iterative query fetching.

/// Active query cursor for iterative fetching (used by cursor commands).
#[derive(Debug, Clone)]
pub(crate) struct CursorState {
    /// The original SQL the cursor was created from.
    pub sql: String,
    /// Current offset (row position).
    pub offset: usize,
    /// Number of rows per fetch.
    #[allow(dead_code, reason = "cursor feature not yet wired")]
    pub page_size: usize,
}

impl super::Database {
    /// Create a new cursor for iterative query fetching.
    pub fn create_cursor(&mut self, name: &str, sql: &str, page_size: usize) {
        self.cursors.insert(
            name.to_lowercase(),
            CursorState {
                sql: sql.to_string(),
                offset: 0,
                page_size,
            },
        );
    }

    /// Drop a cursor by name.
    pub fn drop_cursor(&mut self, name: &str) -> Result<(), String> {
        self.cursors
            .remove(&name.to_lowercase())
            .map(|_| ())
            .ok_or_else(|| format!("Cursor '{}' not found", name))
    }
}
