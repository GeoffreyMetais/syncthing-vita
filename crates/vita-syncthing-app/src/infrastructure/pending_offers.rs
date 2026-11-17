//! `pending_offers.cfg`: folder offers the plugin's background worker received while the app
//! was closed (it never accepts a folder). One line per offer,
//! `<folder_id>|<label>|<device_id_hex>\n`, text fields escaped with
//! `config_fields::escape_field`. The app imports the file once at startup and removes an
//! offer once it is answered or no longer relevant.

use std::fs;

use syncthing_core::DeviceId;

use super::config_fields::{escape_field, unescape_field};
use super::config_repo::{backup_path, read_recovering, write_replacing};
use super::ConfigRepository;
use crate::domain::defaults::MAX_PENDING_OFFERS;
use crate::domain::PendingOffer;

/// Name of the file in the app's data directory.
pub const PENDING_OFFERS_FILE: &str = "pending_offers.cfg";

impl ConfigRepository {
    fn pending_offers_path(&self) -> String {
        format!("{}/{PENDING_OFFERS_FILE}", self.base_data_dir())
    }

    /// Offers recorded by the background worker: lines with an empty folder ID or an invalid
    /// device are dropped, duplicates (same folder and device) kept once, at most
    /// `MAX_PENDING_OFFERS`.
    pub fn load_pending_offers(&self) -> Vec<PendingOffer> {
        let Ok(content) = read_recovering(&self.pending_offers_path()) else {
            return Vec::new();
        };
        let mut out: Vec<PendingOffer> = Vec::new();
        for line in content.lines() {
            let mut fields = line.split('|');
            let (Some(folder), Some(label), Some(hex), None) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let folder_id = unescape_field(folder);
            let Some(device_id) = DeviceId::from_hex(hex.trim()) else {
                continue;
            };
            if folder_id.is_empty()
                || out
                    .iter()
                    .any(|o| o.folder_id == folder_id && o.device_id == device_id)
            {
                continue;
            }
            out.push(PendingOffer {
                folder_id,
                label: unescape_field(label),
                device_id,
            });
            if out.len() == MAX_PENDING_OFFERS {
                break;
            }
        }
        out
    }

    /// Replaces the file with `offers` (removed when empty).
    pub fn save_pending_offers(&self, offers: &[PendingOffer]) {
        let path = self.pending_offers_path();
        if offers.is_empty() {
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(backup_path(&path));
            return;
        }
        let _ = fs::create_dir_all(self.base_data_dir());
        let out: String = offers
            .iter()
            .map(|o| {
                format!(
                    "{}|{}|{}\n",
                    escape_field(&o.folder_id),
                    escape_field(&o.label),
                    o.device_id.to_hex()
                )
            })
            .collect();
        write_replacing(&path, &out);
    }

    /// Removes the offer of `folder_id` by `device_id` (a Short ID placeholder matches the ID
    /// it stands for), if the file holds it.
    pub fn remove_pending_offer(&self, folder_id: &str, device_id: &DeviceId) {
        let mut offers = self.load_pending_offers();
        let before = offers.len();
        offers.retain(|o| !(o.folder_id == folder_id && o.device_id.matches_peer(device_id)));
        if offers.len() != before {
            self.save_pending_offers(&offers);
        }
    }
}
