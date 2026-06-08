use std::{process, time::Instant};

use clap::Parser;
use parlance_compiler::{CompileObject, Compiler};
use parlance_module::Pars;
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

fn new_compiler() -> Compiler {
    let mut compiler = Compiler::new();
    compiler.insert_bytecode_function(print());
    compiler.insert_bytecode_function(add());
    compiler
}

fn run_object(compile_object: CompileObject, verbose: bool) {
    let build_info = compile_object.build_binding("main").unwrap_or_else(|diagnostic| {
        eprintln!("{}", diagnostic.to_string());
        process::exit(1);
    });

    let mut vm = VirtualMachine::new().with_load(build_info);

    if verbose {
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

/// Compile and run a packed `.pars` bundle. Shared by `parlance run` and
/// `astro run`.
pub fn run_pars(pars: &Pars, verbose: bool) {
    let compile_object = new_compiler().compile_pars(pars).unwrap_or_else(|diagnostic| {
        eprintln!("{}", diagnostic.to_string());
        process::exit(1);
    });
    run_object(compile_object, verbose);
}

pub fn run() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { file } => {
            let is_pars = std::path::Path::new(&file)
                .extension()
                .is_some_and(|ext| ext == "pars");

            let compiler = new_compiler();
            let compile_object = if is_pars {
                compiler.compile_pars_file(file)
            } else {
                compiler.compile_source_file(file)
            }
            .unwrap_or_else(|diagnostic| {
                eprintln!("{}", diagnostic.to_string());
                process::exit(1);
            });

            run_object(compile_object, cli.verbose);
        }
    }
}
