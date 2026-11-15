//! One file per command; [`run`] dispatches the parsed command line.

pub mod config_cmd;
pub mod device;
pub mod discover;
pub mod folder;
pub mod id;
pub mod run;

use std::io;

use crate::cli::{Cli, Command};
use crate::paths::resolve_home;

/// Runs the parsed command line.
pub fn run(cli: Cli) -> io::Result<()> {
    let home = resolve_home(cli.home)?;
    match cli.command {
        Command::Id => id::run(&home),
        Command::Device(cmd) => device::run(&home, cmd),
        Command::Folder(cmd) => folder::run(&home, cmd),
        Command::Config(cmd) => config_cmd::run(&home, cmd),
        Command::Discover(args) => discover::run(&home, args),
        Command::Run(args) => run::run(&home, args),
    }
}
