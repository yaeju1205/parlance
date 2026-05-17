pub mod io {
    use std::rc::Rc;

    use parlance_diagnostics::Diagnostics;
    use parlance_ir::Value;
    use parlance_runtime::{BindingValue, Program};

    pub fn print<'a>(
        _: &mut Program<'a>,
        arg: Rc<BindingValue<'a>>,
    ) -> Result<Rc<BindingValue<'a>>, Diagnostics> {
        match arg.as_ref() {
            BindingValue::Value(value) => println!("{:?}", value),
            BindingValue::NativeFunction(_) => println!("<native function>"),
        }
        Ok(Rc::new(BindingValue::Value(Rc::new(Value::String(
            "std::io::print".to_string(),
        )))))
    }
}

pub mod string {
    use std::rc::Rc;

    use parlance_diagnostics::{Diagnostics, Severity, Span};
    use parlance_ir::Value;
    use parlance_runtime::{BindingValue, Program};

    pub fn concat<'a>(
        _: &mut Program<'a>,
        lhs: Rc<BindingValue<'a>>,
    ) -> Result<Rc<BindingValue<'a>>, Diagnostics> {
        match lhs.as_ref() {
            BindingValue::Value(lhs_value) => match lhs_value.as_ref() {
                Value::String(lhs_str) => {
                    let lhs_str_owned = lhs_str.clone();
                    Ok(Rc::new(BindingValue::NativeFunction(Rc::new(
                        move |_, rhs| match rhs.as_ref() {
                            BindingValue::Value(rhs_value) => match rhs_value.as_ref() {
                                Value::String(rhs_str) => Ok(Rc::new(BindingValue::Value(
                                    Rc::new(Value::String(format!("{}{}", lhs_str_owned, rhs_str))),
                                ))),
                                _ => Err(Diagnostics {
                                    severity: Severity::Error,
                                    span: Span::default(),
                                    message: format!("expect string, got {:?}", rhs_value),
                                }),
                            },
                            _ => Err(Diagnostics {
                                severity: Severity::Error,
                                span: Span::default(),
                                message: "expect string, got native function".to_string(),
                            }),
                        },
                    ))))
                }
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span::default(),
                    message: format!("expect string, got {:?}", lhs_value),
                }),
            },
            _ => Err(Diagnostics {
                severity: Severity::Error,
                span: Span::default(),
                message: "expect string, got native function".to_string(),
            }),
        }
    }
}
