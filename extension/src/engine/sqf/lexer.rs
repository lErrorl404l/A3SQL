// SQF expression lexer — tokenizes SQF expressions for the Pratt parser.
//
// Grammar:
//   number  = ["-"] digit+ ["." digit+]
//   string  = '"' {any | '""'} '"'
//   ident   = "_" ident_char {ident_char}
//   keyword = "true" | "false" | "nil" | "select"
//   op      = "||" | "&&" | "==" | "!=" | "<=" | ">=" | "+" | "-" | "*" | "/" | "%" | "<" | ">" | "!" | "#"

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Token {
    Number(String), // raw numeric string
    String(String), // unescaped string content
    Ident(String),  // _variable
    KeywordTrue,
    KeywordFalse,
    KeywordNil,
    Plus,     // +
    Minus,    // -
    Star,     // *
    Slash,    // /
    Percent,  // %
    Eq,       // ==
    Neq,      // !=
    Lt,       // <
    Gt,       // >
    Le,       // <=
    Ge,       // >=
    And,      // &&
    Or,       // ||
    Not,      // !
    Hash,     // # (array access index)
    Select,   // select keyword (array access)
    LParen,   // (
    RParen,   // )
    LBracket, // [
    RBracket, // ]
    Comma,    // ,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Number(s) => write!(f, "{}", s),
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::Ident(s) => write!(f, "{}", s),
            Token::KeywordTrue => write!(f, "true"),
            Token::KeywordFalse => write!(f, "false"),
            Token::KeywordNil => write!(f, "nil"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Percent => write!(f, "%"),
            Token::Eq => write!(f, "=="),
            Token::Neq => write!(f, "!="),
            Token::Lt => write!(f, "<"),
            Token::Gt => write!(f, ">"),
            Token::Le => write!(f, "<="),
            Token::Ge => write!(f, ">="),
            Token::And => write!(f, "&&"),
            Token::Or => write!(f, "||"),
            Token::Not => write!(f, "!"),
            Token::Hash => write!(f, "#"),
            Token::Select => write!(f, "select"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::Comma => write!(f, ","),
        }
    }
}

