use parlance_diagnostics::{Diagnostics, Span};

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
    pub fn new(source: &'a str) -> Self {
        Self { source, current: 0 }
    }

    pub fn parse(&mut self) -> Result<Vec<StatementNode<'a>>, Diagnostics> {
        let mut stats = Vec::new();
        while self.current < self.source.len() {
            stats.push(self.parse_statement()?);
            self.skip_whitespace();
        }
        Ok(stats)
    }

    pub fn parse_expression(&mut self) -> Result<ExpressionNode<'a>, Diagnostics> {
        let start = self.current;
        let mut expr = self.parse_primary_expression()?;
        loop {
            self.skip_whitespace();
            let Some(ch) = self.peek() else {
                return Ok(expr);
            };
            match ch {
                identifier_start!() | '`' | ')' => return Ok(expr),
                '$' => {
                    self.fast_advance();
                    let arg = self.parse_primary_expression()?;
                    expr = self.expr(
                        start,
                        Expression::Call {
                            callee: Box::new(expr),
                            arg: Box::new(arg),
                        },
                    );
                }
                _ => expr = self.parse_infix_tail(start, expr)?,
            }
        }
    }

    pub fn parse_statement(&mut self) -> Result<StatementNode<'a>, Diagnostics> {
        self.skip_whitespace();
        let start = self.current;
        let first = self.peek_or_eof(start, "statement")?;
        let kind = match first {
            identifier_start!() => self.parse_ident_statement(start)?,
            '`' => self.parse_infix_statement(start)?,
            _ => return Err(self.err(start, format!("expected statement, got '{}'", first))),
        };
        Ok(self.stat(start, kind))
    }
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

    fn err(&self, start: usize, msg: impl Into<String>) -> Diagnostics {
        Span {
            start,
            end: self.current,
        }
        .error(msg.into())
    }

    fn peek_or_eof(&self, start: usize, ctx: &str) -> Result<char, Diagnostics> {
        self.peek()
            .ok_or_else(|| self.err(start, format!("expected {}, got EOF", ctx)))
    }

    fn expr(&self, start: usize, kind: Expression<'a>) -> ExpressionNode<'a> {
        ExpressionNode {
            kind,
            span: Span {
                start,
                end: self.current,
            },
        }
    }

    fn stat(&self, start: usize, kind: Statement<'a>) -> StatementNode<'a> {
        StatementNode {
            kind,
            span: Span {
                start,
                end: self.current,
            },
        }
    }

    fn expect(&mut self, target: char) -> Result<(), Diagnostics> {
        self.skip_whitespace();
        let start = self.current;
        let ch = self
            .advance()
            .ok_or_else(|| self.err(start, format!("expected '{}', got EOF", target)))?;
        if ch == target {
            Ok(())
        } else {
            Err(self.err(start, format!("expected '{}', got '{}'", target, ch)))
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

    fn parse_identifier(&mut self) -> Result<&'a str, Diagnostics> {
        let start = self.current;
        let ch = self.peek_or_eof(start, "identifier")?;
        if !matches!(ch, identifier_start!()) {
            return Err(self.err(start, format!("expected identifier, got '{}'", ch)));
        }
        self.fast_advance();
        while let Some(ch) = self.peek() {
            if matches!(ch, identifier_continue!()) {
                self.fast_advance();
            } else {
                break;
            }
        }
        Ok(&self.source[start..self.current])
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
        while let Some(int_ch) = self.peek() {
            if matches!(int_ch, integer_start!()) {
                self.fast_advance();
            } else if matches!(int_ch, identifier_start!()) {
                return Err(Span {
                    start,
                    end: self.current + 1,
                }
                .error(format!("expected digit, got '{}'", int_ch)));
            } else {
                break;
            }
        }

        if self.current == start {
            return Err(Span {
                start,
                end: self.current + 1,
            }
            .error("expected digit, got EOF"));
        }

        let num_str = &self.source[start..self.current];
        num_str
            .parse::<i16>()
            .map_err(|_| self.err(start, format!("integer overflow: {}", num_str)))
    }

    fn parse_params(&mut self) -> Result<Vec<&'a str>, Diagnostics> {
        let start = self.current;
        let mut params: Vec<&'a str> = Vec::new();
        loop {
            self.skip_whitespace();
            let ch = self.peek_or_eof(start, "parameter")?;
            if matches!(ch, identifier_start!()) {
                params.push(self.parse_identifier()?);
            } else {
                return Ok(params);
            }
        }
    }

    fn parse_bracket_block(&mut self) -> Result<Vec<StatementNode<'a>>, Diagnostics> {
        let start = self.current;
        self.expect('{')?;
        let mut block = Vec::new();
        loop {
            self.skip_whitespace();
            let ch = self.peek_or_eof(start, "'}'")?;
            if ch == '}' {
                self.fast_advance();
                return Ok(block);
            }
            block.push(self.parse_statement()?);
        }
    }

    fn parse_where_clause(&mut self) -> Result<Vec<StatementNode<'a>>, Diagnostics> {
        self.skip_whitespace();
        if self.source[self.current..].starts_with("where") {
            self.current += 5;
            self.parse_bracket_block()
        } else {
            Ok(Vec::new())
        }
    }

    fn parse_backtick_name(&mut self) -> Result<&'a str, Diagnostics> {
        let start = self.current;
        self.fast_advance();
        while let Some(ch) = self.peek() {
            match ch {
                '`' => break,
                ')' | identifier_start!() => {
                    return Err(self.err(start, format!("unexpected character '{ch}'")));
                }
                ch if ch.is_whitespace() => {
                    return Err(self.err(start, "unexpected whitespace"));
                }
                _ => self.fast_advance(),
            }
        }
        let name = &self.source[start + 1..self.current];
        self.expect('`')?;
        Ok(name)
    }

    fn parse_primary_expression(&mut self) -> Result<ExpressionNode<'a>, Diagnostics> {
        self.skip_whitespace();
        let start = self.current;
        let first = self.peek_or_eof(start, "expression")?;
        let kind = match first {
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
                    params,
                    body: Box::new(self.parse_expression()?),
                }
            }
            '(' => {
                self.fast_advance();
                let inner = Box::new(self.parse_expression()?);
                self.expect(')')?;
                Expression::Group { inner }
            }
            '`' => Expression::Variable {
                name: self.parse_backtick_name()?,
            },
            '"' => Expression::String(self.parse_string()?),
            integer_start!() => Expression::Integer(self.parse_integer()?),
            _ => return Err(self.err(start, format!("expected expression, got '{}'", first))),
        };
        Ok(self.expr(start, kind))
    }

    fn parse_infix_tail(
        &mut self,
        expr_start: usize,
        lhs: ExpressionNode<'a>,
    ) -> Result<ExpressionNode<'a>, Diagnostics> {
        let op_start = self.current;
        self.fast_advance();
        loop {
            let Some(ch) = self.peek() else {
                return Ok(lhs);
            };
            if ch == '`' {
                return Err(self.err(op_start, format!("unexpected '{ch}'")));
            }
            if ch.is_whitespace() || matches!(ch, identifier_start!()) {
                break;
            }
            self.fast_advance();
        }
        let op_name = &self.source[op_start..self.current];
        let op = self.expr(op_start, Expression::Variable { name: op_name });
        let applied = self.expr(
            expr_start,
            Expression::Call {
                callee: Box::new(op),
                arg: Box::new(lhs),
            },
        );
        let rhs = self.parse_primary_expression()?;
        Ok(self.expr(
            expr_start,
            Expression::Call {
                callee: Box::new(applied),
                arg: Box::new(rhs),
            },
        ))
    }

    fn parse_ident_statement(&mut self, start: usize) -> Result<Statement<'a>, Diagnostics> {
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        let second = self.peek_or_eof(start, "'='")?;
        match second {
            identifier_start!() => {
                let params = self.parse_params()?;
                self.expect('=')?;
                let body = self.parse_expression()?;
                let where_clause = self.parse_where_clause()?;
                Ok(Statement::Function {
                    name,
                    params,
                    body,
                    where_clause,
                })
            }
            '=' => {
                self.fast_advance();
                let value = self.parse_expression()?;
                let where_clause = self.parse_where_clause()?;
                Ok(Statement::Variable {
                    name,
                    value,
                    where_clause,
                })
            }
            _ => Err(self.err(start, format!("expected '=', got '{}'", second))),
        }
    }

    fn parse_infix_statement(&mut self, start: usize) -> Result<Statement<'a>, Diagnostics> {
        let name = self.parse_backtick_name()?;
        self.skip_whitespace();
        let params = self.parse_params()?;
        if params.len() != 2 {
            return Err(self.err(start, "infix operator requires exactly 2 parameters"));
        }
        self.skip_whitespace();
        self.expect('=')?;
        let body = self.parse_expression()?;
        let where_clause = self.parse_where_clause()?;
        Ok(Statement::Function {
            name,
            params,
            body,
            where_clause,
        })
    }
}
