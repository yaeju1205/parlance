use std::{fs, process};

use clap::Parser;
use parlance_compiler::Compiler;
use parlance_diagnostics::Diagnostics;
use parlance_parser::Parser as ParlanceParser;
use parlance_prelude::{io::print, math::add};
use parlance_vm::VirtualMachine;

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
        eprintln!("!) parsing start");
    }

    let mut parser = ParlanceParser::new(source)?;
    let stats = parser.parse()?;

    if verbose {
        eprintln!("!) parsing complete");
    }

    let compiler = Compiler::new(stats, vec![print(), add()])?;

    if verbose {
        eprintln!("!) compile complete");
    }

    let (pc, bytecode, data_pool) = compiler.compile("main")?;

    if verbose {
        eprintln!("!) start pc {pc}");
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
                Ok(_) => eprintln!("!) check passed"),
                Err(diagnostic) => {
                    eprintln!("{}", diagnostic.to_string());
                    process::exit(1);
                }
            }
        }
    }
}
