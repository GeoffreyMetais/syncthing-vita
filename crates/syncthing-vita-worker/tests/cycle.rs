//! One background cycle end to end, against a `syncthing-core` peer on `127.0.0.1`: what it
//! pulls and sends, that it ends on its own, and what stops or skips it.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{
    data_dir_with_identity, device, fresh_temp_dir, repo, share, test_options, ListeningPeer,
};
use syncthing_vita_worker::{
    never_abort, run_cycle, AbortSignal, CycleOutcome, PeerOutcome, PeerStats,
};

fn read_log(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("syncthing.log")).unwrap_or_default()
}

#[test]
fn test_one_cycle_pulls_sends_and_ends() {
    let data = data_dir_with_identity("cycle_both_ways", true);
    let peer_root = fresh_temp_dir("cycle_both_ways_peer");
    std::fs::create_dir_all(peer_root.join("saves")).unwrap();
    std::fs::write(
        peer_root.join("saves/from_peer.txt"),
        b"hello from the peer",
    )
    .unwrap();
    let peer = ListeningPeer::spawn(0x31, &peer_root, &["saves"]);

    let vita_saves = data.join("vita-saves");
    std::fs::create_dir_all(&vita_saves).unwrap();
    std::fs::write(vita_saves.join("from_vita.txt"), b"hello from the vita").unwrap();
    let app = repo(&data);
    app.save_devices(&[device(peer.device_id, "Desktop", &peer.addr.to_string())]);
    app.save_shares(&[share("saves", &vita_saves, true, vec![peer.device_id])]);

    let started = Instant::now();
    let report = run_cycle(&data, never_abort(), &test_options());
    let elapsed = started.elapsed();
    peer.join();

    assert_eq!(report.outcome, CycleOutcome::Completed);
    assert_eq!(report.peers.len(), 1);
    assert_eq!(report.peers[0].0, "Desktop");
    let PeerOutcome::Synced(PeerStats {
        files_downloaded,
        blocks_uploaded,
        ..
    }) = report.peers[0].1
    else {
        panic!("unexpected outcome {:?}", report.peers[0].1);
    };
    assert_eq!(files_downloaded, 1);
    assert!(
        blocks_uploaded >= 1,
        "the peer did not pull the PS Vita's file"
    );
    assert_eq!(
        std::fs::read(vita_saves.join("from_peer.txt")).unwrap(),
        b"hello from the peer"
    );
    assert_eq!(
        std::fs::read(peer_root.join("saves/from_vita.txt")).unwrap(),
        b"hello from the vita"
    );
    assert!(
        elapsed < Duration::from_secs(30),
        "the cycle took {elapsed:?}"
    );
    // The folder database lives where the app keeps it, and is saved.
    let db_files: Vec<_> = std::fs::read_dir(data.join("sync"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        db_files
            .iter()
            .any(|n| n.starts_with(".syncthing_db_saves")),
        "{db_files:?}"
    );
    // No download left behind.
    let leftovers: Vec<_> = std::fs::read_dir(&vita_saves)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    let log = read_log(&data);
    assert!(log.contains("[VITA-SYNCTHING] Background sync: cycle started (1 device(s))"));
    assert!(log.contains("Background sync: pulled saves/from_peer.txt"));
    assert!(log.contains("Background sync: cycle completed: 1/1 device(s) synchronized"));
}

#[test]
fn test_paused_share_is_left_untouched() {
    let data = data_dir_with_identity("cycle_paused", true);
    let peer_root = fresh_temp_dir("cycle_paused_peer");
    std::fs::create_dir_all(peer_root.join("active")).unwrap();
    std::fs::create_dir_all(peer_root.join("paused")).unwrap();
    std::fs::write(peer_root.join("active/a.txt"), b"active").unwrap();
    std::fs::write(peer_root.join("paused/p.txt"), b"paused").unwrap();
    let peer = ListeningPeer::spawn(0x32, &peer_root, &["active", "paused"]);

    let active = data.join("active");
    let paused = data.join("paused");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::create_dir_all(&paused).unwrap();
    std::fs::write(paused.join("local.txt"), b"never sent").unwrap();
    let app = repo(&data);
    app.save_devices(&[device(peer.device_id, "Desktop", &peer.addr.to_string())]);
    app.save_shares(&[
        share("active", &active, true, vec![peer.device_id]),
        share("paused", &paused, false, vec![peer.device_id]),
    ]);

    let report = run_cycle(&data, never_abort(), &test_options());
    peer.join();

    assert_eq!(report.outcome, CycleOutcome::Completed);
    assert!(active.join("a.txt").exists());
    assert!(!paused.join("p.txt").exists(), "a paused share was pulled");
    assert!(
        !peer_root.join("paused/local.txt").exists(),
        "a paused share was sent"
    );
}

#[test]
fn test_abort_signal_stops_the_cycle_promptly() {
    let data = data_dir_with_identity("cycle_abort", true);
    let peer_root = fresh_temp_dir("cycle_abort_peer");
    std::fs::create_dir_all(peer_root.join("saves")).unwrap();
    let peer = ListeningPeer::spawn(0x33, &peer_root, &["saves"]);
    let saves = data.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    let app = repo(&data);
    app.save_devices(&[device(peer.device_id, "Desktop", &peer.addr.to_string())]);
    app.save_shares(&[share("saves", &saves, true, vec![peer.device_id])]);

    // A drain that would last a minute: only the abort ends the cycle early.
    let mut options = test_options();
    options.drain.idle = Duration::from_secs(60);
    options.drain.max = Duration::from_secs(60);
    let fire = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&fire);
    let abort: AbortSignal = Arc::new(move || flag.load(Ordering::SeqCst));
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        fire.store(true, Ordering::SeqCst);
        Instant::now()
    });

    let report = run_cycle(&data, abort, &options);
    let ended = Instant::now();
    let fired = trigger.join().unwrap();
    peer.join();

    assert_eq!(report.outcome, CycleOutcome::Aborted);
    let latency = ended.duration_since(fired);
    assert!(
        latency < Duration::from_secs(3),
        "stopped {latency:?} after the abort"
    );
    assert!(read_log(&data).contains("Background sync: cycle aborted (a game or the app started)"));
}

