use parlance_ast::Parser;
use parlance_diagnostics::Diagnostics;
use parlance_ir::from_ast;
use parlance_runtime::Program;

pub fn load_source<'a>(source: &'a str) -> Result<Program<'a>, Diagnostics> {
    let mut parser = Parser::new(source);
    let stats = parser.parse()?;
    let decs = from_ast(stats);
    Ok(Program::from(decs))
}
