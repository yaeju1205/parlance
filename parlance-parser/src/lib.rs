mod tokenizer;

use std::rc::Rc;

use parlance_diagnostics::{Diagnostics, Severity, Span};

use crate::tokenizer::{Token, TokenKind, tokenize};

pub enum ExpressionKind {
    Variable {
        name: Rc<str>,
    },
    Function {
        params: Vec<Rc<str>>,
        body: Box<Expression>,
    },
    Infix {
        operator: Rc<str>,
    },
    FunctionCall {
        callee: Box<Expression>,
        arg: Box<Expression>,
    },
    InfixCall {
        operator: Rc<str>,
        lhs: Box<Expression>,
        rhs: Box<Expression>,
    },
    String(Rc<str>),
}

pub struct Expression {
    pub span: Span,
    pub kind: ExpressionKind,
}

pub enum InfinixCombineRule {
    Left,
    Right,
}

pub enum StatementKind {
    Variable {
        name: Rc<str>,
        value: Expression,
    },
    Function {
        name: Rc<str>,
        params: Vec<Rc<str>>,
        body: Expression,
    },
    Infix {
        combine_rule: InfinixCombineRule,
        operator: Rc<str>,
        params: Vec<Rc<str>>,
        body: Expression,
    },
}

pub struct Statement {
    pub span: Span,
    pub kind: StatementKind,
    pub scheme: Vec<Statement>,
}

pub struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Rc<Token>>,
    token_index: usize,
}

impl<'a> Parser<'a> {
    fn peek_token(&mut self) -> Option<Rc<Token>> {
        self.tokens.get(self.token_index).cloned()
    }

    fn next_token(&mut self) -> Option<Rc<Token>> {
        let token = self.peek_token();
        self.token_index += 1;
        token
    }

    fn expect_token(&mut self, expect_token: TokenKind) -> Result<Rc<Token>, Diagnostics> {
        let Some(guess_token) = self.next_token() else {
            return Err(Diagnostics {
                message: format!("expected {:?}, found EOF", expect_token),
                severity: Severity::Error,
                span: Span {
                    start: 0,
                    end: self.source.len(),
                },
            });
        };

        if expect_token.mem_equal(&guess_token.kind) {
            Err(Diagnostics {
                message: format!("expected {:?}, found {:?}", expect_token, guess_token.kind),
                severity: Severity::Error,
                span: guess_token.span.clone(),
            })
        } else {
            Ok(guess_token)
        }
    }

    fn parse_params(&mut self) -> Result<Vec<Rc<str>>, Diagnostics> {
        let mut params = Vec::new();

        loop {
            let Some(param_token) = self.next_token() else {
                return Err(Diagnostics {
                    message: "expected '=', found EOF".to_string(),
                    severity: Severity::Error,
                    span: Span {
                        start: 0,
                        end: self.source.len(),
                    },
                });
            };

            match &param_token.kind {
                TokenKind::Identifier(param) => params.push(param.clone()),
                _ => break Ok(params),
            }
        }
    }

    fn parse_scheme(&mut self) -> Result<Vec<Statement>, Diagnostics> {
        match self.peek_token() {
            Some(token) => match &token.kind {
                TokenKind::Let => {}
                _ => return Ok(Vec::new()),
            },
            None => {
                return Err(Diagnostics {
                    message: "expected statement, found EOF".to_string(),
                    severity: Severity::Error,
                    span: Span {
                        start: 0,
                        end: self.source.len(),
                    },
                });
            }
        }

        let mut scheme = Vec::new();

        loop {
            match self.peek_token() {
                Some(token) => match &token.kind {
                    TokenKind::In => {
                        self.next_token();
                        break Ok(scheme);
                    }
                    _ => scheme.push(self.parse_statement()?),
                },
                None => {
                    return Err(Diagnostics {
                        message: "expected in, found EOF".to_string(),
                        severity: Severity::Error,
                        span: Span {
                            start: 0,
                            end: self.source.len(),
                        },
                    });
                }
            }
        }
    }
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Result<Self, Diagnostics> {
        Ok(Self {
            source,
            tokens: tokenize(source)?.into_iter().map(Rc::new).collect(),
            token_index: 0,
        })
    }

