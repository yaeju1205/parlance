use std::{process, time::Instant};

use clap::Parser;
use parlance_compiler::Compiler;
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
}

pub fn run() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { file } => {
            let mut compiler = Compiler::new();

            compiler.insert_bytecode_function(print());
            compiler.insert_bytecode_function(add());

            let build_info = compiler
                .compile_source_file(file)
                .unwrap_or_else(|diagnostic| {
                    eprintln!("{}", diagnostic.to_string());
                    process::exit(1);
                })
                .build_binding("main")
                .unwrap_or_else(|diagnostic| {
                    eprintln!("{}", diagnostic.to_string());
                    process::exit(1);
                });

            let mut vm = VirtualMachine::new().with_load(build_info);

            if cli.verbose {
                let instant = Instant::now();
                unsafe {
                    vm.run();
                }
                println!("running time: {:?}", instant.elapsed());
            } else {
                unsafe {
                    vm.run();
                }
            }
        }
    }
}
