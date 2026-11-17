//! A whole cycle (TLS handshake, pull, scan, publication, drain) fits in the stack the PS Vita
//! worker gives its cycle thread (`CYCLE_THREAD_STACK`), even unoptimized: the host build has
//! guard pages, so an overflow fails this test instead of corrupting memory as it would in
//! `SceShell`.

mod common;

use std::thread;

use common::{data_dir_with_identity, device, fresh_temp_dir, repo, share, test_options};
use syncthing_vita_worker::defaults::CYCLE_THREAD_STACK;
use syncthing_vita_worker::{never_abort, run_cycle, CycleOutcome, PeerOutcome};

#[test]
fn test_cycle_fits_in_the_cycle_thread_stack() {
    let data = data_dir_with_identity("stack_budget", true);
    let peer_root = fresh_temp_dir("stack_budget_peer");
    std::fs::create_dir_all(peer_root.join("saves/sub")).unwrap();
    std::fs::write(peer_root.join("saves/sub/save.bin"), vec![7u8; 300_000]).unwrap();
    let peer = common::ListeningPeer::spawn(0x41, &peer_root, &["saves"]);
    let saves = data.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    std::fs::write(saves.join("local.bin"), vec![9u8; 200_000]).unwrap();
    let app = repo(&data);
    app.save_devices(&[device(peer.device_id, "Desktop", &peer.addr.to_string())]);
    app.save_shares(&[share("saves", &saves, true, vec![peer.device_id])]);

    let report = thread::Builder::new()
        .stack_size(CYCLE_THREAD_STACK)
        .spawn(move || run_cycle(&data, never_abort(), &test_options()))
        .unwrap()
        .join()
        .expect("the cycle thread overflowed its stack");
    peer.join();

    assert_eq!(report.outcome, CycleOutcome::Completed);
    assert!(matches!(report.peers[0].1, PeerOutcome::Synced(_)));
    assert_eq!(
        std::fs::read(saves.join("sub/save.bin")).unwrap().len(),
        300_000
    );
}
