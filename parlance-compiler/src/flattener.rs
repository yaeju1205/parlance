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

#[derive(Debug, Default)]
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

    pub fn insert_binding(&mut self, (name, value): (Rc<str>, FlattenValue)) {
        let value_idx = self.alloc(value);
        self.binding_pool.insert(name, value_idx);
    }

    pub fn alloc(&mut self, value: FlattenValue) -> FlattenIndex {
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

    fn flatten_binding_scheme(
        &mut self,
        scheme: &[DesugarBinding],
        reserved_indices: &HashMap<Rc<str>, FlattenIndex>,
    ) -> Result<(), Diagnostics> {
        for scheme_binding in scheme {
            let reserved_idx = *reserved_indices.get(&scheme_binding.name).unwrap();

            if scheme_binding.scheme.is_empty() {
                let actual_idx = self.flatten_value(scheme_binding.value.clone())?;
                self.flatten_file[reserved_idx] = self.flatten_file[actual_idx].clone();
            } else {
                let saved_scope = self.binding_scope.clone();

                let mut nested_scope = self.binding_scope.clone();
                let mut nested_reserved = HashMap::new();
                for nested_binding in &scheme_binding.scheme {
                    let nested_idx = self.alloc(FlattenValue {
                        span: nested_binding.value.span.clone(),
                        kind: FlattenValueKind::None,
                    });
                    nested_scope.insert(nested_binding.name.clone(), nested_idx);
                    nested_reserved.insert(nested_binding.name.clone(), nested_idx);
                }
                self.binding_scope = nested_scope;

                self.flatten_binding_scheme(&scheme_binding.scheme, &nested_reserved)?;

                let actual_idx = self.flatten_value(scheme_binding.value.clone())?;
                self.flatten_file[reserved_idx] = self.flatten_file[actual_idx].clone();

                self.binding_scope = saved_scope;
            }
        }
        Ok(())
    }

    fn flatten_value_with_scheme(
        &mut self,
        value: Rc<DesugarValue>,
        scheme: Vec<DesugarBinding>,
    ) -> Result<usize, Diagnostics> {
        match &value.kind {
            DesugarValueKind::Function { param, body } => {
                let param_index = self.alloc(FlattenValue {
                    span: param.span.clone(),
                    kind: FlattenValueKind::None,
                });

                let parent_scope = self.binding_scope.clone();
                self.binding_scope.insert(param.name.clone(), param_index);

                let body_idx = self.flatten_value_with_scheme(body.clone(), scheme)?;

                self.binding_scope = parent_scope;

                Ok(self.alloc(FlattenValue {
                    span: value.span.clone(),
                    kind: FlattenValueKind::Function {
                        param: param_index,
                        body: body_idx,
                    },
                }))
            }
            _ => {
                if scheme.is_empty() {
                    return self.flatten_value(value);
                }

                let saved_scope = self.binding_scope.clone();

                let mut scope = self.binding_scope.clone();
                let mut reserved_indices = HashMap::new();
                for scheme_binding in &scheme {
                    let idx = self.alloc(FlattenValue {
                        span: scheme_binding.value.span.clone(),
                        kind: FlattenValueKind::None,
                    });
                    scope.insert(scheme_binding.name.clone(), idx);
                    reserved_indices.insert(scheme_binding.name.clone(), idx);
                }
                self.binding_scope = scope;

                self.flatten_binding_scheme(&scheme, &reserved_indices)?;

                let result = self.flatten_value(value.clone())?;

                self.binding_scope = saved_scope;

                Ok(result)
            }
        }
    }

    pub fn flatten_binding(
        &mut self,
        binding: DesugarBinding,
    ) -> Result<FlattenIndex, Diagnostics> {
        let parent_scope = self.binding_scope.clone();

        let main_reserved_idx = self.alloc(FlattenValue {
            span: binding.value.span.clone(),
            kind: FlattenValueKind::None,
        });
        self.binding_scope
            .insert(binding.name.clone(), main_reserved_idx);

        let actual_main_idx = if binding.scheme.is_empty() {
            self.flatten_value(binding.value.clone())?
        } else {
            self.flatten_value_with_scheme(binding.value.clone(), binding.scheme.clone())?
        };

        let actual_main_value = self.flatten_file[actual_main_idx].clone();
        self.flatten_file[main_reserved_idx] = actual_main_value;

        self.binding_pool
            .insert(binding.name.clone(), main_reserved_idx);

        self.binding_scope = parent_scope;

        Ok(main_reserved_idx)
    }

    pub fn flatten(mut self, bindings: Vec<DesugarBinding>) -> Result<Flatten, Diagnostics> {
        let mut flatten_bindings = HashMap::new();

        for (name, value_idx) in self.binding_pool.iter() {
            flatten_bindings.insert(name.clone(), *value_idx);
        }

        for binding in bindings.into_iter() {
            flatten_bindings.insert(binding.name.clone(), self.flatten_binding(binding)?);
        }

        Ok(Flatten {
            file: self.flatten_file,
            bindings: flatten_bindings,
        })
    }
}
