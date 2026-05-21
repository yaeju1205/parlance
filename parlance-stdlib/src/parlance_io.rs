use std::rc::Rc;

use parlance_ir::Value;
use parlance_runtime::BindingValue;

pub fn parlance_io_print<'a>() -> BindingValue<'a> {
    BindingValue::NativeFunction {
        name: "std::io::print",
        execute_arg: true,
        callee: Rc::new(move |_, arg| {
            match arg.as_ref() {
                BindingValue::Value(value) => println!("{:?}", value),
                BindingValue::NativeFunction { name, .. } => println!("{}", name),
            }
            Ok(Rc::new(BindingValue::Value(Rc::new(Value::String(
                String::from("std::io::print"),
            )))))
        }),
    }
}
