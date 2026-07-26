// A3sqlDialect — custom SQL dialect for a3sql
//
// Delegates everything to GenericDialect. Custom statements handled at dispatch level.

//! Custom SQL dialect — extends GenericDialect with Arma/MySQL compatibility.

use sqlparser::ast::Statement;
use sqlparser::dialect::{Dialect, GenericDialect};
use sqlparser::parser::{Parser, ParserError};
use std::any::TypeId;

#[derive(Debug, Default)]
pub struct A3sqlDialect;

impl Dialect for A3sqlDialect {
    fn dialect(&self) -> TypeId {
        TypeId::of::<GenericDialect>()
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        ch.is_alphabetic() || ch == '_'
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == '$'
    }

    fn parse_statement(&self, _parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        None
    }
}
