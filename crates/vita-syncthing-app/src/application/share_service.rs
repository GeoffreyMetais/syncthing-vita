//! Share & Device management use cases (`ShareService`).
//! Extracted from the display layer so business rules and persistence can be tested independently of the UI.

use std::path::Path;

use syncthing_core::DeviceId;

use crate::application::ShareCreation;
use crate::domain::{
    ActiveTab, AppState, KnownDevice, PickerItem, PickerMode, ShareConfigEntry, NEW_SHARE_TEMPLATES,
};
use crate::infrastructure::ConfigRepository;

pub struct ShareService;

impl ShareService {
    /// Logs a message to both the persistent log file (`syncthing.log`) and the in-memory UI ring buffer.
    pub fn log_event(state: &mut AppState, repo: &ConfigRepository, msg: &str) {
        repo.write_log(msg);
        state.push_log(msg);
    }

    /// Passively records a LAN peer discovered via UDP 21027 without automatically linking or connecting to it.
    /// If the peer matches an existing user-added Short ID placeholder, upgrades it to the full 32-byte ID.
    pub fn register_discovered_peer(
        state: &mut AppState,
        repo: &ConfigRepository,
        device_id: DeviceId,
        name: &str,
        address: &str,
    ) {
        let dev_hex = device_id.to_hex();
        let mut upgraded_from_hex: Option<String> = None;

        if let Some(existing) = state
            .devices
            .iter_mut()
            .find(|d| d.device_id.matches_peer(&device_id))
        {
            if existing.device_id != device_id && existing.device_id.is_short_placeholder() {
                upgraded_from_hex = Some(existing.device_id.to_hex());
                existing.device_id = device_id;
                if existing.name.contains("(Short ID)") {
                    existing.name = format!("Device {}", device_id.short());
                }
            }
            if !name.is_empty() && name != "LAN Peer" {
                existing.name = name.to_owned();
            }
            if !address.is_empty() && address != "dynamic" {
                existing.address = address.to_owned();
            }
        } else {
            let label = if name.is_empty() || name == "LAN Peer" {
                format!("LAN Peer ({})", device_id.short())
            } else {
                name.to_owned()
            };
            state.devices.push(KnownDevice {
                device_id,
                name: label,
                address: address.to_owned(),
            });
            if !state.is_unpaired_lan_peer(&device_id) {
                state.unpaired_lan_peers.push(device_id);
            }
        }

        if let Some(old_hex) = upgraded_from_hex {
            for s in &mut state.shares {
                for h in &mut s.shared_devices {
                    if *h == old_hex {
                        *h = dev_hex.clone();
                    }
                }
                s.shared_devices.dedup();
            }
            repo.save_devices(&state.approved_devices());
            repo.save_shares(&state.shares);
        }
    }

