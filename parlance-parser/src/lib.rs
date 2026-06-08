mod tokenizer;

use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};

use crate::tokenizer::{Token, TokenKind, tokenize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Associativity {
    Left,
    Right,
}

#[derive(Debug, Clone)]
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
        operator: Node<Rc<str>>,
        precedence: u8,
        associativity: Associativity,
        params: Vec<Node<Rc<str>>>,
        body: Rc<Expression>,
    },
}

#[derive(Debug)]
pub struct Statement {
    pub span: Span,
    pub kind: StatementKind,
    pub scheme: Vec<Statement>,
    pub is_public: bool,
}

#[derive(Debug, Clone)]
pub enum PathAnchor {
    SelfMod,
    Super(usize),
    Pkg,
    Plain,
}

#[derive(Debug, Clone)]
pub struct ModulePath {
    pub anchor: PathAnchor,
    pub segments: Vec<Node<Rc<str>>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ItemSpec {
    pub name: Node<Rc<str>>,
    pub alias: Option<Node<Rc<str>>>,
}

#[derive(Debug)]
pub enum ImportTarget {
    Module,
    Glob,
    Items(Vec<ItemSpec>),
}

#[derive(Debug)]
pub struct Import {
    pub path: ModulePath,
    pub target: ImportTarget,
    pub alias: Option<Node<Rc<str>>>,
    pub span: Span,
}

#[derive(Debug)]
pub enum ExportItem {
    Local(Vec<ItemSpec>),
    LocalGlob,
    FromGlob(ModulePath),
    FromItems(ModulePath, Vec<ItemSpec>),
}

#[derive(Debug)]
pub struct Export {
    pub item: ExportItem,
    pub span: Span,
}

#[derive(Debug)]
pub struct Extern {
    pub name: Node<Rc<str>>,
    pub span: Span,
}

pub struct ParseInfo {
    pub externs: Vec<Extern>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
    pub statements: Vec<Statement>,
}

enum PathSelector {
    None,
    Glob,
    Items(Vec<ItemSpec>),
}

pub struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Rc<Token>>,
    token_index: usize,
    infix_table: HashMap<Rc<str>, (u8, Associativity)>,
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
            infix_table: HashMap::new(),
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
            TokenKind::Identifier(name) => {
                let mut full = name.to_string();
                let mut end = token.span.end;

                while matches!(self.peek_token().map(|t| t.kind.mem_equal(&TokenKind::PathSep)), Some(true))
                {
                    self.next_token();
                    let segment = self.expect_token(TokenKind::Identifier(Rc::from("")))?;
                    if let TokenKind::Identifier(part) = &segment.kind {
                        full.push_str("::");
                        full.push_str(part);
                        end = segment.span.end;
                    }
                }

                Ok(Expression {
                    span: Span {
                        start: token.span.start,
                        end,
                    },
                    kind: ExpressionKind::Variable {
                        name: Rc::from(full.as_str()),
                    },
                })
            }
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
                let body = self.parse_expression(0)?;
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
                let inner = self.parse_expression(0)?;
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

