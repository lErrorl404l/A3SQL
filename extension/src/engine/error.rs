// a3sql structured error codes

//! Error handling — typed errors, response formatting, and error codes.
//!
//! Internal engine functions return [`Result<T, EngineError>`]. At the FFI
//! boundary, `EngineError` is converted to an [`A3sqlError`] response string.

use std::fmt;

// ── Typed engine errors ─────────────────────────────────────────────────

/// Typed error used throughout the engine internals.
///
/// Each variant carries the fields needed to produce a useful error message.
/// The [`Display`] impl produces a human-readable message.
/// [`code()`] returns the [`ErrorCode`] for the ABI response.
#[derive(Debug, thiserror::Error)]
pub(crate) enum EngineError {
    #[error("Table '{0}' does not exist")]
    TableNotFound(String),

    #[error("Table '{0}' already exists")]
    TableAlreadyExists(String),

    #[error("Column '{0}' does not exist")]
    ColumnNotFound(String),

    #[error("Column '{name}' not found in table '{table}'")]
    ColumnNotFoundInTable { name: String, table: String },

    #[error("Column '{0}' already exists")]
    ColumnAlreadyExists(String),

    #[error("Duplicate key '{0}'")]
    DuplicateKey(String),

    #[error("Index '{0}' does not exist")]
    IndexNotFound(String),

    #[error("Index '{0}' already exists")]
    IndexAlreadyExists(String),

    #[error("View '{0}' not found")]
    ViewNotFound(String),

    #[error("Trigger '{0}' already exists")]
    TriggerAlreadyExists(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("{0}")]
    Exec(String),

    #[error("Type error: expected {expected}, got {actual}")]
    TypeError { expected: String, actual: String },

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("Savepoint '{0}' already exists")]
    SavepointExists(String),

    #[error("Savepoint '{0}' not found")]
    SavepointNotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl EngineError {
    /// Map to the ABI error code.
    pub(crate) fn code(&self) -> ErrorCode {
        match self {
            EngineError::TableNotFound(_) | EngineError::TableAlreadyExists(_) => ErrorCode::Table,
            EngineError::ColumnNotFound(_)
            | EngineError::ColumnNotFoundInTable { .. }
            | EngineError::ColumnAlreadyExists(_) => ErrorCode::Table,
            EngineError::DuplicateKey(_) => ErrorCode::Pk,
            EngineError::IndexNotFound(_) | EngineError::IndexAlreadyExists(_) => ErrorCode::Table,
            EngineError::ViewNotFound(_) => ErrorCode::Table,
            EngineError::TriggerAlreadyExists(_) => ErrorCode::Exec,
            EngineError::Parse(_) => ErrorCode::Parse,
            EngineError::Exec(_) | EngineError::SavepointExists(_) | EngineError::SavepointNotFound(_) => {
                ErrorCode::Exec
            }
            EngineError::TypeError { .. } => ErrorCode::Type,
            EngineError::Io(_) => ErrorCode::Io,
            EngineError::Internal(_) => ErrorCode::Internal,
        }
    }
}

// ── Legacy error types (for ABI boundary) ───────────────────────────────

/// Structured error with a machine-readable code and human-readable message.
#[derive(Debug, Clone)]
pub(crate) struct A3sqlError {
    pub code: ErrorCode,
    pub message: String,
}

impl A3sqlError {
    pub fn new(code: ErrorCode, msg: impl Into<String>) -> Self {
        A3sqlError {
            code,
            message: msg.into(),
        }
    }

    /// Serialize as a JSON response segment: `[-1,"ERR_CODE","message"]`
    pub fn to_response(&self) -> String {
        format!("[-1,\"{}\",\"{}\"]", self.code, self.message)
    }
}

impl fmt::Display for A3sqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl From<(&str, String)> for A3sqlError {
    fn from((code, msg): (&str, String)) -> Self {
        let ec = ErrorCode::from_str(code).unwrap_or(ErrorCode::Internal);
        A3sqlError::new(ec, msg)
    }
}

impl From<EngineError> for A3sqlError {
    fn from(e: EngineError) -> Self {
        A3sqlError::new(e.code(), e.to_string())
    }
}

