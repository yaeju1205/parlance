use std::{collections::HashMap, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::Parser;
use parlance_vm::{Bytecode, DataPool, Instruction, Operator, VirtualMachineData};

use crate::{
    desugarer::desugar,
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

pub struct BytecodeFunction {
    pub name: String,
    pub build: fn(compile_object: &mut CompileObject, func: Rc<Function>) -> Bytecode,
}

pub struct CompileObject {
    pub flatten: Rc<Flatten>,
    pub allocator: Allocator,
    pub function_map: HashMap<FlattenIndex, Rc<Function>>,
    pub function_bytecode: Bytecode,
    pub binding_map: HashMap<FlattenIndex, usize>,
    pub string_cache: HashMap<String, usize>,
    pub data_pool: DataPool,
}

impl CompileObject {
    pub fn build_value(
        &mut self,
        value_idx: FlattenIndex,
        is_tail: bool,
    ) -> Result<(usize, Bytecode), Diagnostics> {
        let Some(value) = self.flatten.file.get(value_idx) else {
            return Err(Diagnostics::compiler_error(
                format!("not found value {value_idx}"),
                Span::default(),
            ));
        };

        if let Some(&reg) = self.binding_map.get(&value_idx) {
            return Ok((reg, Vec::new()));
        }

        let mut bytecode: Bytecode = Vec::new();

        match &value.kind {
            FlattenValueKind::FunctionCall { callee, arg } => {
                let mut callee_idx = *callee;
                while let FlattenValueKind::Variable(idx) = self.flatten.file[callee_idx].kind {
                    callee_idx = idx;
                }

                let (arg_reg, arg_bc) = self.build_value(*arg, false)?;

                let mut bytecode: Bytecode = Vec::new();

                if let Some(callee_func) = self.function_map.get(&callee_idx).cloned() {
                    bytecode.extend(arg_bc);

                    bytecode.push(Instruction {
                        operator: Operator::Mov,
                        a: callee_func.param_register,
                        b: arg_reg,
                        c: 0,
                    });

                    if is_tail {
                        bytecode.push(Instruction {
                            operator: Operator::Goto,
                            a: callee_func.pc,
                            b: 0,
                            c: 0,
                        });

                        Ok((0, bytecode))
                    } else {
                        let ret_reg = self.allocator.alloc();
                        bytecode.push(Instruction {
                            operator: Operator::Call,
                            a: ret_reg,
                            b: callee_func.pc,
                            c: arg_reg,
                        });

                        self.binding_map.insert(value_idx, ret_reg);
                        Ok((ret_reg, bytecode))
                    }
                } else {
                    let (callee_reg, callee_bc) = self.build_value(callee_idx, false)?;

                    bytecode.extend(arg_bc);
                    bytecode.extend(callee_bc);

                    if is_tail {
                        bytecode.push(Instruction {
                            operator: Operator::TailCallReg,
                            a: 0,
                            b: callee_reg,
                            c: arg_reg,
                        });

                        Ok((0, bytecode))
                    } else {
                        let ret_reg = self.allocator.alloc();
                        bytecode.push(Instruction {
                            operator: Operator::CallReg,
                            a: ret_reg,
                            b: callee_reg,
                            c: arg_reg,
                        });

                        self.binding_map.insert(value_idx, ret_reg);
                        Ok((ret_reg, bytecode))
                    }
                }
            }
            FlattenValueKind::Function { param, body } => {
                let param_register = self.allocator.alloc();
                self.binding_map.insert(*param, param_register);

                let dest = self.allocator.alloc();
                self.binding_map.insert(value_idx, dest);

                let (body_register, body_bytecode) = self.build_value(*body, true)?;

                let func_pc = self.function_bytecode.len();
                let func = Rc::new(Function {
                    param_register,
                    pc: func_pc,
                });
                self.function_map.insert(value_idx, func);

                let mut func_bytecode = body_bytecode;
                func_bytecode.push(Instruction {
                    operator: Operator::Ret,
                    a: body_register,
                    b: 0,
                    c: 0,
                });

                self.function_bytecode.extend(func_bytecode);

                bytecode.push(Instruction {
                    operator: Operator::LoadFunc,
                    a: dest,
                    b: func_pc,
                    c: param_register,
                });
                Ok((dest, bytecode))
            }
            FlattenValueKind::Int(int_value) => {
                let dest = self.allocator.alloc();
                bytecode.push(Instruction {
                    operator: Operator::LoadInt,
                    a: dest,
                    b: *int_value as u32 as usize,
                    c: 0,
                });
                self.binding_map.insert(value_idx, dest);
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

                let dest = self.allocator.alloc();
                bytecode.push(Instruction {
                    operator: Operator::LoadStr,
                    a: dest,
                    b: pool_idx,
                    c: 0,
                });
                self.binding_map.insert(value_idx, dest);
                Ok((dest, bytecode))
            }
            FlattenValueKind::Variable(idx) => {
                let (dest, var_bc) = self.build_value(*idx, is_tail)?;
                self.binding_map.insert(value_idx, dest);
                bytecode.extend(var_bc);
                Ok((dest, bytecode))
            }
            FlattenValueKind::None => {
                if let Some(func) = self.function_map.get(&value_idx).cloned() {
                    let dest = self.allocator.alloc();
                    bytecode.push(Instruction {
                        operator: Operator::LoadFunc,
                        a: dest,
                        b: func.pc,
                        c: func.param_register,
                    });
                    self.binding_map.insert(value_idx, dest);
                    Ok((dest, bytecode))
                } else {
                    let dest = self.allocator.alloc();
                    self.binding_map.insert(value_idx, dest);
                    Ok((dest, bytecode))
                }
            }
        }
    }

    pub fn build_binding(
        mut self,
        binding_name: &str,
    ) -> Result<(usize, Bytecode, DataPool), Diagnostics> {
        if let Some(value_idx) = self.flatten.clone().bindings.get(binding_name) {
            let (_, main_bytecode) = self.build_value(*value_idx, false)?;
            let func_bytecode = self.function_bytecode;
            let pc = func_bytecode.len();

            let mut bytecode: Bytecode = Vec::new();
            bytecode.extend(func_bytecode);
            bytecode.extend(main_bytecode);

            Ok((pc, bytecode, self.data_pool))
        } else {
            Err(Diagnostics::compiler_error(
                format!("not found binding '{binding_name}'"),
                Span::default(),
            ))
        }
    }
}

#[derive(Default)]
pub struct Compiler {
    pub bytecode_functions: Vec<BytecodeFunction>,
    pub flattner: Flattener,
}

impl Compiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_bytecode_function(&mut self, bc_func: BytecodeFunction) {
        self.flattner.insert_binding((
            Rc::from(bc_func.name.as_str()),
            FlattenValue {
                span: Span::default(),
                kind: FlattenValueKind::None,
            },
        ));
        self.bytecode_functions.push(bc_func);
    }

    pub fn compile_source(self, source: &str) -> Result<CompileObject, Diagnostics> {
        let stats = Parser::new(source)?.parse()?.statements;
        let bindings = desugar(stats)?;
        let flatten = self.flattner.flatten(bindings)?;

        let mut compile_object = CompileObject {
            flatten: Rc::new(flatten),
            allocator: Allocator::new(),
            function_map: HashMap::new(),
            function_bytecode: Vec::new(),
            binding_map: HashMap::new(),
            string_cache: HashMap::new(),
            data_pool: Vec::new(),
        };

        for (binding_idx, bytecode_func) in self.bytecode_functions.iter().enumerate() {
            let func = Rc::new(Function {
                param_register: compile_object.allocator.alloc(),
                pc: compile_object.function_bytecode.len(),
            });
            let bytecode = (bytecode_func.build)(&mut compile_object, func.clone());
            compile_object.function_bytecode.extend(bytecode);
            compile_object.function_map.insert(binding_idx, func);
        }

        Ok(compile_object)
    }
}
