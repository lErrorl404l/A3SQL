// a3sql query optimizer — rule-based plan rewriting before execution.
//!
//! Each rule implements [`OptimizerRule`]. Rules run in order until a
//! fixed point is reached (or [`MAX_ITERATIONS`] elapses).
//!
//! # Rules
//!
//! | Rule | Purpose |
//! |------|---------|
//! | [`SimplifyExpressions`] | Constant-fold trivial expressions, flatten AND chains |
//!
//! To add a rule: create a file in this module, impl `OptimizerRule` on a
//! unit struct, then register it in [`Optimizer::new`].

use std::time::Instant;

use sqlparser::ast::Statement;

use super::error::EngineError;

/// Maximum optimisation passes before we give up.
#[allow(dead_code, reason = "optimizer not yet wired into query execution")]
const MAX_ITERATIONS: usize = 10;
/// Per-rule time budget (milliseconds).
#[allow(dead_code, reason = "optimizer not yet wired into query execution")]
const RULE_TIMEOUT_MS: u64 = 100;

// ── Trait ───────────────────────────────────────────────────────────────

/// A single rewrite pass over a [`Statement`].
///
/// Return [`Transformed::Yes(rewritten)`] when a change was made,
/// [`Transformed::No`] when the statement is already optimal for this rule.
#[allow(dead_code, reason = "optimizer not yet wired into query execution")]
pub(crate) trait OptimizerRule: std::fmt::Debug {
    fn name(&self) -> &str;

    /// Rewrite the statement. Returning `Yes` triggers another iteration
    /// (rules are re-run to a fixed point).
    fn rewrite(&self, stmt: Statement) -> Result<Transformed<Statement>, EngineError>;
}

/// Result of a rewrite pass.
#[allow(dead_code, reason = "optimizer not yet wired into query execution")]
pub(crate) enum Transformed<T> {
    Yes(T),
    No(T),
}

impl<T> Transformed<T> {
    #[allow(dead_code, reason = "optimizer not yet wired into query execution")]
    pub(crate) fn into_inner(self) -> T {
        match self {
            Transformed::Yes(t) | Transformed::No(t) => t,
        }
    }

    #[allow(dead_code, reason = "optimizer not yet wired into query execution")]
    pub(crate) fn was_changed(&self) -> bool {
        matches!(self, Transformed::Yes(_))
    }
}

// ── Runner ──────────────────────────────────────────────────────────────

/// Holds the ordered list of optimisation rules and applies them to
/// reach a fixed point.
#[derive(Debug)]
#[allow(dead_code, reason = "optimizer not yet wired into query execution")]
pub(crate) struct Optimizer {
    rules: Vec<Box<dyn OptimizerRule>>,
}

impl Optimizer {
    #[allow(dead_code, reason = "optimizer not yet wired into query execution")]
    pub(crate) fn new() -> Self {
        Optimizer {
            rules: vec![Box::new(SimplifyExpressions)],
        }
    }

    /// Run all rules in order until no rule reports a change, up to
    /// [`MAX_ITERATIONS`] rounds.
    #[allow(dead_code, reason = "optimizer not yet wired into query execution")]
    pub(crate) fn optimize(&self, stmt: Statement) -> Result<Statement, EngineError> {
        let start = Instant::now();
        let mut current = stmt;

        for round in 0..MAX_ITERATIONS {
            if start.elapsed().as_millis() as u64 > RULE_TIMEOUT_MS {
                break; // time budget exhausted
            }

            let mut changed = false;
            for rule in &self.rules {
                let result = rule.rewrite(current)?;
                changed |= result.was_changed();
                current = result.into_inner();
            }

            if !changed {
                break; // fixed point reached
            }

            if round == MAX_ITERATIONS - 1 {
                eprintln!("[a3sql] Optimizer did not converge after {} iterations", MAX_ITERATIONS);
            }
        }

        Ok(current)
    }
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Rules ───────────────────────────────────────────────────────────────

/// Simplify trivial expressions.
///
/// Currently handles:
/// - No optimizations implemented yet (placeholder rule)
#[derive(Debug)]
#[allow(dead_code, reason = "optimizer not yet wired into query execution")]
struct SimplifyExpressions;

impl OptimizerRule for SimplifyExpressions {
    fn name(&self) -> &str {
        "simplify_expressions"
    }

    fn rewrite(&self, stmt: Statement) -> Result<Transformed<Statement>, EngineError> {
        // ponytail: expression simplification is already handled inline
        // during evaluation. This rule is a placeholder for future passes
        // like constant folding, predicate simplification, etc.
        Ok(Transformed::No(stmt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimizer_empty_rules_noop() {
        let opt = Optimizer::new();
        let stmt = crate::parser::parse_sql("SELECT 1")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let result = opt.optimize(stmt).unwrap();
        let output = format!("{}", result);
        assert!(!output.is_empty());
    }

    #[test]
    fn optimizer_fixed_point() {
        let opt = Optimizer::new();
        let stmt = crate::parser::parse_sql("SELECT * FROM t WHERE 1 = 1")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        // Should converge without error
        let result = opt.optimize(stmt).unwrap();
        let _ = format!("{}", result);
    }
}
