//! `pending_offers.cfg`: folders a linked peer offered during a background cycle, kept for the
//! app, which asks the user for consent at its next start (and removes the answered lines).
//!
//! Format (shared with the app): one line per offer, `<folder_id>|<label>|<device_id_hex>\n`,
//! `folder_id` and `label` escaped like every `.cfg` text field (`config::fields`). The file is
//! replaced crash-safely (`storage::atomic`, previous version kept as `.bak` until the new one
//! is in place). It is the only `.cfg` file the worker writes; the app and the worker never run
//! at the same time.
//!
//! Peer-supplied text is bounded: a folder ID must be 1..=`MAX_OFFER_FOLDER_ID_BYTES` bytes
//! without control characters (else the offer is ignored); a label loses its control characters
//! and is cut at `MAX_OFFER_LABEL_CHARS` characters (an empty label becomes the folder ID, as in
//! the app). At most `MAX_PENDING_OFFERS` offers are kept; further ones are dropped.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use syncthing_core::storage::atomic::{recover_replaced_file, replace_file, write_file_synced};
use syncthing_core::DeviceId;

use crate::config::fields::{escape_field, unescape_field};
use crate::config::WorkerConfig;
use crate::defaults::{MAX_OFFER_FOLDER_ID_BYTES, MAX_OFFER_LABEL_CHARS, MAX_PENDING_OFFERS};

/// Name of the file in the app's data directory.
pub const PENDING_OFFERS_FILE: &str = "pending_offers.cfg";

/// One line of `pending_offers.cfg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOffer {
    pub folder_id: String,
    pub label: String,
    pub device_id: DeviceId,
}

/// An offer recorded during this cycle, with the name the user gave the device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOffer {
    pub device_name: String,
    pub folder_id: String,
    pub label: String,
}

/// What became of an offered folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferVerdict {
    /// Recorded now.
    Recorded,
    /// Already pending.
    AlreadyPending,
    /// The folder is already a share linked to that device.
    AlreadyShared,
    /// The user declined it for that device (`ignored_folders.cfg`).
    Declined,
    /// Unusable folder ID (empty, too long, control characters).
    Invalid,
    /// `MAX_PENDING_OFFERS` offers are already pending.
    Full,
}

/// Parses `pending_offers.cfg`. Lines without 3 fields, an invalid folder ID or an invalid
/// device ID are skipped.
pub fn parse_pending(text: &str) -> Vec<PendingOffer> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() != 3 {
                return None;
            }
            let folder_id = unescape_field(parts[0]);
            if !is_valid_folder_id(&folder_id) {
                return None;
            }
            Some(PendingOffer {
                folder_id,
                label: unescape_field(parts[1]),
                device_id: DeviceId::from_hex(parts[2].trim())?,
            })
        })
        .collect()
}

/// Serializes offers in the file format.
pub fn serialize_pending(offers: &[PendingOffer]) -> String {
    offers
        .iter()
        .map(|o| {
            format!(
                "{}|{}|{}\n",
                escape_field(&o.folder_id),
                escape_field(&o.label),
                o.device_id.to_hex()
            )
        })
        .collect()
}

/// A folder ID worth keeping: 1..=`MAX_OFFER_FOLDER_ID_BYTES` bytes, no control character.
pub fn is_valid_folder_id(folder_id: &str) -> bool {
    !folder_id.is_empty()
        && folder_id.len() <= MAX_OFFER_FOLDER_ID_BYTES
        && !folder_id.chars().any(char::is_control)
}

/// `label` without control characters, at most `MAX_OFFER_LABEL_CHARS` characters; the folder
/// ID when it is empty.
pub fn clean_label(label: &str, folder_id: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_OFFER_LABEL_CHARS)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        folder_id.chars().take(MAX_OFFER_LABEL_CHARS).collect()
    } else {
        cleaned.to_owned()
    }
}

/// The pending offers of a cycle: loaded at its start, saved at its end if it changed.
#[derive(Debug)]
pub struct OfferBook {
    path: PathBuf,
    offers: Vec<PendingOffer>,
    new: Vec<NewOffer>,
    dropped: usize,
    full_at_load: bool,
}

impl OfferBook {
    /// Loads `<data_dir>/pending_offers.cfg` (completing an interrupted replacement first).
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(PENDING_OFFERS_FILE);
        recover_replaced_file(&path, &backup_path(&path));
        let offers = fs::read_to_string(&path)
            .map(|text| parse_pending(&text))
            .unwrap_or_default();
        let full_at_load = offers.len() >= MAX_PENDING_OFFERS;
        Self {
            path,
            offers,
            new: Vec::new(),
            dropped: 0,
            full_at_load,
        }
    }

    /// Considers `folder_id` (labelled `label`) offered by `device` (named `device_name` in
    /// `devices.cfg`), records it if it needs the user's consent.
    pub fn consider(
        &mut self,
        config: &WorkerConfig,
        device: DeviceId,
        device_name: &str,
        folder_id: &str,
        label: &str,
    ) -> OfferVerdict {
        if !is_valid_folder_id(folder_id) {
            return OfferVerdict::Invalid;
        }
        if config
            .shares
            .iter()
            .any(|s| s.folder_id == folder_id && s.is_linked_to(&device))
        {
            return OfferVerdict::AlreadyShared;
        }
        if config
            .ignored
            .iter()
            .any(|i| i.folder_id == folder_id && i.device_id.matches_peer(&device))
        {
            return OfferVerdict::Declined;
        }
        if self
            .offers
            .iter()
            .any(|o| o.folder_id == folder_id && o.device_id.matches_peer(&device))
        {
            return OfferVerdict::AlreadyPending;
        }
        if self.offers.len() >= MAX_PENDING_OFFERS {
            self.dropped += 1;
            return OfferVerdict::Full;
        }
        let label = clean_label(label, folder_id);
        self.offers.push(PendingOffer {
            folder_id: folder_id.to_owned(),
            label: label.clone(),
            device_id: device,
        });
        self.new.push(NewOffer {
            device_name: device_name.to_owned(),
            folder_id: folder_id.to_owned(),
            label,
        });
        OfferVerdict::Recorded
    }

    /// Offers recorded during this cycle.
    pub fn new_offers(&self) -> &[NewOffer] {
        &self.new
    }

    /// Every pending offer.
    pub fn offers(&self) -> &[PendingOffer] {
        &self.offers
    }

    /// `true` if offers were dropped because the file became full during this cycle (a file
    /// already full when the cycle started was reported by an earlier one).
    pub fn newly_full(&self) -> bool {
        self.dropped > 0 && !self.full_at_load
    }

    /// Writes the file if this cycle recorded an offer.
    pub fn save(&self) -> io::Result<()> {
        if self.new.is_empty() {
            return Ok(());
        }
        let mut tmp = self.path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        write_file_synced(&tmp, serialize_pending(&self.offers).as_bytes())?;
        replace_file(&tmp, &self.path, &backup_path(&self.path))
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    PathBuf::from(backup)
}
