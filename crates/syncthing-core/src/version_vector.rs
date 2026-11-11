//! BEP version vector arithmetic (`Vector { repeated Counter }`), as used by Syncthing to decide
//! which side of a file change is newer, equal or concurrent.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::bep::BepVersionCounter;
use crate::device_id::DeviceId;

pub fn short_device_id(id: DeviceId) -> u64 {
    u64::from_be_bytes([
        id.0[0], id.0[1], id.0[2], id.0[3], id.0[4], id.0[5], id.0[6], id.0[7],
    ])
}

/// `a >= b`: `a` is equal to or newer than `b` for every device counter.
pub fn version_ge(a: &[BepVersionCounter], b: &[BepVersionCounter]) -> bool {
    b.iter().all(|cb| {
        cb.value == 0
            || a.iter()
                .any(|ca| ca.short_device_id == cb.short_device_id && ca.value >= cb.value)
    })
}

/// Element-wise maximum of two version vectors.
pub fn version_merge(a: &[BepVersionCounter], b: &[BepVersionCounter]) -> Vec<BepVersionCounter> {
    let mut out = a.to_vec();
    for cb in b {
        if let Some(ca) = out
            .iter_mut()
            .find(|c| c.short_device_id == cb.short_device_id)
        {
            ca.value = ca.value.max(cb.value);
        } else {
            out.push(*cb);
        }
    }
    out.sort_by_key(|c| c.short_device_id);
    out
}

/// Records a local change by `short_id` (same rule as Syncthing: `max(current + 1, now)`).
pub fn version_bump(v: &[BepVersionCounter], short_id: u64) -> Vec<BepVersionCounter> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1);
    let mut out = v.to_vec();
    if let Some(c) = out.iter_mut().find(|c| c.short_device_id == short_id) {
        c.value = (c.value + 1).max(now);
    } else {
        out.push(BepVersionCounter {
            short_device_id: short_id,
            value: now,
        });
    }
    out.sort_by_key(|c| c.short_device_id);
    out
}
