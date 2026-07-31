// SQL preprocessor for a3sql
//
/// Transform scientific notation like `1e2` to `1.0e2` so sqlparser
/// tokenises it as a Float literal instead of a number + identifier.
/// Uses a manual scan since Rust's regex crate doesn't support look-around.
fn fix_scientific_notation(s: &str) -> String {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut result = Vec::with_capacity(n);
    let mut i = 0;

    while i < n {
        // Skip non-digit chars
        if !bytes[i].is_ascii_digit() {
            result.push(bytes[i]);
            i += 1;
            continue;
        }
        // Scan consecutive digits (the mantissa)
        let mant_start = i;
        while i < n && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let mantissa = &bytes[mant_start..i];

        // Check if followed by e/E (scientific notation)
        if i < n && (bytes[i] == b'e' || bytes[i] == b'E') {
            // Boundary check: prev char before mantissa must not be alphanumeric/underscore
            let prev_ok =
                mant_start == 0 || (!bytes[mant_start - 1].is_ascii_alphanumeric() && bytes[mant_start - 1] != b'_');
            if prev_ok {
                // Reject if already has a '.' prefix (already-valid float like 1.5e2)
                let mut j = mant_start;
                while j > 0 && bytes[j - 1].is_ascii_digit() {
                    j -= 1;
                }
                let has_dot = j > 0 && bytes[j - 1] == b'.';
                if !has_dot {
                    // Parse and expand: mantissa × 10^exponent
                    let mant_str = std::str::from_utf8(mantissa).unwrap_or("0");
                    i += 1; // skip e/E
                    let sign = if i < n && bytes[i] == b'-' {
                        i += 1;
                        -1
                    } else {
                        if i < n && bytes[i] == b'+' {
                            i += 1;
                        }
                        1
                    };
                    let exp_start = i;
                    while i < n && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    let exp_str = std::str::from_utf8(&bytes[exp_start..i]).unwrap_or("0");
                    let exp: i32 = exp_str.parse().unwrap_or(0) * sign;
                    let mant: f64 = mant_str.parse().unwrap_or(0.0);
                    let val = mant * 10f64.powi(exp);
                    if val.is_finite() && val.abs() < 1e308 {
                        // For values within reasonable range, emit the expanded form.
                        // Use integer format when exact, float Display otherwise.
                        let s = if val.fract() == 0.0 && val >= i64::MIN as f64 && val <= i64::MAX as f64 {
                            format!("{}", val as i64)
                        } else {
                            format!("{}", val)
                        };
                        // Skip if output is absurdly long (sqlparser can't handle
                        // 300-digit numbers). Fall through to emit original notation.
                        if s.len() < 100 {
                            result.extend_from_slice(s.as_bytes());
                            continue;
                        }
                    }
                    // Overflow or too-long decimal — emit as float literal with
                    // decimal point so sqlparser recognises it: 1e308 → 1.0e308.
                    result.extend_from_slice(mantissa);
                    result.extend_from_slice(b".0e");
                    if sign == -1 {
                        result.push(b'-');
                    }
                    result.extend_from_slice(&bytes[exp_start..i]);
                    continue;
                }
            }
        }
        // Not scientific notation (or boundary/dot rejected) — emit digits as-is
        result.extend_from_slice(mantissa);
        // i already advanced past digits; loop will continue from current position
    }

    String::from_utf8(result).unwrap_or_else(|_| s.to_string())
}
/// Strip SQL comments (`--` line comments and `/* */` block comments) while
/// respecting string literals — a `'--'` inside a quoted string must survive.
/// Real-world SQL (e.g. mods' schema files) is full of `--` comments; without
/// this, the trailing comment on a CREATE TABLE column swallowed the rest of
/// the statement.
fn strip_sql_comments(sql: &str) -> String {
    let bytes = sql.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    let mut in_string = false;
    while i < bytes.len() {
        let b = bytes[i];
        // Toggle string state on unescaped single quotes
        if b == b'\'' {
            // Handle '' escaped quote inside strings
            if in_string && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                out.push(b);
                out.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            in_string = !in_string;
            out.push(b);
            i += 1;
            continue;
        }
        if !in_string {
            // Line comment: -- to end of line
            if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            // Block comment: /* ... */
            if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2; // skip closing */
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| sql.to_string())
}

/// Transforms custom a3sql SQL syntax into standard SQL that sqlparser-rs can
/// parse with GenericDialect:
///
///   `%%` fuzzy match operator → `fuzzy_match()` function call
///     Before: SELECT * FROM t WHERE col %% 'pattern'
///     After:  SELECT * FROM t WHERE fuzzy_match(col,'pattern')
///
/// The entire `left %% right` span is replaced, including the left operand.
/// No duplicate text remains.
///
/// Finds each `%%` operator outside string literals and rewrites
/// `left %% right` → `fuzzy_match(left,right)`.
pub fn preprocess(sql: &str) -> String {
    // Remove comments first so `--` / `/* */` can't swallow following tokens
    let mut result = strip_sql_comments(sql);
    // Fix scientific notation before other transforms
    result = fix_scientific_notation(&result);
    // Map array type suffixes to the bare Custom names — `STRINGS`/`FLOATS`
    // resolve to ColumnType::Strings/Floats in parse_data_type (the `[]`
    // suffix is dropped because sqlparser can't parse `STRING[]`).
    result = result.replace("STRINGS[]", "STRINGS");
    result = result.replace("FLOATS[]", "FLOATS");

    // Map date/time types to STRING (engine stores values as JSON-compatible strings)
    result = result.replace(" DATE)", " STRING)");
    result = result.replace(" DATE,", " STRING,");
    result = result.replace(" TIMESTAMP)", " STRING)");
    result = result.replace(" TIMESTAMP,", " STRING,");
    result = result.replace(" TIMESTAMP ", " STRING ");
    result = result.replace(" DATETIME)", " STRING)");
    result = result.replace(" DATETIME,", " STRING,");
    result = result.replace(" DATETIME ", " STRING ");

    let mut search_start = 0;

    while let Some(abs_pos) = find_unescaped_pct(&result, search_start) {
        // ── Left operand (handles identifiers, function calls, parens) ─
        let before = &result[..abs_pos];
        let before_trimmed = before.trim_end();
        let left_content_end = before_trimmed.len();

        // Scan backward tracking parenthesis depth for function calls
        let left_start = find_left_operand_start(&result[..left_content_end]);

        let left_operand = &result[left_start..left_content_end];
        if left_operand.trim().is_empty() {
            search_start = abs_pos + 2;
            continue;
        }

        // ── Right operand ────────────────────────────────────────────
        let after = &result[abs_pos + 2..];
        let after_trimmed = after.trim_start();
        let right_trim_offset = after.len() - after_trimmed.len();
        let right_abs_start = abs_pos + 2 + right_trim_offset;

        let right_abs_end = if after_trimmed.starts_with('\'') {
            // Quoted string — find closing ', handling \' escapes
            let s = after_trimmed.as_bytes();
            let mut j = 1; // skip opening quote
            while j < s.len() {
                if s[j] == b'\\' {
                    j += 2; // skip escaped char and backslash
                    continue;
                }
                if s[j] == b'\'' {
                    j += 1; // include closing quote
                    break;
                }
                j += 1;
            }
            right_abs_start + j
        } else {
            // Unquoted word (identifier, number)
            let word_len: usize = after_trimmed
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
                .map(char::len_utf8)
                .sum();
            if word_len == 0 {
                search_start = abs_pos + 2;
                continue;
            }
            right_abs_start + word_len
        };

        let right_operand = &result[right_abs_start..right_abs_end];
        if right_operand.is_empty() {
            search_start = abs_pos + 2;
            continue;
        }

        // ── Replace: `left %% right` → `fuzzy_match(left,right)` ─────
        let replacement = format!("fuzzy_match({},{})", left_operand, right_operand);
        result.replace_range(left_start..right_abs_end, &replacement);
        search_start = left_start + replacement.len();
    }

    result
}

/// Find the next `%%` that is NOT inside a single-quoted string literal.
fn find_unescaped_pct(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let n = bytes.len();
    let mut i = start;
    let mut in_string = false;

    while i < n {
        if bytes[i] == b'\'' {
            in_string = !in_string;
        }
        if !in_string && bytes[i] == b'%' && i + 1 < n && bytes[i + 1] == b'%' {
            return Some(i);
        }
        i += 1;
    }

    None
}

/// Scan backward from `end` to find the start of the left operand of `%%`.
/// Handles identifiers, dotted paths, and parenthesized expressions (function calls).
fn find_left_operand_start(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut pos = text.len();
    let mut paren_depth = 0u32;

    while pos > 0 {
        let c = bytes[pos - 1] as char;

        if c == ')' {
            paren_depth += 1;
            pos -= 1;
        } else if c == '(' {
            if paren_depth > 0 {
                paren_depth -= 1;
                pos -= 1;
            } else {
                // Unmatched '(' shouldn't normally happen, but stop here if it does
                break;
            }
        } else if paren_depth > 0 {
            // Inside parentheses — consume any char
            pos -= 1;
        } else if c.is_alphanumeric() || c == '_' || c == '.' {
            pos -= 1;
        } else {
            // Hit a boundary (whitespace, operator, comma, etc.) at depth 0
            break;
        }
    }

    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_sql_unchanged() {
        assert_eq!(preprocess("SELECT * FROM t WHERE x = 1"), "SELECT * FROM t WHERE x = 1");
    }

    #[test]
    fn fuzzy_col_string() {
        assert_eq!(
            preprocess("SELECT * FROM t WHERE col %% 'pattern'"),
            "SELECT * FROM t WHERE fuzzy_match(col,'pattern')"
        );
    }

    #[test]
    fn fuzzy_table_col() {
        assert_eq!(preprocess("WHERE t.col %% 'test'"), "WHERE fuzzy_match(t.col,'test')");
    }

    #[test]
    fn fuzzy_extra_ws() {
        assert_eq!(
            preprocess("WHERE col  %%  'pattern'"),
            "WHERE fuzzy_match(col,'pattern')"
        );
    }

    #[test]
    fn fuzzy_in_expression() {
        assert_eq!(
            preprocess("WHERE a = 1 AND b %% 'x' OR c = 2"),
            "WHERE a = 1 AND fuzzy_match(b,'x') OR c = 2"
        );
    }

    #[test]
    fn modulo_unchanged() {
        assert_eq!(preprocess("SELECT 100 % 30 AS r"), "SELECT 100 % 30 AS r");
    }

    #[test]
    fn string_literal_with_pct() {
        assert_eq!(
            preprocess("SELECT 'hello %% world' AS msg"),
            "SELECT 'hello %% world' AS msg"
        );
    }

    #[test]
    fn multiple_fuzzy() {
        assert_eq!(
            preprocess("WHERE a %% 'x' AND b %% 'y'"),
            "WHERE fuzzy_match(a,'x') AND fuzzy_match(b,'y')"
        );
    }

    #[test]
    fn fuzzy_with_escaped_quote() {
        let sql = "WHERE col %% 'it\\'s test'";
        assert_eq!(preprocess(sql), "WHERE fuzzy_match(col,'it\\'s test')");
    }

    #[test]
    fn string_before_unaltered() {
        assert_eq!(
            preprocess("SELECT 'test' AS t WHERE 1 %% 2"),
            "SELECT 'test' AS t WHERE fuzzy_match(1,2)"
        );
    }

    #[test]
    fn identifier_with_underscores() {
        assert_eq!(preprocess("WHERE my_col %% 'val'"), "WHERE fuzzy_match(my_col,'val')");
    }

    #[test]
    fn fuzzy_function_call_lhs() {
        assert_eq!(
            preprocess("WHERE CONCAT(a,b) %% 'pattern'"),
            "WHERE fuzzy_match(CONCAT(a,b),'pattern')"
        );
    }

    #[test]
    fn fuzzy_nested_function() {
        assert_eq!(
            preprocess("WHERE UPPER(name) %% 'test'"),
            "WHERE fuzzy_match(UPPER(name),'test')"
        );
    }

    #[test]
    fn fuzzy_complex_function() {
        assert_eq!(
            preprocess("WHERE COALESCE(a,b,c) %% 'x'"),
            "WHERE fuzzy_match(COALESCE(a,b,c),'x')"
        );
    }

    #[test]
    fn strings_array_type() {
        assert_eq!(
            preprocess("CREATE TABLE t (id STRING PRIMARY KEY, tags STRINGS[])"),
            "CREATE TABLE t (id STRING PRIMARY KEY, tags STRINGS)"
        );
    }

    #[test]
    fn floats_array_type() {
        assert_eq!(
            preprocess("CREATE TABLE t (id STRING PRIMARY KEY, vals FLOATS[])"),
            "CREATE TABLE t (id STRING PRIMARY KEY, vals FLOATS)"
        );
    }

    #[test]
    fn both_array_types() {
        assert_eq!(
            preprocess("CREATE TABLE t (id STRING PRIMARY KEY, tags STRINGS[], vals FLOATS[])"),
            "CREATE TABLE t (id STRING PRIMARY KEY, tags STRINGS, vals FLOATS)"
        );
    }

    #[test]
    fn date_type() {
        assert_eq!(
            preprocess("CREATE TABLE t (id STRING PRIMARY KEY, d DATE)"),
            "CREATE TABLE t (id STRING PRIMARY KEY, d STRING)"
        );
    }

    #[test]
    fn strips_line_comments() {
        // Real mod SQL is full of `--` comments; a trailing comment on a
        // column definition must not swallow the rest of the statement.
        let r = preprocess("CREATE TABLE t (\n  a TEXT,\n  b INTEGER DEFAULT 0 -- CHECKBOX bool\n);\nSELECT 1");
        assert!(!r.contains("-- CHECKBOX"), "comment stripped: {}", r);
        assert!(r.contains("b INTEGER DEFAULT 0"), "column survives: {}", r);
        assert!(r.contains("SELECT 1"), "next statement survives: {}", r);
    }

    #[test]
    fn preserves_dash_inside_string_literal() {
        // `--` inside a quoted string must NOT be treated as a comment
        let r = preprocess("SELECT '-- not a comment'");
        assert!(r.contains("'-- not a comment'"), "string preserved: {}", r);
    }

    #[test]
    fn strips_block_comments() {
        assert_eq!(preprocess("SELECT 1 /* block */ + 1"), "SELECT 1  + 1");
    }

    #[test]
    fn timestamp_type() {
        assert_eq!(
            preprocess("CREATE TABLE t (id STRING PRIMARY KEY, ts TIMESTAMP)"),
            "CREATE TABLE t (id STRING PRIMARY KEY, ts STRING)"
        );
    }

    #[test]
    fn datetime_type() {
        assert_eq!(
            preprocess("CREATE TABLE t (id STRING PRIMARY KEY, dt DATETIME, ts TIMESTAMP)"),
            "CREATE TABLE t (id STRING PRIMARY KEY, dt STRING, ts STRING)"
        );
    }

    #[test]
    fn date_function_not_mangled() {
        // DATE inside function names should not be affected
        let r = preprocess("SELECT DATE_FORMAT(ts, '%Y') AS y FROM t");
        assert!(r.contains("DATE_FORMAT"), "DATE_FORMAT mangled: {}", r);
    }
}
