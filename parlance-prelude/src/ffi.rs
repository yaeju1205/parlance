use parlance_compiler::BytecodeFunction;
use parlance_diagnostics::Diagnostics;
use parlance_vm::{Instruction, Operator, VirtualMachineData};

use crate::FnBuilder;

pub fn rust_string() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::ffi::rust_string".to_string(),
        build: |compile_object, func| {
            let mut builder = FnBuilder::new(compile_object, func);

            let dest = builder.alloc();

            builder.emit(Instruction {
                operator: Operator::RustLoadFnPtr(|value| match &value {
                    VirtualMachineData::StrPtr(str) => {
                        Ok(VirtualMachineData::RustString(str.to_string()))
                    }
                    _ => Err(Diagnostics::runtime_error(format!(
                        "expected string, found {:?}",
                        value
                    ))),
                }),
                a: dest,
                b: 0,
                c: 0,
            });

            builder.emit(Instruction {
                operator: Operator::RustCall,
                a: dest,
                b: dest,
                c: builder.param_register,
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
