// A3DbDialect — custom SQL dialect for a3db
//
// Extends GenericDialect (most permissive) with:
//   - %% fuzzy match operator   → handled via preprocessor → fuzzy_match()
//   - STRINGS[] / FLOATS[] types → GenericDialect handles array syntax natively
//   - IMPORT / EXPORT statements → intercepted at dispatch level (string prefix)
//
// Multi-dialect support: GenericDialect accepts syntax from PostgreSQL,
// MySQL/MariaDB, SQLite, DataFusion/Apache, and most ANSI SQL.
// Broad compatibility without dialect-specific code.
//
// IMPORT/EXPORT are NOT parsed here. They're intercepted at the dispatch
// level (lib.rs) before SQL parsing, keeping the dialect clean.

use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, GenericDialect};
use sqlparser::parser::{Parser, ParserError};
use std::any::TypeId;

/// A3DB's custom SQL dialect — extends GenericDialect implicitly.
///
/// Returns None from parse_statement() for all custom statements,
/// delegating everything to GenericDialect's default behavior.
/// Custom syntax (IMPORT, EXPORT) is handled at the dispatch layer.
#[derive(Debug, Default)]
pub struct A3DbDialect;

impl Dialect for A3DbDialect {
    /// Report as GenericDialect so sqlparser enables MySQL features like AUTO_INCREMENT.
    fn dialect(&self) -> TypeId {
        TypeId::of::<GenericDialect>()
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        ch.is_alphabetic() || ch == '_'
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '$'
    }

    /// Returns None for all custom statements — delegates to GenericDialect fallback.
    fn parse_statement(&self, _parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        None
    }
}
