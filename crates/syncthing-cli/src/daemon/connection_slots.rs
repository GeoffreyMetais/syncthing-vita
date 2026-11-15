//! A bounded number of connections handled at the same time before they become a configured
//! device's session: incoming TCP connections in their TLS handshake (`listener`) and relay
//! invitations being joined (`relay`). Each pool caps the threads that unauthenticated input
//! can make the daemon spend.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A bounded number of slots, each held by one connection until it is dropped.
#[derive(Debug, Clone)]
pub struct ConnectionSlots {
    in_use: Arc<AtomicUsize>,
    capacity: usize,
}

/// One slot of [`ConnectionSlots`], given back when dropped (on every exit path).
#[derive(Debug)]
pub struct ConnectionSlot {
    in_use: Arc<AtomicUsize>,
}

impl ConnectionSlots {
    pub fn new(capacity: usize) -> Self {
        Self {
            in_use: Arc::new(AtomicUsize::new(0)),
            capacity,
        }
    }

    /// Takes a slot; `None` when all of them are in use.
    pub fn try_take(&self) -> Option<ConnectionSlot> {
        self.in_use
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.capacity).then_some(n + 1)
            })
            .ok()
            .map(|_| ConnectionSlot {
                in_use: Arc::clone(&self.in_use),
            })
    }

    /// Slots currently taken.
    pub fn in_use(&self) -> usize {
        self.in_use.load(Ordering::Acquire)
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.in_use.fetch_sub(1, Ordering::AcqRel);
    }
}
