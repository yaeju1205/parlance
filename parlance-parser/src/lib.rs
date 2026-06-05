mod tokenizer;

use std::rc::Rc;

use parlance_diagnostics::{Diagnostics, Span};

use crate::tokenizer::{Token, TokenKind, tokenize};

#[derive(Debug)]
pub struct Node<T> {
    pub span: Span,
    pub kind: T,
}

#[derive(Debug)]
pub enum ExpressionKind {
    Variable {
        name: Rc<str>,
    },
    Function {
        params: Vec<Node<Rc<str>>>,
        body: Rc<Expression>,
    },
    Infix {
        operator: Rc<str>,
    },
    FunctionCall {
        callee: Rc<Expression>,
        arg: Rc<Expression>,
    },
    InfixCall {
        operator: Node<Rc<str>>,
        lhs: Rc<Expression>,
        rhs: Rc<Expression>,
    },
    String(Rc<str>),
    Int(i32),
    Group(Rc<Expression>),
}

#[derive(Debug)]
pub struct Expression {
    pub span: Span,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone, Copy)]
pub enum InfinixCombineRule {
    Left,
    Right,
}

#[derive(Debug)]
pub enum StatementKind {
    Variable {
        name: Rc<str>,
        value: Rc<Expression>,
    },
    Function {
        name: Rc<str>,
        params: Vec<Node<Rc<str>>>,
        body: Rc<Expression>,
    },
    Infix {
        combine_rule: InfinixCombineRule,
        operator: Node<Rc<str>>,
        params: Vec<Node<Rc<str>>>,
        body: Rc<Expression>,
    },
}

