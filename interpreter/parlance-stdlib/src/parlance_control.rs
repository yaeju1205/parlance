use std::rc::Rc;

use parlance_ir::Value;
use parlance_runtime::BindingValue;

pub fn parlance_control_if<'a>() -> BindingValue<'a> {
    BindingValue::NativeFunction {
        execute_arg: true,
        callee: Rc::new(move |_, cond| match cond.as_ref() {
            BindingValue::Value(value) => match value.as_ref() {
                Value::Bool(bool) => {
                    if *bool {
                        Ok(Rc::new(BindingValue::NativeFunction {
                            execute_arg: true,
                            callee: Rc::new(move |_, arg| Ok(arg)),
                        }))
                    } else {
                        Ok(Rc::new(BindingValue::NativeFunction {
                            execute_arg: false,
                            callee: Rc::new(move |_, _| {
                                Ok(Rc::new(BindingValue::NativeFunction {
                                    execute_arg: true,
                                    callee: Rc::new(move |_, arg| Ok(arg)),
                                }))
                            }),
                        }))
                    }
                }
                _ => Ok(Rc::new(BindingValue::NativeFunction {
                    execute_arg: true,
                    callee: Rc::new(move |_, arg| Ok(arg)),
                })),
            },
            BindingValue::NativeFunction { .. } => Ok(Rc::new(BindingValue::NativeFunction {
                execute_arg: true,
                callee: Rc::new(move |_, arg| Ok(arg)),
            })),
        }),
    }
}
