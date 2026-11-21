//! Concurrency and rate-limiting guards for incoming and outgoing BEP/1.0 sessions.
//!
//! All types in this module are pure, lock-safe, and host-testable:
//! - [`ConnectionSlots`] bounds concurrent unauthenticated handshakes or relay joins via an RAII
//!   [`ConnectionSlot`].
//! - [`SessionClaims`] enforces at most one active BEP session per peer (matching full and
//!   short-ID placeholders symmetrically via [`DeviceId::matches_peer`]) and caps total active
//!   sessions, releasing automatically when [`SessionClaim`] drops.
//! - [`UnknownDeviceLog`] deduplicates and bounds reports/offers for unknown incoming devices.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use syncthing_core::DeviceId;

/// Bounded pool of concurrent connection slots (e.g. incoming TLS handshakes or relay joins).
#[derive(Debug)]
pub struct ConnectionSlots {
    in_use: AtomicUsize,
    limit: usize,
}

/// RAII lease on a slot from [`ConnectionSlots`]. Releasing it decrements [`ConnectionSlots::in_use`].
#[derive(Debug)]
pub struct ConnectionSlot {
    slots: Arc<ConnectionSlots>,
}

impl ConnectionSlots {
    /// Creates a slot pool allowing at most `limit` concurrent leases.
    pub fn new(limit: usize) -> Self {
        Self {
            in_use: AtomicUsize::new(0),
            limit,
        }
    }

    /// Attempts to reserve a slot without blocking. Returns `None` if `limit` slots are in use.
    pub fn try_take(self: &Arc<Self>) -> Option<ConnectionSlot> {
        let mut current = self.in_use.load(Ordering::Acquire);
        loop {
            if current >= self.limit {
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
                        slots: Arc::clone(self),
                    });
                }
                Err(actual) => current = actual,
            }
        }
    }

    /// Returns the number of slots currently held.
    pub fn in_use(&self) -> usize {
        self.in_use.load(Ordering::Acquire)
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.slots.in_use.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Shared registry of peers that currently have an active BEP session (inbound or outbound).
///
/// Prevents running duplicate sessions to the same peer even when one side knows a short-ID
/// placeholder and the other authenticated the full 32-byte TLS [`DeviceId`].
#[derive(Debug)]
pub struct SessionClaims {
    max_sessions: usize,
    active: Mutex<Vec<DeviceId>>,
}

/// RAII claim on a peer session in [`SessionClaims`]. Automatically releases the peer on drop.
#[derive(Debug)]
pub struct SessionClaim {
    claims: Arc<SessionClaims>,
    device_id: DeviceId,
}

impl SessionClaims {
    /// Creates a session claim table allowing at most `max_sessions` concurrent peer sessions.
    pub fn new(max_sessions: usize) -> Self {
        Self {
            max_sessions,
            active: Mutex::new(Vec::new()),
        }
    }

    /// Claims an active session slot for `device_id`.
    ///
    /// Returns `None` if `max_sessions` is reached or if any already-claimed peer satisfies
    /// [`DeviceId::matches_peer`] with `device_id`.
    pub fn try_claim(self: &Arc<Self>, device_id: DeviceId) -> Option<SessionClaim> {
        let mut active = self.active.lock().unwrap_or_else(|p| p.into_inner());
        if active.len() >= self.max_sessions || active.iter().any(|id| id.matches_peer(&device_id))
        {
            return None;
        }
        active.push(device_id);
        Some(SessionClaim {
            claims: Arc::clone(self),
            device_id,
        })
    }

    /// Returns `true` if `device_id` (or a matching full/short placeholder) is currently claimed.
    pub fn is_claimed(&self, device_id: &DeviceId) -> bool {
        let active = self.active.lock().unwrap_or_else(|p| p.into_inner());
        active.iter().any(|id| id.matches_peer(device_id))
    }

    /// Returns a snapshot of all currently claimed [`DeviceId`]s.
    pub fn claimed_ids(&self) -> Vec<DeviceId> {
        let active = self.active.lock().unwrap_or_else(|p| p.into_inner());
        active.clone()
    }
}

impl Drop for SessionClaim {
    fn drop(&mut self) {
        let mut active = self.claims.active.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(pos) = active.iter().position(|id| *id == self.device_id) {
            active.swap_remove(pos);
        }
    }
}

/// Outcome of recording an unknown incoming device in [`UnknownDeviceLog`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownReport {
    /// First time this unknown device was seen (within the configured limit).
    First,
    /// Already seen, or the report limit was already reached and previously reported.
    Quiet,
    /// First unknown device observed after reaching the configured limit (reported once).
    LimitReached,
}

#[derive(Debug, Default)]
struct UnknownLogState {
    seen: HashSet<DeviceId>,
    limit_reported: bool,
}

/// Bounded deduplicator for unknown incoming devices across TCP and Relay listeners.
#[derive(Debug)]
pub struct UnknownDeviceLog {
    limit: usize,
    state: Mutex<UnknownLogState>,
}

impl UnknownDeviceLog {
    /// Creates a bounded log remembering at most `limit` distinct unknown devices.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            state: Mutex::new(UnknownLogState::default()),
        }
    }

    /// Records an observation of `device_id` and indicates whether the caller should report it.
    pub fn record(&self, device_id: DeviceId) -> UnknownReport {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.seen.contains(&device_id) {
            return UnknownReport::Quiet;
        }
        if state.seen.len() < self.limit {
            state.seen.insert(device_id);
            return UnknownReport::First;
        }
        if state.limit_reported {
            UnknownReport::Quiet
        } else {
            state.limit_reported = true;
            UnknownReport::LimitReached
        }
    }
}
