pub mod io {
    use std::rc::Rc;

    use parlance_diagnostics::Diagnostics;
    use parlance_ir::Value;
    use parlance_runtime::{BindingValue, Program};

    pub fn print<'a>(
        _: &mut Program<'a>,
        arg: Rc<BindingValue<'a>>,
    ) -> Result<Rc<BindingValue<'a>>, Diagnostics> {
        println!("{:?}", arg);
        Ok(Rc::new(BindingValue::Value(Rc::new(Value::String(
            "std::io::print",
        )))))
    }
}
