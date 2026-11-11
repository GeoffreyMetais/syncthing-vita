//! Application state (`AppState`) holding all domain, navigation, and view-model state
//! independently of the 2D renderer or hardware framebuffer.

use syncthing_core::DeviceId;

use crate::domain::models::{
    ActiveTab, KnownDevice, ModalDialog, PickerItem, PickerMode, ShareConfigEntry, ThemeMode,
    BROWSER_ROWS,
};

/// Pure application state for PS Vita Syncthing.
/// All UI screens and modals read `&AppState` without performing I/O or mutating business rules.
pub struct AppState {
    pub theme_mode: ThemeMode,
    pub active_tab: ActiveTab,
    pub picker_mode: PickerMode,
    pub modal: ModalDialog,
    pub oled_saver_active: bool,
    pub saver_x: f32,
    pub saver_y: f32,
    pub saver_vx: f32,
    pub saver_vy: f32,
    pub sync_in_progress: bool,
    pub vita_device_id: String,
    pub peer_label: String,
    pub folder_label: String,
    pub status_line: String,
    pub current_file: String,
    pub progress_pct: u32,
    pub files_up_to_date: usize,
    pub files_downloaded: usize,
    pub blocks_uploaded: usize,
    pub bytes_transferred: usize,
    pub shares: Vec<ShareConfigEntry>,
    pub devices: Vec<KnownDevice>,
    /// Peers discovered passively on the LAN (UDP 21027) that have NOT been manually linked/paired by the user.
    pub unpaired_lan_peers: Vec<DeviceId>,
    /// True while a persistent `1+2` BEP/1.0 connection is active in the background.
    pub session_connected: bool,
    /// True when automatic background reconnection is permitted (disabled when a peer is awaiting manual pairing).
    pub auto_reconnect_enabled: bool,
    /// Incremented on every data transfer or applied remote change (downloads, uploads, deletions).
    /// Lets the composition root detect transfer activity without the domain knowing about time.
    pub transfer_activity: u64,
    /// True when share/device configuration was modified and the background session should reload once modals close.
    pub config_dirty: bool,
    /// Folder IDs explicitly created, enabled, or linked to a device by the user during this session
    /// that should be advertised as new folder offers in the next BEP `ClusterConfig`.
    pub pending_folder_offers: Vec<String>,
    pub selected_share_idx: usize,
    pub picker_current_dir: String,
    pub picker_items: Vec<PickerItem>,
    pub selected_picker_idx: usize,
    pub picker_top_idx: usize,
    pub logs: Vec<String>,
    pub toast_msg: Option<(String, u32)>,
    /// Current text buffer in the `AddDeviceById` modal (updated by PS Vita `SceIme` or keypad).
    pub device_id_input: String,
    /// True while the native PS Vita System Keyboard (`SceIme`) overlay is active.
    pub ime_active: bool,
}

