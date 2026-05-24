use std::{mem, rc::Rc};

use parlance_diagnostics::{Diagnostics, Severity, Span};

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
    let mut chars = source.chars();

    while let Some(ch) = chars.next() {
        let start = current;
        current += 1;

        if ch.is_whitespace() {
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

        match ch {
            'A'..='Z' | 'a'..='z' | '_' => {
                loop {
                    if !matches!(
                        chars.next(),
                        Some('A'..='Z' | 'a'..='z' | '_' | ':' | '0'..='9')
                    ) {
                        break;
                    }
                    current += 1;
                }
                tokens.push(Token {
                    kind: match &source[start..current] {
                        "let" => TokenKind::Let,
                        "in" => TokenKind::In,
                        "infix" => TokenKind::Infix,
                        _ => TokenKind::Identifier(Rc::from(&source[start..current])),
                    },
                    span: Span {
                        start,
                        end: current,
                    },
                });
            }
            '"' => {
                loop {
                    let Some(ch2) = chars.next() else {
                        return Err(Diagnostics {
                            message: "expected '\"', found EOF".to_string(),
                            severity: Severity::Error,
                            span: Span {
                                start,
                                end: current,
                            },
                        });
                    };
                    current += ch2.len_utf8();

                    if ch2 == '"' {
                        break;
                    }
                }
                tokens.push(Token {
                    kind: TokenKind::String(Rc::from(&source[start + 1..current - 1])),
                    span: Span {
                        start,
                        end: current,
                    },
                });
            }
            '(' => tokens.push(Token {
                kind: TokenKind::LeftParen,
                span: Span {
                    start,
                    end: current,
                },
            }),
            ')' => tokens.push(Token {
                kind: TokenKind::RightParen,
                span: Span {
                    start,
                    end: current,
                },
            }),
            '=' => tokens.push(Token {
                kind: TokenKind::Equal,
                span: Span {
                    start,
                    end: current,
                },
            }),
            '\\' => tokens.push(Token {
                kind: TokenKind::Lambda,
                span: Span {
                    start,
                    end: current,
                },
            }),
            _ => {
                loop {
                    if matches!(ch, 'A'..='Z' | 'a'..='z') {
                        break;
                    }
                    current += ch.len_utf8();
                }
                tokens.push(Token {
                    kind: match &source[start..current] {
                        "->" => TokenKind::Arrow,
                        _ => TokenKind::Symbol(Rc::from(&source[start..current])),
                    },
                    span: Span {
                        start,
                        end: current,
                    },
                });
            }
        }
    }

    Ok(tokens)
}
