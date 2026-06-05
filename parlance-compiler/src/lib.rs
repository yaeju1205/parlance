use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::Statement;
use parlance_vm::{
    Bytecode, DataPool, Instruction, OPERATOR_CALL, OPERATOR_LOAD_INT, OPERATOR_LOAD_STR,
    OPERATOR_MOVE, OPERATOR_RET, VirtualMachineData,
};

use crate::{
    desugarer::Desugarer,
    flattener::{Flatten, FlattenIndex, FlattenValue, FlattenValueKind, Flattener},
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
    pub value_to_register: HashMap<FlattenIndex, usize>,
}

impl Compiler {
    pub fn new(
        stats: Vec<Statement>,
        bytecode_funcs: Vec<impl BytecodeFunction>,
    ) -> Result<Self, Diagnostics> {
        let bindings = Desugarer::new().desugar(stats)?;
        let mut flatten_bindings = Vec::new();

        for bytecode_func in bytecode_funcs.iter() {
            flatten_bindings.push((
                Rc::from(bytecode_func.get_name().as_str()),
                FlattenValue {
                    span: Span::default(),
                    kind: FlattenValueKind::None,
                },
            ));
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
            value_to_register: HashMap::new(),
        };

        for (binding_idx, bytecode_func) in bytecode_funcs.iter().enumerate() {
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

    pub fn compile_value(
        &mut self,
        value_idx: FlattenIndex,
    ) -> Result<(usize, Bytecode), Diagnostics> {
        let flatten = self.flatten.clone();
        let Some(value) = flatten.file.get(value_idx) else {
            return Err(Diagnostics::compiler_error(
                format!("not found value {value_idx}"),
                Span::default(),
            ));
        };

        if let Some(&reg) = self.value_to_register.get(&value_idx) {
            return Ok((reg, Vec::new()));
        }

        let mut bytecode: Bytecode = Vec::new();

        match &value.kind {
            FlattenValueKind::FunctionCall { callee, arg } => {
                let mut actual_callee = *callee;
                while let FlattenValueKind::Variable(idx) = self.flatten.file[actual_callee].kind {
                    actual_callee = idx;
                }

                if !self.function_map.contains_key(&actual_callee) {
                    let (_, callee_bc) = self.compile_value(actual_callee)?;
                    bytecode.extend(callee_bc);
                }

                let Some(callee_func) = self.function_map.get(&actual_callee).cloned() else {
                    return Err(Diagnostics::compiler_error(
                        format!("not found function {actual_callee}"),
                        value.span.clone(),
                    ));
                };

                let (arg_reg, arg_bc) = self.compile_value(*arg)?;
                bytecode.extend(arg_bc);

                bytecode.push(Instruction {
                    operator: OPERATOR_MOVE,
                    a: callee_func.param_register,
                    b: arg_reg,
                    c: 0,
                });

                let ret_reg = self.register_allocator.alloc();
                bytecode.push(Instruction {
                    operator: OPERATOR_CALL,
                    a: ret_reg,
                    b: callee_func.pc,
                    c: arg_reg,
                });

                self.value_to_register.insert(value_idx, ret_reg);
                Ok((ret_reg, bytecode))
            }
            FlattenValueKind::Function { param, body } => {
                let param_register = self.register_allocator.alloc();
                self.value_to_register.insert(*param, param_register);

                let func = Rc::new(Function {
                    param_register,
                    pc: 0,
                });
                self.function_map.insert(value_idx, func);

                let (body_reg, body_bytecode) = self.compile_value(*body)?;

                let actual_pc = self.main_pc;
                self.function_map.insert(
                    value_idx,
                    Rc::new(Function {
                        param_register,
                        pc: actual_pc,
                    }),
                );

                let mut func_bytecode = body_bytecode;
                func_bytecode.push(Instruction {
                    operator: OPERATOR_RET,
                    a: body_reg,
                    b: 0,
                    c: 0,
                });

                self.main_pc += func_bytecode.len();
                self.function_bytecode.extend(func_bytecode);

                let func_node_reg = self.register_allocator.alloc();
                self.value_to_register.insert(value_idx, func_node_reg);
                Ok((func_node_reg, bytecode))
            }
            FlattenValueKind::Int(int_value) => {
                let reg = self.register_allocator.alloc();
                bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_INT,
                    a: reg,
                    b: *int_value as u32 as usize,
                    c: 0,
                });
                self.value_to_register.insert(value_idx, reg);
                Ok((reg, bytecode))
            }
            FlattenValueKind::String(str_value) => {
                let pool_idx = if let Some(idx) = self.string_cache.get(str_value) {
                    *idx
                } else {
                    let idx = self.data_pool.len();
                    self.data_pool
                        .push(VirtualMachineData::StrPtr(str_value.as_str()));
                    self.string_cache.insert(str_value.clone(), idx);
                    idx
                };

                let reg = self.register_allocator.alloc();
                bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_STR,
                    a: reg,
                    b: pool_idx,
                    c: 0,
                });
                self.value_to_register.insert(value_idx, reg);
                Ok((reg, bytecode))
            }
            FlattenValueKind::Variable(idx) => {
                let (reg, var_bc) = self.compile_value(*idx)?;
                self.value_to_register.insert(value_idx, reg);
                bytecode.extend(var_bc);
                Ok((reg, bytecode))
            }
            FlattenValueKind::None => {
                let reg = self.register_allocator.alloc();
                self.value_to_register.insert(value_idx, reg);
                Ok((reg, bytecode))
            }
        }
    }

    pub fn compile(
        mut self,
        binding_name: &str,
    ) -> Result<(usize, Bytecode, DataPool), Diagnostics> {
        if let Some(value_idx) = self.flatten.clone().bindings.get(binding_name) {
            let (_, main_bytecode) = self.compile_value(*value_idx)?;
            let func_bytecode = self.function_bytecode;

            let mut bytecode: Bytecode = Vec::new();
            bytecode.extend(func_bytecode);
            bytecode.extend(main_bytecode);

            Ok((self.main_pc, bytecode, self.data_pool))
        } else {
            Err(Diagnostics::compiler_error(
                format!("not found binding '{binding_name}'"),
                Span::default(),
            ))
        }
    }
}