/// Split input into tokens. Returns an error on unrecognised input.
pub(crate) fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        // Whitespace
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        // String literal
        if c == '"' {
            chars.next(); // consume opening "
            let mut s = String::new();
            loop {
                match chars.next() {
                    None => return Err("unclosed SQF string literal".into()),
                    Some('"') => {
                        // Check for "" escape
                        if chars.peek() == Some(&'"') {
                            chars.next(); // consume second "
                            s.push('"');
                        } else {
                            break; // closing "
                        }
                    }
                    Some(ch) => s.push(ch),
                }
            }
            tokens.push(Token::String(s));
            continue;
        }

        // Identifier or keyword (_name or bareword)
        if c == '_' || c.is_ascii_alphabetic() {
            let mut ident = String::new();
            ident.push(c);
            chars.next();
            while let Some(&ch) = chars.peek() {
                if ch.is_alphanumeric() || ch == '_' {
                    ident.push(ch);
                    chars.next();
                } else {
                    break;
                }
            }
            match ident.as_str() {
                "true" => tokens.push(Token::KeywordTrue),
                "false" => tokens.push(Token::KeywordFalse),
                "nil" => tokens.push(Token::KeywordNil),
                "select" => tokens.push(Token::Select),
                _ => tokens.push(Token::Ident(ident)),
            }
            continue;
        }

        // Number (no leading -, that's handled by unary operator)
        if c.is_ascii_digit() {
            let mut num = String::new();
            if c == '-' {
                num.push('-');
                chars.next();
            }
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_digit() {
                    num.push(ch);
                    chars.next();
                } else {
                    break;
                }
            }
            if chars.peek() == Some(&'.') {
                num.push('.');
                chars.next();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit() {
                        num.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            tokens.push(Token::Number(num));
            continue;
        }

        // Multi-character operators
        let next = chars
            .next()
            .ok_or_else(|| "unexpected end of input after SQF token".to_string())?;
        match next {
            '|' if chars.peek() == Some(&'|') => {
                chars.next();
                tokens.push(Token::Or);
            }
            '&' if chars.peek() == Some(&'&') => {
                chars.next();
                tokens.push(Token::And);
            }
            '=' if chars.peek() == Some(&'=') => {
                chars.next();
                tokens.push(Token::Eq);
            }
            '!' if chars.peek() == Some(&'=') => {
                chars.next();
                tokens.push(Token::Neq);
            }
            '<' if chars.peek() == Some(&'=') => {
                chars.next();
                tokens.push(Token::Le);
            }
            '>' if chars.peek() == Some(&'=') => {
                chars.next();
                tokens.push(Token::Ge);
            }
            '+' => tokens.push(Token::Plus),
            '-' => tokens.push(Token::Minus),
            '*' => tokens.push(Token::Star),
            '/' => tokens.push(Token::Slash),
            '%' => tokens.push(Token::Percent),
            '<' => tokens.push(Token::Lt),
            '>' => tokens.push(Token::Gt),
            '!' => tokens.push(Token::Not),
            '#' => tokens.push(Token::Hash),
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            '[' => tokens.push(Token::LBracket),
            ']' => tokens.push(Token::RBracket),
            ',' => tokens.push(Token::Comma),
            _ => return Err(format!("unexpected character in SQF expression: {:?}", next)),
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numbers() {
        let t = tokenize("42 -3.14").unwrap();
        assert_eq!(
            t,
            vec![Token::Number("42".into()), Token::Minus, Token::Number("3.14".into())]
        );
    }

    #[test]
    fn test_strings() {
        let t = tokenize(r#""hello" "a""b""#).unwrap();
        assert_eq!(t, vec![Token::String("hello".into()), Token::String("a\"b".into())]);
    }

    #[test]
    fn test_idents() {
        let t = tokenize("_x _myVar").unwrap();
        assert_eq!(t, vec![Token::Ident("_x".into()), Token::Ident("_myVar".into())]);
    }

    #[test]
    fn test_keywords() {
        let t = tokenize("true false nil select").unwrap();
        assert_eq!(
            t,
            vec![
                Token::KeywordTrue,
                Token::KeywordFalse,
                Token::KeywordNil,
                Token::Select,
            ]
        );
    }

    #[test]
    fn test_operators() {
        let t = tokenize("+ - * / % == != < > <= >= && || ! #").unwrap();
        assert_eq!(
            t,
            vec![
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::Percent,
                Token::Eq,
                Token::Neq,
                Token::Lt,
                Token::Gt,
                Token::Le,
                Token::Ge,
                Token::And,
                Token::Or,
                Token::Not,
                Token::Hash,
            ]
        );
    }

    #[test]
    fn test_parens_braces() {
        let t = tokenize("( ) [ ] ,").unwrap();
        assert_eq!(
            t,
            vec![
                Token::LParen,
                Token::RParen,
                Token::LBracket,
                Token::RBracket,
                Token::Comma,
            ]
        );
    }

    #[test]
    fn test_complex_expr() {
        let t = tokenize(r#"_x > 0 && _y < 100 || _name == "hello""#).unwrap();
        assert_eq!(
            t,
            vec![
                Token::Ident("_x".into()),
                Token::Gt,
                Token::Number("0".into()),
                Token::And,
                Token::Ident("_y".into()),
                Token::Lt,
                Token::Number("100".into()),
                Token::Or,
                Token::Ident("_name".into()),
                Token::Eq,
                Token::String("hello".into()),
            ]
        );
    }

    #[test]
    fn test_error_unclosed_string() {
        assert!(tokenize("\"unclosed").is_err());
    }

    #[test]
    fn test_error_unexpected_char() {
        assert!(tokenize("hello @world").is_err());
    }
}
