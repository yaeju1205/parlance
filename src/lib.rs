use parlance_ast::Parser;
use parlance_diagnostics::Diagnostics;
use parlance_ir::{Value, Variable};
use parlance_runtime::{Program, stdlib};

pub fn load_source<'a>(source: &'a str) -> Result<Program<'a>, Diagnostics> {
    let mut parser = Parser::new(source);
    let stats = parser.parse()?;
    let mut program = Program::new();
    for stat in stats.into_iter() {
        program.declaration_variable(Variable::from(stat))
    }
    Ok(program)
}