impl AppState {
    pub fn new(
        shares: Vec<ShareConfigEntry>,
        devices: Vec<KnownDevice>,
        default_browse_root: &str,
    ) -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
            active_tab: ActiveTab::Dashboard,
            picker_mode: PickerMode::ChangeShareTarget,
            modal: ModalDialog::None,
            oled_saver_active: false,
            saver_x: 180.0,
            saver_y: 160.0,
            saver_vx: 0.65,
            saver_vy: 0.45,
            sync_in_progress: false,
            vita_device_id: "Initializing...".to_owned(),
            peer_label: "Ready (Manual LAN pairing)".to_owned(),
            folder_label: "No folders synced yet".to_owned(),
            status_line: "Ready · Press Cross to synchronize".to_owned(),
            current_file: "-".to_owned(),
            progress_pct: 0,
            files_up_to_date: 0,
            files_downloaded: 0,
            blocks_uploaded: 0,
            bytes_transferred: 0,
            shares,
            devices,
            unpaired_lan_peers: Vec::new(),
            session_connected: false,
            auto_reconnect_enabled: true,
            transfer_activity: 0,
            config_dirty: false,
            pending_folder_offers: Vec::new(),
            selected_share_idx: 0,
            picker_current_dir: default_browse_root.to_owned(),
            picker_items: Vec::new(),
            selected_picker_idx: 0,
            picker_top_idx: 0,
            logs: Vec::with_capacity(32),
            toast_msg: None,
            device_id_input: String::new(),
            ime_active: false,
        }
    }

    /// Returns `true` if `device_id` was only passively discovered on LAN and not yet approved by the user.
    pub fn is_unpaired_lan_peer(&self, device_id: &DeviceId) -> bool {
        self.unpaired_lan_peers
            .iter()
            .any(|id| id.matches_peer(device_id))
    }

    /// Returns the list of user-approved/linked `KnownDevice`s eligible for BEP/1.0 connection.
    pub fn approved_devices(&self) -> Vec<KnownDevice> {
        self.devices
            .iter()
            .filter(|d| {
                let is_linked_to_share = self.shares.iter().any(|s| {
                    s.shared_devices
                        .iter()
                        .filter_map(|h| DeviceId::from_hex(h))
                        .any(|id| id.matches_peer(&d.device_id))
                });
                is_linked_to_share || !self.is_unpaired_lan_peer(&d.device_id)
            })
            .cloned()
            .collect()
    }

    /// Returns `true` if at least one peer has been manually added or linked to a share.
    pub fn has_configured_peers(&self) -> bool {
        !self.approved_devices().is_empty()
    }

    /// Marks `folder_id` as explicitly offered by the user so the next BEP `ClusterConfig`
    /// advertises it to linked devices.
    pub fn queue_folder_offer(&mut self, folder_id: &str) {
        if !self.pending_folder_offers.iter().any(|f| f == folder_id) {
            self.pending_folder_offers.push(folder_id.to_owned());
        }
    }

    /// Cleans up legacy auto-discovered unpaired LAN peers (`"LAN Peer"`, `"Syncthing Peer"`, `"Syncthing ("`)
    /// that may have been persisted by older versions when an unpaired LAN instance rejected `Hello`.
    pub fn sanitize_legacy_unpaired_peers(&mut self) -> bool {
        let mut unlinked_hexes = Vec::new();
        for dev in &self.devices {
            let is_legacy_unpaired_name = dev.name == "LAN Peer"
                || dev.name == "Syncthing Peer"
                || dev.name.starts_with("Syncthing (")
                || dev.name.starts_with("LAN Peer (");
            if is_legacy_unpaired_name && !self.is_unpaired_lan_peer(&dev.device_id) {
                self.unpaired_lan_peers.push(dev.device_id);
                unlinked_hexes.push(dev.device_id.to_hex());
            }
        }
        if unlinked_hexes.is_empty() {
            return false;
        }
        for s in &mut self.shares {
            s.shared_devices.retain(|h| !unlinked_hexes.contains(h));
        }
        true
    }

    /// Toggles between Dark (VitaSend slate) and OLED (pure `#000000`) theme modes.
    pub fn toggle_theme(&mut self) {
        self.theme_mode = match self.theme_mode {
            ThemeMode::Dark => ThemeMode::Oled,
            ThemeMode::Oled => ThemeMode::Dark,
        };
        let label = match self.theme_mode {
            ThemeMode::Dark => "Theme: Dark (Blue-Grey)",
            ThemeMode::Oled => "Theme: OLED (Pure Black #000000)",
        };
        self.show_toast(label);
    }

    /// Displays a transient toast notification for ~1.8 seconds (110 frames at 60 FPS).
    pub fn show_toast(&mut self, msg: &str) {
        self.toast_msg = Some((msg.to_owned(), 110));
    }

    /// Decrements the toast timer by one frame.
    pub fn tick_toast(&mut self) {
        if let Some((_, ref mut ticks)) = self.toast_msg {
            if *ticks <= 1 {
                self.toast_msg = None;
            } else {
                *ticks -= 1;
            }
        }
    }

    /// Advances the bouncing DVD-style OLED screensaver position within the given screen bounds.
    pub fn step_screensaver(&mut self, screen_w: f32, screen_h: f32, box_w: f32, box_h: f32) {
        self.saver_x += self.saver_vx;
        self.saver_y += self.saver_vy;
        let max_x = screen_w - box_w - 12.0;
        let max_y = screen_h - box_h - 12.0;
        if self.saver_x < 12.0 {
            self.saver_x = 12.0;
            self.saver_vx = -self.saver_vx;
        } else if self.saver_x > max_x {
            self.saver_x = max_x;
            self.saver_vx = -self.saver_vx;
        }
        if self.saver_y < 12.0 {
            self.saver_y = 12.0;
            self.saver_vy = -self.saver_vy;
        } else if self.saver_y > max_y {
            self.saver_y = max_y;
            self.saver_vy = -self.saver_vy;
        }
    }

    /// Ensures the directory picker scroll window (`picker_top_idx`) keeps `selected_picker_idx` visible.
    pub fn clamp_picker_scroll(&mut self) {
        if self.picker_items.is_empty() {
            self.selected_picker_idx = 0;
            self.picker_top_idx = 0;
            return;
        }
        if self.selected_picker_idx >= self.picker_items.len() {
            self.selected_picker_idx = self.picker_items.len() - 1;
        }
        if self.selected_picker_idx < self.picker_top_idx {
            self.picker_top_idx = self.selected_picker_idx;
        } else if self.selected_picker_idx >= self.picker_top_idx + BROWSER_ROWS {
            self.picker_top_idx = self.selected_picker_idx + 1 - BROWSER_ROWS;
        }
    }

    /// Appends a sanitized line to the in-memory protocol log ring buffer (retaining last 14 entries).
    pub fn push_log(&mut self, line: &str) {
        let clean: String = line
            .chars()
            .filter(|&c| c >= ' ' && c != '\u{7f}')
            .collect();
        self.logs.push(clean.trim().to_owned());
        if self.logs.len() > 14 {
            self.logs.remove(0);
        }
    }

    /// Resets transfer counters at the beginning of a manual synchronization cycle.
    pub fn prepare_manual_sync(&mut self) {
        self.sync_in_progress = true;
        self.files_up_to_date = 0;
        self.files_downloaded = 0;
        self.blocks_uploaded = 0;
        self.bytes_transferred = 0;
        self.progress_pct = 5;
        self.status_line = "Starting synchronization...".to_owned();
        self.show_toast("Synchronizing with LAN peers...");
    }

    /// Human-readable summary of devices linked to a share.
    pub fn share_devices_summary(&self, share: &ShareConfigEntry) -> String {
        if share.shared_devices.is_empty() {
            return "No devices linked (Press Cross to add a device)".to_owned();
        }
        let mut names = Vec::new();
        for hex in &share.shared_devices {
            if let Some(dev) = self.devices.iter().find(|d| &d.device_id.to_hex() == hex) {
                names.push(format!("{} ({})", dev.name, dev.device_id.short()));
            } else if let Some(id) = syncthing_core::DeviceId::from_hex(hex) {
                names.push(id.short());
            } else if hex.len() >= 7 {
                names.push(hex[..7].to_uppercase());
            }
        }
        format!("Shared with: {}", names.join(", "))
    }
}
