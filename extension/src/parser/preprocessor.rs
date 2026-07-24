// SQL preprocessor for a3db
//
/// Transforms custom a3db SQL syntax into standard SQL that sqlparser-rs can
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
    let mut result = sql.to_string();
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
}
