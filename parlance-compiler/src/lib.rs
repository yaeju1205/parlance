use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::Statement;
use parlance_vm::{
    Bytecode, DataPool, Instruction, OPERATOR_CALL, OPERATOR_LOAD_INT, OPERATOR_LOAD_STR,
    OPERATOR_RET, OPERATOR_STOP, VirtualMachineData,
};

use crate::{
    desugarer::Desugarer,
    flattener::{Flatten, FlattenBinding, FlattenIndex, FlattenValueKind, Flattener},
};

mod desugarer;
mod flattener;

#[derive(Default)]
pub struct Allocator {
    pub register: usize,
}

impl Allocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc(&mut self) -> usize {
        let reg = self.register;
        self.register += 1;
        reg
    }
}

pub trait BytecodeFunction {
    fn get_name(&self) -> String;
    fn build_bytecode(&self, compiler: &mut Compiler, dest: usize) -> ();
}

pub struct Compiler {
    pub register_allocator: Allocator,
    pub function_map: HashMap<FlattenIndex, usize>,
    pub string_cache: HashMap<String, usize>,
    pub flatten: Rc<Flatten>,
    pub bytecode: Bytecode,
    pub data_pool: DataPool,
}

impl Compiler {
    pub fn new(stats: Vec<Statement>) -> Result<Self, Diagnostics> {
        let bindings = Desugarer::new().desugar(stats)?;
        let flatten = Rc::new(Flattener::new().flatten(bindings)?);
        Ok(Self {
            register_allocator: Allocator::new(),
            function_map: HashMap::new(),
            string_cache: HashMap::new(),
            bytecode: Vec::new(),
            data_pool: Vec::new(),
            flatten,
        })
    }

    pub fn with_bytecode_functions(mut self, bytecode_funcs: Vec<impl BytecodeFunction>) -> Self {
        let mut bindings: HashMap<Rc<str>, FlattenBinding> = HashMap::new();

        for bytecode_func in bytecode_funcs.into_iter() {
            let bytecode_func_idx = self.flatten.bindings.len() + 1;
            bindings.insert(
                Rc::from(bytecode_func.get_name().as_str()),
                FlattenBinding {
                    span: Span::default(),
                    value: bytecode_func_idx,
                },
            );

            let bytecode_func_dest = self.register_allocator.alloc();
            self.function_map
                .insert(bytecode_func_idx, bytecode_func_dest);
            bytecode_func.build_bytecode(&mut self, bytecode_func_dest);
        }

        bindings.extend(self.flatten.bindings.clone());
        let flatten = Flatten {
            file: self.flatten.file.clone(),
            bindings,
        };

        self.flatten = Rc::new(flatten);
        self
    }

    pub fn compile_value(&mut self, value_idx: FlattenIndex) -> Result<usize, Diagnostics> {
        let value = self.flatten.file[value_idx].clone();
        match &value.kind {
            FlattenValueKind::Int(int_value) => {
                let dest = self.register_allocator.alloc();

                self.bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_INT,
                    a: dest,
                    b: int_value.clone() as u32 as usize,
                    c: 0,
                });

                Ok(dest)
            }
            FlattenValueKind::String(str_value) => {
                let pool_idx = *self
                    .string_cache
                    .entry(str_value.clone())
                    .or_insert_with(|| {
                        let idx = self.data_pool.len();
                        let static_ptr: *const str = str_value.as_str();
                        self.data_pool.push(VirtualMachineData::StrPtr(static_ptr));
                        idx
                    });

                let dest = self.register_allocator.alloc();

                self.bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_STR,
                    a: dest,
                    b: pool_idx,
                    c: 0,
                });

                Ok(dest)
            }
            FlattenValueKind::FunctionCall { callee, arg } => {
                let callee_pc = if let Some(reg) = self.function_map.get(callee) {
                    *reg
                } else {
                    self.compile_value(*callee)?
                };

                let arg_reg = self.compile_value(*arg)?;

                let dest = self.register_allocator.alloc();

                self.bytecode.push(Instruction {
                    operator: OPERATOR_CALL,
                    a: dest,
                    b: callee_pc.clone(),
                    c: arg_reg,
                });

                Ok(dest)
            }
            FlattenValueKind::Variable(idx) => self.compile_value(*idx),
            FlattenValueKind::Function { body, .. } => {
                let pc = self.bytecode.len();
                let dest = self.register_allocator.alloc();
                let body_reg = self.compile_value(*body)?;

                if let None = self.function_map.get(&value_idx) {
                    self.function_map.insert(value_idx, body_reg);
                }

                self.bytecode.push(Instruction {
                    operator: OPERATOR_RET,
                    a: dest,
                    b: 0,
                    c: 0,
                });

                Ok(pc)
            }
            FlattenValueKind::Param { .. } => Ok(0),
        }
    }

    pub fn compile_binding(&mut self, binding: &FlattenBinding) -> Result<usize, Diagnostics> {
        let value = self.compile_value(binding.value)?;
        self.bytecode.push(Instruction {
            operator: OPERATOR_STOP,
            a: 0,
            b: 0,
            c: 0,
        });
        Ok(value)
    }

    pub fn compile(
        mut self,
        binding_name: &str,
    ) -> Result<(usize, Bytecode, DataPool), Diagnostics> {
        if let Some(binding) = self.flatten.clone().bindings.get(binding_name) {
            Ok((
                self.compile_binding(binding)?,
                self.bytecode,
                self.data_pool,
            ))
        } else {
            Err(Diagnostics::compiler_error(
                format!("not defined binding '{binding_name}"),
                Span::default(),
            ))
        }
    }
}
