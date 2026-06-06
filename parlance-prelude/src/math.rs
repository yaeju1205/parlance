use parlance_compiler::BytecodeFunction;
use parlance_vm::{Instruction, Operator};

use crate::FnBuilder;

pub fn add() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::math::add".to_string(),
        build_bytecode: |compiler, func| {
            let mut builder = FnBuilder::new(compiler, func);

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

pub fn sub() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::math::sub".to_string(),
        build_bytecode: |compiler, func| {
            let mut builder = FnBuilder::new(compiler, func);

            let lhs_reg = builder.param_register;
            let rhs_reg = builder.alloc_param();
            let dest = builder.alloc();

            builder.emit(Instruction {
                operator: Operator::SubInt,
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

pub fn mul() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::math::mul".to_string(),
        build_bytecode: |compiler, func| {
            let mut builder = FnBuilder::new(compiler, func);

            let lhs_reg = builder.param_register;
            let rhs_reg = builder.alloc_param();
            let dest = builder.alloc();

            builder.emit(Instruction {
                operator: Operator::MulInt,
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

pub fn div() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::math::div".to_string(),
        build_bytecode: |compiler, func| {
            let mut builder = FnBuilder::new(compiler, func);

            let lhs_reg = builder.param_register;
            let rhs_reg = builder.alloc_param();
            let dest = builder.alloc();

            builder.emit(Instruction {
                operator: Operator::DivInt,
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
