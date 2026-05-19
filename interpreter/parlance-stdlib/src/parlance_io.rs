use std::rc::Rc;

use parlance_diagnostics::Diagnostics;
use parlance_ir::Value;
use parlance_runtime::{BindingValue, Program};

pub fn parlance_io_print<'a>(
    _: &mut Program<'a>,
    arg: Rc<BindingValue<'a>>,
) -> Result<Rc<BindingValue<'a>>, Diagnostics> {
    match arg.as_ref() {
        BindingValue::Value(value) => println!("{:?}", value),
        BindingValue::NativeFunction(_) => println!("<native function>"),
    }
    Ok(Rc::new(BindingValue::Value(Rc::new(Value::String(
        String::from("std::io::print"),
    )))))
}
