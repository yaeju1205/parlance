use parlance_compiler::BytecodeFunction;
use parlance_vm::{Instruction, ProgramCount};

use crate::FnBuilder;

pub fn controll_cmp() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::controll::cmp".to_string(),
        build: |mut compile_object, func| {
            let mut builder = FnBuilder::new(&mut compile_object, func);

            let lhs = builder.param_register;
            let rhs = builder.alloc_param()?;
            let dest = builder.alloc()?;

            builder.emit(Instruction::cmp(dest, lhs, rhs));
            builder.emit(Instruction::ret(dest));

            Ok(builder.build())
        },
    }
}

pub fn controll_if() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::controll::if".to_string(),
        build: |mut compile_object, func| {
            let mut builder = FnBuilder::new(&mut compile_object, func);

            let cond_reg = builder.param_register;
            let true_reg = builder.alloc_param()?;
            let false_reg = builder.alloc_param()?;

            builder.emit(Instruction::jmp_eq(
                cond_reg,
                (builder.compile_object.function_bytecode.len() + builder.bytecode.len() + 1)
                    as ProgramCount,
            ));

            builder.emit(Instruction::ret(false_reg));
            builder.emit(Instruction::ret(true_reg));

            Ok(builder.build())
        },
    }
}