    pub fn parse_application(&mut self) -> Result<Expression, Diagnostics> {
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
                    let inner = self.parse_expression(0)?;
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
                _ => break,
            }
        }
        Ok(expr)
    }

    pub fn parse_expression(&mut self, min_prec: u8) -> Result<Expression, Diagnostics> {
        let mut lhs = self.parse_application()?;
        loop {
            let Some(token) = self.peek_token() else {
                return Ok(lhs);
            };

            let kind = &token.kind;

            if matches!(kind, TokenKind::NewLine) {
                return Ok(lhs);
            }

            match kind {
                TokenKind::Symbol(symbol) => {
                    let (prec, assoc) = match self.infix_table.get(symbol.as_ref()) {
                        Some(&info) => info,
                        None => {
                            return Err(Diagnostics::parser_error(
                                format!("undefined infix operator '{}'", symbol),
                                token.span.clone(),
                            ));
                        }
                    };

                    if prec < min_prec {
                        break;
                    }

                    self.next_token();
                    let next_min_prec = match assoc {
                        Associativity::Left => prec + 1,
                        Associativity::Right => prec,
                    };
                    let rhs = self.parse_expression(next_min_prec)?;
                    lhs = Expression {
                        span: Span {
                            start: lhs.span.start,
                            end: rhs.span.end,
                        },
                        kind: ExpressionKind::InfixCall {
                            operator: Node {
                                kind: symbol.clone(),
                                span: token.span.clone(),
                            },
                            lhs: Rc::new(lhs),
                            rhs: Rc::new(rhs),
                        },
                    };
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    pub fn parse_statement(&mut self) -> Result<Statement, Diagnostics> {
        let Some(mut token) = self.next_token() else {
            return Err(Diagnostics::parser_error(
                "expected statement, found EOF".to_string(),
                Span {
                    start: 0,
                    end: self.source.len(),
                },
            ));
        };

        let is_public = if let TokenKind::Public = &token.kind {
            token = if let Some(token) = self.next_token() {
                token
            } else {
                return Err(Diagnostics::parser_error(
                    "expected statement, found EOF".to_string(),
                    Span {
                        start: 0,
                        end: self.source.len(),
                    },
                ));
            };
            true
        } else {
            false
        };

        match &token.kind {
            TokenKind::Identifier(name) => {
                let params = self.parse_params()?;
                self.expect_token(TokenKind::Equal)?;
                let scheme = self.parse_scheme()?;
                let value = self.parse_expression(0)?;
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
                        is_public,
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
                        is_public,
                    })
                }
            }
            TokenKind::Infix => {
                let precedence_token = self.next_token();
                let precedence = match &precedence_token {
                    Some(t) => match &t.kind {
                        TokenKind::Int(n) => *n as u8,
                        _ => {
                            return Err(Diagnostics::parser_error(
                                format!("expected precedence (integer), found {:?}", &t.kind),
                                t.span.clone(),
                            ));
                        }
                    },
                    None => {
                        return Err(Diagnostics::parser_error(
                            "expected precedence, found EOF".to_string(),
                            Span {
                                start: token.span.start.clone(),
                                end: self.source.len(),
                            },
                        ));
                    }
                };

                let mut associativity = Associativity::Left;

                if let Some(next) = self.peek_token() {
                    if let TokenKind::Identifier(word) = &next.kind {
                        match word.as_ref() {
                            "left" => {
                                self.next_token();
                                associativity = Associativity::Left;
                            }
                            "right" => {
                                self.next_token();
                                associativity = Associativity::Right;
                            }
                            _ => {}
                        }
                    }
                }

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

                self.infix_table
                    .insert(operator.kind.clone(), (precedence, associativity));

                let params = self.parse_params()?;
                self.expect_token(TokenKind::Equal)?;
                let scheme = self.parse_scheme()?;
                let body = self.parse_expression(0)?;

                Ok(Statement {
                    span: Span {
                        start: token.span.start.clone(),
                        end: body.span.end.clone(),
                    },
                    kind: StatementKind::Infix {
                        operator,
                        precedence,
                        associativity,
                        params,
                        body: Rc::new(body),
                    },
                    scheme,
                    is_public,
                })
            }
            TokenKind::NewLine => self.parse_statement(),
            _ => Err(Diagnostics::parser_error(
                format!("expected statement, found {:?}", &token.kind),
                token.span.clone(),
            )),
        }
    }

    fn parse_item_list(&mut self) -> Result<Vec<ItemSpec>, Diagnostics> {
        self.expect_token(TokenKind::LeftBrace)?;
        let mut items = Vec::new();

        loop {
            let Some(token) = self.next_token() else {
                return Err(Diagnostics::parser_error(
                    "expected item or '}', found EOF".to_string(),
                    Span {
                        start: 0,
                        end: self.source.len(),
                    },
                ));
            };

            match &token.kind {
                TokenKind::Identifier(name) => {
                    let item_name = Node {
                        kind: name.clone(),
                        span: token.span.clone(),
                    };

                    let mut alias = None;
                    if matches!(self.peek_token().map(|t| t.kind.mem_equal(&TokenKind::As)), Some(true))
                    {
                        self.next_token();
                        let alias_token = self.expect_token(TokenKind::Identifier(Rc::from("")))?;
                        if let TokenKind::Identifier(alias_name) = &alias_token.kind {
                            alias = Some(Node {
                                kind: alias_name.clone(),
                                span: alias_token.span.clone(),
                            });
                        }
                    }

                    items.push(ItemSpec {
                        name: item_name,
                        alias,
                    });
                }
                TokenKind::RightBrace => break,
                TokenKind::NewLine => continue,
                _ => {
                    return Err(Diagnostics::parser_error(
                        format!("expected item, found {:?}", &token.kind),
                        token.span.clone(),
                    ));
                }
            }

            match self.peek_token() {
                Some(token) => match &token.kind {
                    TokenKind::Comma => {
                        self.next_token();
                    }
                    TokenKind::RightBrace => {
                        self.next_token();
                        break;
                    }
                    _ => {
                        return Err(Diagnostics::parser_error(
                            format!("expected ',' or '}}', found {:?}", &token.kind),
                            token.span.clone(),
                        ));
                    }
                },
                None => {
                    return Err(Diagnostics::parser_error(
                        "expected ',' or '}', found EOF".to_string(),
                        Span {
                            start: 0,
                            end: self.source.len(),
                        },
                    ));
                }
            }
        }

        Ok(items)
    }

    fn parse_path(&mut self) -> Result<(ModulePath, PathSelector), Diagnostics> {
        let mut segments: Vec<Node<Rc<str>>> = Vec::new();
        let mut selector = PathSelector::None;

        let start = self.peek_token().map(|t| t.span.start).unwrap_or(0);
        let mut end = start;

        loop {
            let Some(token) = self.next_token() else {
                return Err(Diagnostics::parser_error(
                    "expected path segment, found EOF".to_string(),
                    Span {
                        start,
                        end: self.source.len(),
                    },
                ));
            };

            match &token.kind {
                TokenKind::Identifier(name) => {
                    end = token.span.end;
                    segments.push(Node {
                        kind: name.clone(),
                        span: token.span.clone(),
                    });
                }
                TokenKind::Symbol(symbol) if symbol.as_ref() == "*" => {
                    end = token.span.end;
                    selector = PathSelector::Glob;
                    break;
                }
                TokenKind::LeftBrace => {
                    self.token_index -= 1;
                    let items = self.parse_item_list()?;
                    selector = PathSelector::Items(items);
                    break;
                }
                _ => {
                    return Err(Diagnostics::parser_error(
                        format!("expected path segment, found {:?}", &token.kind),
                        token.span.clone(),
                    ));
                }
            }

            match self.peek_token() {
                Some(token) if token.kind.mem_equal(&TokenKind::PathSep) => {
                    self.next_token();
                }
                _ => break,
            }
        }

        if segments.is_empty() {
            return Err(Diagnostics::parser_error(
                "expected path segment".to_string(),
                Span { start, end },
            ));
        }

        let anchor = match segments[0].kind.as_ref() {
            "self" => {
                segments.remove(0);
                PathAnchor::SelfMod
            }
            "pkg" => {
                segments.remove(0);
                PathAnchor::Pkg
            }
            "super" => {
                let mut count = 0;
                while !segments.is_empty() && segments[0].kind.as_ref() == "super" {
                    segments.remove(0);
                    count += 1;
                }
                PathAnchor::Super(count)
            }
            _ => PathAnchor::Plain,
        };

        Ok((
            ModulePath {
                anchor,
                segments,
                span: Span { start, end },
            },
            selector,
        ))
    }

    fn parse_extern(&mut self) -> Result<Extern, Diagnostics> {
        let keyword = self.expect_token(TokenKind::Extern)?;
        let name_token = self.expect_token(TokenKind::Identifier(Rc::from("")))?;
        let TokenKind::Identifier(name) = &name_token.kind else {
            unreachable!()
        };

        Ok(Extern {
            name: Node {
                kind: name.clone(),
                span: name_token.span.clone(),
            },
            span: Span {
                start: keyword.span.start,
                end: name_token.span.end,
            },
        })
    }

    fn parse_import(&mut self) -> Result<Import, Diagnostics> {
        let keyword = self.expect_token(TokenKind::Import)?;
        let (path, selector) = self.parse_path()?;
        let mut end = path.span.end;

        let target = match selector {
            PathSelector::None => ImportTarget::Module,
            PathSelector::Glob => ImportTarget::Glob,
            PathSelector::Items(items) => ImportTarget::Items(items),
        };

        let mut alias = None;
        if matches!(self.peek_token().map(|t| t.kind.mem_equal(&TokenKind::As)), Some(true)) {
            self.next_token();
            let alias_token = self.expect_token(TokenKind::Identifier(Rc::from("")))?;
            if let TokenKind::Identifier(name) = &alias_token.kind {
                end = alias_token.span.end;
                alias = Some(Node {
                    kind: name.clone(),
                    span: alias_token.span.clone(),
                });
            }
        }

        Ok(Import {
            path,
            target,
            alias,
            span: Span {
                start: keyword.span.start,
                end,
            },
        })
    }

    fn parse_export(&mut self) -> Result<Export, Diagnostics> {
        let keyword = self.expect_token(TokenKind::Export)?;

        if matches!(self.peek_token().map(|t| t.kind.mem_equal(&TokenKind::LeftBrace)), Some(true))
        {
            let items = self.parse_item_list()?;
            let end = items
                .last()
                .map(|item| {
                    item.alias
                        .as_ref()
                        .map(|a| a.span.end)
                        .unwrap_or(item.name.span.end)
                })
                .unwrap_or(keyword.span.end);
            return Ok(Export {
                item: ExportItem::Local(items),
                span: Span {
                    start: keyword.span.start,
                    end,
                },
            });
        }

        if let Some(token) = self.peek_token() {
            if let TokenKind::Symbol(symbol) = &token.kind {
                if symbol.as_ref() == "*" {
                    self.next_token();
                    return Ok(Export {
                        item: ExportItem::LocalGlob,
                        span: Span {
                            start: keyword.span.start,
                            end: token.span.end,
                        },
                    });
                }
            }
        }

        let (mut path, selector) = self.parse_path()?;
        let end = path.span.end;
        let item = match selector {
            PathSelector::Glob => ExportItem::FromGlob(path),
            PathSelector::Items(items) => ExportItem::FromItems(path, items),
            PathSelector::None => {
                if matches!(path.anchor, PathAnchor::Plain) && path.segments.len() == 1 {
                    let name = path.segments.pop().unwrap();
                    ExportItem::Local(vec![ItemSpec { name, alias: None }])
                } else if path.segments.len() >= 2 {
                    let name = path.segments.pop().unwrap();
                    ExportItem::FromItems(path, vec![ItemSpec { name, alias: None }])
                } else {
                    return Err(Diagnostics::parser_error(
                        "invalid export target".to_string(),
                        path.span.clone(),
                    ));
                }
            }
        };

        Ok(Export {
            item,
            span: Span {
                start: keyword.span.start,
                end,
            },
        })
    }

    pub fn parse(&mut self) -> Result<ParseInfo, Diagnostics> {
        let mut externs = Vec::new();
        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut statements = Vec::new();

        while let Some(token) = self.peek_token() {
            match &token.kind {
                TokenKind::NewLine => {
                    self.next_token();
                }
                TokenKind::Extern => externs.push(self.parse_extern()?),
                TokenKind::Import => imports.push(self.parse_import()?),
                TokenKind::Export => exports.push(self.parse_export()?),
                _ => statements.push(self.parse_statement()?),
            }
        }

        Ok(ParseInfo {
            externs,
            imports,
            exports,
            statements,
        })
    }
}
