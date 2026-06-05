use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::{Expression, ExpressionKind, InfinixCombineRule, Statement, StatementKind};

#[derive(Debug)]
pub struct Param {
    pub span: Span,
    pub name: Rc<str>,
}

#[derive(Debug)]
pub enum DesugarValueKind {
    Variable {
        name: Rc<str>,
    },
    Function {
        param: Param,
        body: Rc<DesugarValue>,
    },
    FunctionCall {
        callee: Rc<DesugarValue>,
        arg: Rc<DesugarValue>,
    },
    String(Rc<str>),
    Int(i32),
}

#[derive(Debug)]
pub struct DesugarValue {
    pub span: Span,
    pub kind: DesugarValueKind,
}

#[derive(Debug)]
pub struct DesugarBinding {
    pub name: Rc<str>,
    pub value: Rc<DesugarValue>,
    pub scheme: Vec<DesugarBinding>,
}

#[derive(Default)]
pub struct Desugarer {
    infix_pool: HashMap<Rc<str>, InfinixCombineRule>,
}

impl Desugarer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn desugar_expression(
        &mut self,
        expr: Rc<Expression>,
    ) -> Result<DesugarValue, Diagnostics> {
        match &expr.kind {
            ExpressionKind::Variable { name } => Ok(DesugarValue {
                span: expr.span.clone(),
                kind: DesugarValueKind::Variable { name: name.clone() },
            }),
            ExpressionKind::Function { params, body } => Ok({
                let mut value = self.desugar_expression(body.clone())?;

                for param in params.iter() {
                    value = DesugarValue {
                        span: Span {
                            start: expr.span.start.clone(),
                            end: param.span.end.clone(),
                        },
                        kind: DesugarValueKind::Function {
                            param: Param {
                                span: param.span.clone(),
                                name: param.kind.clone(),
                            },
                            body: Rc::new(value),
                        },
                    }
                }

                value
            }),
            ExpressionKind::Infix { operator } => Ok(DesugarValue {
                span: expr.span.clone(),
                kind: DesugarValueKind::Variable {
                    name: operator.clone(),
                },
            }),
            ExpressionKind::FunctionCall { callee, arg } => Ok(DesugarValue {
                span: expr.span.clone(),
                kind: DesugarValueKind::FunctionCall {
                    callee: Rc::new(self.desugar_expression(callee.clone())?),
                    arg: Rc::new(self.desugar_expression(arg.clone())?),
                },
            }),
            ExpressionKind::InfixCall { operator, lhs, rhs } => {
                let Some(comb_rule) = self.infix_pool.get(&operator.kind) else {
                    return Err(Diagnostics::compiler_error(
                        format!("not found infix '{}'", operator.kind),
                        operator.span.clone(),
                    ));
                };
                match comb_rule {
                    InfinixCombineRule::Left => Ok(DesugarValue {
                        span: expr.span.clone(),
                        kind: DesugarValueKind::FunctionCall {
                            callee: Rc::new(DesugarValue {
                                span: Span {
                                    start: lhs.span.start.clone(),
                                    end: operator.span.end.clone(),
                                },
                                kind: DesugarValueKind::FunctionCall {
                                    callee: Rc::new(DesugarValue {
                                        span: operator.span.clone(),
                                        kind: DesugarValueKind::Variable {
                                            name: operator.kind.clone(),
                                        },
                                    }),
                                    arg: Rc::new(self.desugar_expression(lhs.clone())?),
                                },
                            }),
                            arg: Rc::new(self.desugar_expression(rhs.clone())?),
                        },
                    }),
                    InfinixCombineRule::Right => Ok(DesugarValue {
                        span: expr.span.clone(),
                        kind: DesugarValueKind::FunctionCall {
                            callee: Rc::new(DesugarValue {
                                span: Span {
                                    start: operator.span.start.clone(),
                                    end: rhs.span.end.clone(),
                                },
                                kind: DesugarValueKind::FunctionCall {
                                    callee: Rc::new(DesugarValue {
                                        span: operator.span.clone(),
                                        kind: DesugarValueKind::Variable {
                                            name: operator.kind.clone(),
                                        },
                                    }),
                                    arg: Rc::new(self.desugar_expression(rhs.clone())?),
                                },
                            }),
                            arg: Rc::new(self.desugar_expression(lhs.clone())?),
                        },
                    }),
                }
            }
            ExpressionKind::String(value) => Ok(DesugarValue {
                span: expr.span.clone(),
                kind: DesugarValueKind::String(value.clone()),
            }),
            ExpressionKind::Int(value) => Ok(DesugarValue {
                span: expr.span.clone(),
                kind: DesugarValueKind::Int(value.clone()),
            }),
            ExpressionKind::Group(inner) => Ok(self.desugar_expression(inner.clone())?),
        }
    }

    pub fn desugar_statement(&mut self, stat: Statement) -> Result<DesugarBinding, Diagnostics> {
        let mut scheme = Vec::new();

        for scheme_stat in stat.scheme.into_iter() {
            scheme.push(self.desugar_statement(scheme_stat)?);
        }

        match &stat.kind {
            StatementKind::Variable { name, value } => Ok(DesugarBinding {
                name: name.clone(),
                value: Rc::new(self.desugar_expression(value.clone())?),
                scheme,
            }),
            StatementKind::Function { name, params, body } => {
                let mut value = self.desugar_expression(body.clone())?;

                for param in params.iter() {
                    value = DesugarValue {
                        span: Span {
                            start: stat.span.start.clone(),
                            end: param.span.end.clone(),
                        },
                        kind: DesugarValueKind::Function {
                            param: Param {
                                span: param.span.clone(),
                                name: param.kind.clone(),
                            },
                            body: Rc::new(value),
                        },
                    }
                }

                Ok(DesugarBinding {
                    name: name.clone(),
                    value: Rc::new(value),
                    scheme,
                })
            }
            StatementKind::Infix {
                combine_rule,
                operator,
                params,
                body,
            } => {
                self.infix_pool
                    .insert(operator.kind.clone(), combine_rule.clone());

                let mut value = self.desugar_expression(body.clone())?;

                for param in params.iter() {
                    value = DesugarValue {
                        span: Span {
                            start: stat.span.start.clone(),
                            end: param.span.end.clone(),
                        },
                        kind: DesugarValueKind::Function {
                            param: Param {
                                span: param.span.clone(),
                                name: param.kind.clone(),
                            },
                            body: Rc::new(value),
                        },
                    }
                }

                Ok(DesugarBinding {
                    name: operator.kind.clone(),
                    value: Rc::new(value),
                    scheme,
                })
            }
        }
    }

    pub fn desugar(&mut self, stats: Vec<Statement>) -> Result<Vec<DesugarBinding>, Diagnostics> {
        let mut bindings = Vec::new();

        for stat in stats.into_iter() {
            bindings.push(self.desugar_statement(stat)?);
        }

        Ok(bindings)
    }
}
