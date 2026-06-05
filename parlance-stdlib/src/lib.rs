use std::rc::Rc;

use parlance_compiler::{BytecodeFunction, Compiler, Function};
use parlance_vm::{Bytecode, Instruction, OPERATOR_PRINT, OPERATOR_RET};

pub struct Print;

impl BytecodeFunction for Print {
    fn get_name(&self) -> String {
        "print".to_string()
    }

    fn build_bytecode(&self, _: &mut Compiler, func: Rc<Function>) -> Bytecode {
        let mut bytecode = Vec::new();

        bytecode.push(Instruction {
            operator: OPERATOR_PRINT,
            a: func.param_register,
            b: 0,
            c: 0,
        });
        bytecode.push(Instruction {
            operator: OPERATOR_RET,
            a: 0,
            b: 0,
            c: 0,
        });

        bytecode
    }
}
