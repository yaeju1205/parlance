use std::rc::Rc;

use parlance_diagnostics::{Diagnostics, Severity, Span};
use parlance_ir::Value;
use parlance_runtime::BindingValue;

pub fn parlance_string_concat<'a>() -> BindingValue<'a> {
    BindingValue::NativeFunction {
        name: "std::string::concat",
        execute_arg: true,
        callee: Rc::new(move |_, lhs| match lhs.as_ref() {
            BindingValue::Value(lhs_value) => match lhs_value.as_ref() {
                Value::String(lhs_str) => {
                    let lhs_str_owned = lhs_str.clone();
                    Ok(Rc::new(BindingValue::NativeFunction {
                        name: "std::string::concat::rhs",
                        execute_arg: true,
                        callee: Rc::new(move |_, rhs| match rhs.as_ref() {
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
                            BindingValue::NativeFunction { name, .. } => Err(Diagnostics {
                                severity: Severity::Error,
                                span: Span::default(),
                                message: format!("expect string, got {name}"),
                            }),
                        }),
                    }))
                }
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span::default(),
                    message: format!("expect string, got {:?}", lhs_value),
                }),
            },
            BindingValue::NativeFunction { name, .. } => Err(Diagnostics {
                severity: Severity::Error,
                span: Span::default(),
                message: format!("expect string, got {name}"),
            }),
        }),
    }
}
