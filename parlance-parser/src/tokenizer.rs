use std::{mem, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};

#[derive(Debug, PartialEq)]
pub enum TokenKind {
    Identifier(Rc<str>),
    Symbol(Rc<str>),
    LeftParen,
    RightParen,
    Equal,
    Arrow,
    Lambda,
    Let,
    In,
    Infix,
    NewLine,
    String(Rc<str>),
    Int(i32),
}

impl TokenKind {
    pub fn mem_equal(&self, rhs: &TokenKind) -> bool {
        mem::discriminant(self) == mem::discriminant(rhs)
    }
}

pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn tokenize<'a>(source: &'a str) -> Result<Vec<Token>, Diagnostics> {
    let mut tokens = Vec::new();
    let mut current = 0;

    let mut chars = source.chars().peekable();

    while let Some(&ch) = chars.peek() {
        let start = current;

        if ch.is_whitespace() {
            chars.next();
            current += ch.len_utf8();
            if ch == '\n' {
                tokens.push(Token {
                    kind: TokenKind::NewLine,
                    span: Span {
                        start,
                        end: current,
                    },
                });
            }
            continue;
        }

        if matches!(ch, 'A'..='Z' | 'a'..='z' | '_') {
            while let Some(&next_ch) = chars.peek() {
                if matches!(next_ch, 'A'..='Z' | 'a'..='z' | '_' | ':' | '0'..='9') {
                    chars.next();
                    current += next_ch.len_utf8();
                } else {
                    break;
                }
            }

            let literal = &source[start..current];
            let kind = match literal {
                "let" => TokenKind::Let,
                "in" => TokenKind::In,
                "infix" => TokenKind::Infix,
                _ => TokenKind::Identifier(Rc::from(literal)),
            };

            tokens.push(Token {
                kind,
                span: Span {
                    start,
                    end: current,
                },
            });
            continue;
        }

        if ch == '"' {
            chars.next();
            current += ch.len_utf8();

            loop {
                match chars.next() {
                    Some('"') => {
                        current += '"'.len_utf8();
                        break;
                    }
                    Some(ch2) => {
                        current += ch2.len_utf8();
                    }
                    None => {
                        return Err(Diagnostics::parser_error(
                            "expected '\"', found EOF".to_string(),
                            Span {
                                start,
                                end: current,
                            },
                        ));
                    }
                }
            }

            tokens.push(Token {
                kind: TokenKind::String(Rc::from(&source[start + 1..current - 1])),
                span: Span {
                    start,
                    end: current,
                },
            });
            continue;
        }

        if matches!(ch, '0'..='9') {
            while let Some(&next_ch) = chars.peek() {
                if matches!(next_ch, '0'..='9') {
                    chars.next();
                    current += next_ch.len_utf8();
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: TokenKind::Int(source[start..current].parse().unwrap()),
                span: Span {
                    start,
                    end: current,
                },
            });
            continue;
        }

        if ch == '(' || ch == ')' || ch == '=' || ch == '\\' {
            chars.next();
            current += ch.len_utf8();
            let kind = match ch {
                '(' => TokenKind::LeftParen,
                ')' => TokenKind::RightParen,
                '=' => TokenKind::Equal,
                '\\' => TokenKind::Lambda,
                _ => unreachable!(),
            };
            tokens.push(Token {
                kind,
                span: Span {
                    start,
                    end: current,
                },
            });
            continue;
        }

        while let Some(&next_ch) = chars.peek() {
            if !next_ch.is_whitespace()
                && !matches!(next_ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '(' | ')' | '"')
            {
                chars.next();
                current += next_ch.len_utf8();
            } else {
                break;
            }
        }

        let symbol_str = &source[start..current];
        let kind = match symbol_str {
            "->" => TokenKind::Arrow,
            _ => TokenKind::Symbol(Rc::from(symbol_str)),
        };

        tokens.push(Token {
            kind,
            span: Span {
                start,
                end: current,
            },
        });
    }

    Ok(tokens)
}
