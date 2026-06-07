use parlance_compiler::BytecodeFunction;
use parlance_vm::{Instruction, Operator};

use crate::FnBuilder;

pub fn add() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::math::add".to_string(),
        build: |compile_object, func| {
            let mut builder = FnBuilder::new(compile_object, func);

            let lhs_reg = builder.param_register;
            let rhs_reg = builder.alloc_param();
            let dest = builder.alloc();

            builder.emit(Instruction {
                operator: Operator::AddInt,
                a: dest,
                b: lhs_reg,
                c: rhs_reg,
            });

            builder.emit(Instruction {
                operator: Operator::Ret,
                a: dest,
                b: 0,
                c: 0,
            });

            builder.build()
        },
    }
}
