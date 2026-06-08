use std::{path::PathBuf, process};

use clap::Parser;

#[derive(Parser)]
#[command(name = "astro")]
#[command(version, about = "The Parlance package manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Pack {
        entry: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    Run {
        entry: Option<PathBuf>,
        #[arg(short, long)]
        verbose: bool,
    },
}

/// Resolve an explicit entry, or fall back to `main` in the nearest `astro.toml`.
fn resolve_entry(entry: Option<PathBuf>) -> PathBuf {
    entry.map(Ok).unwrap_or_else(|| {
        std::env::current_dir().and_then(|cwd| astro::default_entry(&cwd))
    }).unwrap_or_else(|err| {
        eprintln!("{err}");
        process::exit(1);
    })
}

fn pack_entry(entry: &PathBuf) -> parlance_module::Pars {
    astro::pack(entry).unwrap_or_else(|err| {
        eprintln!("pack failed: {err}");
        process::exit(1);
    })
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack { entry, output } => {
            let entry = resolve_entry(entry);
            let output = output.unwrap_or_else(|| entry.with_extension("pars"));

            let pars = pack_entry(&entry);

            astro::write_pars(&pars, &output).unwrap_or_else(|err| {
                eprintln!("failed to write {}: {err}", output.display());
                process::exit(1);
            });

            println!("packed {} files into {}", pars.files.len(), output.display());
        }
        Commands::Run { entry, verbose } => {
            let entry = resolve_entry(entry);
            let pars = pack_entry(&entry);
            parlance_cli::run_pars(&pars, verbose);
        }
    }
}
