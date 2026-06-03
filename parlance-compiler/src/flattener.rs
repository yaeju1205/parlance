use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};

use crate::desugarer::{DesugarBinding, DesugarValue, DesugarValueKind};

pub type FlattenIndex = usize;

#[derive(Debug)]
pub enum FlattenValueKind {
    Variable(FlattenIndex),
    Function {
        param: FlattenIndex,
        body: FlattenIndex,
        size: usize,
    },
    Param,
    FunctionCall {
        callee: FlattenIndex,
        arg: FlattenIndex,
    },
    String(String),
    Int(i32),
}

pub struct FlattenValue {
    pub span: Span,
    pub kind: FlattenValueKind,
}

#[derive(Clone)]
pub struct FlattenBinding {
    pub span: Span,
    pub value: FlattenIndex,
}

pub struct Flatten {
    pub file: Vec<Rc<FlattenValue>>,
    pub bindings: HashMap<Rc<str>, FlattenBinding>,
}

#[derive(Default)]
pub struct Flattener {
    flatten_file: Vec<Rc<FlattenValue>>,
    binding_pool: HashMap<Rc<str>, FlattenIndex>,
    binding_scope: HashMap<Rc<str>, FlattenIndex>,
}

impl Flattener {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc(&mut self, value: FlattenValue) -> FlattenIndex {
        let idx = self.flatten_file.len();
        self.flatten_file.push(Rc::new(value));
        idx
    }

    pub fn flatten_value(&mut self, value: Rc<DesugarValue>) -> Result<FlattenValue, Diagnostics> {
        match &value.kind {
            DesugarValueKind::Variable { name } => {
                if let Some(variable_index) = self.binding_scope.get(name) {
                    return Ok(FlattenValue {
                        span: value.span.clone(),
                        kind: FlattenValueKind::Variable(*variable_index),
                    });
                }

                if let Some(binding_value) = self.binding_pool.get(name) {
                    Ok(FlattenValue {
                        span: value.span.clone(),
                        kind: FlattenValueKind::Variable(*binding_value),
                    })
                } else {
                    Err(Diagnostics::compiler_error(
                        format!("unknown variable '{name}'"),
                        value.span.clone(),
                    ))
                }
            }
            DesugarValueKind::Function { param, body } => {
                let param_index = self.alloc(FlattenValue {
                    span: param.span.clone(),
                    kind: FlattenValueKind::Param,
                });

                let parent_scope = self.binding_scope.clone();

                let mut scope = self.binding_scope.clone();
                scope.insert(param.name.clone(), param_index);

                self.binding_scope = scope;

                let body_value = self.flatten_value(body.clone())?;

                self.binding_scope = parent_scope;

                Ok(FlattenValue {
                    span: value.span.clone(),
                    kind: FlattenValueKind::Function {
                        param: param_index,
                        body: self.alloc(body_value),
                        size: self.flatten_file.len() - param_index,
                    },
                })
            }
            DesugarValueKind::FunctionCall { callee, arg } => {
                let callee_value = self.flatten_value(callee.clone())?;
                let arg_value = self.flatten_value(arg.clone())?;

                Ok(FlattenValue {
                    span: value.span.clone(),
                    kind: FlattenValueKind::FunctionCall {
                        callee: self.alloc(callee_value),
                        arg: self.alloc(arg_value),
                    },
                })
            }
            DesugarValueKind::String(str_value) => Ok(FlattenValue {
                span: value.span.clone(),
                kind: FlattenValueKind::String(str_value.to_string()),
            }),
            DesugarValueKind::Int(int_value) => Ok(FlattenValue {
                span: value.span.clone(),
                kind: FlattenValueKind::Int(*int_value),
            }),
        }
    }

    pub fn flatten_binding(
        &mut self,
        binding: Rc<DesugarBinding>,
    ) -> Result<FlattenBinding, Diagnostics> {
        let value = self.flatten_value(binding.value.clone())?;
        let mut scope = HashMap::new();

        for scheme_binding in binding.scheme.iter() {
            let scheme_value = self.flatten_value(scheme_binding.value.clone())?;
            scope.insert(scheme_binding.name.clone(), self.alloc(scheme_value));
        }

        self.binding_scope = scope;

        let flatten_binding = FlattenBinding {
            span: binding.span.clone(),
            value: self.alloc(value),
        };

        self.binding_pool
            .insert(binding.name.clone(), flatten_binding.value);

        Ok(flatten_binding)
    }

    pub fn flatten(mut self, bindings: Vec<DesugarBinding>) -> Result<Flatten, Diagnostics> {
        let mut flatten_bindings = HashMap::new();

        for binding in bindings.into_iter() {
            flatten_bindings.insert(
                binding.name.clone(),
                self.flatten_binding(Rc::new(binding))?,
            );
        }

        Ok(Flatten {
            file: self.flatten_file,
            bindings: flatten_bindings,
        })
    }
}
