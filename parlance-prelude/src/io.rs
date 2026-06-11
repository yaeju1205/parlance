use parlance_compiler::BytecodeFunction;
use parlance_vm::{Instruction, VirtualMachineData};

use crate::FnBuilder;

pub fn print() -> BytecodeFunction {
    BytecodeFunction {
        name: "prelude::io::print".to_string(),
        build: |compile_object, func| {
            let mut builder = FnBuilder::new(compile_object, func);

            let io_fn_ptr = builder.compile_object.data_pool.len();
            let dest = builder.alloc()?;

            builder
                .compile_object
                .data_pool
                .push(VirtualMachineData::RustFnPtr(|data| {
                    match data {
                        VirtualMachineData::StrPtr(ptr) => {
                            println!("{}", ptr.as_ref());
                        }
                        _ => println!("{:?}", data),
                    }
                    VirtualMachineData::None
                }));

            builder.emit(Instruction::rust_call(
                dest,
                io_fn_ptr,
                builder.param_register,
            ));
            builder.emit(Instruction::ret(dest));

            Ok(builder.build())
        },
    }
}
