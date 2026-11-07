//! Share & Device management use cases (`ShareService`).
//! Extracted from the display layer so business rules and persistence can be tested independently of the UI.

use std::path::Path;

use syncthing_core::DeviceId;

use crate::domain::{
    ActiveTab, AppState, KnownDevice, ModalDialog, PickerItem, PickerMode, ShareConfigEntry,
    NEW_SHARE_TEMPLATES,
};
use crate::infrastructure::ConfigRepository;

pub struct ShareService;

impl ShareService {
    /// Logs a message to both the persistent log file (`syncthing.log`) and the in-memory UI ring buffer.
    pub fn log_event(state: &mut AppState, repo: &ConfigRepository, msg: &str) {
        repo.write_log(msg);
        state.push_log(msg);
    }

    /// Registers or updates a discovered/connected Syncthing device and links any unassigned shares.
    pub fn upsert_device(
        state: &mut AppState,
        repo: &ConfigRepository,
        device_id: DeviceId,
        name: &str,
        address: &str,
    ) {
        let dev_hex = device_id.to_hex();
        if let Some(existing) = state.devices.iter_mut().find(|d| d.device_id == device_id) {
            if !name.is_empty() && name != "LAN Peer" {
                existing.name = name.to_owned();
            }
            if !address.is_empty() {
                existing.address = address.to_owned();
            }
        } else {
            let label = if name.is_empty() {
                format!("Device {}", device_id.short())
            } else {
                name.to_owned()
            };
            state.devices.push(KnownDevice {
                device_id,
                name: label,
                address: address.to_owned(),
            });
        }
        // Automatically link any share that had empty devices to this device
        for s in &mut state.shares {
            if s.shared_devices.is_empty() {
                s.shared_devices.push(dev_hex.clone());
            }
        }
        repo.save_devices(&state.devices);
        repo.save_shares(&state.shares);
    }

    /// Creates a new share on PS Vita (or updates if `folder_id` already exists) and links all known devices by default.
    pub fn create_share(
        state: &mut AppState,
        repo: &ConfigRepository,
        folder_id: &str,
        label: &str,
        target_path: &str,
    ) -> usize {
        let default_devs: Vec<String> = state
            .devices
            .iter()
            .map(|d| d.device_id.to_hex())
            .collect();

        repo.ensure_dir(target_path);

        let idx = if let Some(idx) = state.shares.iter().position(|s| s.folder_id == folder_id) {
            state.shares[idx].label = label.to_owned();
            state.shares[idx].target_path = target_path.to_owned();
            state.shares[idx].enabled = true;
            if state.shares[idx].shared_devices.is_empty() {
                state.shares[idx].shared_devices = default_devs;
            }
            idx
        } else {
            state.shares.push(ShareConfigEntry {
                folder_id: folder_id.to_owned(),
                label: label.to_owned(),
                target_path: target_path.to_owned(),
                enabled: true,
                shared_devices: default_devs,
            });
            state.shares.len() - 1
        };

        state.selected_share_idx = idx;
        repo.save_shares(&state.shares);
        idx
    }

