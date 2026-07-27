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
}
