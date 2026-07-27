// SQF expression parser — Pratt parser producing an AST for the evaluator.
//
// Precedence (lowest to highest):
//   1  ||        logical or
//   2  &&        logical and
//   3  == != < > <= >=   comparison
//   4  + -       addition, subtraction, string concat
//   5  * / %     multiplication, division, modulo
//   6  - !       unary prefix
//   7  select #  array access (postfix)
//   8  literals, variables, parens/brackets

use super::lexer::Token;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    Variable(String),
    Command(String, Vec<Expr>), // SQF command with its argument expressions
    Binary(Op, Box<Expr>, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UnaryOp {
    Neg,
    Not,
}

fn precedence(op: &Token) -> u8 {
    match op {
        Token::Or => 1,
        Token::And => 2,
        Token::Eq | Token::Neq | Token::Lt | Token::Gt | Token::Le | Token::Ge => 3,
        Token::Plus | Token::Minus => 4,
        Token::Star | Token::Slash | Token::Percent => 5,
        _ => 0,
    }
}

fn is_binary_op(t: &Token) -> bool {
    matches!(
        t,
        Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::Eq
            | Token::Neq
            | Token::Lt
            | Token::Gt
            | Token::Le
            | Token::Ge
            | Token::And
            | Token::Or
    )
}

fn op_from_token(t: &Token) -> Option<Op> {
    match t {
        Token::Plus => Some(Op::Add),
        Token::Minus => Some(Op::Sub),
        Token::Star => Some(Op::Mul),
        Token::Slash => Some(Op::Div),
        Token::Percent => Some(Op::Mod),
        Token::Eq => Some(Op::Eq),
        Token::Neq => Some(Op::Neq),
        Token::Lt => Some(Op::Lt),
        Token::Gt => Some(Op::Gt),
        Token::Le => Some(Op::Le),
        Token::Ge => Some(Op::Ge),
        Token::And => Some(Op::And),
        Token::Or => Some(Op::Or),
        _ => None,
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let t = self.tokens.get(self.pos);
        self.pos += 1;
        t
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        match self.peek() {
            Some(t) if t == expected => {
                self.pos += 1;
                Ok(())
            }
            Some(t) => Err(format!("expected {}, got {}", expected, t)),
            None => Err(format!("expected {}, got end of input", expected)),
        }
    }

    /// Parse the full expression.
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_bp(0)
    }

    /// Pratt binding-power parser.
    fn parse_bp(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.parse_prefix()?;

        // Binary operators — includes both symbolic operators (+, &&, ==, etc.)
        // and SQF binary commands used in infix position (e.g. `3 min 7`).
        while let Some(op) = self.peek() {
            let (is_infix_binop, prec) = if is_binary_cmd(op) {
                // SQF binary commands bind at the lowest precedence
                (true, 0u8)
            } else if is_binary_op(op) {
                (true, precedence(op))
            } else {
                (false, 0)
            };
            if !is_infix_binop || prec < min_prec {
                break;
            }
            let op_token = self.advance().unwrap().clone();
            if let Token::Ident(name) = &op_token {
                // SQF binary command: lhs <command> rhs
                let rhs = self.parse_bp(0)?;
                lhs = Expr::Command(name.to_ascii_lowercase(), vec![lhs, rhs]);
            } else {
                // Symbolic operator
                let rhs = self.parse_bp(prec + 1)?;
                lhs = Expr::Binary(op_from_token(&op_token).unwrap(), Box::new(lhs), Box::new(rhs));
            }
        }

        Ok(lhs)
    }

    /// Parse a prefix expression (literal, variable, unary, parens).
    fn parse_prefix(&mut self) -> Result<Expr, String> {
        match self.advance() {
            None => Err("unexpected end of SQF expression".to_string()),
            Some(Token::Number(s)) => {
                if s.contains('.') {
                    s.parse::<f64>()
                        .map(Expr::Float)
                        .map_err(|e| format!("bad number: {}", e))
                } else {
                    s.parse::<i64>()
                        .map(Expr::Int)
                        .map_err(|e| format!("bad number: {}", e))
                }
            }
            Some(Token::String(s)) => Ok(Expr::String(s.clone())),
            Some(Token::Ident(raw)) => {
                let name = raw.clone();
                if name.starts_with('_') {
                    Ok(Expr::Variable(name))
                } else if let Some(arity) = super::database::lookup(&name.to_ascii_lowercase()) {
                    // Store lowercased so eval can match case-insensitively
                    let lower = name.to_ascii_lowercase();
                    match arity {
                        super::database::Arity::Nular => Ok(Expr::Command(lower, vec![])),
                        super::database::Arity::Unary => {
                            let arg = self.parse_bp(7)?;
                            Ok(Expr::Command(lower, vec![arg]))
                        }
                        super::database::Arity::Binary => {
                            // Binary commands need a left operand — can't appear at expression start
                            Err(format!(
                                "binary command '{}' used without left operand at expression start",
                                lower
                            ))
                        }
                    }
                } else {
                    Ok(Expr::Variable(name))
                }
            }
            Some(Token::KeywordTrue) => Ok(Expr::Bool(true)),
            Some(Token::KeywordFalse) => Ok(Expr::Bool(false)),
            Some(Token::KeywordNil) => Ok(Expr::Null),
            Some(Token::Minus) => {
                let expr = self.parse_bp(6)?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(expr)))
            }
            Some(Token::Not) => {
                let expr = self.parse_bp(6)?;
                Ok(Expr::Unary(UnaryOp::Not, Box::new(expr)))
            }
            Some(Token::LParen) => {
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Some(t) => Err(format!("unexpected token in SQF expression: {}", t)),
        }
    }
}

