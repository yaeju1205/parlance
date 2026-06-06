use std::rc::Rc;

use parlance_compiler::{Compiler, Function};
use parlance_vm::{Bytecode, Instruction, Operator};

pub(self) struct FnBuilder<'a> {
    bytecode: Bytecode,
    compiler: &'a mut Compiler,
    param_register: usize,
    function: Rc<Function>,
}

impl<'a> FnBuilder<'a> {
    pub fn new(compiler: &'a mut Compiler, func: Rc<Function>) -> Self {
        Self {
            bytecode: Vec::new(),
            compiler,
            param_register: func.param_register,
            function: func,
        }
    }

    pub fn alloc_param(&mut self) -> usize {
        let param_reg = self.compiler.register_allocator.alloc();
        let inner_func_reg = self.compiler.register_allocator.alloc();

        let inner_func_pc = self.function.pc + self.bytecode.len() + 2;

        self.bytecode.push(Instruction {
            operator: Operator::LoadFunc,
            a: inner_func_reg,
            b: inner_func_pc,
            c: param_reg,
        });

        self.bytecode.push(Instruction {
            operator: Operator::Ret,
            a: inner_func_reg,
            b: 0,
            c: 0,
        });

        param_reg
    }

    pub fn alloc(&mut self) -> usize {
        self.compiler.register_allocator.alloc()
    }

    pub fn emit(&mut self, inst: Instruction) {
        self.bytecode.push(inst);
    }

    pub fn build(self) -> Bytecode {
        self.bytecode
    }
}

pub mod io;
pub mod math;
