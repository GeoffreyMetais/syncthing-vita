//! Share management use cases (`ShareService`): creation, target folder, pause, deletion and
//! device linking, plus the protocol log entry point. Devices are managed by `DeviceRegistry`,
//! the `Files` browser by `FolderBrowser`.
//! Extracted from the display layer so business rules and persistence can be tested independently of the UI.

use syncthing_core::DeviceId;

use crate::application::FolderOfferService;
use crate::domain::{ActiveTab, AppState, ShareConfigEntry, ShareMode, UiText};
use crate::infrastructure::ConfigRepository;

pub struct ShareService;

impl ShareService {
    /// Logs a message to both the persistent log file (`syncthing.log`) and the in-memory UI ring buffer.
    pub fn log_event(state: &mut AppState, repo: &ConfigRepository, msg: &str) {
        repo.write_log(msg);
        state.push_log(msg);
    }

    /// Creates a new share on PS Vita (or updates if `folder_id` already exists).
    /// Devices are linked explicitly in the `ManageDevices` modal that opens immediately after creation.
    pub fn create_share(
        state: &mut AppState,
        repo: &ConfigRepository,
        folder_id: &str,
        label: &str,
        target_path: &str,
    ) -> usize {
        repo.ensure_dir(target_path);

        let mut has_linked_devices = false;
        let idx = if let Some(idx) = state.shares.iter().position(|s| s.folder_id == folder_id) {
            state.shares[idx].label = label.to_owned();
            state.shares[idx].target_path = target_path.to_owned();
            state.shares[idx].enabled = true;
            has_linked_devices = !state.shares[idx].shared_devices.is_empty();
            idx
        } else {
            state.shares.push(ShareConfigEntry {
                folder_id: folder_id.to_owned(),
                label: label.to_owned(),
                target_path: target_path.to_owned(),
                enabled: true,
                mode: ShareMode::ReadWrite,
                shared_devices: Vec::new(),
            });
            state.shares.len() - 1
        };

        if has_linked_devices {
            state.queue_folder_offer(folder_id);
        }

        state.selected_share_idx = idx;
        state.config_dirty = true;
        state.auto_reconnect_enabled = true;
        repo.save_shares(&state.shares);
        idx
    }

    /// Sets the target path of the selected share, a folder offer just accepted (its folder is
    /// chosen once, in the `Files` browser), and returns to the `Overview` tab.
    pub fn update_selected_share_target(
        state: &mut AppState,
        repo: &ConfigRepository,
        raw_path: &str,
    ) {
        let chosen_path = raw_path.trim_end_matches('/').to_owned();
        let mut updated_fid = None;
        if let Some(share) = state.shares.get_mut(state.selected_share_idx) {
            share.target_path = chosen_path.clone();
            updated_fid = Some(share.folder_id.clone());
        }
        if let Some(fid) = updated_fid {
            state.config_dirty = true;
            repo.save_shares(&state.shares);
            repo.ensure_dir(&chosen_path);
            state.show_toast(UiText::SavedIn {
                path: chosen_path.clone(),
            });
            Self::log_event(
                state,
                repo,
                &format!("Target directory updated for {} -> {}", fid, chosen_path),
            );
            state.active_tab = ActiveTab::Dashboard;
        }
    }

    /// Removes `shares[share_idx]` from the configuration (files on disk are kept) and registers
    /// the folder as declined for its linked devices, so their next `ClusterConfig` does not offer
    /// it again (undone with `Settings` · `Clear Declined`). Returns the label of the removed share.
    pub fn delete_share(
        state: &mut AppState,
        repo: &ConfigRepository,
        share_idx: usize,
    ) -> Option<String> {
        let removed = Self::remove_share(state, repo, share_idx)?;
        FolderOfferService::ignore(state, repo, &removed.folder_id, &removed.shared_devices);
        Self::log_event(
            state,
            repo,
            &format!(
                "Deleted share '{}' ({}) -> {}",
                removed.label, removed.folder_id, removed.target_path
            ),
        );
        Some(removed.label)
    }

    /// Removes `shares[share_idx]` from state and `shares.cfg` without logging.
    pub(crate) fn remove_share(
        state: &mut AppState,
        repo: &ConfigRepository,
        share_idx: usize,
    ) -> Option<ShareConfigEntry> {
        if share_idx >= state.shares.len() {
            return None;
        }
        let removed = state.shares.remove(share_idx);
        state
            .pending_folder_offers
            .retain(|fid| fid != &removed.folder_id);
        if state.selected_share_idx >= state.shares.len() {
            state.selected_share_idx = state.shares.len().saturating_sub(1);
        }
        state.config_dirty = true;
        repo.save_shares(&state.shares);
        Some(removed)
    }

    /// Toggles `enabled` (Active / Paused) on the currently selected share.
    pub fn toggle_selected_share(state: &mut AppState, repo: &ConfigRepository) {
        let mut toggle_info = None;
        if let Some(share) = state.shares.get_mut(state.selected_share_idx) {
            share.enabled = !share.enabled;
            let fid = share.folder_id.clone();
            let is_enabled = share.enabled;
            let has_devs = !share.shared_devices.is_empty();
            toggle_info = Some((fid, is_enabled, has_devs));
        }
        if let Some((fid, is_enabled, has_devs)) = toggle_info {
            if is_enabled && has_devs {
                state.queue_folder_offer(&fid);
            }
            let st = if is_enabled { "Active" } else { "Paused" };
            state.config_dirty = true;
            state.auto_reconnect_enabled = true;
            repo.save_shares(&state.shares);
            let msg = format!("Share {} is now {}", fid, st);
            state.show_toast(if is_enabled {
                UiText::ShareActive { id: fid.clone() }
            } else {
                UiText::SharePaused { id: fid.clone() }
            });
            Self::log_event(state, repo, &msg);
        }
    }

    /// Toggles whether `device_id` is linked to `shares[share_idx]`.
    pub fn toggle_device_for_share(
        state: &mut AppState,
        repo: &ConfigRepository,
        share_idx: usize,
        device_id: DeviceId,
    ) -> Option<(String, String, bool)> {
        let dev_name = state
            .devices
            .iter()
            .find(|d| d.device_id == device_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| device_id.short());

        if let Some(share) = state.shares.get_mut(share_idx) {
            let folder_id = share.folder_id.clone();
            let folder_label = share.label.clone();
            let is_enabled = share.enabled;
            let added = if let Some(pos) = share.shared_devices.iter().position(|x| *x == device_id)
            {
                share.shared_devices.remove(pos);
                false
            } else {
                share.shared_devices.push(device_id);
                true
            };
            if added {
                state
                    .unpaired_lan_peers
                    .retain(|id| !id.matches_peer(&device_id));
                if is_enabled {
                    state.queue_folder_offer(&folder_id);
                }
            } else {
                let still_linked_anywhere = state.shares.iter().any(|s| s.is_linked_to(&device_id));
                if !still_linked_anywhere && !state.is_unpaired_lan_peer(&device_id) {
                    state.unpaired_lan_peers.push(device_id);
                }
            }
            state.config_dirty = true;
            state.auto_reconnect_enabled = true;
            repo.save_devices(&state.approved_devices());
            repo.save_shares(&state.shares);
            Some((folder_label, dev_name, added))
        } else {
            None
        }
    }
}
