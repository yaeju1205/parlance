use std::{env, fs, process};

use parlance_compiler::Compiler;
use parlance_diagnostics::Diagnostics;
use parlance_parser::Parser;
use parlance_stdlib::Print;
use parlance_vm::VirtualMachine;

fn load_vm(source: &str) -> Result<VirtualMachine, Diagnostics> {
    println!("!) vm load start");

    let mut parser = Parser::new(&source)?;
    let stats = parser.parse()?;
    println!("!) parsing complate");

    let compiler = Compiler::new(stats)?.with_bytecode_functions(vec![Print]);

    let (pc, bytecode, data_pool) = compiler.compile("main")?;
    println!("!) compile complate");
    println!("{:#?}", bytecode);

    let mut vm = VirtualMachine::new();
    vm.load(pc, bytecode, data_pool);

    Ok(vm)
}

fn main() {
    let file_path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: parlance <file>");
        process::exit(1);
    });

    let source = fs::read_to_string(&file_path).unwrap_or_else(|err| {
        eprintln!("{file_path}: {err}");
        process::exit(1);
    });

    let mut vm = load_vm(&source).unwrap_or_else(|diagnostic| {
        eprintln!("{}", diagnostic.to_string());
        process::exit(1);
    });

    unsafe {
        vm.run();
    }
}