    pub fn parse_primary_expression(&mut self) -> Result<Expression, Diagnostics> {
        let Some(token) = self.next_token() else {
            return Err(Diagnostics {
                message: "expected primary expression, found EOF".to_string(),
                severity: Severity::Error,
                span: Span {
                    start: 0,
                    end: self.source.len(),
                },
            });
        };

        match &token.kind {
            TokenKind::Identifier(name) => Ok(Expression {
                span: token.span.clone(),
                kind: ExpressionKind::Variable { name: name.clone() },
            }),
            TokenKind::Infix => {
                let Some(symbol) = self.next_token() else {
                    return Err(Diagnostics {
                        message: "expected Symbol, found EOF".to_string(),
                        severity: Severity::Error,
                        span: Span {
                            start: token.span.start.clone(),
                            end: self.source.len(),
                        },
                    });
                };

                match &symbol.kind {
                    TokenKind::Symbol(operator) => Ok(Expression {
                        span: Span {
                            start: token.span.start.clone(),
                            end: symbol.span.end.clone(),
                        },
                        kind: ExpressionKind::Infix {
                            operator: operator.clone(),
                        },
                    }),
                    _ => Err(Diagnostics {
                        message: format!("expected Symbol, found {:?}", &symbol.kind),
                        severity: Severity::Error,
                        span: symbol.span.clone(),
                    }),
                }
            }
            TokenKind::Lambda => {
                let params = self.parse_params()?;
                self.expect_token(TokenKind::Arrow)?;
                let body = self.parse_expression()?;
                Ok(Expression {
                    span: Span {
                        start: token.span.start.clone(),
                        end: body.span.end.clone(),
                    },
                    kind: ExpressionKind::Function {
                        params,
                        body: Box::new(body),
                    },
                })
            }
            TokenKind::LeftParen => {
                let inner = self.parse_expression()?;
                self.expect_token(TokenKind::RightParen)?;
                Ok(inner)
            }
            TokenKind::String(value) => Ok(Expression {
                span: token.span.clone(),
                kind: ExpressionKind::String(value.to_owned()),
            }),
            TokenKind::NewLine => self.parse_primary_expression(),
            _ => Err(Diagnostics {
                message: format!(
                    "expect primary expression, found {}",
                    &self.source[token.span.start..token.span.end]
                ),
                severity: Severity::Error,
                span: token.span.clone(),
            }),
        }
    }

    pub fn parse_expression(&mut self) -> Result<Expression, Diagnostics> {
        let mut expr = self.parse_primary_expression()?;
        loop {
            let Some(token) = self.next_token() else {
                return Ok(expr);
            };
            let kind = &token.kind;

            if matches!(kind, TokenKind::NewLine) {
                return Ok(expr);
            }

            match kind {
                TokenKind::Symbol(symbol) => {
                    let rhs = self.parse_primary_expression()?;
                    expr = Expression {
                        span: Span {
                            start: expr.span.start,
                            end: rhs.span.end,
                        },
                        kind: ExpressionKind::InfixCall {
                            operator: symbol.clone(),
                            lhs: Box::new(expr),
                            rhs: Box::new(rhs),
                        },
                    };
                }
                TokenKind::Identifier(_) | TokenKind::String(_) => {
                    let arg = self.parse_expression()?;
                    expr = Expression {
                        span: Span {
                            start: expr.span.start.clone(),
                            end: arg.span.end.clone(),
                        },
                        kind: ExpressionKind::FunctionCall {
                            callee: Box::new(expr),
                            arg: Box::new(arg),
                        },
                    };
                }
                _ => {
                    return Err(Diagnostics {
                        message: format!("exepect expression, found {:?}", kind),
                        severity: Severity::Error,
                        span: token.span.clone(),
                    });
                }
            }
        }
    }

