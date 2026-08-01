// a3sql — SQF literal parser (fast-path data ingestion)
//
// Zero-copy nom parser that converts SQF literal strings directly to
// serde_json::Value, which the engine then converts to DbValue via the
// existing json_val_to_dbvalue().
//
// Grammar:
//   value   = nil | bool | number | string | array
//   nil     = "nil"
//   bool    = "true" | "false"
//   number  = ["-"] ("0" | "1-9" {digit}) ["." {digit}]
//   string  = '"' {any | '""'} '"'
//   array   = "[" [value {"," value}] "]"  (ws ignored around values)

use serde_json::Value;

use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, multispace0, one_of},
    combinator::{opt, recognize, value},
    error::Error,
    sequence::{pair, preceded},
};

/// Maximum array nesting depth. Mirrors serde_json's default recursion limit
/// (128) so both decoders share one policy. Deeper input is rejected with a
/// clean error instead of overflowing the stack (SIGSEGV/abort).
const MAX_DEPTH: usize = 128;

// ── nil ─────────────────────────────────────────────────────────────────

fn parse_nil(input: &str) -> IResult<&str, Value> {
    value(Value::Null, tag("nil")).parse(input)
}

// ── bool ────────────────────────────────────────────────────────────────

fn parse_bool(input: &str) -> IResult<&str, Value> {
    alt((
        value(Value::Bool(true), tag("true")),
        value(Value::Bool(false), tag("false")),
    ))
    .parse(input)
}

// ── number ──────────────────────────────────────────────────────────────

/// SQF number: optional `-`, digits, optional fractional part.
fn parse_number(input: &str) -> IResult<&str, Value> {
    recognize((
        opt(one_of("-")),
        alt((tag("0"), nom::character::complete::digit1)),
        opt(pair(char('.'), nom::character::complete::digit1)),
    ))
    .map_res(|s: &str| -> Result<Value, String> {
        if s.contains('.') {
            s.parse::<f64>()
                .map(Value::from)
                .map_err(|e| format!("bad float: {}", e))
        } else {
            s.parse::<i64>().map(Value::from).map_err(|e| format!("bad int: {}", e))
        }
    })
    .parse(input)
}

// ── string ──────────────────────────────────────────────────────────────

/// SQF string delimited by `"`.  Inside, `""` is an escaped literal quote.
/// E.g. `"say ""hello"""` -> `say "hello"`
fn parse_string(input: &str) -> IResult<&str, Value> {
    let (input, _) = char('"').parse(input)?;
    let mut out = String::new();
    let mut rest = input;
    loop {
        match rest.find('"') {
            None => return Err(nom::Err::Error(Error::new(input, nom::error::ErrorKind::Tag))),
            Some(pos) => {
                out.push_str(&rest[..pos]);
                rest = &rest[pos..];
                // If followed by another `"`, it's an escaped quote
                if rest.len() >= 2 && rest.as_bytes()[1] == b'"' {
                    out.push('"');
                    rest = &rest[2..];
                } else {
                    // Single `"` closes the string
                    return Ok((&rest[1..], Value::String(out)));
                }
            }
        }
    }
}

// ── array ───────────────────────────────────────────────────────────────

/// Parse one value, tracking the current array nesting depth. `depth` is the
/// number of enclosing arrays; exceeding `MAX_DEPTH` is a hard `Failure` (not
/// recoverable by backtracking) so a hostile input cannot recurse deeper than
/// the limit and blow the stack.
fn parse_value(input: &str, depth: usize) -> IResult<&str, Value> {
    if depth > MAX_DEPTH {
        return Err(nom::Err::Failure(Error::new(input, nom::error::ErrorKind::TooLarge)));
    }
    match input.as_bytes().first().copied() {
        Some(b'n') => parse_nil(input),
        Some(b't') | Some(b'f') => parse_bool(input),
        Some(b'"') => parse_string(input),
        Some(b'[') => parse_array(input, depth),
        Some(b'-' | b'0'..=b'9') => parse_number(input),
        _ => Err(nom::Err::Error(Error::new(input, nom::error::ErrorKind::Tag))),
    }
}

fn ws_value(input: &str, depth: usize) -> IResult<&str, Value> {
    preceded(multispace0, |i| parse_value(i, depth)).parse(input)
}

