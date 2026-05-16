use std::{env, fs, process::exit, rc::Rc};

use parlance::load_source;
use parlance_ir::Variable;

fn main() {
    let file_path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: parlance <file>");
        exit(1);
    });

    let run_var = env::args().nth(2).unwrap_or("main".to_string());

    let source = fs::read_to_string(&file_path).unwrap_or_else(|err| {
        eprintln!("{file_path}: {err}");
        exit(1);
    });

    let mut program = load_source(&source).unwrap_or_else(|err| {
        eprintln!("{}", err.to_string());
        exit(1);
    });

    if let Some(var) = program.get_variable(&run_var) {
        program.execute_variable(var).unwrap_or_else(|err| {
            eprintln!("{}", err.to_string());
            exit(1);
        });
    }
}
