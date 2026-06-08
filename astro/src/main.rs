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
        entry: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack { entry, output } => {
            let output = output.unwrap_or_else(|| entry.with_extension("pars"));

            let pars = astro::pack(&entry).unwrap_or_else(|err| {
                eprintln!("pack failed: {err}");
                process::exit(1);
            });

            astro::write_pars(&pars, &output).unwrap_or_else(|err| {
                eprintln!("failed to write {}: {err}", output.display());
                process::exit(1);
            });

            println!("packed {} modules into {}", pars.pars.len(), output.display());
        }
    }
}
