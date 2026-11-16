//! `folder`: adds, lists, removes, shares, pauses and resumes shared folders.

use std::fs;
use std::io;
use std::path::Path;

use crate::cli::FolderCommand;
use crate::config::{load_config, parse_device_id, save_config, Config, FolderConfig};

pub fn run(home: &Path, cmd: FolderCommand) -> io::Result<()> {
    let mut cfg = load_config(home)?;
    match cmd {
        FolderCommand::Add {
            id,
            path,
            label,
            devices,
            receive_only,
        } => {
            fs::create_dir_all(&path)?;
            let path = fs::canonicalize(&path)?;
            cfg.add_folder(&id, path.clone(), label, devices, receive_only)?;
            save_config(home, &cfg)?;
            println!("Added folder {} at {}", id.trim(), path.display());
        }
        FolderCommand::List => print_folders(&cfg),
        FolderCommand::Remove { id } => {
            cfg.remove_folder(&id)?;
            save_config(home, &cfg)?;
            println!("Removed folder {} (its files are kept)", id.trim());
        }
        FolderCommand::Share { folder, device } => {
            cfg.share_folder(&folder, &device)?;
            save_config(home, &cfg)?;
            println!("Shared folder {} with {}", folder.trim(), device.trim());
        }
        FolderCommand::Unshare { folder, device } => {
            cfg.unshare_folder(&folder, &device)?;
            save_config(home, &cfg)?;
            println!("Unshared folder {} from {}", folder.trim(), device.trim());
        }
        FolderCommand::Pause { folder } => {
            cfg.set_folder_paused(&folder, true)?;
            save_config(home, &cfg)?;
            println!("Paused folder {}", folder.trim());
        }
        FolderCommand::Resume { folder } => {
            cfg.set_folder_paused(&folder, false)?;
            save_config(home, &cfg)?;
            println!("Resumed folder {}", folder.trim());
        }
    }
    Ok(())
}

fn print_folders(cfg: &Config) {
    if cfg.folders.is_empty() {
        println!("No folder configured.");
    }
    for folder in &cfg.folders {
        println!("{}", folder_line(folder));
    }
}

fn folder_line(folder: &FolderConfig) -> String {
    let mut line = format!("{}  {}  {}", folder.id, folder.label, folder.path.display());
    if folder.receive_only {
        line.push_str("  [receive-only]");
    }
    if folder.paused {
        line.push_str("  [paused]");
    }
    let shares: Vec<String> = folder
        .devices
        .iter()
        .filter_map(|entry| parse_device_id(entry).ok())
        .map(|id| id.short())
        .collect();
    let shares = if shares.is_empty() {
        "none".to_owned()
    } else {
        shares.join(",")
    };
    line.push_str(&format!("  devices: {shares}"));
    line
}
