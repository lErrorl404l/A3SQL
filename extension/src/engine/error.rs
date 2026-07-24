// a3db structured error codes

use std::fmt;

/// Structured error with a machine-readable code and human-readable message.
#[derive(Debug, Clone)]
pub struct A3dbError {
    pub code: ErrorCode,
    pub message: String,
}

impl A3dbError {
    pub fn new(code: ErrorCode, msg: impl Into<String>) -> Self {
        A3dbError {
            code,
            message: msg.into(),
        }
    }

    /// Serialize as a JSON response segment: `[-1,"ERR_CODE","message"]`
    pub fn to_response(&self) -> String {
        format!("[-1,\"{}\",\"{}\"]", self.code, self.message)
    }
}

impl fmt::Display for A3dbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl From<(&str, String)> for A3dbError {
    fn from((code, msg): (&str, String)) -> Self {
        let ec = ErrorCode::from_str(code).unwrap_or(ErrorCode::Internal);
        A3dbError::new(ec, msg)
    }
}

/// Machine-readable error codes from the handoff spec.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorCode {
    Ok,
    Parse,
    Exec,
    Table,
    Type,
    Pk,
    Io,
    Internal,
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
        }
    }
}

/// Build an OK response JSON string.
pub fn ok_response(data: &str) -> String {
    format!("[0,\"OK\",{}]", data)
}

/// Build an error response JSON string.
pub fn error_response(code: ErrorCode, msg: &str) -> String {
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
}