    /// Creates a custom share from a browsed `ux0:` folder path, derives a clean `folder_id` and `label`,
    /// and transitions to the `ManageDevices` modal.
    pub fn create_custom_share_from_path(
        state: &mut AppState,
        repo: &ConfigRepository,
        raw_path: &str,
    ) {
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
        let share_idx = Self::create_share(state, repo, &folder_id, &label, &chosen_path);
        Self::log_event(
            state,
            repo,
            &format!(
                "Created custom share '{}' ({}) -> {}",
                label, folder_id, chosen_path
            ),
        );
        state.show_toast(&format!("Created share '{}'", label));
        state.picker_mode = PickerMode::ChangeShareTarget;
        state.active_tab = ActiveTab::Shares;
        state.modal = ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx: 0,
        };
    }

    /// Updates the target path of the currently selected share and returns to the `Shares` tab.
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
            repo.save_shares(&state.shares);
            repo.ensure_dir(&chosen_path);
            let msg = format!("Saved in {}", chosen_path);
            state.show_toast(&msg);
            Self::log_event(
                state,
                repo,
                &format!("Target directory updated for {} -> {}", fid, chosen_path),
            );
            state.active_tab = ActiveTab::Shares;
        }
    }

    /// Toggles `enabled` (Active / Paused) on the currently selected share.
    pub fn toggle_selected_share(state: &mut AppState, repo: &ConfigRepository) {
        if let Some(share) = state.shares.get_mut(state.selected_share_idx) {
            share.enabled = !share.enabled;
            let fid = share.folder_id.clone();
            let st = if share.enabled { "Active" } else { "Paused" };
            repo.save_shares(&state.shares);
            let msg = format!("Share {} is now {}", fid, st);
            state.show_toast(&msg);
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
        let dev_hex = device_id.to_hex();
        let dev_name = state
            .devices
            .iter()
            .find(|d| d.device_id == device_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| device_id.short());

        if let Some(share) = state.shares.get_mut(share_idx) {
            let folder_label = share.label.clone();
            let added = if let Some(pos) = share.shared_devices.iter().position(|x| x == &dev_hex) {
                share.shared_devices.remove(pos);
                false
            } else {
                share.shared_devices.push(dev_hex);
                true
            };
            repo.save_shares(&state.shares);
            Some((folder_label, dev_name, added))
        } else {
            None
        }
    }

    /// Merges folders received in `ClusterConfig` with local shared folder configuration.
    /// Newly discovered remote folders start paused (`enabled: false`) so nothing syncs without user opt-in.
    pub fn upsert_discovered_folder(
        state: &mut AppState,
        repo: &ConfigRepository,
        folder_id: &str,
        label: &str,
    ) {
        let default_devs: Vec<String> = state
            .devices
            .iter()
            .map(|d| d.device_id.to_hex())
            .collect();

        let template_path = NEW_SHARE_TEMPLATES
            .iter()
            .find(|&&(id, _, _)| id == folder_id && id != "__browse__")
            .map(|&(_, _, p)| p.to_owned());

        let fallback_sync = format!("{}/sync/{}", repo.base_data_dir(), folder_id);

        if let Some(existing) = state.shares.iter_mut().find(|s| s.folder_id == folder_id) {
            if !label.is_empty() {
                existing.label = label.to_owned();
            }
            if existing.shared_devices.is_empty() {
                existing.shared_devices = default_devs;
            }
            if existing.target_path == fallback_sync {
                if let Some(tpl) = template_path {
                    existing.target_path = tpl;
                }
            }
        } else {
            let default_target = template_path.unwrap_or(fallback_sync);
            state.shares.push(ShareConfigEntry {
                folder_id: folder_id.to_owned(),
                label: if label.is_empty() {
                    folder_id.to_owned()
                } else {
                    label.to_owned()
                },
                target_path: default_target,
                enabled: false,
                shared_devices: default_devs,
            });
        }
        repo.save_shares(&state.shares);
    }

    /// Refreshes presets and `ux0:` subdirectories for the currently selected share or new share creation.
    pub fn refresh_picker_items(state: &mut AppState, repo: &ConfigRepository) {
        let base_data_dir = repo.base_data_dir();
        let folder_id = if state.picker_mode == PickerMode::CreateNewShare {
            let tail = state
                .picker_current_dir
                .trim_end_matches('/')
                .rsplit(['/', ':'])
                .next()
                .unwrap_or("vita-folder");
            if tail.is_empty() || tail == "ux0" {
                "vita-shared".to_owned()
            } else {
                tail.to_lowercase()
            }
        } else {
            state
                .shares
                .get(state.selected_share_idx)
                .map(|s| s.folder_id.clone())
                .unwrap_or_else(|| "default".to_owned())
        };

        let current_target = state
            .shares
            .get(state.selected_share_idx)
            .map(|s| s.target_path.clone())
            .unwrap_or_else(|| "ux0:user/00/savedata".to_owned());

        let mut items = if state.picker_mode == PickerMode::CreateNewShare {
            vec![PickerItem {
                label: format!(
                    "Create share from current folder: {}",
                    state.picker_current_dir
                ),
                path: state.picker_current_dir.clone(),
                is_preset: true,
            }]
        } else {
            vec![
                PickerItem {
                    label: format!("Keep current folder ({})", current_target),
                    path: current_target,
                    is_preset: true,
                },
                PickerItem {
                    label: "Save Data (ux0:user/00/savedata)".to_owned(),
                    path: "ux0:user/00/savedata".to_owned(),
                    is_preset: true,
                },
                PickerItem {
                    label: "PS Vita Pictures (ux0:picture)".to_owned(),
                    path: "ux0:picture".to_owned(),
                    is_preset: true,
                },
                PickerItem {
                    label: "PS Vita Music (ux0:music)".to_owned(),
                    path: "ux0:music".to_owned(),
                    is_preset: true,
                },
                PickerItem {
                    label: "RetroArch Savefiles (ux0:data/retroarch/savefiles)".to_owned(),
                    path: "ux0:data/retroarch/savefiles".to_owned(),
                    is_preset: true,
                },
                PickerItem {
                    label: format!(
                        "Default Syncthing folder ({}/sync/{})",
                        base_data_dir, folder_id
                    ),
                    path: format!("{}/sync/{}", base_data_dir, folder_id),
                    is_preset: true,
                },
                PickerItem {
                    label: format!(
                        "Use current browsed folder ({})",
                        state.picker_current_dir.trim_end_matches('/')
                    ),
                    path: state.picker_current_dir.trim_end_matches('/').to_owned(),
                    is_preset: true,
                },
            ]
        };

        for sub in repo.list_subdirectories(&state.picker_current_dir) {
            let full = if state.picker_current_dir.ends_with(':')
                || state.picker_current_dir.ends_with('/')
            {
                format!("{}{}", state.picker_current_dir, sub)
            } else {
                format!("{}/{}", state.picker_current_dir, sub)
            };
            items.push(PickerItem {
                label: format!("{}/", sub),
                path: full,
                is_preset: false,
            });
        }

        state.picker_items = items;
        if state.selected_picker_idx >= state.picker_items.len() {
            state.selected_picker_idx = 0;
        }
        state.clamp_picker_scroll();
    }

    /// Navigates one directory level up in the `ux0:` picker, or returns to the `Shares` tab if at root.
    pub fn navigate_picker_up(state: &mut AppState, repo: &ConfigRepository) {
        if let Some(parent) = Path::new(&state.picker_current_dir).parent() {
            let p_str = parent.to_string_lossy().into_owned();
            if !p_str.is_empty() {
                state.picker_current_dir = p_str;
                Self::refresh_picker_items(state, repo);
                return;
            }
        }
        state.active_tab = ActiveTab::Shares;
    }

    /// Applies selection in the `ux0:` directory browser:
    /// - If `enter_subdir == true` and item is a directory, descends into it.
    /// - If `state.picker_mode == PickerMode::CreateNewShare`, creates a new share and opens device assignment.
    /// - Otherwise, assigns the chosen target path to the selected share.
    pub fn apply_picker_selection(
        state: &mut AppState,
        repo: &ConfigRepository,
        enter_subdir: bool,
    ) {
        let Some(item) = state.picker_items.get(state.selected_picker_idx).cloned() else {
            return;
        };

        if enter_subdir && !item.is_preset {
            state.picker_current_dir = item.path;
            Self::refresh_picker_items(state, repo);
            return;
        }

        if state.picker_mode == PickerMode::CreateNewShare {
            Self::create_custom_share_from_path(state, repo, &item.path);
        } else {
            Self::update_selected_share_target(state, repo, &item.path);
        }
    }
}
