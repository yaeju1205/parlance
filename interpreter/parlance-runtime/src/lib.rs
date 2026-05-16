use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Severity, Span};
use parlance_ir::{Value, Variable};

pub mod stdlib;

pub struct Program<'a> {
    variable_pool_stack: Vec<HashMap<&'a str, Rc<Variable<'a>>>>,
}

impl<'a> Default for Program<'a> {
    fn default() -> Self {
        Self {
            variable_pool_stack: vec![
                HashMap::default(), // stdlib pool
                HashMap::default(), // global pool
            ],
        }
    }
}

impl<'a> From<Vec<Variable<'a>>> for Program<'a> {
    fn from(vars: Vec<Variable<'a>>) -> Self {
        let mut program = Self::default();
        for var in vars {
            program.declaration_variable(var);
        }
        program
    }
}

// impl<'a> Program<'a> {
//     pub fn with_stdlib(&mut self) {
//         let stdlib_pool = &self.variable_pool_stack[0];
//     }
// }

impl<'a> Program<'a> {
    pub fn declaration_variable(&mut self, var: Variable<'a>) {
        if let Some(pool) = self.variable_pool_stack.last_mut() {
            pool.insert(var.name, Rc::new(var));
        }
    }

    pub fn get_variable(&self, name: &'a str) -> Option<Rc<Variable<'a>>> {
        for pool in self.variable_pool_stack.iter().rev() {
            if let Some(var) = pool.get(name) {
                return Some(var.clone());
            }
        }
        None
    }
}

impl<'a> Program<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn execute_value(&mut self, value: &Value<'a>) -> Result<Value<'a>, Diagnostics> {
        match value {
            Value::Variable(name) => {
                if let Some(inner) = self.get_variable(name) {
                    Ok(inner.value.as_ref().clone())
                } else {
                    Err(Diagnostics {
                        severity: Severity::Error,
                        span: Span::default(),
                        message: format!("undefined variable: {}", name),
                    })
                }
            }
            Value::Call { callee, arg } => match self.execute_value(&callee)? {
                Value::Function { param, body } => {
                    let mut func_pool = HashMap::new();
                    func_pool.insert(
                        param,
                        Rc::new(Variable {
                            name: param,
                            value: Rc::new(self.execute_value(&arg)?),
                        }),
                    );

                    self.variable_pool_stack.push(func_pool);
                    let inner_result = self.execute_value(&body);
                    self.variable_pool_stack.pop();

                    inner_result
                }
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span::default(),
                    message: format!("can not call value: {:?}", callee),
                }),
            },
            Value::Group(inner) => Ok(inner.as_ref().clone()),
            _ => Ok(value.clone()),
        }
    }

    pub fn execute_variable(&mut self, var: Rc<Variable<'a>>) -> Result<Value<'a>, Diagnostics> {
        self.execute_value(&var.value)
    }
}
