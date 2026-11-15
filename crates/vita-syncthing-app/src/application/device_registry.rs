//! Known remote devices (`DeviceRegistry`): peers seen on the LAN, devices added by ID and the
//! upgrade of a Short ID placeholder to the full ID a TLS session proved.

use syncthing_core::DeviceId;

use crate::domain::defaults::MAX_DISCOVERED_PEERS;
use crate::domain::{AppState, KnownDevice, UiText};
use crate::infrastructure::ConfigRepository;

pub struct DeviceRegistry;

impl DeviceRegistry {
    /// Passively records a LAN peer discovered via UDP 21027 without automatically linking or
    /// connecting to it. An announcement is not authenticated (anyone on the LAN can send any
    /// ID), so it never upgrades a Short ID placeholder: that only happens once a TLS session
    /// proved the peer owns the ID ([`Self::upsert_device`]). At most `MAX_DISCOVERED_PEERS`
    /// unpaired peers are listed.
    pub fn register_discovered_peer(
        state: &mut AppState,
        device_id: DeviceId,
        name: &str,
        address: &str,
    ) {
        if let Some(existing) = state
            .devices
            .iter_mut()
            .find(|d| d.device_id.matches_peer(&device_id))
        {
            if !name.is_empty() && name != "LAN Peer" {
                existing.name = name.to_owned();
            }
            if !address.is_empty() && address != "dynamic" {
                existing.address = address.to_owned();
            }
        } else if state.unpaired_lan_peers.len() < MAX_DISCOVERED_PEERS {
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
            state.unpaired_lan_peers.push(device_id);
        }
    }

    /// Replaces a Short ID placeholder by the full ID it stood for in every share.
    fn relink_upgraded_device(state: &mut AppState, placeholder: DeviceId, new_id: DeviceId) {
        for s in &mut state.shares {
            for id in &mut s.shared_devices {
                if *id == placeholder {
                    *id = new_id;
                }
            }
            s.shared_devices.dedup();
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
        let mut upgraded_from: Option<DeviceId> = None;

        if let Some(existing) = state
            .devices
            .iter_mut()
            .find(|d| d.device_id.matches_peer(&device_id))
        {
            if existing.device_id != device_id && existing.device_id.is_short_placeholder() {
                upgraded_from = Some(existing.device_id);
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

        if let Some(placeholder) = upgraded_from {
            Self::relink_upgraded_device(state, placeholder, device_id);
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
    ) -> Result<(DeviceId, String, bool), UiText> {
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

        let parsed_id = DeviceId::parse_user_id(id_part).map_err(UiText::InvalidDeviceId)?;

        if let Ok(own_id) = DeviceId::parse_user_id(&state.vita_device_id) {
            if own_id.matches_peer(&parsed_id) {
                return Err(UiText::OwnDeviceId);
            }
        }

        state
            .unpaired_lan_peers
            .retain(|id| !id.matches_peer(&parsed_id));

        let mut upgraded_from: Option<DeviceId> = None;
        let (resolved_id, dev_name) = if let Some(existing) = state
            .devices
            .iter_mut()
            .find(|d| d.device_id.matches_peer(&parsed_id))
        {
            if existing.device_id.is_short_placeholder() && !parsed_id.is_short_placeholder() {
                upgraded_from = Some(existing.device_id);
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

        if let Some(placeholder) = upgraded_from {
            Self::relink_upgraded_device(state, placeholder, resolved_id);
        }

        let mut offered_fid: Option<String> = None;
        if let Some(idx) = share_idx {
            if let Some(share) = state.shares.get_mut(idx) {
                if !share.shared_devices.contains(&resolved_id) {
                    share.shared_devices.push(resolved_id);
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
}
