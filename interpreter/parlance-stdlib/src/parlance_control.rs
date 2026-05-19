use std::rc::Rc;

use parlance_diagnostics::Diagnostics;
use parlance_ir::Value;
use parlance_runtime::{BindingValue, Program};

pub fn parlance_control_if<'a>(
    _: &mut Program<'a>,
    cond: Rc<BindingValue<'a>>,
) -> Result<Rc<BindingValue<'a>>, Diagnostics> {
    // if $ true $ (asdasd) $ else $ asdasd

    // if -> Boolean -> IfExe -> else -> ElseExe

    match cond.as_ref() {
        BindingValue::Value(value) => match value.as_ref() {
            Value::Bool(bool) => {
                if *bool {
                    Ok(Rc::new(BindingValue::NativeFunction(Rc::new(
                        move |_, _| {
                            Ok(Rc::new(BindingValue::from(String::from(
                                "std::control::if",
                            ))))
                        },
                    ))))
                } else {
                    Ok(Rc::new(BindingValue::from(String::from(
                        "std::control::if",
                    ))))
                }
            }
            _ => Ok(Rc::new(BindingValue::NativeFunction(Rc::new(
                move |_, _| {
                    Ok(Rc::new(BindingValue::from(String::from(
                        "std::control::if",
                    ))))
                },
            )))),
        },
        BindingValue::NativeFunction(_) => Ok(Rc::new(BindingValue::NativeFunction(Rc::new(
            move |_, _| {
                Ok(Rc::new(BindingValue::from(String::from(
                    "std::control::if",
                ))))
            },
        )))),
    }
}
