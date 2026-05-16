use std::rc::Rc;

use parlance_ast::{Expression, Statement};

#[derive(Debug, Clone)]
pub enum Value<'a> {
    Variable(&'a str),
    Function {
        param: &'a str,
        body: Box<Value<'a>>,
    },
    String(&'a str),
    Group(Box<Value<'a>>),
    Call {
        callee: Rc<Value<'a>>,
        arg: Box<Value<'a>>,
    },
}

impl<'a> From<Expression<'a>> for Value<'a> {
    fn from(expr: Expression<'a>) -> Self {
        match expr {
            Expression::Variable(name) => Value::Variable(name),
            Expression::Function { params: args, body } => {
                let mut body_value = Value::from(*body);
                for arg in args.into_iter().rev() {
                    body_value = Value::Function {
                        param: arg,
                        body: Box::new(body_value),
                    }
                }
                body_value
            }
            Expression::String(str) => Value::String(str),
            Expression::Group(inner) => Value::from(*inner),
            Expression::Call { callee, arg } => Value::Call {
                callee: Rc::new(Value::from(*callee)),
                arg: Box::new(Value::from(*arg)),
            },
        }
    }
}

#[derive(Debug)]
pub struct Variable<'a> {
    pub name: &'a str,
    pub value: Rc<Value<'a>>,
}

impl<'a> From<Statement<'a>> for Variable<'a> {
    fn from(stat: Statement<'a>) -> Self {
        match stat {
            Statement::Function { name, args, body } => {
                let mut body = Value::from(body);
                for arg in args.into_iter().rev() {
                    body = Value::Function {
                        param: arg,
                        body: Box::new(body),
                    };
                }
                Variable {
                    name,
                    value: Rc::new(body),
                }
            }
            Statement::Variable { name, value } => Variable {
                name,
                value: Rc::new(Value::from(value)),
            },
        }
    }
}

pub fn from_ast<'a>(stats: Vec<Statement<'a>>) -> Vec<Variable<'a>> {
    let mut decs = Vec::new();
    for stat in stats.into_iter() {
        decs.push(Variable::from(stat))
    }
    decs
}
