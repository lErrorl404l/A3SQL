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
        if i + 2 < n
            && bytes[i].is_ascii_digit()
            && (bytes[i + 1] == b'e' || bytes[i + 1] == b'E')
            && (bytes[i + 2].is_ascii_digit() || bytes[i + 2] == b'+' || bytes[i + 2] == b'-')
        {
            let prev_ok = i == 0 || (!bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_');
            if !prev_ok {
                result.push(bytes[i]);
                i += 1;
                continue;
            }
            // Skip if already has '.' prefix (1.5e2 already valid)
            let mut j = i;
            while j > 0 && bytes[j - 1].is_ascii_digit() {
                j -= 1;
            }
            if j > 0 && bytes[j - 1] == b'.' {
                result.push(bytes[i]);
                i += 1;
                continue;
            }
            // Parse mantissa
            let mant_start = i;
            while i < n && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let mant_str = std::str::from_utf8(&bytes[mant_start..i]).unwrap_or("0");
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
            // Expand to decimal string
            let mant: f64 = mant_str.parse().unwrap_or(0.0);
            let expanded = mant * 10f64.powi(exp);
            // Format without unnecessary trailing zeros
            if expanded.fract() == 0.0 && expanded.is_finite() {
                result.extend_from_slice(format!("{}", expanded as i64).as_bytes());
            } else {
                result.extend_from_slice(format!("{}", expanded).as_bytes());
            }
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }

    String::from_utf8(result).unwrap_or_else(|_| s.to_string())
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
    // Fix scientific notation before other transforms
    let mut result = fix_scientific_notation(sql);
    result = result.replace("STRINGS[]", "STRING");
    result = result.replace("FLOATS[]", "FLOAT");

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
            "CREATE TABLE t (id STRING PRIMARY KEY, tags STRING)"
        );
    }

    #[test]
    fn floats_array_type() {
        assert_eq!(
            preprocess("CREATE TABLE t (id STRING PRIMARY KEY, vals FLOATS[])"),
            "CREATE TABLE t (id STRING PRIMARY KEY, vals FLOAT)"
        );
    }

    #[test]
    fn both_array_types() {
        assert_eq!(
            preprocess("CREATE TABLE t (id STRING PRIMARY KEY, tags STRINGS[], vals FLOATS[])"),
            "CREATE TABLE t (id STRING PRIMARY KEY, tags STRING, vals FLOAT)"
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