    /// Registers or updates an approved/paired Syncthing device and links any unassigned shares.
    /// Automatically upgrades any user-added Short ID placeholder matching `device_id` to the full 32-byte ID.
    pub fn upsert_device(
        state: &mut AppState,
        repo: &ConfigRepository,
        device_id: DeviceId,
        name: &str,
        address: &str,
    ) {
        state
            .unpaired_lan_peers
            .retain(|id| !id.matches_peer(&device_id));
        let dev_hex = device_id.to_hex();
        let mut upgraded_from_hex: Option<String> = None;

        if let Some(existing) = state
            .devices
            .iter_mut()
            .find(|d| d.device_id.matches_peer(&device_id))
        {
            if existing.device_id != device_id && existing.device_id.is_short_placeholder() {
                upgraded_from_hex = Some(existing.device_id.to_hex());
                existing.device_id = device_id;
                if existing.name.contains("(Short ID)") {
                    existing.name = format!("Device {}", device_id.short());
                }
            }
            if !name.is_empty() && name != "LAN Peer" {
                existing.name = name.to_owned();
            }
            if !address.is_empty() && address != "dynamic" {
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

        // If a shortened ID placeholder was upgraded to a full 32-byte DeviceId, update all shares
        if let Some(old_hex) = upgraded_from_hex {
            for s in &mut state.shares {
                for h in &mut s.shared_devices {
                    if *h == old_hex {
                        *h = dev_hex.clone();
                    }
                }
                s.shared_devices.dedup();
            }
        }

        repo.save_devices(&state.approved_devices());
        repo.save_shares(&state.shares);
    }

    /// Parses and adds a Syncthing device from user input (7+ char Short ID, full 56-char ID, or `ID@ip[:port]`).
    /// Links the device to `share_idx` if `Some`.
    /// Returns `Ok((resolved_device_id, device_name, is_short_placeholder))`.
    pub fn add_device_by_input(
        state: &mut AppState,
        repo: &ConfigRepository,
        raw_input: &str,
        share_idx: Option<usize>,
    ) -> Result<(DeviceId, String, bool), String> {
        let trimmed = raw_input.trim();
        let (id_part, addr_part) = if let Some((left, right)) = trimmed.split_once('@') {
            let clean_addr = right.trim();
            let formatted_addr = if clean_addr.is_empty() {
                None
            } else if clean_addr.contains(':') {
                Some(clean_addr.to_owned())
            } else {
                Some(format!("{}:22000", clean_addr))
            };
            (left.trim(), formatted_addr)
        } else {
            (trimmed, None)
        };

        let parsed_id = DeviceId::parse_user_id(id_part)?;

        if let Ok(own_id) = DeviceId::parse_user_id(&state.vita_device_id) {
            if own_id.matches_peer(&parsed_id) {
                return Err("Cannot add this PS Vita's own Device ID".to_owned());
            }
        }

        state
            .unpaired_lan_peers
            .retain(|id| !id.matches_peer(&parsed_id));

        let mut upgraded_from_hex: Option<String> = None;
        let (resolved_id, dev_name) = if let Some(existing) = state
            .devices
            .iter_mut()
            .find(|d| d.device_id.matches_peer(&parsed_id))
        {
            if existing.device_id.is_short_placeholder() && !parsed_id.is_short_placeholder() {
                upgraded_from_hex = Some(existing.device_id.to_hex());
                existing.device_id = parsed_id;
                if existing.name.contains("(Short ID)") {
                    existing.name = format!("Device {}", parsed_id.short());
                }
            }
            if let Some(addr) = addr_part {
                existing.address = addr;
            }
            (existing.device_id, existing.name.clone())
        } else {
            let label = if parsed_id.is_short_placeholder() {
                format!("Peer {} (Short ID)", parsed_id.short())
            } else {
                format!("Device {}", parsed_id.short())
            };
            let address = addr_part.unwrap_or_else(|| "dynamic".to_owned());
            state.devices.push(KnownDevice {
                device_id: parsed_id,
                name: label.clone(),
                address,
            });
            (parsed_id, label)
        };

        let dev_hex = resolved_id.to_hex();
        if let Some(old_hex) = upgraded_from_hex {
            for s in &mut state.shares {
                for h in &mut s.shared_devices {
                    if *h == old_hex {
                        *h = dev_hex.clone();
                    }
                }
                s.shared_devices.dedup();
            }
        }

        let mut offered_fid: Option<String> = None;
        if let Some(idx) = share_idx {
            if let Some(share) = state.shares.get_mut(idx) {
                if !share.shared_devices.contains(&dev_hex) {
                    share.shared_devices.push(dev_hex.clone());
                }
                if share.enabled {
                    offered_fid = Some(share.folder_id.clone());
                }
            }
        }
        if let Some(fid) = offered_fid {
            state.queue_folder_offer(&fid);
        }

        state.config_dirty = true;
        state.auto_reconnect_enabled = true;
        repo.save_devices(&state.approved_devices());
        repo.save_shares(&state.shares);

        Ok((resolved_id, dev_name, resolved_id.is_short_placeholder()))
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
            state.config_dirty = true;
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

    /// Removes `shares[share_idx]` from the configuration (files on disk are kept).
    /// Returns the label of the removed share.
    pub fn delete_share(
        state: &mut AppState,
        repo: &ConfigRepository,
        share_idx: usize,
    ) -> Option<String> {
        let removed = Self::remove_share(state, repo, share_idx)?;
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
            let folder_id = share.folder_id.clone();
            let folder_label = share.label.clone();
            let is_enabled = share.enabled;
            let added = if let Some(pos) = share.shared_devices.iter().position(|x| x == &dev_hex) {
                share.shared_devices.remove(pos);
                false
            } else {
                share.shared_devices.push(dev_hex);
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
                let still_linked_anywhere = state.shares.iter().any(|s| {
                    s.shared_devices
                        .iter()
                        .filter_map(|h| DeviceId::from_hex(h))
                        .any(|id| id.matches_peer(&device_id))
                });
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

    /// Merges folders received in `ClusterConfig` from `peer_device_id` with local shared folder configuration.
    /// Newly discovered remote folders start paused (`enabled: false`) so nothing syncs without user opt-in.
    pub fn upsert_discovered_folder(
        state: &mut AppState,
        repo: &ConfigRepository,
        peer_device_id: DeviceId,
        folder_id: &str,
        label: &str,
    ) {
        let peer_hex = peer_device_id.to_hex();

        let template_path = NEW_SHARE_TEMPLATES
            .iter()
            .find(|&&(id, _, _)| id == folder_id && id != "__browse__")
            .map(|&(_, _, p)| p.to_owned());

        let fallback_sync = format!("{}/sync/{}", repo.base_data_dir(), folder_id);

        if let Some(existing) = state.shares.iter_mut().find(|s| s.folder_id == folder_id) {
            if !label.is_empty() {
                existing.label = label.to_owned();
            }
            if !existing.shared_devices.contains(&peer_hex) {
                existing.shared_devices.push(peer_hex);
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
                shared_devices: vec![peer_hex],
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
            ShareCreation::begin_from_path(state, repo, &item.path);
        } else {
            Self::update_selected_share_target(state, repo, &item.path);
        }
    }
}
