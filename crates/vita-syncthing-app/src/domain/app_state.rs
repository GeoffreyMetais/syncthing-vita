//! Application state (`AppState`) holding all domain, navigation, and view-model state
//! independently of the 2D renderer or hardware framebuffer.

use syncthing_core::DeviceId;

use crate::domain::models::{
    ActiveTab, CameraPreview, FolderOffer, IgnoredFolder, KnownDevice, Language,
    LanguagePreference, ModalDialog, PickerItem, PickerMode, ShareConfigEntry, ThemeMode,
    BROWSER_ROWS,
};
use crate::domain::initial_sync::InitialSyncPeers;
use crate::domain::interaction::{FocusSlot, HitMap};
use crate::domain::navigation::{
    default_col, starts_on_header, NavFocus, NavState, DASHBOARD_HEADER_ACTIONS,
};
use crate::domain::ui_text::UiText;

/// Pure application state for PS Vita Syncthing.
/// All UI screens and modals read `&AppState` without performing I/O or mutating business rules.
pub struct AppState {
    pub theme_mode: ThemeMode,
    /// Language actually displayed: `language_pref` resolved against `system_language`.
    pub language: Language,
    /// Language chosen in Settings (`language.cfg`), defaulting to the system language.
    pub language_pref: LanguagePreference,
    /// Language detected from the PS Vita system settings.
    pub system_language: Language,
    pub active_tab: ActiveTab,
    /// D-pad focus (sidebar or page element).
    pub nav: NavState,
    /// Interactive elements of the frame on screen, declared by the UI while drawing it.
    pub hit_map: HitMap,
    pub picker_mode: PickerMode,
    pub modal: ModalDialog,
    pub oled_saver_active: bool,
    pub saver_x: f32,
    pub saver_y: f32,
    pub saver_vx: f32,
    pub saver_vy: f32,
    pub sync_in_progress: bool,
    pub vita_device_id: String,
    pub peer_label: UiText,
    pub folder_label: UiText,
    pub status_line: UiText,
    pub current_file: UiText,
    /// `None` while initializing / connecting (indeterminate); `Some(0..=100)` once bytes are transferred or sync completes.
    pub progress_pct: Option<u32>,
    /// VBlank tick counter used to animate the indeterminate progress bar.
    pub progress_anim_tick: u32,
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
    /// Peers in their initial sync phase (the CPU runs at full speed meanwhile).
    pub initial_sync: InitialSyncPeers,
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
    /// Folder ID of a share being created: it is discarded unless a device gets linked before
    /// the `ManageDevices` modal closes.
    pub pending_share_creation: Option<String>,
    /// Folders remote devices offered in their `ClusterConfig`, waiting for the user's consent.
    pub folder_offers: Vec<FolderOffer>,
    /// Folder offers the user declined (persisted in `ignored_folders.cfg`).
    pub ignored_folders: Vec<IgnoredFolder>,
    pub selected_share_idx: usize,
    pub picker_current_dir: String,
    pub picker_items: Vec<PickerItem>,
    pub selected_picker_idx: usize,
    pub picker_top_idx: usize,
    pub logs: Vec<String>,
    pub toast_msg: Option<(UiText, u32)>,
    /// Current text buffer in the `AddDeviceById` modal (updated by PS Vita `SceIme` or keypad).
    pub device_id_input: String,
    /// True while the native PS Vita System Keyboard (`SceIme`) overlay is active.
    pub ime_active: bool,
    /// True when the on-screen fallback Base32 keypad (`Virtual Keyboard`) is shown in `AddDeviceById`.
    pub fallback_keypad_visible: bool,
    /// Latest rear-camera frame shown while `ModalDialog::ScanDeviceQr` is open.
    pub qr_preview: CameraPreview,
    /// Camera failure reported while scanning (e.g. PlayStation TV has no camera).
    pub qr_camera_error: Option<String>,
}

