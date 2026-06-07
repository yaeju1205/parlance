use parlance_compiler::BytecodeFunction;
use parlance_vm::{Instruction, Operator};

use crate::FnBuilder;

pub fn print() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::io::print".to_string(),
        build: |compile_object, func| {
            let mut builder = FnBuilder::new(compile_object, func);

            builder.emit(Instruction {
                operator: Operator::Print,
                a: builder.param_register,
                b: 0,
                c: 0,
            });
            builder.emit(Instruction {
                operator: Operator::Ret,
                a: builder.param_register,
                b: 0,
                c: 0,
            });

            builder.build()
        },
    }
}
