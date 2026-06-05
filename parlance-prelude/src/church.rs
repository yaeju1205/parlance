use parlance_compiler::BytecodeFunction;
use parlance_vm::{Instruction, OPERATOR_CALL_REG, OPERATOR_LOAD_FUNC, OPERATOR_RET};

pub fn pair() -> BytecodeFunction {
    BytecodeFunction {
        name: "std::church::pair".to_string(),
        build_bytecode: |compiler, func| {
            let mut bytecode = Vec::new();

            let x_reg = func.param_register;
            let y_reg = compiler.register_allocator.alloc();
            let f_reg = compiler.register_allocator.alloc();

            let pair_y_reg = compiler.register_allocator.alloc();
            let pair_f_reg = compiler.register_allocator.alloc();
            let f_x_reg = compiler.register_allocator.alloc();
            let result_reg = compiler.register_allocator.alloc();

            let pair_y_pc = func.pc + 2;
            let pair_f_pc = func.pc + 4;

            bytecode.push(Instruction {
                operator: OPERATOR_LOAD_FUNC,
                a: pair_y_reg,
                b: pair_y_pc,
                c: y_reg,
            });
            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: pair_y_reg,
                b: 0,
                c: 0,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_LOAD_FUNC,
                a: pair_f_reg,
                b: pair_f_pc,
                c: f_reg,
            });
            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: pair_f_reg,
                b: 0,
                c: 0,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_CALL_REG,
                a: f_x_reg,
                b: f_reg,
                c: x_reg,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_CALL_REG,
                a: result_reg,
                b: f_x_reg,
                c: y_reg,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: result_reg,
                b: 0,
                c: 0,
            });

            bytecode
        },
    }
}

pub fn first() -> BytecodeFunction {
    BytecodeFunction {
        name: "std::church::first".to_string(),
        build_bytecode: |compiler, func| {
            let mut bytecode = Vec::new();

            let p_reg = func.param_register;
            let x_reg = compiler.register_allocator.alloc();
            let y_reg = compiler.register_allocator.alloc();

            let sel_x_reg = compiler.register_allocator.alloc();
            let sel_y_reg = compiler.register_allocator.alloc();
            let result_reg = compiler.register_allocator.alloc();

            let sel_x_pc = func.pc + 3;
            let sel_y_pc = func.pc + 5;

            bytecode.push(Instruction {
                operator: OPERATOR_LOAD_FUNC,
                a: sel_x_reg,
                b: sel_x_pc,
                c: x_reg,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_CALL_REG,
                a: result_reg,
                b: p_reg,
                c: sel_x_reg,
            });
            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: result_reg,
                b: 0,
                c: 0,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_LOAD_FUNC,
                a: sel_y_reg,
                b: sel_y_pc,
                c: y_reg,
            });
            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: sel_y_reg,
                b: 0,
                c: 0,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: x_reg,
                b: 0,
                c: 0,
            });

            bytecode
        },
    }
}

pub fn second() -> BytecodeFunction {
    BytecodeFunction {
        name: "std::church::second".to_string(),
        build_bytecode: |compiler, func| {
            let mut bytecode = Vec::new();

            let p_reg = func.param_register;
            let x_reg = compiler.register_allocator.alloc();
            let y_reg = compiler.register_allocator.alloc();

            let sel_x_reg = compiler.register_allocator.alloc();
            let sel_y_reg = compiler.register_allocator.alloc();
            let result_reg = compiler.register_allocator.alloc();

            let sel_x_pc = func.pc + 3;
            let sel_y_pc = func.pc + 5;

            bytecode.push(Instruction {
                operator: OPERATOR_LOAD_FUNC,
                a: sel_x_reg,
                b: sel_x_pc,
                c: x_reg,
            });
            bytecode.push(Instruction {
                operator: OPERATOR_CALL_REG,
                a: result_reg,
                b: p_reg,
                c: sel_x_reg,
            });
            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: result_reg,
                b: 0,
                c: 0,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_LOAD_FUNC,
                a: sel_y_reg,
                b: sel_y_pc,
                c: y_reg,
            });
            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: sel_y_reg,
                b: 0,
                c: 0,
            });

            bytecode.push(Instruction {
                operator: OPERATOR_RET,
                a: y_reg,
                b: 0,
                c: 0,
            });

            bytecode
        },
    }
}
