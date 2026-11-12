//! Power policies: standby blocking during synchronization and CPU clock levels.

mod common;

use std::time::Duration;

use vita_syncthing_app::application::power_policy::should_block_standby;

#[test]
fn test_standby_policy() {
    let recent = Some(Duration::from_secs(10));
    let old = Some(Duration::from_secs(600));
    // No sync running: never block.
    assert!(!should_block_standby(false, true, recent));
    // Charging: always block while syncing.
    assert!(should_block_standby(true, true, None));
    // On battery: only during transfers and the grace period after them.
    assert!(should_block_standby(true, false, recent));
    assert!(!should_block_standby(true, false, old));
    assert!(!should_block_standby(true, false, None));
}

#[test]
fn test_cpu_clock_full_speed_only_during_initial_sync_and_transfers() {
    use vita_syncthing_app::application::power_policy::{
        cpu_clock, CpuClock, CPU_INTERACTIVE_GRACE, CPU_TRANSFER_GRACE,
    };
    let idle_input = CPU_INTERACTIVE_GRACE + Duration::from_secs(1);
    let recent_input = Duration::from_millis(500);
    let recent_transfer = Some(Duration::from_secs(2));
    let old_transfer = Some(CPU_TRANSFER_GRACE + Duration::from_secs(1));

    // Transfers (and the short grace period after them) run at full speed.
    assert_eq!(
        cpu_clock(false, recent_transfer, idle_input, false),
        CpuClock::Full
    );
    assert_eq!(
        cpu_clock(false, recent_transfer, recent_input, true),
        CpuClock::Full
    );
    // The initial sync phase (index exchange, local scan) runs at full speed.
    assert_eq!(cpu_clock(true, None, idle_input, false), CpuClock::Full);
    assert_eq!(
        cpu_clock(true, old_transfer, recent_input, true),
        CpuClock::Full
    );
    // User interaction and QR decoding: medium speed.
    assert_eq!(
        cpu_clock(false, old_transfer, recent_input, false),
        CpuClock::Medium
    );
    assert_eq!(cpu_clock(false, None, idle_input, true), CpuClock::Medium);
    // Static screen, waiting or idle session: low speed.
    assert_eq!(
        cpu_clock(false, old_transfer, idle_input, false),
        CpuClock::Low
    );
    assert_eq!(cpu_clock(false, None, idle_input, false), CpuClock::Low);

    assert_eq!(CpuClock::Low.mhz(), 111);
    assert_eq!(CpuClock::Medium.mhz(), 222);
    assert_eq!(CpuClock::Full.mhz(), 333);
}

#[test]
fn test_initial_sync_peers() {
    use syncthing_core::DeviceId;
    use vita_syncthing_app::domain::InitialSyncPeers;
    let (a, b) = (DeviceId([1; 32]), DeviceId([2; 32]));
    let mut peers = InitialSyncPeers::default();
    assert!(!peers.is_active());
    peers.begin(a);
    peers.begin(b);
    peers.begin(a); // reconnection: no duplicate
    peers.end(a);
    assert!(peers.is_active(), "b is still in its initial phase");
    peers.end(b);
    assert!(!peers.is_active());
    // Later cycles of an established session do not restart it.
    peers.end(a);
    assert!(!peers.is_active());
    peers.begin(a);
    peers.clear();
    assert!(!peers.is_active());
}

#[test]
fn test_sync_events_drive_the_initial_sync_phase() {
    use std::sync::mpsc;
    use syncthing_core::{DeviceId, SyncEvent};
    use vita_syncthing_app::application::SyncCoordinator;
    use vita_syncthing_app::domain::{AppState, UiText};
    use vita_syncthing_app::infrastructure::{ConfigRepository, WorkerMsg};

    let repo = ConfigRepository::new(&common::temp_test_dir("initial-sync-phase"));
    let mut state = AppState::new(Vec::new(), Vec::new(), "ux0:");
    let (tx, rx) = mpsc::channel();
    let mut worker_rx = Some(rx);
    let peer = DeviceId([7; 32]);
    let cycle = |peer_device_id| {
        WorkerMsg::SyncEvent(SyncEvent::SyncCycleCompleted {
            peer_device_id,
            active_connections: 1,
            files_up_to_date: 0,
            files_downloaded: 0,
            bytes_downloaded: 0,
            blocks_uploaded: 0,
            bytes_uploaded: 0,
        })
    };

    tx.send(WorkerMsg::SyncEvent(SyncEvent::PeerConnected {
        peer_addr: "192.168.1.10:22000".parse().unwrap(),
        peer_device_id: peer,
        peer_name: "Desktop".to_owned(),
        client_version: "v1".to_owned(),
    }))
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(state.initial_sync.is_active());

    // Another peer's cycle does not end this peer's initial phase.
    tx.send(cycle(DeviceId([8; 32]))).unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(state.initial_sync.is_active());

    tx.send(cycle(peer)).unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!state.initial_sync.is_active());

    // The worker stopping ends every initial phase.
    state.initial_sync.begin(peer);
    tx.send(WorkerMsg::Finished {
        summary: UiText::SyncFinished,
        allow_auto_reconnect: true,
    })
    .unwrap();
    SyncCoordinator::process_worker_messages(&mut state, &repo, &mut worker_rx);
    assert!(!state.initial_sync.is_active());
}
