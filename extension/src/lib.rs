// a3sql — Arma 3 Database Engine
// C ABI: RVExtension, RVExtensionArgs, RVExtensionVersion
//
// Build targets:
//   Linux:   x86_64-unknown-linux-gnu, i686-unknown-linux-gnu
//   Windows: x86_64-pc-windows-gnu,     i686-pc-windows-gnu
// Windows x86 (32-bit) needs a .def file or link args for decorated exports:
//   _RVExtensionVersion@8, _RVExtension@12, _RVExtensionArgs@20

#![allow(non_snake_case)]
// ponytail: unused items kept for phased implementation
#![allow(dead_code)]

pub(crate) mod auth;
pub(crate) mod config;
pub(crate) mod dispatch;
mod engine;
pub mod ffi;
pub mod parser;
pub(crate) mod server;

// Re-exports for standalone binary
pub use dispatch::dispatch;
pub use server::start_server;

// ── Tests ────────────────────────────────────────────────────────────────
// Integration tests (abi, dispatch, plugins) live in tests/abi.rs.
// This module keeps unit tests for internal pub(crate) functions only.

#[cfg(test)]
mod tests {

    #[test]
    fn dispatch_split_sql() {
        use crate::dispatch::split_sql;
        let stmts = split_sql("SELECT 1; SELECT 2;");
        assert_eq!(stmts.len(), 2, "expected 2 statements, got: {:?}", stmts);
    }

    #[test]
    fn dispatch_split_sql_with_string() {
        use crate::dispatch::split_sql;
        let stmts = split_sql("SELECT 'hello;world'; SELECT 2");
        assert_eq!(stmts.len(), 2, "expected 2 statements, got: {:?}", stmts);
    }

    #[test]
    fn substitute_params_empty() {
        use crate::dispatch::substitute_params;
        assert_eq!(substitute_params("SELECT 1", &[]), "SELECT 1");
    }

    #[test]
    fn substitute_params_string() {
        use crate::dispatch::substitute_params;
        assert_eq!(substitute_params("SELECT $1", &["hello"]), "SELECT 'hello'");
    }

    #[test]
    fn substitute_params_string_escape_quote() {
        use crate::dispatch::substitute_params;
        assert_eq!(substitute_params("SELECT $1", &["it's"]), "SELECT 'it''s'");
    }

    #[test]
    fn substitute_params_injection_attempt() {
        use crate::dispatch::substitute_params;
        assert_eq!(
            substitute_params("SELECT $1", &["' OR 1=1 --"]),
            "SELECT ''' OR 1=1 --'"
        );
    }

    #[test]
    fn substitute_params_null() {
        use crate::dispatch::substitute_params;
        assert_eq!(substitute_params("SELECT $1", &["NULL"]), "SELECT NULL");
    }

    #[test]
    fn substitute_params_int() {
        use crate::dispatch::substitute_params;
        assert_eq!(substitute_params("SELECT $1", &["42"]), "SELECT 42");
    }

    #[test]
    fn substitute_params_float() {
        use crate::dispatch::substitute_params;
        assert_eq!(substitute_params("SELECT $1", &["3.14"]), "SELECT 3.14");
    }

    #[test]
    fn substitute_params_bool() {
        use crate::dispatch::substitute_params;
        assert_eq!(substitute_params("SELECT $1", &["true"]), "SELECT true");
    }

    #[test]
    fn substitute_params_multi() {
        use crate::dispatch::substitute_params;
        assert_eq!(substitute_params("SELECT $1, $2", &["a", "b"]), "SELECT 'a', 'b'");
    }

    #[test]
    fn substitute_params_respects_string_context() {
        use crate::dispatch::substitute_params;
        // $1 inside a string literal should not be substituted
        assert_eq!(substitute_params("SELECT '$1', $2", &["x", "y"]), "SELECT '$1', 'y'");
    }
}
