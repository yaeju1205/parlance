use parlance_diagnostics::{Diagnostics, Severity, Span};

#[derive(Debug)]
pub enum Expression<'a> {
    Variable(&'a str),
    Function {
        params: Vec<&'a str>,
        body: Box<Expression<'a>>,
    },
    String(&'a str),
    Group(Box<Expression<'a>>),
    Call {
        callee: Box<Expression<'a>>,
        arg: Box<Expression<'a>>,
    },
}

#[derive(Debug)]
pub enum Statement<'a> {
    Function {
        name: &'a str,
        args: Vec<&'a str>,
        body: Expression<'a>,
        where_clause: Vec<Statement<'a>>,
    },
    Variable {
        name: &'a str,
        value: Expression<'a>,
        where_clause: Vec<Statement<'a>>,
    },
}

pub struct Parser<'a> {
    source: &'a str,
    current: usize,
}

impl<'a> Parser<'a> {
    fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    fn peek(&self) -> Option<char> {
        self.source.chars().nth(self.current)
    }

    fn advance(&mut self) -> Option<char> {
        if self.is_at_end() {
            None
        } else {
            let ch = self.peek();
            self.current += 1;
            ch
        }
    }

    fn fast_advance(&mut self) {
        if !self.is_at_end() {
            self.current += 1;
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

macro_rules! identifier_start{
    () => {
        'a'..='z' | 'A'..='Z' | '_'
    };
}

macro_rules! identifier_continue {
    () => {
        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | ':'
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
                self.current += 1;
            } else {
                break;
            }
        }
        self.expect('"')?;
        Ok(&self.source[start..self.current - 1])
    }

    fn parse_args(&mut self) -> Result<Vec<&'a str>, Diagnostics> {
        let start = self.current;
        let mut args: Vec<&'a str> = Vec::new();
        self.skip_whitespace();
        loop {
            args.push(self.parse_identifier()?);
            self.skip_whitespace();
            if let Some(ch) = self.peek() {
                if !matches!(ch, identifier_continue!()) {
                    break Ok(args);
                }
            } else {
                Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: String::from("expect args, got EOF"),
                })?
            }
        }
    }

    fn parse_bracket_block(&mut self) -> Result<Vec<Statement<'a>>, Diagnostics> {
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
                    message: "expect '}', got EOF".to_string(),
                });
            }
        }
    }
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, current: 0 }
    }

    fn parse_primary_expression(&mut self) -> Result<Expression<'a>, Diagnostics> {
        self.skip_whitespace();
        if let Some(first_char) = self.peek() {
            let start = self.current;
            Ok(match first_char {
                identifier_start!() => Expression::Variable(self.parse_identifier()?),
                '\\' => {
                    self.fast_advance();
                    let args = self.parse_args()?;
                    self.skip_whitespace();
                    self.expect('-')?;
                    self.expect('>')?;
                    Expression::Function {
                        params: args,
                        body: Box::new(self.parse_expression()?),
                    }
                }
                '"' => Expression::String(self.parse_string()?),
                '(' => {
                    self.fast_advance();
                    let inner = Box::new(self.parse_expression()?);
                    self.expect(')')?;
                    Expression::Group(inner)
                }
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: format!("expect expression, got '{}'", first_char),
                })?,
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

    pub fn parse_expression(&mut self) -> Result<Expression<'a>, Diagnostics> {
        let mut expression = self.parse_primary_expression()?;
        loop {
            self.skip_whitespace();
            if let Some('$') = self.peek() {
                self.fast_advance();
                expression = Expression::Call {
                    callee: Box::new(expression),
                    arg: Box::new(self.parse_primary_expression()?),
                };
            } else {
                break Ok(expression);
            }
        }
    }

    pub fn parse_statement(&mut self) -> Result<Statement<'a>, Diagnostics> {
        self.skip_whitespace();

        if let Some(first_char) = self.peek() {
            let start = self.current;
            match first_char {
                identifier_start!() => {
                    let name = self.parse_identifier()?;
                    self.skip_whitespace();
                    if let Some(second_char) = self.peek() {
                        match second_char {
                            identifier_start!() => {
                                let args = self.parse_args()?;
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
                                        Ok(Statement::Function {
                                            name,
                                            args,
                                            body,
                                            where_clause: self.parse_bracket_block()?,
                                        })
                                    } else {
                                        Ok(Statement::Function {
                                            name,
                                            args,
                                            body,
                                            where_clause: Vec::new(),
                                        })
                                    }
                                } else {
                                    Ok(Statement::Function {
                                        name,
                                        args,
                                        body,
                                        where_clause: Vec::new(),
                                    })
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
                                        Ok(Statement::Variable {
                                            name,
                                            value,
                                            where_clause: self.parse_bracket_block()?,
                                        })
                                    } else {
                                        Ok(Statement::Variable {
                                            name,
                                            value,
                                            where_clause: Vec::new(),
                                        })
                                    }
                                } else {
                                    Ok(Statement::Variable {
                                        name,
                                        value,
                                        where_clause: Vec::new(),
                                    })
                                }
                            }
                            _ => Err(Diagnostics {
                                severity: Severity::Error,
                                span: Span {
                                    start,
                                    end: self.current,
                                },
                                message: format!("expect '=', got '{}'", second_char),
                            }),
                        }
                    } else {
                        Err(Diagnostics {
                            severity: Severity::Error,
                            span: Span {
                                start,
                                end: self.current,
                            },
                            message: String::from("expect '=', got EOF"),
                        })
                    }
                }
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span {
                        start,
                        end: self.current,
                    },
                    message: format!("expect statement, got '{}'", first_char),
                }),
            }
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

    pub fn parse(&mut self) -> Result<Vec<Statement<'a>>, Diagnostics> {
        let mut stats = Vec::new();
        while !self.is_at_end() {
            stats.push(self.parse_statement()?);
            self.skip_whitespace();
        }
        Ok(stats)
    }
}
