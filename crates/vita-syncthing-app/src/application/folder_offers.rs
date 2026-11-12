//! Folder offers received from remote devices (`FolderOfferService`): a folder shared by a peer in
//! its `ClusterConfig` only becomes a share once the user accepts it. Declined offers are
//! remembered in `ignored_folders.cfg` so they are never prompted again.

use syncthing_core::DeviceId;

use crate::application::ShareService;
use crate::domain::{
    ActiveTab, AppState, FolderOffer, IgnoredFolder, ModalDialog, PickerMode, ShareConfigEntry,
    UiText, NEW_SHARE_TEMPLATES,
};
use crate::infrastructure::ConfigRepository;

pub struct FolderOfferService;

impl FolderOfferService {
    /// Handles a folder listed in `peer_device_id`'s `ClusterConfig`.
    ///
    /// Shares already linked to the peer only get their label refreshed. Anything else (a new
    /// folder, a deleted share, or a share not linked to this peer) is queued as an offer the user
    /// must accept, unless the user declined it before.
    pub fn receive(
        state: &mut AppState,
        repo: &ConfigRepository,
        peer_device_id: DeviceId,
        folder_id: &str,
        label: &str,
    ) {
        let peer_hex = peer_device_id.to_hex();
        if let Some(share) = state
            .shares
            .iter_mut()
            .find(|s| s.folder_id == folder_id && s.shared_devices.contains(&peer_hex))
        {
            if !label.is_empty() && share.label != label {
                share.label = label.to_owned();
                repo.save_shares(&state.shares);
            }
            return;
        }
        if state.is_folder_ignored(folder_id, &peer_device_id) {
            return;
        }
        if let Some(offer) = state
            .folder_offers
            .iter_mut()
            .find(|o| o.folder_id == folder_id && o.device_id.matches_peer(&peer_device_id))
        {
            if !label.is_empty() {
                offer.label = label.to_owned();
            }
            return;
        }
        let label = if label.is_empty() { folder_id } else { label };
        state.folder_offers.push(FolderOffer {
            folder_id: folder_id.to_owned(),
            label: label.to_owned(),
            device_id: peer_device_id,
            snoozed: false,
        });
        let msg = format!(
            "{} wants to share '{}' ({}): waiting for approval",
            Self::device_name(state, &peer_device_id),
            label,
            folder_id
        );
        ShareService::log_event(state, repo, &msg);
    }

    /// Opens the `FolderOffer` modal for the next pending offer when no other modal is open.
    pub fn prompt_next(state: &mut AppState) {
        if state.modal != ModalDialog::None {
            return;
        }
        if let Some(offer_idx) = state.next_folder_offer_to_prompt() {
            state.modal = ModalDialog::FolderOffer { offer_idx };
        }
    }

    /// Accepts `folder_offers[offer_idx]`: links the peer to the share (created paused if new) and
    /// opens the `Files` browser so the user picks where the folder is stored on the PS Vita.
    pub fn accept(state: &mut AppState, repo: &ConfigRepository, offer_idx: usize) {
        state.modal = ModalDialog::None;
        if offer_idx >= state.folder_offers.len() {
            return;
        }
        let offer = state.folder_offers.remove(offer_idx);
        let peer_hex = offer.device_id.to_hex();
        let share_idx = match state
            .shares
            .iter()
            .position(|s| s.folder_id == offer.folder_id)
        {
            Some(idx) => {
                let share = &mut state.shares[idx];
                if !share.shared_devices.contains(&peer_hex) {
                    share.shared_devices.push(peer_hex);
                }
                idx
            }
            None => {
                state.shares.push(ShareConfigEntry {
                    folder_id: offer.folder_id.clone(),
                    label: offer.label.clone(),
                    target_path: Self::default_target(repo, &offer.folder_id),
                    enabled: false,
                    shared_devices: vec![peer_hex],
                });
                state.shares.len() - 1
            }
        };
        state
            .unpaired_lan_peers
            .retain(|id| !id.matches_peer(&offer.device_id));
        state.config_dirty = true;
        state.auto_reconnect_enabled = true;
        repo.save_devices(&state.approved_devices());
        repo.save_shares(&state.shares);
        let msg = format!("Accepted share '{}' ({})", offer.label, offer.folder_id);
        ShareService::log_event(state, repo, &msg);

        state.selected_share_idx = share_idx;
        if state.shares[share_idx].enabled {
            state.show_toast(UiText::OfferAccepted {
                label: offer.label.clone(),
                id: offer.folder_id.clone(),
            });
            state.active_tab = ActiveTab::Shares;
        } else {
            state.picker_mode = PickerMode::ChangeShareTarget;
            ShareService::refresh_picker_items(state, repo);
            state.active_tab = ActiveTab::FolderPicker;
            state.reset_nav_for_tab();
            state.show_toast(UiText::ChooseWhereToStore {
                label: offer.label.clone(),
            });
        }
    }

