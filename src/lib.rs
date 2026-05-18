use std::rc::Rc;

use parlance_ast::Parser;
use parlance_diagnostics::Diagnostics;
use parlance_ir::Variable;
use parlance_runtime::{Binding, BindingValue, Program};

pub fn load_source<'a>(source: &'a str) -> Result<Program<'a>, Diagnostics> {
    let mut parser = Parser::new(source);
    let stats = parser.parse()?;
    let mut program = Program::new();

    program.binding(Binding {
        name: "std::io::print",
        value: Rc::new(BindingValue::NativeFunction(Rc::new(|program, args| {
            parlance_stdlib::io::print(program, args)
        }))),
    });

    program.binding(Binding {
        name: "std::string::concat",
        value: Rc::new(BindingValue::NativeFunction(Rc::new(|program, args| {
            parlance_stdlib::string::concat(program, args)
        }))),
    });

    program.binding(Binding {
        name: "std::int::add",
        value: Rc::new(BindingValue::NativeFunction(Rc::new(|program, args| {
            parlance_stdlib::int::add(program, args)
        }))),
    });

    for stat in stats.into_iter() {
        program.binding(Binding::from(Variable::from(stat.kind)));
    }

    Ok(program)
}
