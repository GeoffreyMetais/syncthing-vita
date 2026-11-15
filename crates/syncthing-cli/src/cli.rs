//! Command-line definition (`clap`).

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Command-line Syncthing client (BEP/1.0, LAN discovery, Global Discovery, relays).
#[derive(Debug, Parser)]
#[command(name = "syncthing-cli", version, about)]
pub struct Cli {
    /// Configuration directory (identity, `config.toml`, folder databases).
    /// Defaults to `$SYNCTHING_CLI_HOME`, then the platform configuration directory.
    #[arg(long, global = true, value_name = "DIR")]
    pub home: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print this device's ID (creates the identity on first use).
    Id,
    /// Manage remote devices.
    #[command(subcommand)]
    Device(DeviceCommand),
    /// Manage shared folders.
    #[command(subcommand)]
    Folder(FolderCommand),
    /// Show or change general options.
    #[command(subcommand)]
    Config(ConfigCommand),
    /// Listen for Syncthing devices announcing themselves on the LAN.
    Discover(DiscoverArgs),
    /// Run the sync daemon until interrupted.
    Run(RunArgs),
}

#[derive(Debug, Subcommand)]
pub enum DeviceCommand {
    /// Add a remote device.
    Add {
        /// Full device ID (`XXXXXXX-XXXXXXX-...`).
        id: String,
        /// Display name.
        #[arg(long)]
        name: Option<String>,
        /// Address (`tcp://host:port`, `host:port` or `dynamic`); repeatable. Default: `dynamic`.
        #[arg(long = "address", value_name = "ADDR")]
        addresses: Vec<String>,
    },
    /// List the configured devices.
    List,
    /// Remove a device (it is also unshared from every folder).
    Remove {
        /// Device ID.
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum FolderCommand {
    /// Add a shared folder.
    Add {
        /// Folder ID (identical on every device sharing it).
        id: String,
        /// Local directory (created if missing).
        path: PathBuf,
        /// Display label (default: the folder ID).
        #[arg(long)]
        label: Option<String>,
        /// Device to share the folder with; repeatable.
        #[arg(long = "device", value_name = "DEVICE_ID")]
        devices: Vec<String>,
        /// Receive changes only: local changes are never sent.
        #[arg(long)]
        receive_only: bool,
    },
    /// List the configured folders.
    List,
    /// Remove a folder (its files are kept).
    Remove {
        /// Folder ID.
        id: String,
    },
    /// Share a folder with a device.
    Share { folder: String, device: String },
    /// Stop sharing a folder with a device.
    Unshare { folder: String, device: String },
    /// Pause a folder.
    Pause { folder: String },
    /// Resume a paused folder.
    Resume { folder: String },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the configuration.
    Show,
    /// Change an option: `name`, `listen`, `local-discovery`, `global-discovery`, `relays`,
    /// `rescan-interval` (seconds). Server lists are edited in `config.toml`.
    Set { key: String, value: String },
}

#[derive(Debug, Args)]
pub struct DiscoverArgs {
    /// How long to listen.
    #[arg(long, default_value_t = 10)]
    pub seconds: u64,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Listen address (overrides the configuration), e.g. `0.0.0.0:22000`.
    #[arg(long, value_name = "ADDR")]
    pub listen: Option<String>,
    /// Disable LAN discovery (UDP 21027).
    #[arg(long)]
    pub no_local_discovery: bool,
    /// Disable Global Discovery (lookup and announcement).
    #[arg(long)]
    pub no_global_discovery: bool,
    /// Disable relays (neither dialed nor joined).
    #[arg(long)]
    pub no_relays: bool,
    /// Exit after this many seconds (for scripts and tests).
    #[arg(long, value_name = "SECONDS")]
    pub exit_after: Option<u64>,
    /// Exit as soon as every connected device completed a sync cycle and no transfer is running.
    #[arg(long)]
    pub exit_when_synced: bool,
}
