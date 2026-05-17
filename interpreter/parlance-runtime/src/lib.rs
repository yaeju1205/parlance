use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Severity, Span};
use parlance_ir::{Value, Variable};

#[derive(Debug, Clone)]
pub enum BindingValue<'a> {
    NativeFunction(
        fn(&mut Program<'a>, Rc<BindingValue<'a>>) -> Result<Rc<BindingValue<'a>>, Diagnostics>,
    ),
    Value(Rc<Value<'a>>),
}

pub struct Binding<'a> {
    pub name: &'a str,
    pub value: Rc<BindingValue<'a>>,
}

impl<'a> From<Variable<'a>> for Binding<'a> {
    fn from(value: Variable<'a>) -> Self {
        Self {
            name: value.name,
            value: Rc::new(BindingValue::Value(value.value)),
        }
    }
}

pub struct Program<'a> {
    bind_pool_stack: Vec<HashMap<&'a str, Rc<Binding<'a>>>>,
}

impl<'a> Default for Program<'a> {
    fn default() -> Self {
        Self {
            bind_pool_stack: vec![HashMap::default()],
        }
    }
}

impl<'a> Program<'a> {
    pub fn binding(&mut self, bind: Binding<'a>) {
        if let Some(pool) = self.bind_pool_stack.last_mut() {
            pool.insert(bind.name, Rc::new(bind));
        }
    }

    pub fn get_bind(&self, name: &'a str) -> Option<Rc<Binding<'a>>> {
        for pool in self.bind_pool_stack.iter().rev() {
            if let Some(bind) = pool.get(name) {
                return Some(bind.clone());
            }
        }
        None
    }
}

impl<'a> Program<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn execute_bind_value(
        &mut self,
        bind_value: Rc<BindingValue<'a>>,
    ) -> Result<Rc<BindingValue<'a>>, Diagnostics> {
        match bind_value.as_ref() {
            BindingValue::Value(value) => match value.as_ref() {
                Value::Variable(name) => {
                    if let Some(inner) = self.get_bind(name) {
                        Ok(self.execute_bind(inner)?)
                    } else {
                        Err(Diagnostics {
                            severity: Severity::Error,
                            span: Span::default(),
                            message: format!("undefined variable: {}", name),
                        })
                    }
                }
                Value::Call { callee, arg } => {
                    let callee =
                        self.execute_bind_value(Rc::new(BindingValue::Value(callee.clone())))?;
                    let arg = self.execute_bind_value(Rc::new(BindingValue::Value(arg.clone())))?;
                    match callee.as_ref() {
                        BindingValue::Value(callee) => match callee.as_ref() {
                            Value::Function { param, body } => {
                                let param = *param;
                                let mut func_pool = HashMap::new();
                                func_pool.insert(
                                    param,
                                    Rc::new(Binding {
                                        name: param,
                                        value: arg,
                                    }),
                                );

                                self.bind_pool_stack.push(func_pool);
                                let inner_result = self.execute_bind_value(Rc::new(
                                    BindingValue::Value(body.clone()),
                                ))?;
                                self.bind_pool_stack.pop();

                                Ok(inner_result)
                            }
                            _ => Err(Diagnostics {
                                severity: Severity::Error,
                                span: Span::default(),
                                message: format!("can not call value: {:?}", callee),
                            }),
                        },
                        BindingValue::NativeFunction(nf) => nf(self, arg),
                    }
                }
                _ => Ok(Rc::new(BindingValue::Value(value.clone()))),
            },
            BindingValue::NativeFunction(nf) => Ok(Rc::new(BindingValue::NativeFunction(*nf))),
        }
    }

    pub fn execute_bind(
        &mut self,
        bind: Rc<Binding<'a>>,
    ) -> Result<Rc<BindingValue<'a>>, Diagnostics> {
        match bind.value.as_ref() {
            BindingValue::Value(value) => {
                self.execute_bind_value(Rc::new(BindingValue::Value(value.clone())))
            }
            BindingValue::NativeFunction(nf) => {
                self.execute_bind_value(Rc::new(BindingValue::NativeFunction(nf.clone())))
            }
        }
    }
}
