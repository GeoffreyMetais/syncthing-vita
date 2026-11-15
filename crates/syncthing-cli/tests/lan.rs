//! Local Discovery socket: several programs on one computer share UDP 21027.

use syncthing_cli::daemon::lan::{our_announce, LanBeacon};
use syncthing_core::{DeviceIdentity, DISCOVERY_PORT};

#[test]
fn two_beacons_share_the_discovery_port() {
    let id = |seed| DeviceIdentity::from_seed(&[seed; 32]).unwrap().device_id;
    let first = LanBeacon::bind(our_announce(id(71), Some(22000))).unwrap();
    if first.local_port().unwrap() != DISCOVERY_PORT {
        eprintln!("UDP {DISCOVERY_PORT} is held exclusively by another program: not checked");
        return;
    }
    let second = LanBeacon::bind(our_announce(id(72), None)).unwrap();
    assert_eq!(second.local_port().unwrap(), DISCOVERY_PORT);
}
