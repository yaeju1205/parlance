use std::{fs, process};

use clap::Parser;
use parlance_compiler::Compiler;
use parlance_prelude::io::print;
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
            let mut compiler = Compiler::new();
            compiler.insert_bytecode_function(print());

            let build_info = compiler
                .compile_source(&source)
                .unwrap_or_else(|diagnostic| {
                    eprintln!("{}", diagnostic.to_string());
                    process::exit(1);
                })
                .build_binding("main")
                .unwrap_or_else(|diagnostic| {
                    eprintln!("{}", diagnostic.to_string());
                    process::exit(1);
                });

            let mut vm = VirtualMachine::new();
            vm.load(build_info);

            unsafe {
                vm.run();
            }
        }
    }
}
