use std::{collections::HashMap, path::{Path, PathBuf}, rc::Rc};

use parlance_diagnostics::{Diagnostics, Span};
use parlance_parser::Parser;
use parlance_vm::{Bytecode, DataPool, Instruction, Operator, VirtualMachineData};

use parlance_module::Pars;

use crate::{
    desugarer::{DesugarBinding, desugar},
    flattener::{Flatten, FlattenIndex, FlattenValue, FlattenValueKind, Flattener},
    resolver::{resolve_pars, resolve_program},
};

mod desugarer;
mod flattener;
mod resolver;

#[derive(Debug, Default)]
pub struct Allocator {
    pub register: u32,
}

impl Allocator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn alloc(&mut self) -> u32 {
        let reg = self.register;
        self.register += 1;
        reg
    }
}

#[derive(Debug)]
pub struct Function {
    pub param_register: u32,
    pub pc: u32,
}

pub struct BytecodeFunction {
    pub name: String,
    pub build: fn(compile_object: &mut CompileObject, func: Rc<Function>) -> Bytecode,
}

#[derive(Debug, Default)]
pub struct CompileObject {
    pub flatten: Rc<Flatten>,
    pub allocator: Allocator,
    pub function_map: HashMap<FlattenIndex, Rc<Function>>,
    pub function_bytecode: Bytecode,
    pub binding_map: HashMap<FlattenIndex, u32>,
    pub string_cache: HashMap<String, u32>,
    pub data_pool: DataPool,
}

impl CompileObject {
    pub fn new(flatten: Rc<Flatten>) -> Self {
        Self {
            flatten,
            allocator: Allocator::new(),
            function_map: HashMap::new(),
            function_bytecode: Vec::new(),
            binding_map: HashMap::new(),
            string_cache: HashMap::new(),
            data_pool: Vec::new(),
        }
    }

    pub fn link_object(&mut self, mut object: CompileObject) {
        self.function_map.extend(object.function_map.drain());
        self.binding_map.extend(object.binding_map.drain());
        self.string_cache.extend(object.string_cache.drain());

        self.function_bytecode.append(&mut object.function_bytecode);
        self.data_pool.append(&mut object.data_pool);
    }

    fn build_value_inner(
        &mut self,
        value_idx: FlattenIndex,
        is_tail: bool,
    ) -> Result<(u32, Bytecode), Diagnostics> {
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

                let (arg_reg, mut arg_bc) = self.build_value_inner(*arg, false)?;

                let mut bytecode: Bytecode = Vec::new();

                if let Some(callee_func) = self.function_map.get(&callee_idx).cloned() {
                    bytecode.append(&mut arg_bc);

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
                    let (callee_reg, mut callee_bc) = self.build_value_inner(callee_idx, false)?;

                    bytecode.append(&mut arg_bc);
                    bytecode.append(&mut callee_bc);

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

                let (body_register, body_bytecode) = self.build_value_inner(*body, true)?;

                let func_pc = self.function_bytecode.len() as u32;
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

                self.function_bytecode.append(&mut func_bytecode);

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
                    b: int_value.clone() as u32,
                    c: 0,
                });
                self.binding_map.insert(value_idx, dest);
                Ok((dest, bytecode))
            }
            FlattenValueKind::String(str_value) => {
                let pool_idx = if let Some(idx) = self.string_cache.get(str_value) {
                    *idx
                } else {
                    let idx = self.data_pool.len() as u32;
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
                let (dest, mut var_bc) = self.build_value_inner(*idx, is_tail)?;
                self.binding_map.insert(value_idx, dest);
                bytecode.append(&mut var_bc);
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

    pub fn build_value(&mut self, value_idx: FlattenIndex) -> Result<(u32, Bytecode), Diagnostics> {
        self.build_value_inner(value_idx, false)
    }

    pub fn build_binding(
        mut self,
        binding_name: &str,
    ) -> Result<(u32, Bytecode, DataPool), Diagnostics> {
        if let Some(value_idx) = self.flatten.clone().bindings.get(binding_name) {
            let (_, mut main_bytecode) = self.build_value(*value_idx)?;
            let mut func_bytecode = self.function_bytecode;
            let pc = func_bytecode.len() as u32;

            let mut bytecode: Bytecode = Vec::new();
            bytecode.append(&mut func_bytecode);
            bytecode.append(&mut main_bytecode);

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
    pub externs: HashMap<Rc<str>, PathBuf>,
}

impl Compiler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_extern<P: AsRef<Path>>(&mut self, name: &str, root: P) {
        self.externs
            .insert(Rc::from(name), root.as_ref().to_path_buf());
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

        let mut compile_object = CompileObject::new(Rc::new(flatten));

        for bytecode_func in &self.bytecode_functions {
            if let Some(&flatten_idx) = compile_object
                .flatten
                .bindings
                .get(bytecode_func.name.as_str())
            {
                let func = Rc::new(Function {
                    param_register: compile_object.allocator.alloc(),
                    pc: compile_object.function_bytecode.len() as u32,
                });
                let mut bytecode = (bytecode_func.build)(&mut compile_object, func.clone());
                compile_object.function_bytecode.append(&mut bytecode);
                compile_object.function_map.insert(flatten_idx, func);
            }
        }

        Ok(compile_object)
    }

    pub fn compile_source_file<P: AsRef<Path>>(
        self,
        path: P,
    ) -> Result<CompileObject, Diagnostics> {
        let prelude_names: Vec<Rc<str>> = self
            .bytecode_functions
            .iter()
            .map(|func| Rc::from(func.name.as_str()))
            .collect();

        let bindings = resolve_program(path.as_ref(), self.externs.clone(), &prelude_names)?;
        self.compile_bindings(bindings)
    }

    pub fn compile_pars(self, pars: &Pars) -> Result<CompileObject, Diagnostics> {
        let prelude_names: Vec<Rc<str>> = self
            .bytecode_functions
            .iter()
            .map(|func| Rc::from(func.name.as_str()))
            .collect();

        let bindings = resolve_pars(pars, self.externs.clone(), &prelude_names)?;
        self.compile_bindings(bindings)
    }

    pub fn compile_pars_file<P: AsRef<Path>>(
        self,
        path: P,
    ) -> Result<CompileObject, Diagnostics> {
        let bytes = std::fs::read(path.as_ref()).map_err(|err| {
            Diagnostics::compiler_error(
                format!("can not read pars {}: {}", path.as_ref().display(), err),
                Span::default(),
            )
        })?;
        let pars = Pars::from_bytes(&bytes).map_err(|err| {
            Diagnostics::compiler_error(format!("invalid pars bundle: {err}"), Span::default())
        })?;
        self.compile_pars(&pars)
    }

    fn compile_bindings(
        self,
        bindings: Vec<DesugarBinding>,
    ) -> Result<CompileObject, Diagnostics> {
        let flatten = Rc::new(self.flattner.flatten(bindings)?);

        let mut compile_object = CompileObject::new(flatten.clone());

        for bytecode_func in &self.bytecode_functions {
            if let Some(&flatten_idx) = flatten.bindings.get(bytecode_func.name.as_str()) {
                let func = Rc::new(Function {
                    param_register: compile_object.allocator.alloc(),
                    pc: compile_object.function_bytecode.len() as u32,
                });
                let mut bytecode = (bytecode_func.build)(&mut compile_object, func.clone());
                compile_object.function_bytecode.append(&mut bytecode);
                compile_object.function_map.insert(flatten_idx, func);
            }
        }

        Ok(compile_object)
    }
}
