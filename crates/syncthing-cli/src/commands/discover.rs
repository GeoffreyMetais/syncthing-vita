//! `discover`: lists the devices announcing themselves on the LAN.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use syncthing_core::DeviceId;

use crate::cli::DiscoverArgs;
use crate::config::load_config;
use crate::daemon::lan::{format_addrs, our_announce, LanBeacon};
use crate::identity::load_or_create_identity;

pub fn run(home: &Path, args: DiscoverArgs) -> io::Result<()> {
    let config = load_config(home)?;
    let identity = load_or_create_identity(home)?;
    // We do not listen while discovering: announce no address.
    let mut beacon = LanBeacon::bind(our_announce(identity.device_id, None))?;
    println!("Listening for LAN announcements for {} s...", args.seconds);
    let deadline = Instant::now() + Duration::from_secs(args.seconds);
    let mut seen: Vec<DeviceId> = Vec::new();
    while Instant::now() < deadline {
        let Some(peer) = beacon.step() else {
            continue;
        };
        if seen.contains(&peer.id) {
            continue;
        }
        seen.push(peer.id);
        let known = config
            .devices
            .iter()
            .find(|d| d.device_id() == Some(peer.id))
            .map(|d| format!("configured as \"{}\"", d.name))
            .unwrap_or_else(|| "not configured".to_owned());
        println!(
            "{}  {}  ({known})",
            peer.id,
            format_addrs(&peer.addrs, peer.source)
        );
    }
    if seen.is_empty() {
        println!("No device found.");
    }
    Ok(())
}