#[derive(Debug)]
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

    fn expect_token(&mut self, answer: TokenKind) -> Result<Rc<Token>, Diagnostics> {
        let Some(guess_token) = self.next_token() else {
            return Err(Diagnostics::parser_error(
                format!("expected {:?}, found EOF", answer),
                Span {
                    start: 0,
                    end: self.source.len(),
                },
            ));
        };

        if answer.mem_equal(&guess_token.kind) {
            Ok(guess_token)
        } else {
            Err(Diagnostics::parser_error(
                format!("expected {:?}, found {:?}", answer, guess_token.kind),
                guess_token.span.clone(),
            ))
        }
    }

    fn parse_params(&mut self) -> Result<Vec<Node<Rc<str>>>, Diagnostics> {
        let mut params = Vec::new();

        loop {
            let Some(param_token) = self.peek_token() else {
                return Err(Diagnostics::parser_error(
                    "expected '=', found EOF".to_string(),
                    Span {
                        start: 0,
                        end: self.source.len(),
                    },
                ));
            };

            match &param_token.kind {
                TokenKind::Identifier(param) => params.push(Node {
                    kind: param.clone(),
                    span: param_token.span.clone(),
                }),
                _ => break Ok(params),
            }

            self.next_token();
        }
    }

    fn parse_scheme(&mut self) -> Result<Vec<Statement>, Diagnostics> {
        let let_token = match self.peek_token() {
            Some(token) => match &token.kind {
                TokenKind::Let => token,
                _ => return Ok(Vec::new()),
            },
            None => {
                return Err(Diagnostics::parser_error(
                    "expected '=', found EOF".to_string(),
                    Span {
                        start: 0,
                        end: self.source.len(),
                    },
                ));
            }
        };

        self.next_token();
        let mut scheme = Vec::new();

        loop {
            match self.peek_token() {
                Some(token) => match &token.kind {
                    TokenKind::In => {
                        self.next_token();
                        break Ok(scheme);
                    }
                    TokenKind::NewLine => {
                        self.next_token();
                    }
                    _ => scheme.push(self.parse_statement()?),
                },
                None => {
                    return Err(Diagnostics::parser_error(
                        "expected in, found EOF".to_string(),
                        Span {
                            start: let_token.span.start.clone(),
                            end: self.source.len(),
                        },
                    ));
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
            return Err(Diagnostics::parser_error(
                "expected primary expression, found EOF".to_string(),
                Span {
                    start: 0,
                    end: self.source.len(),
                },
            ));
        };

        match &token.kind {
            TokenKind::Identifier(name) => Ok(Expression {
                span: token.span.clone(),
                kind: ExpressionKind::Variable { name: name.clone() },
            }),
            TokenKind::Infix => {
                let Some(symbol) = self.next_token() else {
                    return Err(Diagnostics::parser_error(
                        "expected Symbol, found EOF".to_string(),
                        Span {
                            start: token.span.start.clone(),
                            end: self.source.len(),
                        },
                    ));
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
                    _ => Err(Diagnostics::parser_error(
                        format!("expected Symbol, found {:?}", &symbol.kind),
                        symbol.span.clone(),
                    )),
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
                        body: Rc::new(body),
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
            TokenKind::Int(value) => Ok(Expression {
                span: token.span.clone(),
                kind: ExpressionKind::Int(value.clone()),
            }),
            TokenKind::NewLine => self.parse_primary_expression(),
            _ => Err(Diagnostics::parser_error(
                format!("expect primary expression, found {:?}", &token.kind),
                token.span.clone(),
            )),
        }
    }

    pub fn parse_expression(&mut self) -> Result<Expression, Diagnostics> {
        let mut expr = self.parse_primary_expression()?;
        loop {
            let Some(token) = self.peek_token() else {
                return Ok(expr);
            };

            let kind = &token.kind;

            if matches!(kind, TokenKind::NewLine) {
                return Ok(expr);
            }

            match kind {
                TokenKind::Symbol(symbol) => {
                    self.next_token();
                    let rhs = self.parse_primary_expression()?;
                    expr = Expression {
                        span: Span {
                            start: expr.span.start,
                            end: rhs.span.end,
                        },
                        kind: ExpressionKind::InfixCall {
                            operator: Node {
                                kind: symbol.clone(),
                                span: token.span.clone(),
                            },
                            lhs: Rc::new(expr),
                            rhs: Rc::new(rhs),
                        },
                    };
                }
                TokenKind::Identifier(_) | TokenKind::String(_) | TokenKind::Int(_) => {
                    let arg = self.parse_primary_expression()?;
                    expr = Expression {
                        span: Span {
                            start: expr.span.start.clone(),
                            end: arg.span.end.clone(),
                        },
                        kind: ExpressionKind::FunctionCall {
                            callee: Rc::new(expr),
                            arg: Rc::new(arg),
                        },
                    };
                }
                TokenKind::LeftParen => {
                    self.next_token();
                    let inner = self.parse_expression()?;
                    let right_paren = self.expect_token(TokenKind::RightParen)?;
                    expr = Expression {
                        span: Span {
                            start: expr.span.start.clone(),
                            end: right_paren.span.end.clone(),
                        },
                        kind: ExpressionKind::FunctionCall {
                            callee: Rc::new(expr),
                            arg: Rc::new(inner),
                        },
                    }
                }
                _ => break Ok(expr),
            }
        }
    }

    pub fn parse_statement(&mut self) -> Result<Statement, Diagnostics> {
        let Some(token) = self.next_token() else {
            return Err(Diagnostics::parser_error(
                "expected statement, found EOF".to_string(),
                Span {
                    start: 0,
                    end: self.source.len(),
                },
            ));
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
                            value: Rc::new(value),
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
                            body: Rc::new(value),
                        },
                        scheme,
                    })
                }
            }
            TokenKind::Infix => {
                let Some(comb_token) = self.next_token() else {
                    return Err(Diagnostics::parser_error(
                        "expected 'left' or 'right', found EOF".to_string(),
                        Span {
                            start: token.span.start.clone(),
                            end: self.source.len(),
                        },
                    ));
                };
                let combine_rule = match &comb_token.kind {
                    TokenKind::Identifier(rule) => match rule.as_ref() {
                        "left" => InfinixCombineRule::Left,
                        "right" => InfinixCombineRule::Right,
                        _ => {
                            return Err(Diagnostics::parser_error(
                                format!(
                                    "expected 'left' or 'right', found '{:?}'",
                                    &comb_token.kind
                                ),
                                comb_token.span.clone(),
                            ));
                        }
                    },
                    _ => {
                        return Err(Diagnostics::parser_error(
                            format!("expected 'left' or 'right', found {:?}", &comb_token.kind),
                            comb_token.span.clone(),
                        ));
                    }
                };

                let Some(operator_token) = self.next_token() else {
                    return Err(Diagnostics::parser_error(
                        "expected Symbol, found EOF".to_string(),
                        Span {
                            start: token.span.start.clone(),
                            end: self.source.len(),
                        },
                    ));
                };
                let operator = match &operator_token.kind {
                    TokenKind::Symbol(operator) => Node {
                        kind: operator.clone(),
                        span: operator_token.span.clone(),
                    },
                    _ => {
                        return Err(Diagnostics::parser_error(
                            format!("expected Symbol, found {:?}", &operator_token.kind),
                            operator_token.span.clone(),
                        ));
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
                        body: Rc::new(body),
                    },
                    scheme,
                })
            }
            TokenKind::NewLine => self.parse_statement(),
            _ => Err(Diagnostics::parser_error(
                format!("expected statement, found {:?}", &token.kind),
                token.span.clone(),
            )),
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Statement>, Diagnostics> {
        let mut stats = Vec::new();

        while let Some(token) = self.peek_token() {
            if let TokenKind::NewLine = &token.kind {
                self.next_token();
                continue;
            }
            stats.push(self.parse_statement()?);
        }

        Ok(stats)
    }
}