    pub fn parse_statement(&mut self) -> Result<Statement, Diagnostics> {
        let Some(token) = self.next_token() else {
            return Err(Diagnostics {
                message: "expected statement, found EOF".to_string(),
                severity: Severity::Error,
                span: Span {
                    start: 0,
                    end: self.source.len(),
                },
            });
        };

        match &token.kind {
            TokenKind::Identifier(name) => {
                let params = self.parse_params()?;
                self.expect_token(TokenKind::Equal)?;
                let scheme = self.parse_scheme()?;
                let value = self.parse_expression()?;
                if params.is_empty() {
                    Ok(Statement {
                        span: Span {
                            start: token.span.start.clone(),
                            end: value.span.end.clone(),
                        },
                        kind: StatementKind::Variable {
                            name: name.clone(),
                            value,
                        },
                        scheme,
                    })
                } else {
                    Ok(Statement {
                        span: Span {
                            start: token.span.start.clone(),
                            end: value.span.end.clone(),
                        },
                        kind: StatementKind::Function {
                            name: name.clone(),
                            params,
                            body: value,
                        },
                        scheme,
                    })
                }
            }
            TokenKind::Infix => {
                let Some(comb_token) = self.next_token() else {
                    return Err(Diagnostics {
                        message: "expected 'left' or 'right', found EOF".to_string(),
                        severity: Severity::Error,
                        span: Span {
                            start: token.span.start.clone(),
                            end: self.source.len(),
                        },
                    });
                };
                let combine_rule = match &comb_token.kind {
                    TokenKind::Identifier(rule) => match rule.as_ref() {
                        "left" => InfinixCombineRule::Left,
                        "right" => InfinixCombineRule::Right,
                        _ => {
                            return Err(Diagnostics {
                                message: format!(
                                    "expected 'left' or 'right', found '{:?}'",
                                    &comb_token.kind
                                ),
                                severity: Severity::Error,
                                span: comb_token.span.clone(),
                            });
                        }
                    },
                    _ => {
                        return Err(Diagnostics {
                            message: format!(
                                "expected 'left' or 'right', found {:?}",
                                &comb_token.kind
                            ),
                            severity: Severity::Error,
                            span: comb_token.span.clone(),
                        });
                    }
                };

                let Some(operator_token) = self.next_token() else {
                    return Err(Diagnostics {
                        message: "expected Symbol, found EOF".to_string(),
                        severity: Severity::Error,
                        span: Span {
                            start: token.span.start.clone(),
                            end: self.source.len(),
                        },
                    });
                };
                let operator = match &operator_token.kind {
                    TokenKind::Symbol(operator) => operator.clone(),
                    _ => {
                        return Err(Diagnostics {
                            message: format!("expected Symbol, found {:?}", &operator_token.kind),
                            severity: Severity::Error,
                            span: operator_token.span.clone(),
                        });
                    }
                };

                let params = self.parse_params()?;
                self.expect_token(TokenKind::Equal)?;
                let scheme = self.parse_scheme()?;
                let body = self.parse_expression()?;

                Ok(Statement {
                    span: Span {
                        start: token.span.start.clone(),
                        end: body.span.end.clone(),
                    },
                    kind: StatementKind::Infix {
                        combine_rule,
                        operator,
                        params,
                        body,
                    },
                    scheme,
                })
            }
            TokenKind::NewLine => self.parse_statement(),
            _ => Err(Diagnostics {
                message: format!("expected statement, found {:?}", &token.kind),
                severity: Severity::Error,
                span: Span {
                    start: 0,
                    end: self.source.len(),
                },
            }),
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Statement>, Diagnostics> {
        let mut stats = Vec::new();

        while let Some(_) = self.peek_token() {
            stats.push(self.parse_statement()?);
        }

        Ok(stats)
    }
}
