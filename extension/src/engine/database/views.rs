//! View management — logical views over table data.

impl super::Database {
    /// Store a view definition.
    pub fn create_view(&mut self, name: &str, sql: &str) -> Result<(), String> {
        if self.has_table(name) {
            return Err(format!("'{}' is a table name", name));
        }
        if self.views.contains_key(name) {
            return Err(format!("View '{}' already exists", name));
        }
        self.views.insert(name.to_string(), sql.to_string());
        Ok(())
    }

    /// Remove a view definition.
    pub fn drop_view(&mut self, name: &str) -> Result<(), String> {
        if self.views.remove(name).is_none() {
            return Err(format!("View '{}' does not exist", name));
        }
        Ok(())
    }

    /// Get a view's SQL text.
    pub fn get_view(&self, name: &str) -> Option<&String> {
        self.views.get(name)
    }

    /// Check if a view exists.
    pub fn has_view(&self, name: &str) -> bool {
        self.views.contains_key(name)
    }

    /// List all view names.
    pub fn view_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.views.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}
