//! `relays`: lists the relays of the configured relay pool.

use std::io;
use std::path::Path;

use syncthing_core::{fetch_relay_pool, HttpsServer};

use crate::config::load_config;
use crate::daemon::timing::HTTPS_TIMEOUT;
use crate::https_env::https_client_from_env;

pub fn run(home: &Path) -> io::Result<()> {
    let config = load_config(home)?;
    let pool = HttpsServer::parse(&config.relay_pool_url).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid relay pool URL {}", config.relay_pool_url),
        )
    })?;
    let (client, notes) = https_client_from_env();
    notes.iter().for_each(|n| eprintln!("{n}"));
    let relays = fetch_relay_pool(&pool, &client, HTTPS_TIMEOUT)?;
    for relay in &relays {
        println!("{relay}");
    }
    eprintln!("{} relay(s) in {}", relays.len(), pool.host);
    Ok(())
}
