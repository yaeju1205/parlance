use std::rc::Rc;

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::{Expression, ExpressionKind, Statement, StatementKind};

#[derive(Debug, Clone)]
pub struct Param {
    pub span: Span,
    pub name: Rc<str>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct DesugarValue {
    pub span: Span,
    pub kind: DesugarValueKind,
}

#[derive(Debug, Clone)]
pub struct DesugarBinding {
    pub name: Rc<str>,
    pub value: Rc<DesugarValue>,
    pub scheme: Vec<DesugarBinding>,
}

pub fn desugar_expression(expr: Rc<Expression>) -> Result<DesugarValue, Diagnostics> {
    match &expr.kind {
        ExpressionKind::Variable { name } => Ok(DesugarValue {
            span: expr.span.clone(),
            kind: DesugarValueKind::Variable { name: name.clone() },
        }),
        ExpressionKind::Function { params, body } => Ok({
            let mut value = desugar_expression(body.clone())?;

            for param in params.iter().rev() {
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
                callee: Rc::new(desugar_expression(callee.clone())?),
                arg: Rc::new(desugar_expression(arg.clone())?),
            },
        }),
        ExpressionKind::InfixCall { operator, lhs, rhs } => Ok(DesugarValue {
            span: expr.span.clone(),
            kind: DesugarValueKind::FunctionCall {
                callee: Rc::new(DesugarValue {
                    span: Span {
                        start: operator.span.start.clone(),
                        end: lhs.span.end.clone(),
                    },
                    kind: DesugarValueKind::FunctionCall {
                        callee: Rc::new(DesugarValue {
                            span: operator.span.clone(),
                            kind: DesugarValueKind::Variable {
                                name: operator.kind.clone(),
                            },
                        }),
                        arg: Rc::new(desugar_expression(lhs.clone())?),
                    },
                }),
                arg: Rc::new(desugar_expression(rhs.clone())?),
            },
        }),
        ExpressionKind::String(value) => Ok(DesugarValue {
            span: expr.span.clone(),
            kind: DesugarValueKind::String(value.clone()),
        }),
        ExpressionKind::Int(value) => Ok(DesugarValue {
            span: expr.span.clone(),
            kind: DesugarValueKind::Int(value.clone()),
        }),
        ExpressionKind::Group(inner) => Ok(desugar_expression(inner.clone())?),
    }
}

pub fn desugar_statement(stat: Statement) -> Result<DesugarBinding, Diagnostics> {
    let mut scheme = Vec::new();

    for scheme_stat in stat.scheme.into_iter() {
        scheme.push(desugar_statement(scheme_stat)?);
    }

    match &stat.kind {
        StatementKind::Variable { name, value } => Ok(DesugarBinding {
            name: name.clone(),
            value: Rc::new(desugar_expression(value.clone())?),
            scheme,
        }),
        StatementKind::Function { name, params, body } => {
            let mut value = desugar_expression(body.clone())?;

            for param in params.iter().rev() {
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
            operator,
            params,
            body,
            ..
        } => {
            let mut value = desugar_expression(body.clone())?;

            for param in params.iter().rev() {
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

pub fn desugar(stats: Vec<Statement>) -> Result<Vec<DesugarBinding>, Diagnostics> {
    let mut bindings = Vec::new();

    for stat in stats.into_iter() {
        bindings.push(desugar_statement(stat)?);
    }

    Ok(bindings)
}
