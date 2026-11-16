//! `device`: adds, lists and removes remote devices.

use std::io;
use std::path::Path;

use crate::cli::DeviceCommand;
use crate::config::{load_config, save_config};

pub fn run(home: &Path, cmd: DeviceCommand) -> io::Result<()> {
    let mut cfg = load_config(home)?;
    match cmd {
        DeviceCommand::Add {
            id,
            name,
            addresses,
        } => {
            let device = cfg.add_device(&id, name, addresses)?;
            save_config(home, &cfg)?;
            println!("Added device {} ({})", device.id, device.name);
        }
        DeviceCommand::List => {
            if cfg.devices.is_empty() {
                println!("No device configured.");
            }
            for device in &cfg.devices {
                println!(
                    "{}  {}  {}",
                    device.id,
                    device.name,
                    device.addresses.join(",")
                );
            }
        }
        DeviceCommand::Remove { id } => {
            cfg.remove_device(&id)?;
            save_config(home, &cfg)?;
            println!("Removed device {}", id.trim());
        }
    }
    Ok(())
}
