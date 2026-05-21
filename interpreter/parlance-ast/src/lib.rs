use parlance_diagnostics::{Diagnostics, Severity, Span};

#[derive(Debug)]
pub enum Expression<'a> {
    Variable {
        name: &'a str,
    },
    Function {
        params: Vec<&'a str>,
        body: Box<ExpressionNode<'a>>,
    },
    Group {
        inner: Box<ExpressionNode<'a>>,
    },
    Integer(i16),
    String(&'a str),
    Call {
        callee: Box<ExpressionNode<'a>>,
        arg: Box<ExpressionNode<'a>>,
    },
}

#[derive(Debug)]
pub struct ExpressionNode<'a> {
    pub kind: Expression<'a>,
    pub span: Span,
}

#[derive(Debug)]
pub enum Statement<'a> {
    Function {
        name: &'a str,
        params: Vec<&'a str>,
        body: ExpressionNode<'a>,
        where_clause: Vec<StatementNode<'a>>,
    },
    Variable {
        name: &'a str,
        value: ExpressionNode<'a>,
        where_clause: Vec<StatementNode<'a>>,
    },
}

#[derive(Debug)]
pub struct StatementNode<'a> {
    pub kind: Statement<'a>,
    pub span: Span,
}

pub struct Parser<'a> {
    source: &'a str,
    current: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.source[self.current..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.current += ch.len_utf8();
        Some(ch)
    }

    fn fast_advance(&mut self) {
        if let Some(ch) = self.peek() {
            self.current += ch.len_utf8();
        }
    }

    fn expect(&mut self, target: char) -> Result<(), Diagnostics> {
        self.skip_whitespace();
        let start = self.current;
        if let Some(ch) = self.advance() {
            if ch == target {
                Ok(())
            } else {
                Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: format!("expect '{}', got '{}'", target, ch),
                })
            }
        } else {
            Err(Diagnostics {
                severity: Severity::Error,
                span: Span {
                    start,
                    end: self.current,
                },
                message: format!("expect '{}', got EOF", target),
            })
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }
}

macro_rules! identifier_start {
    () => {
        'a'..='z' | 'A'..='Z' | '_'
    };
}

macro_rules! identifier_continue {
    () => {
        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | ':'
    };
}

macro_rules! integer_start {
    () => {
        '0'..='9'
    };
}

