use std::rc::Rc;

use parlance_ast::Parser;
use parlance_diagnostics::Diagnostics;
use parlance_ir::{Value, Variable};
use parlance_runtime::{Binding, BindingValue, Program};

pub fn load_source<'a>(source: &'a str) -> Result<Program<'a>, Diagnostics> {
    let mut parser = Parser::new(source);
    let stats = parser.parse()?;
    let mut program = Program::new();

    program.binding(Binding {
        name: "true",
        value: Rc::new(BindingValue::Value(Rc::new(Value::Bool(true)))),
    });

    program.binding(Binding {
        name: "false",
        value: Rc::new(BindingValue::Value(Rc::new(Value::Bool(false)))),
    });

    program.binding(Binding {
        name: "std::io::print",
        value: Rc::new(parlance_stdlib::parlance_io::parlance_io_print()),
    });

    program.binding(Binding {
        name: "std::string::concat",
        value: Rc::new(parlance_stdlib::parlance_string::parlance_string_concat()),
    });

    program.binding(Binding {
        name: "std::control::if",
        value: Rc::new(parlance_stdlib::parlance_control::parlance_control_if()),
    });

    for stat in stats.into_iter() {
        program.binding(Binding::from(Variable::from(stat.kind)));
    }

    Ok(program)
}
