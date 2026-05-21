use std::rc::Rc;

use parlance_diagnostics::{Diagnostics, Severity, Span};
use parlance_ir::Value;
use parlance_runtime::BindingValue;

pub fn parlance_parser_whitespace<'a>() -> BindingValue<'a> {
    BindingValue::NativeFunction {
        name: "std::parser::whitespace",
        execute_arg: true,
        callee: Rc::new(move |_, arg| match arg.as_ref() {
            BindingValue::Value(value) => match value.as_ref() {
                Value::String(value_string) => {
                    let base_string = value_string.to_owned();
                    Ok(Rc::new(BindingValue::from(
                        base_string.trim_start().to_string(),
                    )))
                }
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span::default(),
                    message: format!("expect string, got {:?}", value),
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

pub fn parlance_parser_text<'a>() -> BindingValue<'a> {
    BindingValue::NativeFunction {
        name: "std::parser::text",
        execute_arg: true,
        callee: Rc::new(move |_, arg| match arg.as_ref() {
            BindingValue::Value(value) => match value.as_ref() {
                Value::String(value_string) => {
                    let text = value_string.clone();
                    for (i, ch) in text.char_indices() {
                        if ch.is_whitespace() {
                            return Ok(Rc::new(BindingValue::from(text[..i].to_string())));
                        }
                    }
                    Ok(Rc::new(BindingValue::from(text)))
                }
                _ => Err(Diagnostics {
                    severity: Severity::Error,
                    span: Span::default(),
                    message: format!("expect string, got {:?}", value),
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
