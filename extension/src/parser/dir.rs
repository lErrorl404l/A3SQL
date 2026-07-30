// a3sql SQL parser — wraps sqlparser-rs with custom dialect and preprocessing

//! SQL parser — wraps sqlparser-rs with Arma-specific dialect and preprocessing.

pub mod dialect;
pub mod preprocessor;
pub mod sqf_literal;

use sqlparser::ast::Statement;
use sqlparser::parser::ParserError;

/// Parse SQL string into a vec of sqlparser Statements.
/// Runs preprocessor first (%% → fuzzy_match, etc.), then parses with A3sqlDialect.
pub fn parse_sql(sql: &str) -> Result<Vec<Statement>, ParserError> {
    let cleaned = preprocessor::preprocess(sql);
    let dialect = dialect::A3sqlDialect;
    sqlparser::parser::Parser::parse_sql(&dialect, &cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_create_table() {
        let sql = "CREATE TABLE weapons (id STRING PRIMARY KEY, caliber STRING, barrelLength FLOAT)";
        let stmts = parse_sql(sql).unwrap();
        assert!(!stmts.is_empty());
        assert!(matches!(&stmts[0], Statement::CreateTable { .. }));
    }

    #[test]
    fn parse_insert() {
        let sql = "INSERT INTO weapons VALUES ('m4a1', '5.56x45mm', 368.3)";
        let stmts = parse_sql(sql).unwrap();
        assert!(matches!(&stmts[0], Statement::Insert { .. }));
    }

    #[test]
    fn parse_select() {
        let sql = "SELECT * FROM weapons WHERE caliber = '5.56x45mm'";
        let stmts = parse_sql(sql).unwrap();
        assert!(matches!(&stmts[0], Statement::Query { .. }));
    }

    #[test]
    fn parse_fuzzy_preprocessed() {
        let sql = "SELECT * FROM weapons WHERE id %% 'rhs_m4'";
        let stmts = parse_sql(sql).unwrap();
        // %% should be rewritten to fuzzy_match(id, 'rhs_m4')
        assert!(matches!(&stmts[0], Statement::Query { .. }));
    }

    #[test]
    fn parse_delete() {
        let sql = "DELETE FROM weapons WHERE id = 'test'";
        let stmts = parse_sql(sql).unwrap();
        assert!(matches!(&stmts[0], Statement::Delete { .. }));
    }

    #[test]
    fn parse_drop() {
        let sql = "DROP TABLE weapons";
        let stmts = parse_sql(sql).unwrap();
        assert!(matches!(&stmts[0], Statement::Drop { .. }));
    }

    #[test]
    fn parse_create_with_array_types() {
        let sql = "CREATE TABLE t (id STRING PRIMARY KEY, tags STRINGS[], vals FLOATS[])";
        let stmts = parse_sql(sql).unwrap();
        assert!(matches!(&stmts[0], Statement::CreateTable { .. }));
    }
}