/// Check if a token is a known binary command name.
fn is_binary_cmd(t: &Token) -> bool {
    if let Token::Ident(name) = t {
        super::database::lookup(name) == Some(super::database::Arity::Binary)
    } else {
        false
    }
}

/// Parse an SQF expression string into an AST.
pub(crate) fn parse(tokens: Vec<Token>) -> Result<Expr, String> {
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr()?;
    if parser.peek().is_some() {
        return Err(format!(
            "trailing tokens in SQF expression: {}",
            parser.tokens[parser.pos..]
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        ));
    }
    Ok(expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sqf::lexer::tokenize;

    fn parse_expr(input: &str) -> Result<Expr, String> {
        let tokens = tokenize(input)?;
        parse(tokens)
    }

    #[test]
    fn test_int_literal() {
        assert_eq!(parse_expr("42").unwrap(), Expr::Int(42));
    }

    #[test]
    fn test_float_literal() {
        assert_eq!(parse_expr("3.14").unwrap(), Expr::Float(3.14));
    }

    #[test]
    fn test_string_literal() {
        assert_eq!(parse_expr(r#""hello""#).unwrap(), Expr::String("hello".into()));
    }

    #[test]
    fn test_bool_literal() {
        assert_eq!(parse_expr("true").unwrap(), Expr::Bool(true));
        assert_eq!(parse_expr("false").unwrap(), Expr::Bool(false));
    }

    #[test]
    fn test_nil() {
        assert_eq!(parse_expr("nil").unwrap(), Expr::Null);
    }

    #[test]
    fn test_variable() {
        assert_eq!(parse_expr("_x").unwrap(), Expr::Variable("_x".into()));
    }

    #[test]
    fn test_binary_add() {
        let e = parse_expr("1 + 2").unwrap();
        assert_eq!(e, Expr::Binary(Op::Add, Box::new(Expr::Int(1)), Box::new(Expr::Int(2))));
    }

    #[test]
    fn test_precedence() {
        let e = parse_expr("1 + 2 * 3").unwrap();
        // 1 + (2 * 3)
        assert_eq!(
            e,
            Expr::Binary(
                Op::Add,
                Box::new(Expr::Int(1)),
                Box::new(Expr::Binary(Op::Mul, Box::new(Expr::Int(2)), Box::new(Expr::Int(3)))),
            )
        );
    }

    #[test]
    fn test_parens() {
        let e = parse_expr("(1 + 2) * 3").unwrap();
        assert_eq!(
            e,
            Expr::Binary(
                Op::Mul,
                Box::new(Expr::Binary(Op::Add, Box::new(Expr::Int(1)), Box::new(Expr::Int(2)))),
                Box::new(Expr::Int(3)),
            )
        );
    }

    #[test]
    fn test_unary_neg() {
        let e = parse_expr("-5").unwrap();
        assert_eq!(e, Expr::Unary(UnaryOp::Neg, Box::new(Expr::Int(5))));
    }

    #[test]
    fn test_unary_not() {
        let e = parse_expr("!true").unwrap();
        assert_eq!(e, Expr::Unary(UnaryOp::Not, Box::new(Expr::Bool(true))));
    }

    #[test]
    fn test_comparison() {
        let e = parse_expr("_x > 0 && _y < 100").unwrap();
        assert_eq!(
            e,
            Expr::Binary(
                Op::And,
                Box::new(Expr::Binary(
                    Op::Gt,
                    Box::new(Expr::Variable("_x".into())),
                    Box::new(Expr::Int(0))
                )),
                Box::new(Expr::Binary(
                    Op::Lt,
                    Box::new(Expr::Variable("_y".into())),
                    Box::new(Expr::Int(100))
                )),
            )
        );
    }

    #[test]
    fn test_associativity() {
        let e = parse_expr("1 - 2 - 3").unwrap();
        // Left-assoc: (1 - 2) - 3
        assert_eq!(
            e,
            Expr::Binary(
                Op::Sub,
                Box::new(Expr::Binary(Op::Sub, Box::new(Expr::Int(1)), Box::new(Expr::Int(2)))),
                Box::new(Expr::Int(3)),
            )
        );
    }

    #[test]
    fn test_binary_command_infix() {
        // "3 min 7" should parse as Command("min", [Int(3), Int(7)])
        let e = parse_expr("3 min 7").unwrap();
        assert_eq!(e, Expr::Command("min".into(), vec![Expr::Int(3), Expr::Int(7)]));
    }

    #[test]
    fn test_binary_command_with_arith() {
        // "3 + 4 min 7" should parse as min((3+4), 7) since binary commands bind lower
        let e = parse_expr("3 + 4 min 7").unwrap();
        assert_eq!(
            e,
            Expr::Command(
                "min".into(),
                vec![
                    Expr::Binary(Op::Add, Box::new(Expr::Int(3)), Box::new(Expr::Int(4))),
                    Expr::Int(7),
                ]
            )
        );
    }

    #[test]
    fn test_error_empty() {
        assert!(parse_expr("").is_err());
    }

    #[test]
    fn test_error_trailing() {
        assert!(parse_expr("1 2").is_err());
    }
}
