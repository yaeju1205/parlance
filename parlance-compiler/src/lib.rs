use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::Statement;
use parlance_vm::{
    Bytecode, DataPool, Instruction, OPERATOR_CALL, OPERATOR_CALL_REG, OPERATOR_LOAD_FUNC,
    OPERATOR_LOAD_INT, OPERATOR_LOAD_STR, OPERATOR_MOVE, OPERATOR_RET, VirtualMachineData,
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
    pub register_bindings: HashMap<FlattenIndex, usize>,
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
            register_bindings: HashMap::new(),
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

        if let Some(&reg) = self.register_bindings.get(&value_idx) {
            return Ok((reg, Vec::new()));
        }

        let mut bytecode: Bytecode = Vec::new();

        match &value.kind {
            FlattenValueKind::FunctionCall { callee, arg } => {
                let mut callee_idx = *callee;
                while let FlattenValueKind::Variable(idx) = self.flatten.file[callee_idx].kind {
                    callee_idx = idx;
                }

                let (arg_reg, arg_bc) = self.compile_value(*arg)?;
                let ret_reg = self.register_allocator.alloc();

                let mut bytecode: Bytecode = Vec::new();

                if let Some(callee_func) = self.function_map.get(&callee_idx).cloned() {
                    bytecode.extend(arg_bc);

                    bytecode.push(Instruction {
                        operator: OPERATOR_MOVE,
                        a: callee_func.param_register,
                        b: arg_reg,
                        c: 0,
                    });

                    bytecode.push(Instruction {
                        operator: OPERATOR_CALL,
                        a: ret_reg,
                        b: callee_func.pc,
                        c: arg_reg,
                    });
                } else {
                    let (callee_reg, callee_bc) = self.compile_value(*callee)?;

                    bytecode.extend(callee_bc);
                    bytecode.extend(arg_bc);

                    bytecode.push(Instruction {
                        operator: OPERATOR_CALL_REG,
                        a: ret_reg,
                        b: callee_reg,
                        c: arg_reg,
                    });
                }

                self.register_bindings.insert(value_idx, ret_reg);
                Ok((ret_reg, bytecode))
            }
            FlattenValueKind::Function { param, body } => {
                let param_register = self.register_allocator.alloc();
                self.register_bindings.insert(*param, param_register);

                let (body_register, body_bytecode) = self.compile_value(*body)?;

                let func_pc = self.main_pc;
                let func = Rc::new(Function {
                    param_register,
                    pc: func_pc,
                });
                self.function_map.insert(value_idx, func);

                let mut func_bytecode = body_bytecode;
                func_bytecode.push(Instruction {
                    operator: OPERATOR_RET,
                    a: body_register,
                    b: 0,
                    c: 0,
                });

                self.main_pc += func_bytecode.len();
                self.function_bytecode.extend(func_bytecode);

                let dest = self.register_allocator.alloc();
                bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_FUNC,
                    a: dest,
                    b: func_pc,
                    c: param_register,
                });
                self.register_bindings.insert(value_idx, dest);
                Ok((dest, bytecode))
            }
            FlattenValueKind::Int(int_value) => {
                let dest = self.register_allocator.alloc();
                bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_INT,
                    a: dest,
                    b: *int_value as u32 as usize,
                    c: 0,
                });
                self.register_bindings.insert(value_idx, dest);
                Ok((dest, bytecode))
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

                let dest = self.register_allocator.alloc();
                bytecode.push(Instruction {
                    operator: OPERATOR_LOAD_STR,
                    a: dest,
                    b: pool_idx,
                    c: 0,
                });
                self.register_bindings.insert(value_idx, dest);
                Ok((dest, bytecode))
            }
            FlattenValueKind::Variable(idx) => {
                let (dest, var_bc) = self.compile_value(*idx)?;
                self.register_bindings.insert(value_idx, dest);
                bytecode.extend(var_bc);
                Ok((dest, bytecode))
            }
            FlattenValueKind::None => {
                let dest = self.register_allocator.alloc();
                self.register_bindings.insert(value_idx, dest);
                Ok((dest, bytecode))
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

            println!("start pc {}", self.main_pc);
            Ok((self.main_pc, bytecode, self.data_pool))
        } else {
            Err(Diagnostics::compiler_error(
                format!("not found binding '{binding_name}'"),
                Span::default(),
            ))
        }
    }
}
