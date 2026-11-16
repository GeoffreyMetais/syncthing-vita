//! Version vector ordering, merging and bumping.

use syncthing_core::version_vector::{version_bump, version_ge, version_merge};
use syncthing_core::BepVersionCounter;

fn c(id: u64, v: u64) -> BepVersionCounter {
    BepVersionCounter {
        short_device_id: id,
        value: v,
    }
}

#[test]
fn test_version_vector_ordering() {
    assert!(version_ge(&[c(1, 5)], &[c(1, 5)]));
    assert!(version_ge(&[c(1, 5), c(2, 1)], &[c(1, 5)]));
    assert!(!version_ge(&[c(1, 5)], &[c(2, 1)]));
    assert_eq!(
        version_merge(&[c(1, 5)], &[c(2, 3), c(1, 2)]),
        vec![c(1, 5), c(2, 3)]
    );
    assert!(version_bump(&[c(1, 5)], 1)[0].value > 5);
}

#[test]
fn short_device_id_is_the_first_eight_bytes_big_endian() {
    use syncthing_core::version_vector::short_device_id;
    use syncthing_core::DeviceId;
    let mut raw = [0xFFu8; 32];
    raw[..8].copy_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
    assert_eq!(short_device_id(DeviceId(raw)), 0x0102_0304_0506_0708);
    assert_eq!(short_device_id(DeviceId([0u8; 32])), 0);
    let mut high = [0u8; 32];
    high[0] = 0x80;
    assert_eq!(short_device_id(DeviceId(high)), 1 << 63);
}

#[test]
fn version_ge_ignores_zero_counters_and_requires_every_device() {
    // A zero counter in `b` is "nothing seen": any vector dominates it.
    assert!(version_ge(&[], &[c(1, 0)]));
    assert!(!version_ge(&[c(1, 4)], &[c(1, 5)]));
    assert!(!version_ge(&[c(1, 9)], &[c(1, 5), c(2, 1)]));
    assert!(version_ge(&[c(1, 9), c(2, 1)], &[c(1, 5), c(2, 1)]));
}

#[test]
fn version_merge_takes_element_wise_maximum_sorted_by_device() {
    assert_eq!(
        version_merge(&[c(2, 9)], &[c(2, 3), c(1, 1)]),
        vec![c(1, 1), c(2, 9)]
    );
    assert_eq!(version_merge(&[], &[]), Vec::new());
}

#[test]
fn version_bump_increments_or_adds_the_device_counter() {
    // An existing counter far in the future is only incremented.
    let far = 1u64 << 40;
    assert_eq!(version_bump(&[c(1, far)], 1), vec![c(1, far + 1)]);

    // A new device gets the current time (seconds), which is far above 1.
    let bumped = version_bump(&[c(9, 3)], 4);
    assert_eq!(bumped.len(), 2);
    assert_eq!(bumped[0], c(4, bumped[0].value));
    assert!(
        bumped[0].value > 1_600_000_000,
        "clock value {}",
        bumped[0].value
    );
    assert_eq!(bumped[1], c(9, 3));
}
