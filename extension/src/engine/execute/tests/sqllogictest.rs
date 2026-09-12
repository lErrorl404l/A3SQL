// sqllogictest adapter for A3SQL
//
// Implements the sqllogictest `DB` trait so the engine can be verified
// against SQL logic test files. Same harness as DuckDB and DataFusion
// use. Run with: cargo test --manifest-path extension/Cargo.toml sqllogic
//
// The .slt files live in extension/tests/slt/.

use sqllogictest::{DBOutput, DefaultColumnType};

use crate::engine::database::Database;
use crate::engine::execute::util::parse_and_exec;
use crate::parser::parse_sql;

/// sqllogictest-compatible error wrapper.
#[derive(Debug)]
pub struct SltError(pub String);

impl std::fmt::Display for SltError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SltError {}

/// Render a DbValue as sqllogictest expects: plain text, no JSON quoting.
/// NULL renders as the string "NULL" (the sqllogictest convention).
fn render_value(v: &crate::engine::value::DbValue) -> String {
    match v {
        crate::engine::value::DbValue::Null => "NULL".to_string(),
        crate::engine::value::DbValue::Bool(b) => b.to_string(),
        crate::engine::value::DbValue::Int(n) => n.to_string(),
        crate::engine::value::DbValue::Float(f) => format!("{}", f),
        crate::engine::value::DbValue::String(s) => s.clone(),
        crate::engine::value::DbValue::Strings(v) => v.join(","),
        crate::engine::value::DbValue::Floats(v) => v.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","),
    }
}

/// The sqllogictest database handle.
pub struct A3sqlSlt {
    pub db: Database,
}

impl sqllogictest::DB for A3sqlSlt {
    type Error = SltError;
    type ColumnType = DefaultColumnType;

    fn run(&mut self, sql: &str) -> Result<DBOutput<Self::ColumnType>, Self::Error> {
        // Parse to determine whether this is a SELECT query (returns rows)
        // or a statement (returns a completion count).
        let stmts = parse_sql(sql).map_err(|e| SltError(e.to_string()))?;

        // A single query: return its result rows.
        if stmts.len() == 1 && matches!(&stmts[0], sqlparser::ast::Statement::Query(_)) {
            let query = match &stmts[0] {
                sqlparser::ast::Statement::Query(q) => q,
                _ => unreachable!(),
            };
            let (headers, rows) = crate::engine::execute::select::exec_select_rows(query, &mut self.db)
                .map_err(|e| SltError(format!("{:?}", e)))?;

            let types = headers.iter().map(|_| DefaultColumnType::Any).collect();
            let rendered: Vec<Vec<String>> = rows.iter().map(|row| row.iter().map(render_value).collect()).collect();
            if sql.contains("GROUP BY name") && sql.contains("COUNT") {
                eprintln!("[SLT DEBUG] SQL: {}", sql);
                eprintln!("[SLT DEBUG] Raw rows: {:?}", rows);
                eprintln!("[SLT DEBUG] Rendered: {:?}", rendered);
            }
            Ok(DBOutput::Rows { types, rows: rendered })
        } else {
            // Statement: execute and report completion.
            parse_and_exec(sql, &mut self.db).map_err(|e| SltError(e.to_string()))?;
            Ok(DBOutput::StatementComplete(0))
        }
    }

    fn engine_name(&self) -> &str {
        "a3sql"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqllogictest::Runner;

    /// Run the sqllogictest corpus. Test files are in extension/tests/slt/.
    #[test]
    fn run_slt_corpus() {
        let mut runner = Runner::new(|| async { Ok(A3sqlSlt { db: Database::new() }) });
        let col = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/slt");
        let mut files = std::fs::read_dir(&col)
            .expect("slt dir should exist")
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "slt"))
            .collect::<Vec<_>>();
        files.sort();

        let mut failures = 0;
        for f in &files {
            let result = runner.run_file(f);
            match result {
                Ok(_) => println!("PASS: {}", f.display()),
                Err(e) => {
                    failures += 1;
                    println!("FAIL: {} | {}", f.display(), e);
                }
            }
        }
        assert_eq!(failures, 0, "{} sqllogictest file(s) failed", failures);
    }
}
