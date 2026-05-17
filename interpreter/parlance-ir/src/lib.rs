use std::rc::Rc;

use parlance_ast::{Expression, Statement};

#[derive(Debug)]
pub enum Value<'a> {
    Variable(&'a str),
    Function {
        param: &'a str,
        body: Rc<Value<'a>>,
    },
    String(String),
    Group(Rc<Value<'a>>),
    Call {
        callee: Rc<Value<'a>>,
        arg: Rc<Value<'a>>,
    },
}

impl<'a> From<Expression<'a>> for Value<'a> {
    fn from(expr: Expression<'a>) -> Self {
        match expr {
            Expression::Variable(name) => Value::Variable(name),
            Expression::Function { params, body } => {
                let mut body_value = Value::from(*body);
                for param in params.into_iter().rev() {
                    body_value = Value::Function {
                        param,
                        body: Rc::new(body_value),
                    }
                }
                body_value
            }
            Expression::String(str) => Value::String(str.to_string()),
            Expression::Group(inner) => Value::from(*inner),
            Expression::Call { callee, arg } => Value::Call {
                callee: Rc::new(Value::from(*callee)),
                arg: Rc::new(Value::from(*arg)),
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
            Statement::Function {
                name,
                args,
                body,
                where_clause,
            } => {
                let mut body = Value::from(body);
                for where_stat in where_clause.into_iter().rev() {
                    let where_var = Variable::from(where_stat);
                    body = Value::Call {
                        callee: Rc::new(Value::Function {
                            param: where_var.name,
                            body: Rc::new(body),
                        }),
                        arg: where_var.value,
                    };
                }
                for arg in args.into_iter().rev() {
                    body = Value::Function {
                        param: arg,
                        body: Rc::new(body),
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
