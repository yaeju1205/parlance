use parlance_compiler::{BytecodeFunction, Compiler};
use parlance_vm::{
    Instruction, OPERATOR_ADD_INT, OPERATOR_CALL, OPERATOR_GOTO, OPERATOR_MOVE, OPERATOR_PRINT,
    OPERATOR_RET,
};

pub struct Print;

impl BytecodeFunction for Print {
    fn get_name(&self) -> String {
        "print".to_string()
    }

    fn build_bytecode(&self, compiler: &mut Compiler, _: usize) -> () {
        compiler.bytecode.push(Instruction {
            operator: OPERATOR_PRINT,
            a: 0,
            b: 0,
            c: 0,
        });
        compiler.bytecode.push(Instruction {
            operator: OPERATOR_RET,
            a: 0,
            b: 0,
            c: 0,
        });
    }
}
