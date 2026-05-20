use std::rc::Rc;

use parlance_ir::Value;
use parlance_runtime::BindingValue;

pub fn parlance_control_if_true<'a>() -> BindingValue<'a> {
    BindingValue::NativeFunction {
        name: "std::control::if::true",
        execute_arg: true,
        callee: Rc::new(move |_, arg| {
            Ok(Rc::new(BindingValue::NativeFunction {
                name: "std::control::if::true",
                execute_arg: false,
                callee: Rc::new(move |_, _| Ok(arg.clone())),
            }))
        }),
    }
}

pub fn parlance_control_if_false<'a>() -> BindingValue<'a> {
    BindingValue::NativeFunction {
        name: "std::control::if::false",
        execute_arg: false,
        callee: Rc::new(move |_, _| {
            Ok(Rc::new(BindingValue::NativeFunction {
                name: "std::control::if::false",
                execute_arg: true,
                callee: Rc::new(move |_, arg| Ok(arg)),
            }))
        }),
    }
}

pub fn parlance_control_if<'a>() -> BindingValue<'a> {
    BindingValue::NativeFunction {
        name: "std::control::if",
        execute_arg: true,
        callee: Rc::new(move |_, cond| match cond.as_ref() {
            BindingValue::Value(value) => match value.as_ref() {
                Value::Bool(bool) => {
                    if *bool {
                        Ok(Rc::new(parlance_control_if_true()))
                    } else {
                        Ok(Rc::new(parlance_control_if_false()))
                    }
                }
                _ => Ok(Rc::new(parlance_control_if_true())),
            },
            BindingValue::NativeFunction { .. } => Ok(Rc::new(parlance_control_if_true())),
        }),
    }
}
