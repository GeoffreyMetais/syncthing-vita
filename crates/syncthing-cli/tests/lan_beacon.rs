//! Local Discovery behaviour: the `LanBeacon` schedule (answers, own announcements, broadcasts),
//! the address display, and the daemon's LAN thread.
//!
//! Each test holds UDP 21027 exclusively (a plain bind, without `SO_REUSEPORT`). The beacons
//! then fall back to a random port, so packets sent to them are not shared with any other
//! program on the machine, and the broadcasts they send are heard by the held socket.

mod common;

use std::net::{SocketAddr, UdpSocket};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use syncthing_cli::config::{save_config, Config, DeviceConfig};
use syncthing_cli::daemon::lan::{format_addrs, our_announce, LanBeacon};
use syncthing_cli::daemon::timing::MAX_LAN_REPLIES_TO_UNKNOWN;
use syncthing_core::{Announce, DeviceId, DeviceIdentity, DISCOVERY_PORT};

use common::{free_port, temp_home, Cli};

/// Serializes the tests that hold UDP 21027.
static DISCOVERY_PORT_LOCK: Mutex<()> = Mutex::new(());

/// The exclusive holder of UDP 21027, or `None` when another program holds the port.
fn hold_discovery_port() -> Option<UdpSocket> {
    let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).ok()?;
    socket
        .set_read_timeout(Some(Duration::from_millis(300)))
        .ok()?;
    Some(socket)
}

fn id(seed: u8) -> DeviceId {
    DeviceIdentity::from_seed(&[seed; 32])
        .expect("valid seed")
        .device_id
}

fn announce_packet(id: DeviceId, addresses: &[&str]) -> Vec<u8> {
    Announce {
        id,
        addresses: addresses.iter().map(|a| (*a).to_owned()).collect(),
        instance_id: 1,
    }
    .encode_packet()
}

/// Sends `packet` from `from` to the loopback port `port`.
fn send(from: &UdpSocket, packet: &[u8], port: u16) {
    from.send_to(packet, ("127.0.0.1", port))
        .expect("send announcement");
}

#[test]
fn beacon_answers_unknown_devices_on_a_bounded_schedule_but_always_answers_configured_ones() {
    let _guard = DISCOVERY_PORT_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let Some(_holder) = hold_discovery_port() else {
        eprintln!("UDP {DISCOVERY_PORT} is held by another program: not checked");
        return;
    };
    let mut beacon = LanBeacon::bind(our_announce(id(1), None)).expect("bind beacon");
    let port = beacon.local_port().expect("beacon port");
    assert_ne!(
        port, DISCOVERY_PORT,
        "the beacon must use its own port here"
    );
    let configured = id(200);
    beacon.set_configured(vec![configured]);

    // Fill the schedule with unknown devices, then announce the configured one last.
    let peer = UdpSocket::bind("127.0.0.1:0").expect("bind peer");
    peer.set_read_timeout(Some(Duration::from_millis(300)))
        .expect("read timeout");
    let unknown: Vec<Vec<u8>> = (0..MAX_LAN_REPLIES_TO_UNKNOWN)
        .map(|n| announce_packet(id(100 + n as u8), &[]))
        .collect();
    for packet in &unknown {
        send(&peer, packet, port);
    }
    send(&peer, &announce_packet(configured, &[]), port);
    for _ in 0..=unknown.len() {
        beacon.step();
    }

    // Every announcement is answered except the unknown devices beyond the schedule.
    let mut replies = 0;
    let mut buf = [0u8; 2048];
    while peer.recv_from(&mut buf).is_ok() {
        replies += 1;
    }
    assert_eq!(
        replies,
        MAX_LAN_REPLIES_TO_UNKNOWN + 1,
        "each unknown device up to the limit, and the configured device, get one answer"
    );
}

#[test]
fn beacon_hears_other_devices_but_never_its_own_announcement() {
    let _guard = DISCOVERY_PORT_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let Some(_holder) = hold_discovery_port() else {
        eprintln!("UDP {DISCOVERY_PORT} is held by another program: not checked");
        return;
    };
    let me = id(7);
    let other = id(8);
    let mut beacon = LanBeacon::bind(our_announce(me, None)).expect("bind beacon");
    let port = beacon.local_port().expect("beacon port");
    let sender = UdpSocket::bind("127.0.0.1:0").expect("bind sender");
    send(&sender, &announce_packet(me, &[]), port);
    send(
        &sender,
        &announce_packet(other, &["tcp://127.0.0.1:22000"]),
        port,
    );

    let mut heard = Vec::new();
    for _ in 0..4 {
        if let Some(peer) = beacon.step() {
            heard.push(peer);
        }
    }
    assert!(
        heard.iter().any(|p| p.id == other),
        "the other device is heard"
    );
    assert!(
        heard.iter().all(|p| p.id != me),
        "our own announcement is ignored"
    );
    let other_heard = heard.iter().find(|p| p.id == other).expect("other device");
    assert_eq!(
        other_heard.addrs,
        vec!["127.0.0.1:22000".parse::<SocketAddr>().unwrap()]
    );
}