impl<'a> Parser<'a> {
    fn parse_identifier(&mut self) -> Result<&'a str, Diagnostics> {
        let start = self.current;
        if let Some(ch) = self.peek() {
            if matches!(ch, identifier_start!()) {
                self.fast_advance();
                while let Some(ch) = self.peek() {
                    if matches!(ch, identifier_continue!()) {
                        self.fast_advance();
                    } else {
                        break;
                    }
                }
                Ok(&self.source[start..self.current])
            } else {
                Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: format!("expect identifier, got {}", ch),
                })
            }
        } else {
            Err(Diagnostics {
                severity: Severity::Error,
                span: Span {
                    start,
                    end: self.current,
                },
                message: String::from("expect identifier, got EOR"),
            })
        }
    }

    fn parse_string(&mut self) -> Result<&'a str, Diagnostics> {
        self.expect('"')?;
        let start = self.current;
        while let Some(ch) = self.peek() {
            if ch != '"' {
                self.fast_advance();
            } else {
                break;
            }
        }
        self.expect('"')?;
        Ok(&self.source[start..self.current - 1])
    }

    fn parse_integer(&mut self) -> Result<i16, Diagnostics> {
        let start = self.current;
        self.skip_whitespace();

        while let Some(int_ch) = self.peek() {
            if matches!(int_ch, integer_start!()) {
                self.fast_advance();
            } else if matches!(int_ch, identifier_start!()) {
                return Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current + 1,
                    },
                    message: format!("expect 0~9, got {int_ch}"),
                });
            } else {
                break;
            }
        }

        if self.current == start {
            return Err(Diagnostics {
                severity: Severity::Error,
                span: Span {
                    start,
                    end: self.current + 1,
                },
                message: format!("expect 0~9, got EOF"),
            });
        }

        let num_str = &self.source[start..self.current];
        match num_str.parse::<i16>() {
            Ok(num) => Ok(num),
            Err(_) => Err(Diagnostics {
                severity: Severity::Error,
                span: Span {
                    start,
                    end: self.current,
                },
                message: format!("Overflow integer: {num_str}"),
            }),
        }
    }

    fn parse_params(&mut self) -> Result<Vec<&'a str>, Diagnostics> {
        let start = self.current;
        let mut params: Vec<&'a str> = Vec::new();
        loop {
            self.skip_whitespace();
            if let Some(ch) = self.peek() {
                if matches!(ch, identifier_continue!()) {
                    params.push(self.parse_identifier()?);
                } else {
                    break Ok(params);
                }
            } else {
                Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: String::from("expect params, got EOF"),
                })?
            }
        }
    }

    fn parse_bracket_block(&mut self) -> Result<Vec<StatementNode<'a>>, Diagnostics> {
        let start = self.current;
        self.expect('{')?;
        let mut block = Vec::new();
        loop {
            self.skip_whitespace();
            if let Some(ch) = self.peek() {
                if ch == '}' {
                    self.fast_advance();
                    break Ok(block);
                } else {
                    block.push(self.parse_statement()?);
                }
            } else {
                break Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: String::from("expect '}', got EOF"),
                });
            }
        }
    }
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, current: 0 }
    }

    fn parse_primary_expression(&mut self) -> Result<ExpressionNode<'a>, Diagnostics> {
        self.skip_whitespace();
        if let Some(first_char) = self.peek() {
            let start = self.current;
            let expr = match first_char {
                identifier_start!() => Expression::Variable {
                    name: self.parse_identifier()?,
                },
                '\\' => {
                    self.fast_advance();
                    let params = self.parse_params()?;
                    self.skip_whitespace();
                    self.expect('-')?;
                    self.expect('>')?;
                    Expression::Function {
                        params: params,
                        body: Box::new(self.parse_expression()?),
                    }
                }
                '(' => {
                    self.fast_advance();
                    let inner = Box::new(self.parse_expression()?);
                    self.expect(')')?;
                    Expression::Group { inner }
                }
                '`' => {
                    self.fast_advance();
                    while let Some(ch) = self.peek() {
                        match ch {
                            '`' => break,
                            ')' | identifier_start!() => {
                                return Err(Diagnostics {
                                    severity: Severity::Error,
                                    span: Span {
                                        start,
                                        end: self.current,
                                    },
                                    message: format!("unexpect character '{ch}'"),
                                });
                            }
                            ch if ch.is_whitespace() => {
                                return Err(Diagnostics {
                                    severity: Severity::Error,
                                    span: Span {
                                        start,
                                        end: self.current,
                                    },
                                    message: String::from("unexpect whitespace"),
                                });
                            }
                            _ => self.fast_advance(),
                        }
                    }
                    self.expect('`')?;
                    Expression::Variable {
                        name: &self.source[start + 1..self.current - 1],
                    }
                }
                '"' => Expression::String(self.parse_string()?),
                integer_start!() => Expression::Integer(self.parse_integer()?),
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: format!("expect expression, got '{}'", first_char),
                })?,
            };

            Ok(ExpressionNode {
                kind: expr,
                span: Span {
                    start,
                    end: self.current,
                },
            })
        } else {
            Err(Diagnostics {
                severity: Severity::Error,
                span: Span {
                    start: self.current,
                    end: self.current,
                },
                message: String::from("expect expression, got EOF"),
            })
        }
    }

    pub fn parse_expression(&mut self) -> Result<ExpressionNode<'a>, Diagnostics> {
        let start = self.current;
        let mut expression = self.parse_primary_expression()?;
        loop {
            self.skip_whitespace();
            if let Some(ch) = self.peek() {
                match ch {
                    '$' => {
                        self.fast_advance();
                        expression = ExpressionNode {
                            kind: Expression::Call {
                                callee: Box::new(expression),
                                arg: Box::new(self.parse_primary_expression()?),
                            },
                            span: Span {
                                start,
                                end: self.current,
                            },
                        };
                    }
                    identifier_start!() | '`' | ')' => break Ok(expression),
                    _ => {
                        let infix_start = self.current;
                        self.fast_advance();
                        while let Some(ch) = self.peek() {
                            match ch {
                                '`' => {
                                    return Err(Diagnostics {
                                        severity: Severity::Error,
                                        span: Span {
                                            start: infix_start,
                                            end: self.current,
                                        },
                                        message: format!("unexpect '{ch}'"),
                                    });
                                }
                                ch if ch.is_whitespace() || matches!(ch, identifier_start!()) => {
                                    expression = ExpressionNode {
                                        kind: Expression::Call {
                                            callee: Box::new(ExpressionNode {
                                                kind: Expression::Variable {
                                                    name: &self.source[infix_start..self.current],
                                                },
                                                span: Span {
                                                    start: infix_start,
                                                    end: self.current,
                                                },
                                            }),
                                            arg: Box::new(expression),
                                        },
                                        span: Span {
                                            start,
                                            end: self.current,
                                        },
                                    };
                                    let rhs = self.parse_primary_expression()?;
                                    expression = ExpressionNode {
                                        kind: Expression::Call {
                                            callee: Box::new(expression),
                                            arg: Box::new(rhs),
                                        },
                                        span: Span {
                                            start,
                                            end: self.current,
                                        },
                                    };
                                    break;
                                }
                                _ => self.fast_advance(),
                            }
                        }
                    }
                }
            } else {
                break Ok(expression);
            }
        }
    }

    pub fn parse_statement(&mut self) -> Result<StatementNode<'a>, Diagnostics> {
        self.skip_whitespace();
        if let Some(first_char) = self.peek() {
            let start = self.current;
            let stat = match first_char {
                identifier_start!() => {
                    let name = self.parse_identifier()?;
                    self.skip_whitespace();
                    if let Some(second_char) = self.peek() {
                        match second_char {
                            identifier_start!() => {
                                let params = self.parse_params()?;
                                self.expect('=')?;
                                let body = self.parse_expression()?;
                                self.skip_whitespace();
                                if let Some('w') = self.peek()
                                    && self.source.len() - self.current > 4
                                {
                                    if self.source.get(self.current..self.current + 5)
                                        == Some("where")
                                    {
                                        self.current += 5;
                                        Statement::Function {
                                            name,
                                            params,
                                            body,
                                            where_clause: self.parse_bracket_block()?,
                                        }
                                    } else {
                                        Statement::Function {
                                            name,
                                            params,
                                            body,
                                            where_clause: Vec::new(),
                                        }
                                    }
                                } else {
                                    Statement::Function {
                                        name,
                                        params,
                                        body,
                                        where_clause: Vec::new(),
                                    }
                                }
                            }
                            '=' => {
                                self.fast_advance();
                                let value = self.parse_expression()?;
                                self.skip_whitespace();
                                if let Some('w') = self.peek()
                                    && self.source.len() - self.current > 4
                                {
                                    if self.source.get(self.current..self.current + 5)
                                        == Some("where")
                                    {
                                        self.current += 5;
                                        Statement::Variable {
                                            name,
                                            value,
                                            where_clause: self.parse_bracket_block()?,
                                        }
                                    } else {
                                        Statement::Variable {
                                            name,
                                            value,
                                            where_clause: Vec::new(),
                                        }
                                    }
                                } else {
                                    Statement::Variable {
                                        name,
                                        value,
                                        where_clause: Vec::new(),
                                    }
                                }
                            }
                            _ => Err(Diagnostics {
                                severity: Severity::Error,
                                span: Span {
                                    start,
                                    end: self.current,
                                },
                                message: format!("expect '=', got '{}'", second_char),
                            })?,
                        }
                    } else {
                        Err(Diagnostics {
                            severity: Severity::Error,
                            span: Span {
                                start,
                                end: self.current,
                            },
                            message: String::from("expect '=', got EOF"),
                        })?
                    }
                }
                '`' => {
                    let start = self.current;
                    self.fast_advance();
                    while let Some(ch) = self.peek() {
                        match ch {
                            '`' => break,
                            ')' | identifier_start!() => {
                                return Err(Diagnostics {
                                    severity: Severity::Error,
                                    span: Span {
                                        start,
                                        end: self.current,
                                    },
                                    message: format!("unexpect character '{ch}'"),
                                });
                            }
                            ch if ch.is_whitespace() => {
                                return Err(Diagnostics {
                                    severity: Severity::Error,
                                    span: Span {
                                        start,
                                        end: self.current,
                                    },
                                    message: String::from("unexpect whitespace"),
                                });
                            }
                            _ => self.fast_advance(),
                        }
                    }
                    let name = &self.source[start + 1..self.current];
                    self.expect('`')?;
                    self.skip_whitespace();
                    let params = self.parse_params()?;
                    if params.len() != 2 {
                        return Err(Diagnostics {
                            severity: Severity::Error,
                            span: Span {
                                start,
                                end: self.current,
                            },
                            message: String::from("infix can has only 2 params"),
                        });
                    }
                    self.skip_whitespace();
                    self.expect('=')?;
                    let body = self.parse_expression()?;
                    self.skip_whitespace();
                    if let Some('w') = self.peek()
                        && self.source.len() - self.current > 4
                    {
                        if self.source.get(self.current..self.current + 5) == Some("where") {
                            self.current += 5;
                            Statement::Function {
                                name,
                                params,
                                body,
                                where_clause: self.parse_bracket_block()?,
                            }
                        } else {
                            Statement::Function {
                                name,
                                params,
                                body,
                                where_clause: Vec::new(),
                            }
                        }
                    } else {
                        Statement::Function {
                            name,
                            params,
                            body,
                            where_clause: Vec::new(),
                        }
                    }
                }
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: format!("expect statement, got '{}'", first_char),
                })?,
            };

            Ok(StatementNode {
                kind: stat,
                span: Span {
                    start,
                    end: self.current,
                },
            })
        } else {
            Err(Diagnostics {
                severity: Severity::Error,
                span: Span {
                    start: self.current,
                    end: self.current,
                },
                message: String::from("expect statement, got EOF"),
            })
        }
    }

    pub fn parse(&mut self) -> Result<Vec<StatementNode<'a>>, Diagnostics> {
        let mut stats = Vec::new();
        while self.current < self.source.len() {
            stats.push(self.parse_statement()?);
            self.skip_whitespace();
        }
        Ok(stats)
    }
}
