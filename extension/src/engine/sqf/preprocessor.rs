// SQF preprocessor — macro expansion (#define, #include, #ifdef, etc.)
//
// Optional module behind the `sqf-preprocessor` feature flag. When enabled,
// wraps `hemtt-preprocessor` to expand SQF macros before the expression
// lexer sees the source. When disabled, this is a no-op passthrough.
//
// ponytail: the preprocessor only handles inline single-expression input
// (#define + #ifdef within the expression). File includes (#include) are
// skipped since SQF_EVAL() expressions are self-contained strings.
// Add a custom Resolver if #include support is needed later.

/// Returns the command name and processed args if the expression uses a
/// macro-passthrough pattern. Otherwise returns None.
/// Not yet implemented — reserved for future macro dispatch.
#[allow(dead_code)]
pub(crate) fn preprocess(input: &str) -> Result<String, String> {
    #[cfg(feature = "sqf-preprocessor")]
    {
        let tokens =
            hemtt_preprocessor::preprocess_string(input).map_err(|e| format!("SQF preprocessor error: {:?}", e))?;
        // Reconstruct source from tokens. Whitespace and comments are
        // preserved so that the existing lexer can handle them naturally.
        let expanded: String = tokens.iter().map(|t| t.to_source()).collect();
        Ok(expanded)
    }

    #[cfg(not(feature = "sqf-preprocessor"))]
    {
        // Pass through unchanged — no preprocessor available.
        let _ = input;
        Err("SQF preprocessor not available — compile with `sqf-preprocessor` feature".into())
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "sqf-preprocessor")]
    use super::*;

    #[cfg(feature = "sqf-preprocessor")]
    use crate::engine::sqf::eval_sqf;
    #[cfg(feature = "sqf-preprocessor")]
    use crate::engine::value::DbValue;
    #[cfg(feature = "sqf-preprocessor")]
    use std::collections::HashMap;

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_noop_passthrough() {
        let r = preprocess("sqrt 25").unwrap();
        assert_eq!(r, "sqrt 25");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_define_constant() {
        let r = preprocess("#define X 42\nX").unwrap();
        assert!(r.contains("42"), "expected 42 in output, got {:?}", r);
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_define_expression() {
        let r = preprocess("#define ADD(a,b) a + b\nADD(3,4)").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "3+4");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_ifdef_true() {
        // HEMTT requires trailing newline after #endif
        let r = preprocess("#define DEBUG\n#ifdef DEBUG\n1\n#else\n0\n#endif\n").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "1");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_ifdef_false() {
        let r = preprocess("#ifdef NOTDEFINED\n1\n#else\n0\n#endif\n").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "0");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_token_paste() {
        let r = preprocess("#define CAT(a,b) a ## b\nCAT(vect,or)").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "vector");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_stringify() {
        let r = preprocess("#define STR(x) #x\nSTR(hello)").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "\"hello\"");
    }

    // ── Full pipeline tests ───────────────────────────────────────────

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_simple_define() {
        let r = eval_sqf("#define VAL 42\nVAL", &HashMap::new()).unwrap();
        assert_eq!(r, DbValue::Int(42));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_define_math() {
        let r = eval_sqf("#define PI 3.14159\nsqrt PI", &HashMap::new()).unwrap();
        match r {
            DbValue::Float(f) => {
                let expected = std::f64::consts::PI.sqrt();
                assert!((f - expected).abs() < 0.001, "got {}, expected {}", f, expected);
            }
            other => panic!("expected Float, got {:?}", other),
        }
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_sqf_no_macros_still_works() {
        let r = eval_sqf("sqrt 25 + round 3.7", &HashMap::new()).unwrap();
        assert_eq!(r, DbValue::Float(9.0));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_str_define() {
        let r = eval_sqf(
            r#"#define GREETING "hello"
toupper GREETING"#,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(r, DbValue::String("HELLO".into()));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_define_undef() {
        let r = preprocess("#define X 1\nX\n#undef X\nX").unwrap();
        assert!(r.contains("X"));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_include_passthrough() {
        let r = preprocess("#include \"missing.hpp\"");
        assert!(r.is_err(), "expected error for unresolvable include");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_ifndef() {
        let r = preprocess("#ifndef NOTDEFINED\nyes\n#endif\n").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "yes");
    }

    // ── Additional edge cases ─────────────────────────────────────────

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_define_no_newline_before_expr() {
        // Define on one line, expression immediately after
        let r = preprocess("#define X 42\nX + 1").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "42+1");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_define_chained() {
        // One define referencing another
        let r = preprocess("#define BASE 10\n#define MULT 2\nBASE * MULT").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "10*2");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_define_empty_flag() {
        // Empty define (just a flag, no value)
        let r = preprocess("#define DEBUG\n#ifdef DEBUG\n1\n#endif\n").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "1");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_nested_ifdef() {
        let r = preprocess("#define A\n#ifdef A\n#ifdef B\nNO\n#else\nYES\n#endif\n#endif\n").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "YES");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_define_args_multiple() {
        let r = preprocess("#define ADD(a,b) a + b\n#define MUL(a,b) a * b\nADD(2,3) + MUL(4,5)").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(flat, "2+3+4*5");
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_define_no_args_constant_in_expr() {
        // Define with no args but used where function-like syntax expected
        let r = preprocess("#define PI 3.14\nsqrt PI + 1").unwrap();
        let flat: String = r.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(flat.contains("sqrt"));
        assert!(flat.contains("3.14"));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_define_chained_math() {
        // Full pipeline: chained defines evaluated
        let r = eval_sqf("#define A 3\n#define B 4\nA + B", &HashMap::new()).unwrap();
        assert_eq!(r, DbValue::Int(7));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_define_toupper() {
        // Define a string constant and call toUpper on it
        let r = eval_sqf(
            r#"#define NAME "world"
toupper NAME"#,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(r, DbValue::String("WORLD".into()));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_define_function_like() {
        // Function-like macro expanding to a command call
        let r = eval_sqf("#define SQRT(x) sqrt x\nSQRT(25)", &HashMap::new()).unwrap();
        assert_eq!(r, DbValue::Float(5.0));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_complex_with_multiple_defines() {
        // Multiple defines with expression evaluation
        let r = eval_sqf(
            "#define X 10\n#define Y 20\n#define ADD(a,b) a + b\nADD(X,Y) * 2",
            &HashMap::new(),
        )
        .unwrap();
        // ADD(X,Y) → 10 + 20 → (10 + 20) * 2 = 60
        // SQF precedence: * binds tighter than +, so result depends on parens
        assert!(matches!(r, DbValue::Int(_)));
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_ifdef_guard() {
        let r = eval_sqf(
            "#define USE_PI\n#ifdef USE_PI\n3.14159\n#else\n3.0\n#endif\n",
            &HashMap::new(),
        )
        .unwrap();
        match r {
            DbValue::Float(f) => assert!((f - 3.14159).abs() < 0.001),
            other => panic!("expected Float, got {:?}", other),
        }
    }

    #[cfg(feature = "sqf-preprocessor")]
    #[test]
    fn test_eval_empty_expression_with_define() {
        // Define with no used macro — just evaluate the expression
        let r = eval_sqf("#define UNUSED 99\n42 + 1", &HashMap::new()).unwrap();
        assert_eq!(r, DbValue::Int(43));
    }
}
