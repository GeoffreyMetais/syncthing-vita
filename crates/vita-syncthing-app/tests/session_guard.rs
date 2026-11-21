use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use syncthing_core::{DeviceId, DeviceIdentity};
use vita_syncthing_app::infrastructure::session_guard::{
    ConnectionSlots, SessionClaims, UnknownDeviceLog, UnknownReport,
};

fn id_from_seed(byte: u8) -> DeviceId {
    DeviceIdentity::from_seed(&[byte; 32]).unwrap().device_id
}

#[test]
fn connection_slots_enforce_limit_and_release_on_drop() {
    let slots = Arc::new(ConnectionSlots::new(2));
    assert_eq!(slots.in_use(), 0);

    let s1 = slots.try_take().expect("first slot");
    assert_eq!(slots.in_use(), 1);

    let s2 = slots.try_take().expect("second slot");
    assert_eq!(slots.in_use(), 2);

    assert!(slots.try_take().is_none(), "third slot must be rejected");
    assert_eq!(slots.in_use(), 2);

    drop(s1);
    assert_eq!(slots.in_use(), 1);

    let s3 = slots.try_take().expect("slot available after drop");
    assert_eq!(slots.in_use(), 2);

    drop(s2);
    drop(s3);
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn connection_slots_zero_limit_always_rejects() {
    let slots = Arc::new(ConnectionSlots::new(0));
    assert!(slots.try_take().is_none());
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn connection_slots_concurrent_cas_never_exceeds_limit() {
    const LIMIT: usize = 2;
    const THREADS: usize = 12;
    const ITERATIONS: usize = 200;

    let slots = Arc::new(ConnectionSlots::new(LIMIT));
    let barrier = Arc::new(Barrier::new(THREADS));
    let max_seen = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let slots = Arc::clone(&slots);
            let barrier = Arc::clone(&barrier);
            let max_seen = Arc::clone(&max_seen);
            thread::spawn(move || {
                barrier.wait();
                for _ in 0..ITERATIONS {
                    if let Some(_slot) = slots.try_take() {
                        let current = slots.in_use();
                        max_seen.fetch_max(current, Ordering::AcqRel);
                        thread::yield_now();
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert!(max_seen.load(Ordering::Acquire) <= LIMIT);
    assert_eq!(slots.in_use(), 0);
}

#[test]
fn session_claims_deduplicate_full_and_short_ids_symmetrically() {
    let claims = Arc::new(SessionClaims::new(4));
    let full_a = id_from_seed(1);
    let short_a = DeviceId::parse_user_id(&full_a.short()).unwrap();
    let full_b = id_from_seed(2);

    assert!(full_a.matches_peer(&short_a));
    assert!(short_a.matches_peer(&full_a));

    let claim_full = claims.try_claim(full_a).expect("claim full_a");
    assert!(claims.is_claimed(&full_a));
    assert!(claims.is_claimed(&short_a));
    assert!(!claims.is_claimed(&full_b));
    assert!(claims.try_claim(full_a).is_none());
    assert!(claims.try_claim(short_a).is_none());
    drop(claim_full);

    assert!(!claims.is_claimed(&full_a));
    assert!(!claims.is_claimed(&short_a));

    let claim_short = claims.try_claim(short_a).expect("claim short_a");
    assert!(claims.is_claimed(&full_a));
    assert!(claims.is_claimed(&short_a));
    assert!(claims.try_claim(full_a).is_none());
    drop(claim_short);

    assert!(claims.claimed_ids().is_empty());
}

#[test]
fn session_claims_enforce_max_sessions_and_release_on_drop() {
    let claims = Arc::new(SessionClaims::new(2));
    let id1 = id_from_seed(10);
    let id2 = id_from_seed(11);
    let id3 = id_from_seed(12);

    let c1 = claims.try_claim(id1).expect("claim id1");
    let c2 = claims.try_claim(id2).expect("claim id2");
    assert_eq!(claims.claimed_ids().len(), 2);
    assert!(claims.try_claim(id3).is_none(), "max_sessions=2 reached");

    drop(c1);
    assert!(!claims.is_claimed(&id1));

    let c3 = claims.try_claim(id3).expect("claim id3 after c1 dropped");
    assert!(claims.try_claim(id1).is_none(), "cap still 2");

    drop(c2);
    let c1_again = claims.try_claim(id1).expect("re-claim id1");
    assert_eq!(claims.claimed_ids().len(), 2);

    drop(c3);
    drop(c1_again);
    assert!(claims.claimed_ids().is_empty());
}

#[test]
fn session_claims_zero_cap_always_rejects() {
    let claims = Arc::new(SessionClaims::new(0));
    assert!(claims.try_claim(id_from_seed(20)).is_none());
    assert!(claims.claimed_ids().is_empty());
}

#[test]
fn unknown_device_log_deduplicates_and_reports_limit_once() {
    let log = UnknownDeviceLog::new(2);
    let id1 = id_from_seed(30);
    let id2 = id_from_seed(31);
    let id3 = id_from_seed(32);
    let id4 = id_from_seed(33);

    assert_eq!(log.record(id1), UnknownReport::First);
    assert_eq!(log.record(id1), UnknownReport::Quiet);

    assert_eq!(log.record(id2), UnknownReport::First);
    assert_eq!(log.record(id2), UnknownReport::Quiet);

    assert_eq!(log.record(id3), UnknownReport::LimitReached);
    assert_eq!(log.record(id3), UnknownReport::Quiet);
    assert_eq!(log.record(id4), UnknownReport::Quiet);
    assert_eq!(log.record(id1), UnknownReport::Quiet);
}

#[test]
fn unknown_device_log_zero_limit_reports_limit_once() {
    let log = UnknownDeviceLog::new(0);
    let id1 = id_from_seed(40);
    let id2 = id_from_seed(41);

    assert_eq!(log.record(id1), UnknownReport::LimitReached);
    assert_eq!(log.record(id1), UnknownReport::Quiet);
    assert_eq!(log.record(id2), UnknownReport::Quiet);
}

#[test]
fn session_claims_distinguish_dialing_from_established_for_global_lookup() {
    use vita_syncthing_app::domain::KnownDevice;
    use vita_syncthing_app::infrastructure::peer_endpoints::needs_global_lookup;

    let claims = Arc::new(SessionClaims::new(4));
    let peer_id = id_from_seed(50);
    let kd = KnownDevice {
        device_id: peer_id,
        name: "Home Server".to_owned(),
        address: "192.168.1.20:22000".to_owned(),
    };

    // While a peer session thread is dialing an unreachable LAN address, the claim is held for
    // deduplication, but the session is not yet established: Global Discovery must still run.
    let claim = claims.try_claim(peer_id).expect("claim peer_id");
    assert!(claims.is_claimed(&peer_id));
    assert_eq!(claims.claimed_ids(), vec![peer_id]);
    assert!(claims.established_ids().is_empty());
    assert!(
        needs_global_lookup(&kd, &claims.established_ids()),
        "dialing an unreachable LAN IP must not suppress Global Discovery lookup"
    );

    // Once the TLS + BEP handshake completes (PeerConnected), the session is established and
    // Global Discovery lookup is no longer needed.
    claims.mark_established(&peer_id);
    assert_eq!(claims.established_ids(), vec![peer_id]);
    assert!(!needs_global_lookup(&kd, &claims.established_ids()));

    // Dropping the claim clears both claimed and established state.
    drop(claim);
    assert!(!claims.is_claimed(&peer_id));
    assert!(claims.established_ids().is_empty());
    assert!(needs_global_lookup(&kd, &claims.established_ids()));
}

#[test]
fn session_claims_mark_established_matches_short_placeholder_and_full_id() {
    let claims = Arc::new(SessionClaims::new(4));
    let full_id = id_from_seed(51);
    let short_id = DeviceId::parse_user_id(&full_id.short()).unwrap();

    let _claim = claims.try_claim(short_id).expect("claim short placeholder");
    assert!(claims.established_ids().is_empty());

    claims.mark_established(&full_id);
    assert_eq!(claims.established_ids(), vec![short_id]);
}