// Bridge: allow `?` to propagate EngineError through `Result<_, String>` —
// used by test modules and intermediate code that hasn't migrated yet.
impl From<EngineError> for String {
    fn from(e: EngineError) -> Self {
        e.to_string()
    }
}

// Bridge: allow `?` to propagate `String` errors through `Result<_, EngineError>` —
// used while stmts modules are still migrating.
impl From<String> for EngineError {
    fn from(s: String) -> Self {
        EngineError::Exec(s)
    }
}

// ── Error codes ─────────────────────────────────────────────────────────

/// Machine-readable error codes from the handoff spec.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ErrorCode {
    Ok,
    Parse,
    Exec,
    Table,
    Type,
    Pk,
    Io,
    Internal,
    Auth,
}

impl ErrorCode {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "OK" => Some(ErrorCode::Ok),
            "ERR_PARSE" | "PARSE" => Some(ErrorCode::Parse),
            "ERR_EXEC" | "EXEC" => Some(ErrorCode::Exec),
            "ERR_TABLE" | "TABLE" => Some(ErrorCode::Table),
            "ERR_TYPE" | "TYPE" => Some(ErrorCode::Type),
            "ERR_PK" | "PK" => Some(ErrorCode::Pk),
            "ERR_IO" | "IO" => Some(ErrorCode::Io),
            "ERR_INTERNAL" | "INTERNAL" => Some(ErrorCode::Internal),
            "ERR_AUTH" | "AUTH" => Some(ErrorCode::Auth),
            _ => None,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorCode::Ok => write!(f, "OK"),
            ErrorCode::Parse => write!(f, "ERR_PARSE"),
            ErrorCode::Exec => write!(f, "ERR_EXEC"),
            ErrorCode::Table => write!(f, "ERR_TABLE"),
            ErrorCode::Type => write!(f, "ERR_TYPE"),
            ErrorCode::Pk => write!(f, "ERR_PK"),
            ErrorCode::Io => write!(f, "ERR_IO"),
            ErrorCode::Internal => write!(f, "ERR_INTERNAL"),
            ErrorCode::Auth => write!(f, "ERR_AUTH"),
        }
    }
}

/// Build an OK response JSON string.
pub(crate) fn ok_response(data: &str) -> String {
    format!("[0,\"OK\",{}]", data)
}

/// Build an error response JSON string.
pub(crate) fn error_response(code: ErrorCode, msg: &str) -> String {
    format!("[-1,\"{}\",\"{}\"]", code, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_display() {
        assert_eq!(format!("{}", ErrorCode::Ok), "OK");
        assert_eq!(format!("{}", ErrorCode::Parse), "ERR_PARSE");
        assert_eq!(format!("{}", ErrorCode::Exec), "ERR_EXEC");
    }

    #[test]
    fn error_code_from_str() {
        assert_eq!(ErrorCode::from_str("ERR_PARSE"), Some(ErrorCode::Parse));
        assert_eq!(ErrorCode::from_str("exec"), Some(ErrorCode::Exec));
        assert_eq!(ErrorCode::from_str("unknown"), None);
    }

    #[test]
    fn ok_response_format() {
        let r = ok_response("\"hello\"");
        assert_eq!(r, "[0,\"OK\",\"hello\"]");
    }

    #[test]
    fn error_response_format() {
        let r = error_response(ErrorCode::Table, "table not found");
        assert_eq!(r, "[-1,\"ERR_TABLE\",\"table not found\"]");
    }

    #[test]
    fn engine_error_table_not_found() {
        let e = EngineError::TableNotFound("users".into());
        assert_eq!(e.to_string(), "Table 'users' does not exist");
        assert_eq!(e.code(), ErrorCode::Table);
    }

    #[test]
    fn engine_error_duplicate_key() {
        let e = EngineError::DuplicateKey("id_42".into());
        assert_eq!(e.to_string(), "Duplicate key 'id_42'");
        assert_eq!(e.code(), ErrorCode::Pk);
    }

    #[test]
    fn engine_error_to_a3sql() {
        let e = EngineError::TableNotFound("orders".into());
        let a: A3sqlError = e.into();
        assert_eq!(a.code, ErrorCode::Table);
        assert!(a.message.contains("orders"));
    }
}
