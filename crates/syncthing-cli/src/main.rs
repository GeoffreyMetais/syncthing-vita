//! Entry point: parses the command line and runs the command.

use clap::Parser;
use syncthing_cli::cli::Cli;
use syncthing_cli::commands;

fn main() {
    let cli = Cli::parse();
    if let Err(e) = commands::run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
