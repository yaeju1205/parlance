use std::{fs, process};

use clap::Parser;
use parlance_compiler::Compiler;
use parlance_diagnostics::Diagnostics;
use parlance_parser::Parser as ParlanceParser;
use parlance_prelude::{
    io::print,
    math::{add, div, mul, sub},
};
use parlance_vm::{Instruction, VirtualMachine};

#[derive(Parser)]
#[command(name = "parlance")]
#[command(version, about = "The Parlance programming language")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    Run { file: String },
    Check { file: String },
}

fn compile_source(source: &str, verbose: bool) -> Result<VirtualMachine, Diagnostics> {
    if verbose {
        println!("!) parsing start");
    }

    let mut parser = ParlanceParser::new(source)?;
    let parse_info = parser.parse()?;

    if verbose {
        println!("!) parsing complete");
    }

    let compiler = Compiler::new(parse_info, vec![print(), add(), sub(), mul(), div()])?;

    if verbose {
        println!("!) compile complete");
    }

    let (pc, bytecode, data_pool) = compiler.compile("main")?;

    if verbose {
        println!("!) start pc {pc}");
        println!("!) bytecode length {}", bytecode.len());
        println!("!) instruction memory {} bytes", size_of::<Instruction>());
        println!("!) bytecode capacity {}", bytecode.capacity());
        println!(
            "!) bytecode memory {} bytes",
            bytecode.capacity() * size_of::<Instruction>()
        );
    }

    let mut vm = VirtualMachine::new();
    vm.load(pc, bytecode, data_pool);

    Ok(vm)
}

fn read_source(file: &str) -> String {
    fs::read_to_string(file).unwrap_or_else(|err| {
        eprintln!("{file}: {err}");
        process::exit(1);
    })
}

pub fn run() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { file } => {
            let source = read_source(&file);
            let mut vm = compile_source(&source, cli.verbose).unwrap_or_else(|diagnostic| {
                eprintln!("{}", diagnostic.to_string());
                process::exit(1);
            });

            unsafe {
                vm.run();
            }
        }
        Commands::Check { file } => {
            let source = read_source(&file);
            match compile_source(&source, cli.verbose) {
                Ok(_) => println!("!) check passed"),
                Err(diagnostic) => {
                    eprintln!("{}", diagnostic.to_string());
                    process::exit(1);
                }
            }
        }
    }
}
