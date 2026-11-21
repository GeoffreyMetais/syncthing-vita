//! Host-observable start-up of the sync worker: shares whose folder is the app's own data are
//! refused with a log line, and the linked peers are listed (and capped) before any session.
//! The worker is stopped right after its start-up messages, so no network lookup is made.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use syncthing_core::DeviceIdentity;
use vita_syncthing_app::domain::{KnownDevice, ShareConfigEntry, ShareMode};
use vita_syncthing_app::infrastructure::{spawn_sync_worker, WorkerMsg};

mod common;
use common::temp_test_dir;

const WAIT: Duration = Duration::from_secs(30);

/// Receives messages until `stop` says so or the worker ends (its sender is dropped).
fn drain_until(rx: &Receiver<WorkerMsg>, mut stop: impl FnMut(&WorkerMsg) -> bool) -> Vec<String> {
    let mut logs = Vec::new();
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(msg) => {
                if let WorkerMsg::Log(line) = &msg {
                    logs.push(line.clone());
                }
                if stop(&msg) {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    logs
}

fn identity() -> DeviceIdentity {
    DeviceIdentity::from_seed(&[7u8; 32]).unwrap()
}

fn share(folder_id: &str, target: String) -> ShareConfigEntry {
    ShareConfigEntry {
        folder_id: folder_id.to_owned(),
        label: folder_id.to_owned(),
        target_path: target,
        enabled: true,
        mode: ShareMode::ReadWrite,
        shared_devices: Vec::new(),
    }
}

#[test]
fn test_share_of_the_app_data_is_refused_and_others_are_not() {
    let base = temp_test_dir("worker-protected");
    let shares = vec![
        // The data directory itself: refused, and logged with its folder ID and path.
        share("own", base.clone()),
        // A folder inside the data directory's sync area: accepted.
        share("ok", format!("{base}/sync/ok")),
    ];
    let (rx, control) = spawn_sync_worker(
        &base,
        identity(),
        shares,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );

    let mut listening = false;
    let logs = drain_until(&rx, |msg| match msg {
        WorkerMsg::Log(line) if line.starts_with("Listening for incoming BEP/1.0") => {
            listening = true;
            true
        }
        _ => false,
    });
    control.request_stop();
    let _ = drain_until(&rx, |_| false);

    let refused = format!("Share own not synchronized: {base} holds the app's own data.");
    assert!(logs.contains(&refused), "logs: {logs:?}");
    assert!(
        !logs.iter().any(|l| l.contains("Share ok not synchronized")),
        "a folder inside the sync area is not refused"
    );
    // Even with 0 paired devices, the worker binds the incoming BEP listener and stays reachable.
    assert!(listening, "incoming BEP listener started: {logs:?}");
}

#[test]
fn test_linked_peers_are_listed_then_capped_at_the_session_limit() {
    let base = temp_test_dir("worker-peers");
    let devices: Vec<KnownDevice> = (1u8..=5)
        .map(|i| KnownDevice {
            device_id: syncthing_core::DeviceId([i; 32]),
            name: format!("Dev{i}"),
            address: "127.0.0.1:1".to_owned(),
        })
        .collect();
    let (rx, control) = spawn_sync_worker(
        &base,
        identity(),
        Vec::new(),
        devices.clone(),
        devices.clone(),
        Vec::new(),
    );

    let logs = drain_until(
        &rx,
        |msg| matches!(msg, WorkerMsg::Log(line) if line.starts_with("Linked peers")),
    );
    control.request_stop();
    let rest = drain_until(&rx, |_| false);

    let listed: Vec<String> = devices[..4]
        .iter()
        .map(|kd| format!("{} [{}] @ {}", kd.name, kd.device_id.short(), kd.address))
        .collect();
    let linked = logs
        .iter()
        .find(|l| l.starts_with("Linked peers: "))
        .unwrap_or_else(|| panic!("no peer list in {logs:?}"));
    assert_eq!(linked, &format!("Linked peers: {}", listed.join(", ")));
    assert!(
        !linked.contains("Dev5"),
        "only the first four peers are listed"
    );
    assert!(
        logs.iter()
            .chain(rest.iter())
            .any(|l| l == "5 devices are linked: only the first 4 are synchronized."),
        "the cap is reported once: {logs:?} {rest:?}"
    );
}
