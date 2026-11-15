//! `run`: the sync daemon.

use std::io;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use crate::cli::RunArgs;
use crate::config::{load_config, Config};
use crate::daemon::context::DaemonOptions;
use crate::daemon::log::log;
use crate::daemon::{run_daemon, ExitPolicy};
use crate::https_env::https_client_from_env;
use crate::identity::load_or_create_identity;

/// The configuration, overridden by the command-line flags.
pub fn daemon_options(config: &Config, args: &RunArgs) -> io::Result<DaemonOptions> {
    let listen = args.listen.as_deref().unwrap_or(&config.listen);
    let listen = if listen.is_empty() || listen.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(listen.parse::<SocketAddr>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid listen address `{listen}`"),
            )
        })?)
    };
    Ok(DaemonOptions {
        listen,
        local_discovery: config.local_discovery && !args.no_local_discovery,
        global_discovery: config.global_discovery && !args.no_global_discovery,
        relays: config.relays && !args.no_relays,
        announce_servers: config.global_announce_servers.clone(),
        lookup_servers: config.global_lookup_servers.clone(),
        relay_pool_url: config.relay_pool_url.clone(),
        static_relays: config.static_relays.clone(),
        https: https_client_from_env().0,
    })
}

pub fn run(home: &Path, args: RunArgs) -> io::Result<()> {
    let config = load_config(home)?;
    let identity = load_or_create_identity(home)?;
    let options = daemon_options(&config, &args)?;
    log(&format!(
        "syncthing-cli {} — device {} ({})",
        env!("CARGO_PKG_VERSION"),
        identity.device_id,
        config.device_name
    ));
    if options.global_discovery || options.relays {
        for note in https_client_from_env().1 {
            log(&note);
        }
    }
    if config.devices.is_empty() {
        log("No device configured: add one with `syncthing-cli device add <ID>`");
    }
    for folder in &config.folders {
        log(&format!(
            "Folder {} at {} shared with {} device(s){}{}",
            folder.id,
            folder.path.display(),
            folder.devices.len(),
            if folder.receive_only {
                ", receive only"
            } else {
                ""
            },
            if folder.paused { ", paused" } else { "" }
        ));
    }
    let exit = ExitPolicy {
        after: args.exit_after.map(Duration::from_secs),
        when_synced: args.exit_when_synced,
    };
    run_daemon(home, &config, identity, options, exit)
}
