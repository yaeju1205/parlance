use std::rc::Rc;

use parlance_compiler::{CompileObject, Function};
use parlance_diagnostics::Diagnostics;
use parlance_vm::{Bytecode, Instruction, Register};

pub(self) struct FnBuilder<'a> {
    bytecode: Bytecode,
    compile_object: &'a mut CompileObject,
    param_register: Register,
    function: Rc<Function>,
}

impl<'a> FnBuilder<'a> {
    pub fn new(compile_object: &'a mut CompileObject, func: Rc<Function>) -> Self {
        Self {
            bytecode: Vec::new(),
            compile_object,
            param_register: func.param_register,
            function: func,
        }
    }

    pub fn alloc_param(&mut self) -> Result<Register, Diagnostics> {
        let param_reg = self.alloc()?;
        let inner_func_reg = self.alloc()?;

        let inner_func_pc = self.function.pc + self.bytecode.len() as u32 + 2;

        self.bytecode.push(Instruction::load_func(
            inner_func_reg,
            inner_func_pc,
            param_reg,
        ));

        self.bytecode.push(Instruction::ret(inner_func_reg));

        Ok(param_reg)
    }

    pub fn alloc(&mut self) -> Result<Register, Diagnostics> {
        self.compile_object.allocator.alloc()
    }

    pub fn emit(&mut self, inst: Instruction) {
        self.bytecode.push(inst);
    }

    pub fn build(self) -> Bytecode {
        self.bytecode
    }
}

// pub mod ffi;
pub mod io;
pub mod math;
