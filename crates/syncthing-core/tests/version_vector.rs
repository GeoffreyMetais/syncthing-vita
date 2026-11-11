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