impl AppState {
    pub fn new(
        shares: Vec<ShareConfigEntry>,
        devices: Vec<KnownDevice>,
        default_browse_root: &str,
    ) -> Self {
        Self {
            theme_mode: ThemeMode::Oled,
            language: Language::English,
            language_pref: LanguagePreference::System,
            system_language: Language::English,
            active_tab: ActiveTab::Dashboard,
            nav: NavState::new(ActiveTab::Dashboard),
            hit_map: HitMap::default(),
            picker_mode: PickerMode::ChangeShareTarget,
            modal: ModalDialog::None,
            oled_saver_active: false,
            saver_x: 180.0,
            saver_y: 160.0,
            saver_vx: 0.65,
            saver_vy: 0.45,
            sync_in_progress: false,
            vita_device_id: "Initializing...".to_owned(),
            peer_label: UiText::ReadyManualPairing,
            folder_label: UiText::NoFoldersSynced,
            status_line: UiText::ReadyTapSyncCard,
            current_file: UiText::raw("-"),
            progress_pct: Some(0),
            progress_anim_tick: 0,
            files_up_to_date: 0,
            files_downloaded: 0,
            blocks_uploaded: 0,
            bytes_transferred: 0,
            shares,
            devices,
            unpaired_lan_peers: Vec::new(),
            session_connected: false,
            initial_sync: InitialSyncPeers::default(),
            auto_reconnect_enabled: true,
            transfer_activity: 0,
            config_dirty: false,
            pending_folder_offers: Vec::new(),
            pending_share_creation: None,
            folder_offers: Vec::new(),
            ignored_folders: Vec::new(),
            selected_share_idx: 0,
            picker_current_dir: default_browse_root.to_owned(),
            picker_items: Vec::new(),
            selected_picker_idx: 0,
            picker_top_idx: 0,
            logs: Vec::with_capacity(32),
            toast_msg: None,
            device_id_input: String::new(),
            ime_active: false,
            fallback_keypad_visible: false,
            qr_preview: CameraPreview::default(),
            qr_camera_error: None,
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

    /// True if the user declined `folder_id` when `device_id` offered it.
    pub fn is_folder_ignored(&self, folder_id: &str, device_id: &DeviceId) -> bool {
        self.ignored_folders
            .iter()
            .any(|f| f.folder_id == folder_id && f.device_id.matches_peer(device_id))
    }

    /// Index of the first folder offer that should be prompted (not postponed with `Later`).
    pub fn next_folder_offer_to_prompt(&self) -> Option<usize> {
        self.folder_offers.iter().position(|o| !o.snoozed)
    }

    /// True while `shares[share_idx]` is being created and has no linked device yet.
    pub fn is_share_creation_incomplete(&self, share_idx: usize) -> bool {
        self.shares.get(share_idx).is_some_and(|s| {
            s.shared_devices.is_empty()
                && self.pending_share_creation.as_deref() == Some(s.folder_id.as_str())
        })
    }

    /// Resets the focused page element after the active tab changed (focus side is kept).
    pub fn reset_nav_for_tab(&mut self) {
        self.nav.col = default_col(self.active_tab);
        self.nav.header = starts_on_header(self.active_tab);
        self.nav.row = 0;
    }

    /// D-pad focus position on the current page (meaningful while the page has focus).
    pub fn focus_slot(&self) -> FocusSlot {
        match self.active_tab {
            ActiveTab::Dashboard if self.nav.header || self.shares.is_empty() => {
                FocusSlot::Header {
                    col: self.nav.col.min(DASHBOARD_HEADER_ACTIONS - 1),
                }
            }
            ActiveTab::Dashboard => FocusSlot::RowAction {
                row: self.selected_share_idx,
                col: self.nav.col,
            },
            ActiveTab::FolderPicker if self.nav.header => FocusSlot::Header { col: 0 },
            ActiveTab::FolderPicker => FocusSlot::RowAction {
                row: self.selected_picker_idx,
                col: 0,
            },
            ActiveTab::Settings => FocusSlot::RowAction {
                row: self.nav.row,
                col: self.nav.col.min(
                    crate::domain::settings_row_action_count(self.nav.row).saturating_sub(1),
                ),
            },
        }
    }

    /// True when the element holding `slot` on the current page has D-pad focus.
    pub fn is_focused(&self, slot: FocusSlot) -> bool {
        self.nav.focus == NavFocus::Content && self.focus_slot() == slot
    }

    /// Focused action button of `tab`'s current row, if the page (not the sidebar) has focus.
    pub fn focused_action(&self, tab: ActiveTab) -> Option<usize> {
        (self.active_tab == tab && self.nav.focus == NavFocus::Content && !self.nav.header)
            .then_some(self.nav.col)
    }

    /// True when a share row of the `Overview` (not its device card) has D-pad focus.
    pub fn is_share_row_focused(&self) -> bool {
        self.active_tab == ActiveTab::Dashboard
            && self.nav.focus == NavFocus::Content
            && !self.nav.header
            && self.selected_share_idx < self.shares.len()
    }

    /// True when the Settings card `row` has D-pad focus.
    pub fn is_settings_row_focused(&self, row: usize) -> bool {
        self.active_tab == ActiveTab::Settings
            && self.nav.focus == NavFocus::Content
            && self.nav.row == row
    }

    /// True when `tab`'s top button (`New Share`, `Up`) has D-pad focus.
    pub fn is_header_focused(&self, tab: ActiveTab) -> bool {
        self.active_tab == tab && self.nav.focus == NavFocus::Content && self.nav.header
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
    /// True while the QR scanner modal is open (the camera must run and the screensaver stay off).
    pub fn is_scanning_qr(&self) -> bool {
        matches!(self.modal, ModalDialog::ScanDeviceQr { .. })
    }

    pub fn toggle_theme(&mut self) {
        self.theme_mode = match self.theme_mode {
            ThemeMode::Dark => ThemeMode::Oled,
            ThemeMode::Oled => ThemeMode::Dark,
        };
        self.show_toast(match self.theme_mode {
            ThemeMode::Dark => UiText::ThemeDark,
            ThemeMode::Oled => UiText::ThemeOled,
        });
    }

    /// Applies a language preference and updates the displayed language.
    pub fn set_language_preference(&mut self, pref: LanguagePreference) {
        self.language_pref = pref;
        self.language = pref.resolve(self.system_language);
    }

    /// Sets the detected system language (used while the preference is `System`).
    pub fn set_system_language(&mut self, lang: Language) {
        self.system_language = lang;
        self.language = self.language_pref.resolve(lang);
    }

    /// Cycles the language preference
    /// (System -> English -> Français -> Español -> Deutsch -> Italiano -> System).
    pub fn cycle_language(&mut self) {
        self.set_language_preference(self.language_pref.next());
        self.show_toast(UiText::LanguageChanged(self.language));
    }

    /// Displays a transient toast notification for ~1.8 seconds (110 display VBlanks at 60 Hz).
    pub fn show_toast(&mut self, msg: UiText) {
        self.toast_msg = Some((msg, 110));
    }

    /// Decrements the toast timer by one display VBlank (1/60 s).
    /// Returns `true` if the toast just expired (the screen must be redrawn).
    pub fn tick_toast(&mut self) -> bool {
        if let Some((_, ref mut ticks)) = self.toast_msg {
            if *ticks <= 1 {
                self.toast_msg = None;
                return true;
            }
            *ticks -= 1;
        }
        false
    }

    /// Advances the indeterminate progress bar animation by one display VBlank.
    pub fn tick_progress_anim(&mut self) {
        if self.progress_pct.is_none() {
            self.progress_anim_tick = self.progress_anim_tick.wrapping_add(1);
        }
    }

    /// Advances the bouncing DVD-style OLED screensaver position within the given screen bounds
    /// by `frames` display VBlanks (the speed is the same whatever the rendering frame rate).
    pub fn step_screensaver(
        &mut self,
        screen_w: f32,
        screen_h: f32,
        box_w: f32,
        box_h: f32,
        frames: u32,
    ) {
        for _ in 0..frames {
            self.step_screensaver_once(screen_w, screen_h, box_w, box_h);
        }
    }

    fn step_screensaver_once(&mut self, screen_w: f32, screen_h: f32, box_w: f32, box_h: f32) {
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
        self.progress_pct = None;
        self.status_line = UiText::StartingSync;
        self.show_toast(UiText::SyncingWithPeers);
    }

    /// Human-readable summary of devices linked to a share.
    pub fn share_devices_summary(&self, share: &ShareConfigEntry) -> UiText {
        if share.shared_devices.is_empty() {
            return UiText::NoDevicesLinked;
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
        UiText::SharedWith {
            devices: names.join(", "),
        }
    }
}
