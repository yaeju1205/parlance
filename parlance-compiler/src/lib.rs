use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::Statement;
use parlance_vm::{
    Bytecode, DataPool, Instruction, OPERATOR_CALL, OPERATOR_LOAD_INT, OPERATOR_LOAD_STR,
    OPERATOR_MOVE, VirtualMachineData,
};

use crate::{
    desugarer::Desugarer,
    flattener::{Flatten, FlattenIndex, FlattenValueKind, Flattener},
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

pub struct Function {
    pub param_register: usize,
    pub pc: usize,
}

pub trait BytecodeFunction {
    fn get_name(&self) -> String;
    fn build_bytecode(&self, compiler: &mut Compiler, func: Rc<Function>) -> Bytecode;
}

pub struct Compiler {
    pub register_allocator: Allocator,
    pub main_pc: usize,
    pub function_bytecode: Bytecode,
    pub function_map: HashMap<FlattenIndex, Rc<Function>>,
    pub string_cache: HashMap<String, usize>,
    pub data_pool: DataPool,
    pub flatten: Rc<Flatten>,
}

impl Compiler {
    pub fn new(
        stats: Vec<Statement>,
        bytecode_funcs: Vec<impl BytecodeFunction>,
    ) -> Result<Self, Diagnostics> {
        let bindings = Desugarer::new().desugar(stats)?;
        let mut flatten_bindings: HashMap<Rc<str>, FlattenIndex> = HashMap::new();

        for (binding_ndx, bytecode_func) in bytecode_funcs.iter().enumerate() {
            flatten_bindings.insert(Rc::from(bytecode_func.get_name().as_str()), binding_ndx);
        }

        let flatten = Rc::new(
            Flattener::new()
                .with_bindings(flatten_bindings)
                .flatten(bindings)?,
        );

        let mut compiler = Self {
            register_allocator: Allocator::new(),
            main_pc: 0,
            function_bytecode: Vec::new(),
            function_map: HashMap::new(),
            string_cache: HashMap::new(),
            data_pool: Vec::new(),
            flatten,
        };

        for (binding_idx, bytecode_func) in bytecode_funcs.into_iter().enumerate() {
            let func = Rc::new(Function {
                param_register: compiler.register_allocator.alloc(),
                pc: compiler.main_pc,
            });
            let bytecode = bytecode_func.build_bytecode(&mut compiler, func.clone());
            compiler.main_pc += bytecode.len();
            compiler.function_bytecode.extend(bytecode);
            compiler.function_map.insert(binding_idx, func);
        }

        Ok(compiler)
    }

    pub fn compile_value(&mut self, value_idx: FlattenIndex) -> Result<Bytecode, Diagnostics> {
        let flatten = self.flatten.clone();
        let Some(value) = flatten.file.get(value_idx) else {
            return Err(Diagnostics::compiler_error(
                format!("not found value {value_idx}"),
                Span::default(),
            ));
        };

        let mut bytecode: Bytecode = Vec::new();

        match &value.kind {
            FlattenValueKind::FunctionCall { callee, arg } => {
                if !self.function_map.contains_key(callee) {
                    self.compile_value(*callee)?;
                }

                let Some(callee) = self.function_map.get(callee).cloned() else {
                    return Err(Diagnostics::compiler_error(
                        format!("not found function {callee}"),
                        value.span.clone(),
                    ));
                };

                bytecode.extend(self.compile_value(*arg)?);

                let arg_reg = self.register_allocator.register - 1;

                bytecode.push(Instruction {
                    operator: OPERATOR_MOVE,
                    a: callee.param_register,
                    b: arg_reg,
                    c: 0,
                });
                bytecode.push(Instruction {
                    operator: OPERATOR_CALL,
                    a: self.register_allocator.alloc(),
                    b: callee.pc,
                    c: arg_reg,
                });

                Ok(bytecode)
            }
            FlattenValueKind::Function { body, .. } => {
                let bytecode = self.compile_value(*body)?;

                self.main_pc += bytecode.len();
                self.function_bytecode.extend(bytecode);
                self.function_map.insert(
                    value_idx,
                    Rc::new(Function {
                        param_register: self.register_allocator.alloc(),
                        pc: self.main_pc,
                    }),
                );

                Ok(Vec::new())
            }
            FlattenValueKind::Param { include_in } => {
                if self.function_map.contains_key(include_in) {
                    Ok(Vec::new())
                } else {
                    Err(Diagnostics::compiler_error(
                        format!("param {value_idx} is not include in {include_in}"),
                        value.span.clone(),
                    ))
                }
            }
            FlattenValueKind::Int(int_value) => {
                bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_INT,
                    a: self.register_allocator.alloc(),
                    b: int_value.clone() as u32 as usize,
                    c: 0,
                });

                Ok(bytecode)
            }
            FlattenValueKind::String(str_value) => {
                let pool_idx = if let Some(idx) = self.string_cache.get(str_value) {
                    *idx
                } else {
                    let idx = self.data_pool.len();
                    self.data_pool
                        .push(VirtualMachineData::StrPtr(str_value.as_str()));
                    idx
                };

                bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_STR,
                    a: self.register_allocator.alloc(),
                    b: pool_idx,
                    c: 0,
                });

                Ok(bytecode)
            }
            FlattenValueKind::Variable(idx) => self.compile_value(*idx),
        }
    }

    pub fn compile(
        mut self,
        binding_name: &str,
    ) -> Result<(usize, Bytecode, DataPool), Diagnostics> {
        if let Some(value_idx) = self.flatten.clone().bindings.get(binding_name) {
            let main_bytecode = self.compile_value(*value_idx)?;
            let func_bytecode = self.function_bytecode;

            let mut bytecode: Bytecode = Vec::new();
            bytecode.extend(func_bytecode);
            bytecode.extend(main_bytecode);

            Ok((self.main_pc, bytecode, self.data_pool))
        } else {
            Err(Diagnostics::compiler_error(
                format!("not found binding '{binding_name}"),
                Span::default(),
            ))
        }
    }
}
