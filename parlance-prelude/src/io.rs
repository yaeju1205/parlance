use parlance_compiler::BytecodeFunction;
use parlance_vm::{Instruction, OPERATOR_PRINT, OPERATOR_RET};

pub fn print() -> BytecodeFunction {
    BytecodeFunction {
        name: "std::io::print".to_string(),
        build_bytecode: |compiler, func| {
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
        },
    }
}
