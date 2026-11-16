//! What the daemon already reported, to log a recurring situation once, with bounded memory:
//! LAN announcements, connections and relay invitations carry device IDs and addresses anyone
//! can choose, and must not make the daemon grow (or log) without limit.
//!
//! Two separate memories, so that attacker-chosen values never crowd out the reports about the
//! configuration:
//! - per subject (a configured device or server, and a kind of report): at most
//!   [`MAX_REPORTS_PER_SUBJECT`] details each, then that subject is quiet;
//! - unknown devices: at most [`MAX_UNKNOWN_DEVICE_REPORTS`], then one
//!   [`UnknownReport::LimitReached`] and silence.

use std::collections::{HashMap, HashSet};

use syncthing_core::DeviceId;

use super::timing::{MAX_REPORTS_PER_SUBJECT, MAX_UNKNOWN_DEVICE_REPORTS};

/// What to do about an unknown device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownReport {
    /// First time this device is seen on this channel: report it.
    First,
    /// Already reported, or the limit was reached and announced: stay quiet.
    Quiet,
    /// A new device while the limit is reached, for the first time: say that no more devices
    /// are reported.
    LimitReached,
}

/// The reports already made.
#[derive(Debug, Default)]
pub struct ReportMemory {
    /// Details reported, per subject. Subjects come from the configuration only (configured
    /// devices and servers), so their number is bounded by it.
    by_subject: HashMap<String, HashSet<String>>,
    /// Unknown devices reported, with the channel they were seen on (`lan`, `tcp`, ...).
    unknown: HashSet<(&'static str, DeviceId)>,
    /// The limit of unknown devices was reached and announced.
    unknown_limit_reported: bool,
}

impl ReportMemory {
    /// `true` the first time `detail` is reported for `subject`, while the subject holds fewer
    /// than [`MAX_REPORTS_PER_SUBJECT`] details. `subject` must name something configured (a
    /// configured device, a discovery server): never a value received from the network.
    pub fn first(&mut self, subject: &str, detail: &str) -> bool {
        let details = self.by_subject.entry(subject.to_owned()).or_default();
        if details.contains(detail) || details.len() >= MAX_REPORTS_PER_SUBJECT {
            return false;
        }
        details.insert(detail.to_owned());
        true
    }

    /// Whether to report the unknown device `id` seen on `channel`.
    pub fn unknown_device(&mut self, channel: &'static str, id: DeviceId) -> UnknownReport {
        if self.unknown.contains(&(channel, id)) {
            return UnknownReport::Quiet;
        }
        if self.unknown.len() < MAX_UNKNOWN_DEVICE_REPORTS {
            self.unknown.insert((channel, id));
            return UnknownReport::First;
        }
        if self.unknown_limit_reported {
            UnknownReport::Quiet
        } else {
            self.unknown_limit_reported = true;
            UnknownReport::LimitReached
        }
    }

    /// Number of entries remembered (subjects, their details and unknown devices).
    pub fn len(&self) -> usize {
        self.by_subject.len()
            + self.by_subject.values().map(HashSet::len).sum::<usize>()
            + self.unknown.len()
    }

    /// `true` when nothing was reported yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
