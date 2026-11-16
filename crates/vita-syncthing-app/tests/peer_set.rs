//! `PeerSet` (initial sync and connected peers): tracking per peer, no duplicates.

use syncthing_core::DeviceId;
use vita_syncthing_app::domain::{ConnectedPeers, InitialSyncPeers, PeerSet};

const A: DeviceId = DeviceId([0x41; 32]);
const B: DeviceId = DeviceId([0x42; 32]);

#[test]
fn test_new_set_is_empty_and_inactive() {
    let set = PeerSet::default();
    assert!(!set.is_active());
    assert_eq!(set, InitialSyncPeers::default());
    assert_eq!(set, ConnectedPeers::default());
}

#[test]
fn test_begin_activates_and_end_deactivates_only_that_peer() {
    let mut set = PeerSet::default();
    set.begin(A);
    assert!(set.is_active());
    set.begin(B);
    set.end(A);
    assert!(set.is_active(), "B is still in progress");
    set.end(B);
    assert!(!set.is_active());
}

#[test]
fn test_begin_twice_counts_the_peer_once() {
    let mut set = PeerSet::default();
    set.begin(A);
    set.begin(A);
    set.end(A);
    assert!(!set.is_active(), "a single end removes the peer");
}

#[test]
fn test_end_of_an_unknown_peer_changes_nothing() {
    let mut set = PeerSet::default();
    set.begin(A);
    let before = set.clone();
    set.end(B);
    assert_eq!(set, before);
    assert!(set.is_active());
}

#[test]
fn test_clear_forgets_every_peer() {
    let mut set = PeerSet::default();
    set.begin(A);
    set.begin(B);
    set.clear();
    assert!(!set.is_active());
    assert_eq!(set, PeerSet::default());
}