fn parse_array(input: &str, depth: usize) -> IResult<&str, Value> {
    let (input, _) = preceded(multispace0, char('[')).parse(input)?;
    let mut values = Vec::new();
    let (mut input, _) = multispace0(input)?;
    if let Some(rest) = input.strip_prefix(']') {
        return Ok((rest, Value::Array(values)));
    }
    loop {
        let (rest, v) = ws_value(input, depth + 1)?;
        values.push(v);
        let (rest, _) = multispace0(rest)?;
        if let Some(rest) = rest.strip_prefix(',') {
            // Allow a trailing comma: `[1, 2,]` is a valid empty tail.
            let (rest, _) = multispace0(rest)?;
            if let Some(rest) = rest.strip_prefix(']') {
                return Ok((rest, Value::Array(values)));
            }
            input = rest;
        } else if let Some(rest) = rest.strip_prefix(']') {
            return Ok((rest, Value::Array(values)));
        } else {
            return Err(nom::Err::Error(Error::new(rest, nom::error::ErrorKind::Tag)));
        }
    }
}

// ── public API ──────────────────────────────────────────────────────────

/// Parse an SQF literal string into a `serde_json::Value`.
///
/// Handles `nil`, `true`/`false`, numbers (int/float), `"strings"` (with `""`
/// escaping), and nested `[arrays]`.
///
/// # Errors
/// Returns a description of the parse failure.
///
/// # Example
/// ```
/// use a3sql::parser::sqf_literal::parse_sqf_literal;
/// let v = parse_sqf_literal(r#"[1, "hello", true, nil]"#).unwrap();
/// assert_eq!(v[0], 1);
/// assert_eq!(v[1], "hello");
/// assert!(v[2].as_bool().unwrap());
/// assert!(v[3].is_null());
/// ```
pub fn parse_sqf_literal(input: &str) -> Result<Value, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty input".to_string());
    }
    match preceded(multispace0, |i| parse_value(i, 0)).parse(trimmed) {
        Ok((remaining, value)) => {
            let remaining = remaining.trim();
            if remaining.is_empty() {
                Ok(value)
            } else {
                Err(format!("trailing characters after SQF literal: {:?}", remaining))
            }
        }
        Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => {
            if matches!(e.code, nom::error::ErrorKind::TooLarge) {
                return Err(format!("maximum nesting depth exceeded ({} levels)", MAX_DEPTH));
            }
            Err(format!(
                "SQF parse error near byte {}: {:?}",
                trimmed.len() - e.input.len(),
                e.code
            ))
        }
        Err(nom::Err::Incomplete(_)) => Err("incomplete SQF literal".to_string()),
    }
}

