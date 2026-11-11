//! Share creation workflow (`ShareCreation`): a new share is only kept once at least one remote
//! device is linked to it in the `ManageDevices` modal that follows creation.

use crate::application::ShareService;
use crate::domain::{ActiveTab, AppState, ModalDialog, PickerMode};
use crate::infrastructure::ConfigRepository;

pub struct ShareCreation;

impl ShareCreation {
    /// Creates `folder_id` and opens the `ManageDevices` modal to link devices to it.
    /// A brand-new share stays pending until `finish` sees a linked device.
    pub fn begin(
        state: &mut AppState,
        repo: &ConfigRepository,
        folder_id: &str,
        label: &str,
        target_path: &str,
    ) -> usize {
        let is_new = !state.shares.iter().any(|s| s.folder_id == folder_id);
        let share_idx = ShareService::create_share(state, repo, folder_id, label, target_path);
        if is_new {
            state.pending_share_creation = Some(folder_id.to_owned());
        }
        state.show_toast(&format!("Link a device to share '{}'", label));
        state.modal = ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx: 0,
        };
        share_idx
    }

    /// Starts creating a share from a browsed `ux0:` folder path, deriving a clean `folder_id`
    /// and `label` from the folder name, and switches to the `Shares` tab.
    pub fn begin_from_path(state: &mut AppState, repo: &ConfigRepository, raw_path: &str) -> usize {
        let chosen_path = raw_path.trim_end_matches('/').to_owned();
        let raw_name = chosen_path
            .rsplit(['/', ':'])
            .next()
            .filter(|s| !s.is_empty() && *s != "ux0")
            .unwrap_or("vita-shared");
        let folder_id: String = raw_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let label = format!("Vita {}", raw_name);
        state.picker_mode = PickerMode::ChangeShareTarget;
        state.active_tab = ActiveTab::Shares;
        Self::begin(state, repo, &folder_id, &label, &chosen_path)
    }

    /// Ends a pending creation when the `ManageDevices` modal closes: the share is kept if a
    /// device is linked, discarded otherwise. Returns `false` if the share was discarded.
    pub fn finish(state: &mut AppState, repo: &ConfigRepository) -> bool {
        let Some(folder_id) = state.pending_share_creation.take() else {
            return true;
        };
        let Some(idx) = state.shares.iter().position(|s| s.folder_id == folder_id) else {
            return true;
        };
        let share = &state.shares[idx];
        let (label, path) = (share.label.clone(), share.target_path.clone());
        if !share.shared_devices.is_empty() {
            ShareService::log_event(
                state,
                repo,
                &format!("Created share '{}' ({}) -> {}", label, folder_id, path),
            );
            state.show_toast(&format!("Created share '{}'", label));
            return true;
        }
        ShareService::remove_share(state, repo, idx);
        ShareService::log_event(
            state,
            repo,
            &format!("Share '{}' not created: no device linked", label),
        );
        state.show_toast(&format!("Share '{}' not created: no device linked", label));
        false
    }
}
