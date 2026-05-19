use std::rc::Rc;

use parlance_diagnostics::{Diagnostics, Severity, Span};
use parlance_ir::Value;
use parlance_runtime::{BindingValue, Program};

pub fn parlance_int_add<'a>(
    _: &mut Program<'a>,
    lhs: Rc<BindingValue<'a>>,
) -> Result<Rc<BindingValue<'a>>, Diagnostics> {
    match lhs.as_ref() {
        BindingValue::Value(lhs_value) => match lhs_value.as_ref() {
            Value::Integer(lhs_int) => {
                let lhs_int_owned = lhs_int.clone();
                Ok(Rc::new(BindingValue::NativeFunction(Rc::new(
                    move |_, rhs| match rhs.as_ref() {
                        BindingValue::Value(rhs_value) => match rhs_value.as_ref() {
                            Value::Integer(rhs_int) => Ok(Rc::new(BindingValue::Value(Rc::new(
                                Value::Integer(lhs_int_owned + rhs_int),
                            )))),
                            _ => Err(Diagnostics {
                                severity: Severity::Error,
                                span: Span::default(),
                                message: format!("expect integer, got {:?}", rhs_value),
                            }),
                        },
                        _ => Err(Diagnostics {
                            severity: Severity::Error,
                            span: Span::default(),
                            message: String::from("expect integer, got native function"),
                        }),
                    },
                ))))
            }
            _ => Err(Diagnostics {
                severity: Severity::Error,
                span: Span::default(),
                message: format!("expect integer, got {:?}", lhs_value),
            }),
        },
        _ => Err(Diagnostics {
            severity: Severity::Error,
            span: Span::default(),
            message: String::from("expect integer, got native function"),
        }),
    }
}
