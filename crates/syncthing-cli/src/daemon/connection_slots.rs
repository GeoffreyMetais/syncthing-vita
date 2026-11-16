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
        // An explicit compare-exchange loop: `fetch_update` is deprecated on
        // recent toolchains and its `try_update` replacement is still unstable
        // on others, so neither can be used on every supported compiler.
        let mut current = self.in_use.load(Ordering::Acquire);
        loop {
            if current >= self.capacity {
                return None;
            }
            match self.in_use.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(ConnectionSlot {
                        in_use: Arc::clone(&self.in_use),
                    })
                }
                Err(actual) => current = actual,
            }
        }
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