#[test]
fn beacon_broadcasts_its_announcement_once_per_interval() {
    let _guard = DISCOVERY_PORT_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let Some(holder) = hold_discovery_port() else {
        eprintln!("UDP {DISCOVERY_PORT} is held by another program: not checked");
        return;
    };
    let me = id(9);
    let mut beacon = LanBeacon::bind(our_announce(me, None)).expect("bind beacon");
    // Each step with nothing to hear waits for the receive timeout: four steps in all.
    for _ in 0..4 {
        assert!(beacon.step().is_none());
    }

    let mut broadcasts = 0;
    let mut buf = [0u8; 2048];
    while let Ok((len, _)) = holder.recv_from(&mut buf) {
        if Announce::decode_packet(&buf[..len]).is_ok_and(|a| a.id == me) {
            broadcasts += 1;
        }
    }
    // One broadcast (global, plus the subnet one when the local address is known) in the
    // interval; a beacon broadcasting on every step would send at least four.
    assert!(broadcasts >= 1, "the announcement is broadcast");
    assert!(
        broadcasts <= 2,
        "{broadcasts} broadcast packets in one interval"
    );
}

#[test]
fn address_display_names_the_addresses_or_the_source_of_a_device_without_any() {
    let source: SocketAddr = "192.0.2.9:21027".parse().unwrap();
    assert_eq!(
        format_addrs(&[], source),
        "no address (heard from 192.0.2.9)"
    );
    let one: SocketAddr = "10.0.0.5:22000".parse().unwrap();
    assert_eq!(format_addrs(&[one], source), "tcp://10.0.0.5:22000");
    let two: SocketAddr = "10.0.0.6:22000".parse().unwrap();
    assert_eq!(
        format_addrs(&[one, two], source),
        "tcp://10.0.0.5:22000, tcp://10.0.0.6:22000"
    );
}

/// The daemon's LAN thread: a configured device heard on the LAN is dialed at the address it
/// announced, and its announcement is logged.
#[test]
fn daemon_learns_the_lan_address_of_a_configured_device() {
    let _guard = DISCOVERY_PORT_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let Some(holder) = hold_discovery_port() else {
        eprintln!("UDP {DISCOVERY_PORT} is held by another program: not checked");
        return;
    };
    let home = temp_home("lan-daemon");
    let cli = Cli::new(&home);
    let daemon_id = cli.id();
    let laptop = id(120).to_syncthing_string();
    let mut config = Config::default();
    config.devices.push(DeviceConfig {
        id: laptop.clone(),
        name: "laptop".to_owned(),
        addresses: vec!["dynamic".to_owned()],
    });
    save_config(&home, &config).expect("save configuration");

    let daemon = cli.spawn_daemon(&["--listen", "none", "--no-global-discovery", "--no-relays"]);

    // The daemon's own broadcast tells where it listens (its port is random here).
    let daemon_source = wait_for_announcement_from(&holder, &daemon_id);

    let closed = free_port();
    let announced = format!("tcp://127.0.0.1:{closed}");
    let packet = announce_packet(id(120), &[announced.as_str()]);
    let deadline = Instant::now() + Duration::from_secs(15);
    let expected = format!("Cannot sync with laptop at {announced}");
    while !daemon.stdout().contains(&expected) {
        if Instant::now() >= deadline {
            panic!(
                "the daemon never dialed the announced address\n--- stdout ---\n{}\n--- stderr ---\n{}",
                daemon.stdout(),
                daemon.stderr()
            );
        }
        let _ = holder.send_to(&packet, daemon_source);
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        daemon
            .stdout()
            .contains(&format!("LAN: laptop announces {announced}")),
        "stdout:\n{}",
        daemon.stdout()
    );
}

/// Waits for a broadcast of the device `id` on the held port and returns its source address.
fn wait_for_announcement_from(holder: &UdpSocket, id: &str) -> SocketAddr {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut buf = [0u8; 2048];
    loop {
        assert!(
            Instant::now() < deadline,
            "the daemon never broadcast its announcement"
        );
        if let Ok((len, source)) = holder.recv_from(&mut buf) {
            if let Ok(announce) = Announce::decode_packet(&buf[..len]) {
                if announce.id.to_syncthing_string() == id {
                    return source;
                }
            }
        }
    }
}