#[test]
fn test_deadline_stops_the_cycle() {
    let data = data_dir_with_identity("cycle_deadline", true);
    let peer_root = fresh_temp_dir("cycle_deadline_peer");
    std::fs::create_dir_all(peer_root.join("saves")).unwrap();
    let peer = ListeningPeer::spawn(0x34, &peer_root, &["saves"]);
    let saves = data.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    let app = repo(&data);
    app.save_devices(&[device(peer.device_id, "Desktop", &peer.addr.to_string())]);
    app.save_shares(&[share("saves", &saves, true, vec![peer.device_id])]);

    let mut options = test_options();
    options.drain.idle = Duration::from_secs(60);
    options.drain.max = Duration::from_secs(60);
    options.max_duration = Duration::from_secs(4);
    let started = Instant::now();
    let report = run_cycle(&data, never_abort(), &options);
    peer.join();

    assert_eq!(report.outcome, CycleOutcome::DeadlineReached);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn test_abort_before_the_cycle_does_nothing() {
    let data = data_dir_with_identity("cycle_abort_first", true);
    let abort: AbortSignal = Arc::new(|| true);
    let report = run_cycle(&data, abort, &test_options());
    assert_eq!(report.outcome, CycleOutcome::Aborted);
    assert!(report.peers.is_empty());
}

#[test]
fn test_disabled_or_missing_plugin_cfg_does_nothing() {
    let data = data_dir_with_identity("cycle_disabled", false);
    let report = run_cycle(&data, never_abort(), &test_options());
    assert_eq!(report.outcome, CycleOutcome::Disabled);
    assert!(!data.join("syncthing.log").exists());

    std::fs::remove_file(data.join("plugin.cfg")).unwrap();
    let report = run_cycle(&data, never_abort(), &test_options());
    assert_eq!(report.outcome, CycleOutcome::Disabled);
}

#[test]
fn test_unreachable_peer_is_reported_and_the_cycle_ends() {
    let data = data_dir_with_identity("cycle_unreachable", true);
    let saves = data.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    // A port nothing listens on.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = closed.local_addr().unwrap();
    drop(closed);
    let peer_id = syncthing_core::DeviceIdentity::from_seed(&[0x35; 32])
        .unwrap()
        .device_id;
    let app = repo(&data);
    app.save_devices(&[device(peer_id, "Laptop", &addr.to_string())]);
    app.save_shares(&[share("saves", &saves, true, vec![peer_id])]);

    let report = run_cycle(&data, never_abort(), &test_options());
    assert_eq!(report.outcome, CycleOutcome::Completed);
    assert_eq!(
        report.peers,
        vec![("Laptop".to_owned(), PeerOutcome::Unreachable)]
    );
    let log = read_log(&data);
    assert!(log.contains("Background sync: Laptop: unreachable"));
    assert!(log.contains("Background sync: cycle completed: 0/1 device(s) synchronized"));
}

#[test]
fn test_nothing_to_sync_and_not_configured() {
    let data = data_dir_with_identity("cycle_nothing", true);
    let report = run_cycle(&data, never_abort(), &test_options());
    assert_eq!(report.outcome, CycleOutcome::NothingToSync);

    let no_identity = fresh_temp_dir("cycle_no_identity");
    std::fs::write(no_identity.join("plugin.cfg"), "enabled=1\n").unwrap();
    let report = run_cycle(&no_identity, never_abort(), &test_options());
    assert!(matches!(report.outcome, CycleOutcome::NotConfigured(_)));
    assert!(
        !no_identity.join("device_seed.bin").exists(),
        "the worker never creates an identity"
    );
}