    /// Declines `folder_offers[offer_idx]` for good (persisted in `ignored_folders.cfg`).
    pub fn decline(state: &mut AppState, repo: &ConfigRepository, offer_idx: usize) {
        state.modal = ModalDialog::None;
        if offer_idx >= state.folder_offers.len() {
            return;
        }
        let offer = state.folder_offers.remove(offer_idx);
        Self::ignore(state, repo, &offer.folder_id, &[offer.device_id]);
        let msg = format!("Declined share '{}' ({})", offer.label, offer.folder_id);
        state.show_toast(UiText::OfferDeclined {
            label: offer.label.clone(),
            id: offer.folder_id.clone(),
        });
        ShareService::log_event(state, repo, &msg);
    }

    /// Registers `folder_id` as declined for each of `devices` (persisted in `ignored_folders.cfg`)
    /// and drops their pending offers of it, so these peers never bring the folder back unasked.
    pub fn ignore(
        state: &mut AppState,
        repo: &ConfigRepository,
        folder_id: &str,
        devices: &[DeviceId],
    ) {
        for device_id in devices {
            state
                .folder_offers
                .retain(|o| !(o.folder_id == folder_id && o.device_id.matches_peer(device_id)));
            if !state.is_folder_ignored(folder_id, device_id) {
                state.ignored_folders.push(IgnoredFolder {
                    folder_id: folder_id.to_owned(),
                    device_id: *device_id,
                });
            }
        }
        if let ModalDialog::FolderOffer { offer_idx } = state.modal {
            if offer_idx >= state.folder_offers.len() {
                state.modal = ModalDialog::None;
            }
        }
        repo.save_ignored_folders(&state.ignored_folders);
    }

    /// Postpones `folder_offers[offer_idx]` until the app restarts.
    pub fn later(state: &mut AppState, offer_idx: usize) {
        state.modal = ModalDialog::None;
        if let Some(offer) = state.folder_offers.get_mut(offer_idx) {
            offer.snoozed = true;
        }
    }

    /// Forgets every declined folder offer (`ignored_folders.cfg`) so peers can offer them again.
    /// The session is restarted so peers resend their `ClusterConfig` right away.
    /// Returns the number of cleared entries.
    pub fn clear_declined(state: &mut AppState, repo: &ConfigRepository) -> usize {
        let count = state.ignored_folders.len();
        if count == 0 {
            state.show_toast(UiText::NoDeclinedFolders);
            return 0;
        }
        state.ignored_folders.clear();
        repo.save_ignored_folders(&state.ignored_folders);
        state.config_dirty = true;
        state.auto_reconnect_enabled = true;
        let msg = format!(
            "Cleared {} declined folder(s): they will be offered again",
            count
        );
        state.show_toast(UiText::DeclinedCleared { count });
        ShareService::log_event(state, repo, &msg);
        count
    }

    /// Display name of `device_id` (known name or short ID).
    pub fn device_name(state: &AppState, device_id: &DeviceId) -> String {
        state
            .devices
            .iter()
            .find(|d| d.device_id.matches_peer(device_id))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| format!("Device {}", device_id.short()))
    }

    /// Preset path for well-known folder IDs, otherwise `<base_data_dir>/sync/<folder_id>`.
    fn default_target(repo: &ConfigRepository, folder_id: &str) -> String {
        NEW_SHARE_TEMPLATES
            .iter()
            .find(|&&(id, _, _)| id == folder_id && id != "__browse__")
            .map(|&(_, _, p)| p.to_owned())
            .unwrap_or_else(|| format!("{}/sync/{}", repo.base_data_dir(), folder_id))
    }
}