/// Parse an SQF literal string and convert directly to a `DbValue`.
#[cfg(test)]
pub(crate) fn parse_sqf_to_dbvalue(input: &str) -> Result<crate::engine::value::DbValue, String> {
    parse_sqf_literal(input).map(|v| crate::engine::value::json_val_to_dbvalue(&v))
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_nil() {
        assert_eq!(parse_sqf_literal("nil").unwrap(), Value::Null);
    }

    #[test]
    fn test_bool_true() {
        assert_eq!(parse_sqf_literal("true").unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_bool_false() {
        assert_eq!(parse_sqf_literal("false").unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_int_zero() {
        assert_eq!(parse_sqf_literal("0").unwrap(), json!(0));
    }

    #[test]
    fn test_int_positive() {
        assert_eq!(parse_sqf_literal("42").unwrap(), json!(42));
    }

    #[test]
    fn test_int_negative() {
        assert_eq!(parse_sqf_literal("-1").unwrap(), json!(-1));
    }

    #[test]
    fn test_float() {
        assert_eq!(parse_sqf_literal("1.5").unwrap(), json!(1.5));
    }

    #[test]
    fn test_float_negative() {
        assert_eq!(parse_sqf_literal("-0.5").unwrap(), json!(-0.5));
    }

    #[test]
    fn test_string_simple() {
        assert_eq!(parse_sqf_literal(r#""hello""#).unwrap(), json!("hello"));
    }

    #[test]
    fn test_string_escaped_quote() {
        // SQF "a""b" → a"b
        let v = parse_sqf_literal(r#""a""b""#).unwrap();
        assert_eq!(v, json!("a\"b"));
    }

    #[test]
    fn test_string_double_escaped_quotes() {
        // SQF "say ""hello""" → say "hello"
        let v = parse_sqf_literal(r#""say ""hello""""#).unwrap();
        assert_eq!(v, json!("say \"hello\""));
    }

    #[test]
    fn test_string_empty() {
        assert_eq!(parse_sqf_literal(r#""""#).unwrap(), json!(""));
    }

    #[test]
    fn test_array_empty() {
        assert_eq!(parse_sqf_literal("[]").unwrap(), json!([]));
    }

    #[test]
    fn test_array_flat() {
        let v = parse_sqf_literal(r#"[1, "hello", true, nil]"#).unwrap();
        assert_eq!(v[0], json!(1));
        assert_eq!(v[1], json!("hello"));
        assert_eq!(v[2], json!(true));
        assert_eq!(v[3], json!(null));
    }

    #[test]
    fn test_array_nested() {
        let v = parse_sqf_literal(r#"[1, [2, [3]]]"#).unwrap();
        assert_eq!(v[0], json!(1));
        assert_eq!(v[1][0], json!(2));
        assert_eq!(v[1][1][0], json!(3));
    }

    #[test]
    fn test_array_with_whitespace() {
        let v = parse_sqf_literal(r#"  [ 1 , "a" , true ]  "#).unwrap();
        assert_eq!(v[0], json!(1));
        assert_eq!(v[1], json!("a"));
        assert_eq!(v[2], json!(true));
    }

    #[test]
    fn test_array_trailing_comma() {
        let v = parse_sqf_literal("[1, 2,]").unwrap();
        assert_eq!(v, json!([1, 2]));
    }

    #[test]
    fn test_array_nesting_within_limit_ok() {
        // 100 levels deep — well inside MAX_DEPTH — must still parse.
        let input = format!("{}1{}", "[".repeat(100), "]".repeat(100));
        assert!(parse_sqf_literal(&input).is_ok());
    }

    #[test]
    fn test_array_deep_nesting_is_capped_cleanly() {
        // 10_000 levels deep must be a clean error, not a stack overflow
        // (SIGSEGV would abort the whole test process).
        let input = format!("{}1{}", "[".repeat(10_000), "]".repeat(10_000));
        let err = parse_sqf_literal(&input).expect_err("deep nesting must be rejected");
        assert!(
            err.contains("maximum nesting depth"),
            "error must name the depth limit, got: {}",
            err
        );
    }

    #[test]
    fn test_parse_error_trailing() {
        let r = parse_sqf_literal("42 extra");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("trailing"));
    }

    #[test]
    fn test_parse_error_invalid() {
        let r = parse_sqf_literal("hello");
        assert!(r.is_err());
    }

    #[test]
    fn test_parse_error_empty() {
        let r = parse_sqf_literal("");
        assert!(r.is_err());
    }

    #[test]
    fn test_parse_error_unclosed_string() {
        let r = parse_sqf_literal(r#""unclosed"#);
        assert!(r.is_err());
    }

    #[test]
    fn test_sqf_to_dbvalue_null() {
        use crate::engine::value::DbValue;
        assert_eq!(parse_sqf_to_dbvalue("nil").unwrap(), DbValue::Null);
    }

    #[test]
    fn test_sqf_to_dbvalue_bool() {
        use crate::engine::value::DbValue;
        assert_eq!(parse_sqf_to_dbvalue("true").unwrap(), DbValue::Bool(true));
        assert_eq!(parse_sqf_to_dbvalue("false").unwrap(), DbValue::Bool(false));
    }

    #[test]
    fn test_sqf_to_dbvalue_int() {
        use crate::engine::value::DbValue;
        assert_eq!(parse_sqf_to_dbvalue("42").unwrap(), DbValue::Int(42));
        assert_eq!(parse_sqf_to_dbvalue("-1").unwrap(), DbValue::Int(-1));
    }

    #[test]
    fn test_sqf_to_dbvalue_float() {
        use crate::engine::value::DbValue;
        assert_eq!(parse_sqf_to_dbvalue("3.5").unwrap(), DbValue::Float(3.5));
    }

    #[test]
    fn test_sqf_to_dbvalue_string() {
        use crate::engine::value::DbValue;
        assert_eq!(
            parse_sqf_to_dbvalue(r#""hello""#).unwrap(),
            DbValue::String("hello".into())
        );
    }

    #[test]
    fn test_sqf_to_dbvalue_flat_array() {
        use crate::engine::value::DbValue;
        // Mixed-type array flattens to JSON string via json_val_to_dbvalue's catch-all
        let v = parse_sqf_to_dbvalue(r#"[1, "hello", true]"#).unwrap();
        assert!(matches!(v, DbValue::String(_)));
    }
}
