//! Relay mode of the daemon: joining a static relay, losing it, and the sessions a relay
//! invites the daemon to, including the refusal of an invitation whose TLS identity is not
//! the device that was invited.

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use syncthing_cli::config::{save_config, Config, DeviceConfig};
use syncthing_core::tls::client::wrap_bep_tls_stream;
use syncthing_core::DeviceIdentity;

use common::servers::{
    drain_frames, invitation_frame, isolate_proxy_env, read_frame, response_frame, spawn_relay,
    spawn_tcp_service, MSG_JOIN_SESSION_REQUEST,
};
use common::{temp_home, Cli, Daemon};

/// Seed of the device the tests invite (the configured peer).
const LAPTOP_SEED: u8 = 0x51;
/// Seed of the relay.
const RELAY_SEED: u8 = 0x52;
/// How long a relayed TLS handshake may take.
const TIMEOUT: Duration = Duration::from_secs(5);

fn identity(seed: u8) -> DeviceIdentity {
    DeviceIdentity::from_seed(&[seed; 32]).expect("valid seed")
}

/// A home whose only device is `laptop` (no address: it is found through the relay), and
/// whose only relay is the relay on `relay_port`.
fn configure_relay_home(home: &Path, relay_port: u16) {
    let relay = identity(RELAY_SEED);
    let mut config = Config::default();
    config.static_relays = vec![format!(
        "relay://127.0.0.1:{relay_port}/?id={}",
        relay.device_id.to_syncthing_string()
    )];
    config.devices.push(DeviceConfig {
        id: identity(LAPTOP_SEED).device_id.to_syncthing_string(),
        name: "laptop".to_owned(),
        addresses: vec!["dynamic".to_owned()],
    });
    save_config(home, &config).expect("save configuration");
}

/// Starts the daemon of `home` with no listener and no discovery besides its relays.
fn spawn_relay_daemon(cli: &Cli) -> Daemon {
    cli.spawn_daemon(&[
        "--listen",
        "none",
        "--no-local-discovery",
        "--no-global-discovery",
    ])
}

#[test]
fn daemon_joins_its_static_relay_and_stays_joined() {
    isolate_proxy_env();
    let home = temp_home("relay-join");
    let relay_port = spawn_relay(&identity(RELAY_SEED), |stream| drain_frames(stream));
    configure_relay_home(&home, relay_port);
    let cli = Cli::new(&home);
    let daemon = spawn_relay_daemon(&cli);

    daemon.wait_for_log_line(
        &format!("Reachable through relay 127.0.0.1:{relay_port}"),
        Duration::from_secs(15),
    );
}

#[test]
fn daemon_reports_when_its_relay_connection_is_lost() {
    isolate_proxy_env();
    let home = temp_home("relay-lost");
    // The relay drops the connection right after the join.
    let relay_port = spawn_relay(&identity(RELAY_SEED), |_| {});
    configure_relay_home(&home, relay_port);
    let cli = Cli::new(&home);
    let daemon = spawn_relay_daemon(&cli);

    daemon.wait_for_log_line(
        &format!("Lost relay 127.0.0.1:{relay_port}"),
        Duration::from_secs(15),
    );
}

/// Runs a daemon whose relay invites `laptop` to a session on a plain TCP port, which answers
/// the join and then runs `session` on the connection. Returns the daemon's log.
fn run_invitation<S>(name: &str, server_socket: bool, session: S) -> Daemon
where
    S: Fn(TcpStream) + Send + Sync + 'static,
{
    let session_port = spawn_tcp_service(move |mut tcp| {
        match read_frame(&mut tcp) {
            Ok((MSG_JOIN_SESSION_REQUEST, _)) => {}
            _ => return,
        }
        if tcp.write_all(&response_frame(0, "success")).is_err() {
            return;
        }
        session(tcp);
    });
    let laptop = identity(LAPTOP_SEED).device_id;
    let raw_from = laptop.0.to_vec();
    let relay_port = spawn_relay(&identity(RELAY_SEED), move |stream| {
        let _ = stream.write_all(&invitation_frame(
            &raw_from,
            b"session-key",
            session_port,
            server_socket,
        ));
        drain_frames(stream);
    });
    let home = temp_home(name);
    configure_relay_home(&home, relay_port);
    spawn_relay_daemon(&Cli::new(&home))
}

#[test]
fn daemon_joins_the_session_a_configured_device_invites_it_to() {
    isolate_proxy_env();
    // The invited session fails at its TLS handshake: the failure is logged with the device.
    let daemon = run_invitation("relay-invite", false, |_tcp| {});
    daemon.wait_for_log_line(
        "Relayed session with laptop failed",
        Duration::from_secs(20),
    );
}

/// The TLS identity presented on the relayed connection: the daemon accepts it as the server
/// of the session, then runs the session only when the identity is the invited device's.
#[test]
fn relayed_connection_from_another_device_than_the_invited_one_is_dropped() {
    isolate_proxy_env();
    let impostor = identity(0x53);
    let daemon = run_invitation("relay-impostor", true, move |tcp| {
        present_identity(&impostor, tcp)
    });
    daemon.wait_for_log_line(
        "Relay session announced as laptop came from another device: dropped",
        Duration::from_secs(20),
    );
    assert!(
        !daemon.stdout().contains("Relayed session with laptop"),
        "a session must not run with the impostor:\n{}",
        daemon.stdout()
    );
}

#[test]
fn relayed_connection_from_the_invited_device_runs_a_session() {
    isolate_proxy_env();
    let laptop = identity(LAPTOP_SEED);
    let daemon = run_invitation("relay-honest", true, move |tcp| {
        present_identity(&laptop, tcp)
    });
    // The session runs with the invited device (it ends without its acceptance here).
    daemon.wait_for_log_line("laptop (relayed)", Duration::from_secs(20));
    assert!(
        !daemon.stdout().contains("came from another device"),
        "the invited device must not be dropped:\n{}",
        daemon.stdout()
    );
}

/// Plays the device `who` on the relayed connection: a TLS client presenting its identity,
/// which waits for the daemon's first message (or its closing) and then ends.
fn present_identity(who: &DeviceIdentity, tcp: TcpStream) {
    let Ok((mut stream, _)) = wrap_bep_tls_stream(who, tcp, None, TIMEOUT) else {
        return;
    };
    let mut buf = [0u8; 64];
    let _ = stream.read(&mut buf);
}
