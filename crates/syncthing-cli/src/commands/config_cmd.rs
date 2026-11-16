//! `config`: shows the configuration and changes general options.

use std::io;
use std::path::Path;

use crate::cli::ConfigCommand;
use crate::config::{load_config, save_config};
use crate::identity::load_or_create_identity;

pub fn run(home: &Path, cmd: ConfigCommand) -> io::Result<()> {
    match cmd {
        ConfigCommand::Show => {
            let cfg = load_config(home)?;
            let identity = load_or_create_identity(home)?;
            let text = toml::to_string_pretty(&cfg).map_err(|e| io::Error::other(e.to_string()))?;
            println!("# device id: {}", identity.device_id.to_syncthing_string());
            print!("{text}");
        }
        ConfigCommand::Set { key, value } => {
            let mut cfg = load_config(home)?;
            cfg.set_option(&key, &value)?;
            save_config(home, &cfg)?;
            println!("Set {key} to {}", value.trim());
        }
    }
    Ok(())
}
