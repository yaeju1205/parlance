use parlance_compiler::BytecodeFunction;
use parlance_vm::{Instruction, OPERATOR_ADD_INT, OPERATOR_LOAD_FUNC, OPERATOR_RET};

pub fn add() -> BytecodeFunction {
    BytecodeFunction {
        name: "std::math::add".to_string(),
        build_bytecode: |compiler, func| {
            let mut bytecode = Vec::new();

            let lhs_register = func.param_register;
            let rhs_register = compiler.register_allocator.alloc();
            let result_register = compiler.register_allocator.alloc();
            let inner_func_register = compiler.register_allocator.alloc();

            let inner_func_pc = func.pc + 2;

            bytecode.push(Instruction {
                operator: OPERATOR_LOAD_FUNC,
                a: inner_func_register,
                b: inner_func_pc,
                c: rhs_register,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: inner_func_register,
                b: 0,
                c: 0,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_ADD_INT,
                a: result_register,
                b: lhs_register,
                c: rhs_register,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: result_register,
                b: 0,
                c: 0,
            });

            bytecode
        },
    }
}
