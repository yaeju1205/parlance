use parlance_compiler::BytecodeFunction;
use parlance_vm::Instruction;

use crate::FnBuilder;

pub fn add() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::math::add".to_string(),
        build: |mut compile_object, func| {
            let mut builder = FnBuilder::new(&mut compile_object, func);

            let lhs = builder.param_register;
            let rhs = builder.alloc_param();
            let dest = builder.alloc();

            builder.emit(Instruction::add_int(dest, lhs, rhs));

            builder.emit(Instruction::ret(dest));

            builder.build()
        },
    }
}
