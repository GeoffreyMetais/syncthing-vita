//! `lookup`: asks the configured Global Discovery servers where a device can be reached.

use std::io;
use std::path::Path;

use syncthing_core::discovery::PeerEndpoint;
use syncthing_core::{lookup_peer_on, HttpsServer};

use crate::config::{load_config, parse_device_id};
use crate::daemon::timing::HTTPS_TIMEOUT;
use crate::https_env::https_client_from_env;

pub fn run(home: &Path, id: &str) -> io::Result<()> {
    let device = parse_device_id(id)?;
    let config = load_config(home)?;
    let (client, notes) = https_client_from_env();
    notes.iter().for_each(|n| eprintln!("{n}"));
    let mut failures = 0;
    for url in &config.global_lookup_servers {
        let Some(server) = HttpsServer::parse(url) else {
            eprintln!("invalid lookup server URL: {url}");
            failures += 1;
            continue;
        };
        match lookup_peer_on(&server, &client, device, HTTPS_TIMEOUT) {
            Ok(endpoints) if endpoints.is_empty() => {
                println!("{}: {device} is not announced", server.host);
            }
            Ok(endpoints) => {
                println!("{}: {device}", server.host);
                for ep in endpoints {
                    match ep {
                        PeerEndpoint::Direct(addr) => println!("  tcp://{addr}"),
                        PeerEndpoint::Relay { addr, device_id } => {
                            println!("  relay://{addr}/?id={device_id}")
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("{}: lookup failed: {e}", server.host);
                failures += 1;
            }
        }
    }
    if failures == config.global_lookup_servers.len() {
        return Err(io::Error::other("no Global Discovery server answered"));
    }
    Ok(())
}
