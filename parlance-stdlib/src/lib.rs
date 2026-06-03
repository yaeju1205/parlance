use parlance_compiler::{BytecodeFunction, Compiler};
use parlance_vm::{
    Instruction, OPERATOR_ADD_INT, OPERATOR_CALL, OPERATOR_GOTO, OPERATOR_MOVE, OPERATOR_RET,
};

pub struct IntAdd;

impl BytecodeFunction for IntAdd {
    fn get_name(&self) -> String {
        "add".to_string()
    }

    fn build_bytecode(&self, compiler: &mut Compiler) -> () {
        let dest = compiler.allocator.alloc();
        let lhs = compiler.allocator.alloc();

        compiler.bytecode.push(Instruction {
            operator: OPERATOR_MOVE,
            a: lhs,
            b: 0,
            c: 0,
        });

        compiler.bytecode.push(Instruction {
            operator: OPERATOR_RET,
            a: 0, // argument
            b: 0,
            c: 0,
        });

        let add_pc = compiler.allocator.alloc();

        compiler.bytecode.push(Instruction {
            operator: OPERATOR_GOTO,
            a: add_pc,
            b: 0,
            c: 0,
        });

        compiler.bytecode.push(Instruction {
            operator: OPERATOR_ADD_INT,
            a: dest,
            b: lhs,
            c: 0,
        });

        compiler.bytecode.push(Instruction {
            operator: OPERATOR_RET,
            a: dest,
            b: 0,
            c: 0,
        });
    }
}
