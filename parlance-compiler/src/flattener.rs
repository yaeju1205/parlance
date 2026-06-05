use crate::desugarer::{DesugarBinding, DesugarValue, DesugarValueKind};
use parlance_diagnostics::{Diagnostics, Span};
use std::{collections::HashMap, rc::Rc};

pub type FlattenIndex = usize;

#[derive(Debug, Clone)]
pub enum FlattenValueKind {
    Variable(FlattenIndex),
    Function {
        param: FlattenIndex,
        body: FlattenIndex,
    },
    FunctionCall {
        callee: FlattenIndex,
        arg: FlattenIndex,
    },
    String(String),
    Int(i32),
    None,
}

#[derive(Debug, Clone)]
pub struct FlattenValue {
    pub span: Span,
    pub kind: FlattenValueKind,
}

#[derive(Debug)]
pub struct Flatten {
    pub file: Vec<Rc<FlattenValue>>,
    pub bindings: HashMap<Rc<str>, FlattenIndex>,
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

    pub fn with_bindings(mut self, bindings: Vec<(Rc<str>, FlattenValue)>) -> Self {
        for (binding_name, binding_value) in bindings.into_iter() {
            let value_idx = self.alloc(binding_value);
            self.binding_pool.insert(binding_name, value_idx);
        }
        self
    }

    fn alloc(&mut self, value: FlattenValue) -> FlattenIndex {
        let idx = self.flatten_file.len();
        self.flatten_file.push(Rc::new(value));
        idx
    }

    pub fn flatten_value(&mut self, value: Rc<DesugarValue>) -> Result<usize, Diagnostics> {
        match &value.kind {
            DesugarValueKind::Variable { name } => {
                if let Some(variable_index) = self.binding_scope.get(name) {
                    return Ok(self.alloc(FlattenValue {
                        span: value.span.clone(),
                        kind: FlattenValueKind::Variable(*variable_index),
                    }));
                }

                if let Some(binding_value) = self.binding_pool.get(name) {
                    Ok(self.alloc(FlattenValue {
                        span: value.span.clone(),
                        kind: FlattenValueKind::Variable(*binding_value),
                    }))
                } else {
                    Err(Diagnostics::compiler_error(
                        format!("not found variable '{name}'"),
                        value.span.clone(),
                    ))
                }
            }
            DesugarValueKind::Function { param, body } => {
                let param_index = self.alloc(FlattenValue {
                    span: param.span.clone(),
                    kind: FlattenValueKind::None,
                });

                let parent_scope = self.binding_scope.clone();
                self.binding_scope.insert(param.name.clone(), param_index);

                let body_idx = self.flatten_value(body.clone())?;

                self.binding_scope = parent_scope;

                Ok(self.alloc(FlattenValue {
                    span: value.span.clone(),
                    kind: FlattenValueKind::Function {
                        param: param_index,
                        body: body_idx,
                    },
                }))
            }
            DesugarValueKind::FunctionCall { callee, arg } => {
                let callee_idx = self.flatten_value(callee.clone())?;
                let arg_idx = self.flatten_value(arg.clone())?;

                Ok(self.alloc(FlattenValue {
                    span: value.span.clone(),
                    kind: FlattenValueKind::FunctionCall {
                        callee: callee_idx,
                        arg: arg_idx,
                    },
                }))
            }
            DesugarValueKind::String(str_value) => Ok(self.alloc(FlattenValue {
                span: value.span.clone(),
                kind: FlattenValueKind::String(str_value.to_string()),
            })),
            DesugarValueKind::Int(int_value) => Ok(self.alloc(FlattenValue {
                span: value.span.clone(),
                kind: FlattenValueKind::Int(int_value.clone()),
            })),
        }
    }

    pub fn flatten_binding(
        &mut self,
        binding: Rc<DesugarBinding>,
    ) -> Result<FlattenIndex, Diagnostics> {
        let parent_scope = self.binding_scope.clone();
        let mut scope = HashMap::new();

        for scheme_binding in binding.scheme.iter() {
            let scheme_value = self.flatten_value(scheme_binding.value.clone())?;
            scope.insert(scheme_binding.name.clone(), scheme_value);
        }

        self.binding_scope = scope;

        let value_idx = self.flatten_value(binding.value.clone())?;
        self.binding_pool.insert(binding.name.clone(), value_idx);

        self.binding_scope = parent_scope;

        Ok(value_idx)
    }

    pub fn flatten(mut self, bindings: Vec<DesugarBinding>) -> Result<Flatten, Diagnostics> {
        let mut flatten_bindings = HashMap::new();

        for (name, value_idx) in self.binding_pool.iter() {
            flatten_bindings.insert(name.clone(), *value_idx);
        }

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
