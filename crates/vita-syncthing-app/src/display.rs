//! Native PS Vita Graphical User Interface (`960x544` RGBA8888 via `SceDisplay` + `SceSysmem`)
//! Modeled after **VitaSend** (`ui.c`, `app_pages.c`, `app_files.c`, `app_saver.c`, `app_dialogs.c`):
//! - Left sidebar navigation with vector icons, Wi-Fi indicator, and live sync mini-card
//! - Anti-aliased Poppins TrueType typography (Regular, Medium, Bold)
//! - Authentic colored PlayStation button badges (`Cross`, `Circle`, `Square`, `Triangle`, `L`, `R`, `START`, `SELECT`)
//! - Share Creation modal dialog (`+ New Share`) & Device Assignment modal dialog (`Add / Toggle Devices`)
//! - Dual themes: **Dark** (slate blue-grey + teal accent) and **OLED** (pure `#000000` black + crisp dividers)
//! - Bouncing DVD-style OLED Screensaver with live transfer progress bar

use std::fs;
use std::path::Path;

use qrcodegen::{QrCode, QrCodeEcc};
use syncthing_core::DeviceId;

use crate::gfx::{rgb, rgba, with_alpha, Align, Color, FontWeight, Gfx, Icon, PadButton, GFX_H, GFX_W};

#[allow(dead_code)]
pub const SCREEN_W: usize = GFX_W as usize;
#[allow(dead_code)]
pub const SCREEN_H: usize = GFX_H as usize;
const FB_ALIGNED_SIZE: usize = 2 * 1024 * 1024;

pub const SIDEBAR_W: i32 = 212;
pub const CONTENT_X: i32 = 236;
pub const CONTENT_R: i32 = 936;
pub const CONTENT_W: i32 = CONTENT_R - CONTENT_X;
pub const CONTENT_Y: i32 = 86;
pub const CONTENT_B: i32 = 488;
pub const HINT_Y: i32 = 500;

pub const BROWSER_TOP: i32 = 134;
pub const BROWSER_ROWS: usize = 8;
pub const BROWSER_STEP: i32 = 42;

const C_GOOD: Color = rgb(102, 187, 106);
const C_WARN: Color = rgb(255, 193, 77);
const C_BAD: Color = rgb(239, 83, 80);

#[cfg(target_os = "vita")]
mod display_ffi {
    use libc::{c_char, c_int, c_uint, c_void};

    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW: c_uint = 0x0C20D060;
    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_MAIN_PHYCONT_NC_RW: c_uint = 0x0C20D050;
    pub const SCE_DISPLAY_SETBUF_NEXTFRAME: c_int = 1;

    #[repr(C)]
    pub struct SceDisplayFrameBuf {
        pub size: c_uint,
        pub base: *mut c_void,
        pub pitch: c_uint,
        pub pixelformat: c_uint,
        pub width: c_uint,
        pub height: c_uint,
    }

    #[link(name = "SceSysmem_stub")]
    extern "C" {
        pub fn sceKernelAllocMemBlock(
            name: *const c_char,
            block_type: c_uint,
            size: c_int,
            opt: *const c_void,
        ) -> c_int;
        pub fn sceKernelGetMemBlockBase(uid: c_int, basep: *mut *mut c_void) -> c_int;
    }

    #[link(name = "SceDisplay_stub")]
    extern "C" {
        pub fn sceDisplaySetFrameBuf(
            pParam: *const SceDisplayFrameBuf,
            sync: c_int,
        ) -> c_int;
        pub fn sceDisplayWaitVblankStart() -> c_int;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark = 0,
    Oled = 1,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct UiTheme {
    pub bg: Color,
    pub sidebar: Color,
    pub card: Color,
    pub card_hi: Color,
    pub border: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent_hi: Color,
    pub accent_bg: Color,
    pub on_accent: Color,
    pub divider: bool,
}

impl UiTheme {
    pub const fn dark() -> Self {
        Self {
            bg: rgb(15, 19, 23),
            sidebar: rgb(11, 14, 17),
            card: rgb(25, 31, 37),
            card_hi: rgb(31, 40, 48),
            border: rgb(40, 49, 58),
            text: rgb(232, 238, 243),
            dim: rgb(140, 152, 164),
            faint: rgb(88, 99, 110),
            accent: rgb(38, 166, 154),
            accent_hi: rgb(72, 208, 190),
            accent_bg: rgba(38, 166, 154, 46),
            on_accent: rgb(6, 24, 22),
            divider: false,
        }
    }

    pub const fn oled() -> Self {
        Self {
            bg: rgb(0, 0, 0),
            sidebar: rgb(0, 0, 0),
            card: rgb(15, 15, 16),
            card_hi: rgb(26, 27, 28),
            border: rgb(38, 40, 42),
            text: rgb(240, 242, 244),
            dim: rgb(150, 156, 162),
            faint: rgb(96, 101, 106),
            accent: rgb(38, 166, 154),
            accent_hi: rgb(72, 208, 190),
            accent_bg: rgba(38, 166, 154, 46),
            on_accent: rgb(2, 20, 18),
            divider: true,
        }
    }
}

/// Main navigation tabs in the left sidebar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Dashboard = 0,
    Shares = 1,
    FolderPicker = 2,
    Settings = 3,
}

/// Mode for the `Files` (`FolderPicker`) tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    /// Change target directory for `selected_share_idx`
    ChangeShareTarget,
    /// Create a brand-new Syncthing share from the chosen directory
    CreateNewShare,
}

/// VitaSend-style modal dialogs drawn over dimmed background
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalDialog {
    None,
    /// Create a new share from a template or by browsing `ux0:`
    CreateShare { selected_idx: usize },
    /// Add or remove remote Syncthing devices for a share
    ManageDevices {
        share_idx: usize,
        selected_dev_idx: usize,
    },
    /// Display the PS Vita's full Syncthing Device ID and scannable QR code
    ShowDeviceId,
    /// Confirmation dialog before quitting the application (`START` button)
    ConfirmQuit,
}

/// Known or discovered remote Syncthing device
#[derive(Debug, Clone)]
pub struct KnownDevice {
    pub device_id: DeviceId,
    pub name: String,
    pub address: String,
}

/// Syncthing shared folder configuration on PS Vita
#[derive(Debug, Clone)]
pub struct ShareConfigEntry {
    pub folder_id: String,
    pub label: String,
    pub target_path: String,
    pub enabled: bool,
    /// Hex strings (`DeviceId::to_hex()`) of remote devices this folder is shared with
    pub shared_devices: Vec<String>,
}

/// Item in target folder picker (`ux0:`)
#[derive(Debug, Clone)]
pub struct PickerItem {
    pub label: String,
    pub path: String,
    pub is_preset: bool,
}

/// Templates offered in the `Create New Share` modal dialog
pub const NEW_SHARE_TEMPLATES: &[(&str, &str, &str)] = &[
    ("vita-savedata", "Vita Save Data", "ux0:user/00/savedata"),
    ("vita-pictures", "Vita Screenshots & Pictures", "ux0:picture"),
    ("vita-music", "Vita Music Library", "ux0:music"),
    ("retroarch-saves", "RetroArch Savefiles", "ux0:data/retroarch/savefiles"),
    ("vita-shared", "Vita Shared Folder", "ux0:data/syncthing/shared"),
    ("__browse__", "Browse ux0: to pick any custom folder...", "ux0:"),
];

/// Format byte sizes cleanly ("1.5 KB", "3.2 MB")
pub fn fmt_size(b: usize) -> String {
    if b < 1024 {
        format!("{} B", b)
    } else if b < 1024 * 1024 {
        format!("{:.1} KB", (b as f64) / 1024.0)
    } else {
        format!("{:.1} MB", (b as f64) / (1024.0 * 1024.0))
    }
}

/// Complete PS Vita dashboard UI state (VitaSend style).
pub struct VitaDashboard {
    #[cfg(target_os = "vita")]
    mem_uid: i32,
    #[cfg(target_os = "vita")]
    fb_ptr: *mut u32,
    pub gfx: Gfx,
    pub theme_mode: ThemeMode,
    pub th: UiTheme,
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
    pub selected_share_idx: usize,
    pub picker_current_dir: String,
    pub picker_items: Vec<PickerItem>,
    pub selected_picker_idx: usize,
    pub picker_top_idx: usize,
    pub logs: Vec<String>,
    pub toast_msg: Option<(String, u32)>,
}

impl VitaDashboard {
    pub fn new(base_data_dir: &str) -> Self {
        #[cfg(target_os = "vita")]
        let (mem_uid, fb_ptr) = Self::init_vita_fb();

        let devices = Self::load_devices_config(base_data_dir);
        let shares = Self::load_shares_config(base_data_dir);

        let mut dash = Self {
            #[cfg(target_os = "vita")]
            mem_uid,
            #[cfg(target_os = "vita")]
            fb_ptr,
            gfx: Gfx::new(),
            theme_mode: ThemeMode::Dark,
            th: UiTheme::dark(),
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
            peer_label: "Scanning UDP 21027...".to_owned(),
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
            selected_share_idx: 0,
            picker_current_dir: Self::default_browse_root().to_owned(),
            picker_items: Vec::new(),
            selected_picker_idx: 0,
            picker_top_idx: 0,
            logs: Vec::with_capacity(32),
            toast_msg: None,
        };
        dash.refresh_picker_items(base_data_dir);
        dash.render();
        dash
    }

    pub fn toggle_theme(&mut self) {
        self.theme_mode = match self.theme_mode {
            ThemeMode::Dark => ThemeMode::Oled,
            ThemeMode::Oled => ThemeMode::Dark,
        };
        self.th = match self.theme_mode {
            ThemeMode::Dark => UiTheme::dark(),
            ThemeMode::Oled => UiTheme::oled(),
        };
        let label = match self.theme_mode {
            ThemeMode::Dark => "Theme: Dark (Blue-Grey)",
            ThemeMode::Oled => "Theme: OLED (Pure Black #000000)",
        };
        self.show_toast(label);
    }

    pub fn show_toast(&mut self, msg: &str) {
        self.toast_msg = Some((msg.to_owned(), 110));
    }

    #[cfg(target_os = "vita")]
    fn default_browse_root() -> &'static str {
        "ux0:"
    }

    #[cfg(not(target_os = "vita"))]
    fn default_browse_root() -> &'static str {
        "/tmp/syncthing-vita"
    }

    #[cfg(target_os = "vita")]
    fn init_vita_fb() -> (i32, *mut u32) {
        use display_ffi::*;
        use std::ptr;

        unsafe {
            let mut uid = sceKernelAllocMemBlock(
                b"vita_syncthing_fb\0".as_ptr() as *const libc::c_char,
                SCE_KERNEL_MEMBLOCK_TYPE_USER_CDRAM_RW,
                FB_ALIGNED_SIZE as i32,
                ptr::null(),
            );
            if uid < 0 {
                uid = sceKernelAllocMemBlock(
                    b"vita_syncthing_fb\0".as_ptr() as *const libc::c_char,
                    SCE_KERNEL_MEMBLOCK_TYPE_USER_MAIN_PHYCONT_NC_RW,
                    FB_ALIGNED_SIZE as i32,
                    ptr::null(),
                );
            }
            if uid < 0 {
                return (-1, ptr::null_mut());
            }

            let mut base: *mut libc::c_void = ptr::null_mut();
            if sceKernelGetMemBlockBase(uid, &mut base) < 0 || base.is_null() {
                return (uid, ptr::null_mut());
            }

            let fb_param = SceDisplayFrameBuf {
                size: std::mem::size_of::<SceDisplayFrameBuf>() as u32,
                base,
                pitch: SCREEN_W as u32,
                pixelformat: 0,
                width: SCREEN_W as u32,
                height: SCREEN_H as u32,
            };
            let _ = sceDisplaySetFrameBuf(&fb_param, SCE_DISPLAY_SETBUF_NEXTFRAME);
            let _ = sceDisplayWaitVblankStart();

            (uid, base as *mut u32)
        }
    }

    /// Load known remote devices from `ux0:data/syncthing/devices.cfg`
    pub fn load_devices_config(base_data_dir: &str) -> Vec<KnownDevice> {
        let cfg_path = format!("{}/devices.cfg", base_data_dir);
        let mut out = Vec::new();
        if let Ok(content) = fs::read_to_string(&cfg_path) {
            for line in content.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3 {
                    if let Some(device_id) = DeviceId::from_hex(parts[0]) {
                        out.push(KnownDevice {
                            device_id,
                            name: parts[1].to_owned(),
                            address: parts[2].to_owned(),
                        });
                    }
                }
            }
        }
        out
    }

    /// Save known remote devices to `ux0:data/syncthing/devices.cfg`
    pub fn save_devices_config(&self, base_data_dir: &str) {
        let _ = fs::create_dir_all(base_data_dir);
        let cfg_path = format!("{}/devices.cfg", base_data_dir);
        let mut out = String::new();
        for d in &self.devices {
            out.push_str(&format!(
                "{}|{}|{}\n",
                d.device_id.to_hex(),
                d.name,
                d.address
            ));
        }
        let _ = fs::write(cfg_path, out);
    }

    /// Register or update a discovered/connected Syncthing device
    pub fn upsert_device(
        &mut self,
        base_data_dir: &str,
        device_id: DeviceId,
        name: &str,
        address: &str,
    ) {
        let dev_hex = device_id.to_hex();
        if let Some(existing) = self.devices.iter_mut().find(|d| d.device_id == device_id) {
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
            self.devices.push(KnownDevice {
                device_id,
                name: label,
                address: address.to_owned(),
            });
        }
        // Automatically link any share that had empty devices to this device
        for s in &mut self.shares {
            if s.shared_devices.is_empty() {
                s.shared_devices.push(dev_hex.clone());
            }
        }
        self.save_devices_config(base_data_dir);
        self.save_shares_config(base_data_dir);
    }

    /// Load shared folder configuration from `ux0:data/syncthing/shares.cfg`
    pub fn load_shares_config(base_data_dir: &str) -> Vec<ShareConfigEntry> {
        let cfg_path = format!("{}/shares.cfg", base_data_dir);
        let mut entries = Vec::new();
        let mut needs_save = false;
        if let Ok(content) = fs::read_to_string(&cfg_path) {
            for line in content.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 4 {
                    let shared_devices = if parts.len() >= 5 && !parts[4].trim().is_empty() {
                        parts[4]
                            .split(',')
                            .map(|s| s.trim().to_owned())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let folder_id = parts[0].to_owned();
                    let mut target_path = parts[2].to_owned();
                    let default_sync_path = format!("{}/sync/{}", base_data_dir, folder_id);
                    if target_path == default_sync_path {
                        if let Some(&(_, _, tpl_path)) = NEW_SHARE_TEMPLATES
                            .iter()
                            .find(|&&(id, _, _)| id == folder_id && id != "__browse__")
                        {
                            target_path = tpl_path.to_owned();
                            needs_save = true;
                        }
                    }
                    entries.push(ShareConfigEntry {
                        folder_id,
                        label: parts[1].to_owned(),
                        target_path,
                        enabled: parts[3] != "0",
                        shared_devices,
                    });
                }
            }
        }
        if needs_save {
            let mut out = String::new();
            for s in &entries {
                let devs = s.shared_devices.join(",");
                out.push_str(&format!(
                    "{}|{}|{}|{}|{}\n",
                    s.folder_id,
                    s.label,
                    s.target_path,
                    if s.enabled { "1" } else { "0" },
                    devs
                ));
            }
            let _ = fs::write(&cfg_path, out);
        }
        entries
    }

    /// Save shared folder configuration to `ux0:data/syncthing/shares.cfg`
    pub fn save_shares_config(&self, base_data_dir: &str) {
        let _ = fs::create_dir_all(base_data_dir);
        let cfg_path = format!("{}/shares.cfg", base_data_dir);
        let mut out = String::new();
        for s in &self.shares {
            let devs = s.shared_devices.join(",");
            out.push_str(&format!(
                "{}|{}|{}|{}|{}\n",
                s.folder_id,
                s.label,
                s.target_path,
                if s.enabled { "1" } else { "0" },
                devs
            ));
        }
        let _ = fs::write(cfg_path, out);
    }

    /// Create a new share on PS Vita (or update if `folder_id` already exists) and link all known devices by default.
    pub fn create_share(
        &mut self,
        base_data_dir: &str,
        folder_id: &str,
        label: &str,
        target_path: &str,
    ) -> usize {
        let default_devs: Vec<String> = self
            .devices
            .iter()
            .map(|d| d.device_id.to_hex())
            .collect();

        let _ = fs::create_dir_all(target_path);

        if let Some(idx) = self.shares.iter().position(|s| s.folder_id == folder_id) {
            self.shares[idx].label = label.to_owned();
            self.shares[idx].target_path = target_path.to_owned();
            self.shares[idx].enabled = true;
            if self.shares[idx].shared_devices.is_empty() {
                self.shares[idx].shared_devices = default_devs;
            }
            self.selected_share_idx = idx;
            self.save_shares_config(base_data_dir);
            idx
        } else {
            self.shares.push(ShareConfigEntry {
                folder_id: folder_id.to_owned(),
                label: label.to_owned(),
                target_path: target_path.to_owned(),
                enabled: true,
                shared_devices: default_devs,
            });
            let idx = self.shares.len() - 1;
            self.selected_share_idx = idx;
            self.save_shares_config(base_data_dir);
            idx
        }
    }

    /// Toggle whether `device_id` is added to `shares[share_idx]`.
    pub fn toggle_device_for_share(
        &mut self,
        base_data_dir: &str,
        share_idx: usize,
        device_id: DeviceId,
    ) -> Option<(String, String, bool)> {
        let dev_hex = device_id.to_hex();
        let dev_name = self
            .devices
            .iter()
            .find(|d| d.device_id == device_id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| device_id.short());

        if let Some(share) = self.shares.get_mut(share_idx) {
            let folder_label = share.label.clone();
            let added = if let Some(pos) = share.shared_devices.iter().position(|x| x == &dev_hex) {
                share.shared_devices.remove(pos);
                false
            } else {
                share.shared_devices.push(dev_hex);
                true
            };
            self.save_shares_config(base_data_dir);
            Some((folder_label, dev_name, added))
        } else {
            None
        }
    }

    /// Merge folders received in `ClusterConfig` with local shared folder configuration.
    pub fn upsert_discovered_folder(
        &mut self,
        base_data_dir: &str,
        folder_id: &str,
        label: &str,
    ) {
        let default_devs: Vec<String> = self
            .devices
            .iter()
            .map(|d| d.device_id.to_hex())
            .collect();

        let template_path = NEW_SHARE_TEMPLATES
            .iter()
            .find(|&&(id, _, _)| id == folder_id && id != "__browse__")
            .map(|&(_, _, p)| p.to_owned());

        let fallback_sync = format!("{}/sync/{}", base_data_dir, folder_id);

        if let Some(existing) = self.shares.iter_mut().find(|s| s.folder_id == folder_id) {
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
            self.shares.push(ShareConfigEntry {
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
        self.save_shares_config(base_data_dir);
    }

    /// Human-readable summary of devices linked to a share
    fn share_devices_summary(&self, share: &ShareConfigEntry) -> String {
        if share.shared_devices.is_empty() {
            return "No devices linked (Press Cross to add a device)".to_owned();
        }
        let mut names = Vec::new();
        for hex in &share.shared_devices {
            if let Some(dev) = self.devices.iter().find(|d| &d.device_id.to_hex() == hex) {
                names.push(format!("{} ({})", dev.name, dev.device_id.short()));
            } else if hex.len() >= 7 {
                names.push(hex[..7].to_uppercase());
            }
        }
        format!("Shared with: {}", names.join(", "))
    }

    /// Refresh presets and `ux0:` subdirectories for the currently selected share or new share creation.
    pub fn refresh_picker_items(&mut self, base_data_dir: &str) {
        let folder_id = if self.picker_mode == PickerMode::CreateNewShare {
            let tail = self
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
            self.shares
                .get(self.selected_share_idx)
                .map(|s| s.folder_id.clone())
                .unwrap_or_else(|| "default".to_owned())
        };

        let current_target = self
            .shares
            .get(self.selected_share_idx)
            .map(|s| s.target_path.clone())
            .unwrap_or_else(|| "ux0:user/00/savedata".to_owned());

        let mut items = if self.picker_mode == PickerMode::CreateNewShare {
            vec![PickerItem {
                label: format!(
                    "Create share from current folder: {}",
                    self.picker_current_dir
                ),
                path: self.picker_current_dir.clone(),
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
                        self.picker_current_dir.trim_end_matches('/')
                    ),
                    path: self.picker_current_dir.trim_end_matches('/').to_owned(),
                    is_preset: true,
                },
            ]
        };

        let scan_dir = if self.picker_current_dir == "ux0:" {
            "ux0:/"
        } else {
            &self.picker_current_dir
        };

        if let Ok(read_dir) = fs::read_dir(scan_dir) {
            let mut subdirs = Vec::new();
            for entry in read_dir.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if !name.starts_with('.') {
                            subdirs.push(name);
                        }
                    }
                }
            }
            subdirs.sort();
            for sub in subdirs {
                let full = if self.picker_current_dir.ends_with(':')
                    || self.picker_current_dir.ends_with('/')
                {
                    format!("{}{}", self.picker_current_dir, sub)
                } else {
                    format!("{}/{}", self.picker_current_dir, sub)
                };
                items.push(PickerItem {
                    label: format!("{}/", sub),
                    path: full,
                    is_preset: false,
                });
            }
        }

        self.picker_items = items;
        if self.selected_picker_idx >= self.picker_items.len() {
            self.selected_picker_idx = 0;
        }
        self.clamp_picker_scroll();
    }

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

    // ============================================================== VitaSend UI Widgets

    fn draw_card(&mut self, x: i32, y: i32, w: i32, h: i32, focused: bool) {
        let bg = if focused { self.th.card_hi } else { self.th.card };
        self.gfx.rrect(x, y, w, h, 14, bg);
        if focused {
            self.gfx.rrect_outline(x, y, w, h, 14, 2, self.th.accent);
        } else {
            self.gfx.rrect_outline(x, y, w, h, 14, 1, self.th.border);
        }
    }

    fn draw_chip(&mut self, right_x: i32, cy: i32, text: &str, col: Color) -> i32 {
        let w = self.gfx.text_width(13, FontWeight::Medium, text) + 22;
        self.gfx.rrect(right_x - w, cy - 12, w, 24, 12, with_alpha(col, 44));
        self.gfx.text(
            right_x - w / 2,
            cy,
            13,
            FontWeight::Medium,
            col,
            Align::Center,
            text,
        );
        w
    }

    fn action_width(&mut self, btn: PadButton, label: &str) -> i32 {
        8 + Gfx::pad_button_width(btn, 22)
            + 8
            + self.gfx.text_width(15, FontWeight::Medium, label)
            + 16
    }

    fn draw_action(&mut self, x: i32, cy: i32, btn: PadButton, label: &str) -> i32 {
        let gw = Gfx::pad_button_width(btn, 22);
        let tw = self.gfx.text_width(15, FontWeight::Medium, label);
        let w = 8 + gw + 8 + tw + 16;
        self.gfx.rrect(x, cy - 17, w, 34, 17, rgb(36, 45, 54));
        self.gfx.rrect_outline(x, cy - 17, w, 34, 17, 1, self.th.border);
        self.gfx.pad_button(btn, x + 8 + gw / 2 + 1, cy, 22);
        self.gfx.text(
            x + 8 + gw + 8,
            cy,
            15,
            FontWeight::Medium,
            self.th.text,
            Align::Left,
            label,
        );
        w
    }

    /// Circular checkbox modeled after VitaSend `row_tick`
    fn draw_row_tick(&mut self, cx: i32, cy: i32, on: bool) {
        if on {
            self.gfx.circle(cx as f32, cy as f32, 10.0, self.th.accent);
            self.gfx
                .icon(Icon::Check, cx as f32, cy as f32, 16.0, self.th.on_accent);
        } else {
            self.gfx
                .ring(cx as f32, cy as f32, 9.0, 1.8, self.th.faint);
        }
    }

    fn draw_progress(&mut self, x: i32, y: i32, w: i32, h: i32, pct: u32, col: Color) {
        self.gfx.rrect(x, y, w, h, h / 2, rgb(38, 47, 56));
        let p = pct.min(100);
        if p == 0 {
            return;
        }
        let fw = ((w as u32) * p / 100) as i32;
        self.gfx.rrect(x, y, fw.max(h), h, h / 2, col);
    }

    fn draw_scrollbar(&mut self, x: i32, y: i32, h: i32, top: usize, rows: usize, count: usize) {
        if count <= rows || rows == 0 {
            return;
        }
        self.gfx.rrect(x, y, 4, h, 2, rgb(30, 38, 45));
        let th = ((h as usize) * rows / count).max(16) as i32;
        let ty = y + ((h - th) as usize * top / (count - rows)) as i32;
        self.gfx.rrect(x, ty, 4, th, 2, self.th.faint);
    }

    fn draw_page_title(&mut self, title: &str, right: Option<&str>) {
        self.gfx.text_fit(
            CONTENT_X,
            46,
            CONTENT_W - 200,
            26,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            title,
        );
        if let Some(r) = right {
            if !r.is_empty() {
                self.gfx.text_fit(
                    CONTENT_R,
                    48,
                    240,
                    14,
                    FontWeight::Regular,
                    self.th.dim,
                    Align::Right,
                    r,
                );
            }
        }
    }

    fn draw_chrome(&mut self) {
        let tabs = [
            ("Overview", Icon::Vita, ActiveTab::Dashboard),
            ("Shares", Icon::Folder, ActiveTab::Shares),
            ("Files", Icon::Receive, ActiveTab::FolderPicker),
            ("Settings", Icon::Settings, ActiveTab::Settings),
        ];

        self.gfx.clear(self.th.bg);
        self.gfx.rect(0, 0, SIDEBAR_W, HINT_Y, self.th.sidebar);
        self.gfx.rect(0, HINT_Y, GFX_W, GFX_H - HINT_Y, self.th.sidebar);
        if self.th.divider {
            self.gfx.rect(SIDEBAR_W - 1, 0, 1, HINT_Y, self.th.border);
            self.gfx.rect(0, HINT_Y, GFX_W, 1, self.th.border);
        }

        // Top-left VitaSend-style rounded logo + title
        self.gfx.rrect(20, 22, 40, 40, 12, self.th.accent);
        self.gfx.icon(Icon::Logo, 40.0, 42.0, 34.0, rgb(255, 255, 255));
        self.gfx.text(
            70,
            42,
            21,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            "VitaSync",
        );

        // Sidebar tabs
        for (i, &(name, icon, tab)) in tabs.iter().enumerate() {
            let y = 90 + (i as i32) * 50;
            let on = self.active_tab == tab;
            if on {
                self.gfx.rrect(12, y, SIDEBAR_W - 24, 44, 12, self.th.accent_bg);
                self.gfx
                    .rrect_outline(12, y, SIDEBAR_W - 24, 44, 12, 1, self.th.accent);
            }
            let fg = if on { self.th.accent_hi } else { self.th.dim };
            self.gfx.icon(icon, 38.0, (y + 22) as f32, 24.0, fg);
            self.gfx
                .text(62, y + 22, 17, FontWeight::Medium, fg, Align::Left, name);
        }

        // Sidebar mini transfer card (like VitaSend xfer card)
        let card_y = HINT_Y - 150;
        self.gfx.rrect(12, card_y, SIDEBAR_W - 24, 82, 12, self.th.card);
        self.gfx
            .rrect_outline(12, card_y, SIDEBAR_W - 24, 82, 12, 1, self.th.border);
        let xfer_title = if self.sync_in_progress {
            "Syncing BEP/1.0..."
        } else {
            "Last sync session"
        };
        let peer_short = self.peer_label.clone();
        let pct = self.progress_pct.min(100);
        self.gfx.text(
            24,
            card_y + 20,
            13,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            xfer_title,
        );
        self.gfx.text_fit(
            24,
            card_y + 42,
            SIDEBAR_W - 48,
            14,
            FontWeight::Medium,
            self.th.text,
            Align::Left,
            &peer_short,
        );
        let accent_col = self.th.accent;
        self.draw_progress(24, card_y + 58, SIDEBAR_W - 88, 8, pct, accent_col);
        let pct_str = format!("{}%", pct);
        self.gfx.text(
            SIDEBAR_W - 24,
            card_y + 62,
            12,
            FontWeight::Medium,
            self.th.dim,
            Align::Right,
            &pct_str,
        );

        // Sidebar bottom Wi-Fi & version status
        let base = HINT_Y - 18;
        let wifi_ok = !self.peer_label.contains("Scanning");
        self.gfx.icon(
            Icon::Wifi,
            30.0,
            (base - 4) as f32,
            18.0,
            if wifi_ok { C_GOOD } else { C_WARN },
        );
        let net_txt = if wifi_ok { "BEP/1.0 TLS 1.3" } else { "UDP 21027..." };
        self.gfx.text_fit(
            48,
            base,
            SIDEBAR_W - 60,
            14,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            net_txt,
        );
        self.gfx.text(
            20,
            base - 28,
            12,
            FontWeight::Regular,
            self.th.faint,
            Align::Left,
            "v0.1.0 · Pure Rust",
        );
    }

    fn draw_hints(&mut self, items: &[(PadButton, Option<PadButton>, &str)]) {
        self.gfx.rect(0, HINT_Y, GFX_W, GFX_H - HINT_Y, self.th.sidebar);
        if self.th.divider {
            self.gfx.rect(0, HINT_Y, GFX_W, 1, self.th.border);
        }
        let cy = HINT_Y + 22;
        let size = 24;
        let mut x = 22;
        for &(b1, b2, label) in items {
            let w1 = Gfx::pad_button_width(b1, size);
            self.gfx.pad_button(b1, x + w1 / 2, cy, size);
            x += w1 + 4;
            if let Some(sec) = b2 {
                let w2 = Gfx::pad_button_width(sec, size);
                self.gfx.pad_button(sec, x + w2 / 2, cy, size);
                x += w2 + 4;
            }
            x += 5;
            x += self.gfx.text(
                x,
                cy,
                15,
                FontWeight::Medium,
                rgb(176, 186, 198),
                Align::Left,
                label,
            ) + 22;
        }
    }

    fn draw_toast(&mut self) {
        if let Some((ref msg, ticks)) = self.toast_msg.clone() {
            let size = 16;
            let maxw = 700;
            let tw = self.gfx.text_width(size, FontWeight::Medium, msg).min(maxw);
            let w = tw + 48;
            let h = 42;
            let x = (GFX_W - w) / 2;
            let y = HINT_Y - 18 - h;
            self.gfx.shadow(x, y, w, h, 21);
            self.gfx.rrect(x, y, w, h, 21, rgb(54, 63, 74));
            self.gfx.text_fit(
                GFX_W / 2,
                y + h / 2,
                maxw,
                size,
                FontWeight::Medium,
                self.th.text,
                Align::Center,
                msg,
            );
            if ticks <= 1 {
                self.toast_msg = None;
            } else {
                self.toast_msg = Some((msg.clone(), ticks - 1));
            }
        }
    }

    // ============================================================== Render Frame

    pub fn render(&mut self) {
        if self.oled_saver_active {
            self.render_oled_screensaver();
            self.present();
            return;
        }

        self.draw_chrome();

        match self.active_tab {
            ActiveTab::Dashboard => self.render_dashboard_tab(),
            ActiveTab::Shares => self.render_shares_tab(),
            ActiveTab::FolderPicker => self.render_folder_picker_tab(),
            ActiveTab::Settings => self.render_settings_tab(),
        }

        // Draw active modal dialog on top of dimmed screen (`ui_dialog_begin` style)
        match self.modal {
            ModalDialog::None => {}
            ModalDialog::CreateShare { selected_idx } => {
                self.render_create_share_modal(selected_idx);
            }
            ModalDialog::ManageDevices {
                share_idx,
                selected_dev_idx,
            } => {
                self.render_manage_devices_modal(share_idx, selected_dev_idx);
            }
            ModalDialog::ShowDeviceId => {
                self.render_device_id_modal();
            }
            ModalDialog::ConfirmQuit => {
                self.render_confirm_quit_modal();
            }
        }

        self.draw_toast();
        self.present();
    }

    /// Bouncing DVD-style **OLED Screensaver** modeled directly after `VitaSend` (`app_saver.c`)
    fn render_oled_screensaver(&mut self) {
        const SAVER_W: f32 = 410.0;
        const SAVER_H: f32 = 132.0;

        self.gfx.clear(rgb(0, 0, 0));

        self.saver_x += self.saver_vx;
        self.saver_y += self.saver_vy;
        let max_x = (GFX_W as f32) - SAVER_W - 12.0;
        let max_y = (GFX_H as f32) - SAVER_H - 12.0;
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

        let bx = self.saver_x as i32;
        let by = self.saver_y as i32;

        self.gfx.rrect(bx, by, 64, 64, 18, self.th.accent);
        self.gfx.icon(
            Icon::Logo,
            (bx + 32) as f32,
            (by + 32) as f32,
            54.0,
            rgb(255, 255, 255),
        );
        self.gfx.text(
            bx + 82,
            by + 32,
            38,
            FontWeight::Bold,
            rgb(235, 240, 245),
            Align::Left,
            "VitaSync",
        );

        let pct = self.progress_pct.min(100);
        let status_line = if self.sync_in_progress {
            format!("Syncing {}% · {}", pct, self.current_file)
        } else {
            format!("Idle ({}%) · {}", pct, self.status_line)
        };
        self.gfx.text_fit(
            bx + 2,
            by + 88,
            (SAVER_W as i32) - 8,
            16,
            FontWeight::Medium,
            self.th.accent_hi,
            Align::Left,
            &status_line,
        );
        let accent = self.th.accent;
        self.draw_progress(bx + 2, by + 104, (SAVER_W as i32) - 8, 8, pct, accent);
        self.gfx.text(
            bx + 2,
            by + 124,
            12,
            FontWeight::Regular,
            rgb(96, 106, 118),
            Align::Left,
            "Touch screen or press any button to wake",
        );
    }

    fn render_dashboard_tab(&mut self) {
        let active_count = self.shares.iter().filter(|s| s.enabled).count();
        let subtitle = format!(
            "{} active share(s) · {} known device(s)",
            active_count,
            self.devices.len()
        );
        self.draw_page_title("Overview", Some(&subtitle));

        // 1. "This device" hero card
        self.draw_card(CONTENT_X, CONTENT_Y, CONTENT_W, 152, true);
        let lx = CONTENT_X + 24;
        self.gfx
            .rrect(lx, CONTENT_Y + 24, 76, 76, 20, self.th.accent_bg);
        self.gfx.icon(
            Icon::Vita,
            (lx + 38) as f32,
            (CONTENT_Y + 62) as f32,
            60.0,
            self.th.accent_hi,
        );

        let tx = lx + 100;
        let (chip_txt, chip_col) = if self.sync_in_progress {
            ("Syncing", self.th.accent_hi)
        } else if self.peer_label.contains("Scanning") {
            ("Scanning", C_WARN)
        } else {
            ("Ready", C_GOOD)
        };
        let cw = self.draw_chip(CONTENT_R - 22, CONTENT_Y + 26, chip_txt, chip_col);
        let avail = CONTENT_R - 24 - tx - cw;

        self.gfx.text(
            tx,
            CONTENT_Y + 24,
            13,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            "This device",
        );
        self.gfx.text_fit(
            tx,
            CONTENT_Y + 52,
            avail,
            24,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            "PlayStation Vita",
        );

        let peer_str = format!("Peer: {} · Folders: {}", self.peer_label, self.folder_label);
        self.gfx.text_fit(
            tx,
            CONTENT_Y + 80,
            CONTENT_R - 24 - tx,
            15,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            &peer_str,
        );

        let st_line = self.status_line.clone();
        self.gfx.text_fit(
            tx,
            CONTENT_Y + 104,
            CONTENT_R - 24 - tx,
            15,
            FontWeight::Medium,
            self.th.accent_hi,
            Align::Left,
            &st_line,
        );

        let act_sync_w = self.action_width(PadButton::Cross, "Sync Now");
        let act_qr_w = self.action_width(PadButton::Circle, "QR / ID");
        let bx_sync = CONTENT_R - 18 - act_sync_w;
        let bx_qr = bx_sync - 10 - act_qr_w;

        self.gfx.text(
            lx,
            CONTENT_Y + 134,
            13,
            FontWeight::Regular,
            self.th.faint,
            Align::Left,
            "Device ID",
        );
        let dev_id = self.vita_device_id.clone();
        self.gfx.text_fit(
            lx + 82,
            CONTENT_Y + 134,
            bx_qr - 14 - (lx + 82),
            13,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            &dev_id,
        );

        self.draw_action(bx_qr, CONTENT_Y + 128, PadButton::Circle, "QR / ID");
        self.draw_action(bx_sync, CONTENT_Y + 128, PadButton::Cross, "Sync Now");

        // 2. "Recent activity & Statistics" card
        let ry = CONTENT_Y + 166;
        let rh = CONTENT_B - ry;
        self.draw_card(CONTENT_X, ry, CONTENT_W, rh, false);
        self.gfx.text(
            lx,
            ry + 24,
            16,
            FontWeight::Medium,
            self.th.text,
            Align::Left,
            "Recent activity",
        );

        let stats_chip = format!(
            "Delta OK: {}  ·  DL: {}  ·  UL: {}  ·  {}",
            self.files_up_to_date,
            self.files_downloaded,
            self.blocks_uploaded,
            fmt_size(self.bytes_transferred)
        );
        self.draw_chip(CONTENT_R - 22, ry + 24, &stats_chip, self.th.accent_hi);

        let max_rows = 6usize;
        let start = self.logs.len().saturating_sub(max_rows);
        let recent = &self.logs[start..];

        if recent.is_empty() {
            self.gfx.text(
                lx,
                ry + 68,
                15,
                FontWeight::Regular,
                self.th.faint,
                Align::Left,
                "No synchronization events recorded yet.",
            );
        } else {
            for (i, line) in recent.iter().rev().enumerate() {
                let cy = ry + 58 + (i as i32) * 30;
                let (icon, col) = if line.contains("Error") || line.contains("Failed") {
                    (Icon::Close, C_BAD)
                } else if line.contains("Block uploaded") || line.contains("Local index") {
                    (Icon::ArrowUp, self.th.accent_hi)
                } else if line.contains("Synced:") {
                    (Icon::ArrowDown, C_GOOD)
                } else if line.contains("up-to-date") || line.contains("Sync OK") {
                    (Icon::Check, C_GOOD)
                } else {
                    (Icon::Refresh, self.th.dim)
                };

                self.gfx
                    .circle((lx + 12) as f32, cy as f32, 12.0, rgba(255, 255, 255, 14));
                self.gfx.icon(icon, (lx + 12) as f32, cy as f32, 16.0, col);
                self.gfx.text_fit(
                    lx + 36,
                    cy,
                    CONTENT_W - 84,
                    14,
                    FontWeight::Regular,
                    if col == C_BAD { C_BAD } else { self.th.text },
                    Align::Left,
                    line,
                );
            }
        }

        self.draw_hints(&[
            (PadButton::Cross, None, "Sync Now"),
            (PadButton::Circle, None, "Device ID / QR"),
            (PadButton::Square, None, "Pause/Resume"),
            (PadButton::Triangle, None, "Pick Folder"),
            (PadButton::L, Some(PadButton::R), "Tabs"),
            (PadButton::Select, None, "Screensaver"),
        ]);
    }

    fn render_shares_tab(&mut self) {
        self.draw_page_title("Shared Folders", None);

        // Top-right "+ New Share" action button (Circle)
        let w_new = self.action_width(PadButton::Circle, "+ New Share");
        self.draw_action(CONTENT_R - w_new, 46, PadButton::Circle, "+ New Share");

        if self.shares.is_empty() {
            self.draw_card(CONTENT_X, CONTENT_Y, CONTENT_W, 150, false);
            self.gfx.text(
                CONTENT_X + 28,
                CONTENT_Y + 46,
                18,
                FontWeight::Medium,
                self.th.text,
                Align::Left,
                "No shared folders configured yet",
            );
            self.gfx.text(
                CONTENT_X + 28,
                CONTENT_Y + 78,
                15,
                FontWeight::Regular,
                self.th.dim,
                Align::Left,
                "Press Circle (+ New Share) to create a share on your PS Vita and link a device,",
            );
            self.gfx.text(
                CONTENT_X + 28,
                CONTENT_Y + 104,
                15,
                FontWeight::Regular,
                self.th.dim,
                Align::Left,
                "or press Overview -> Sync Now to import folders shared by a remote peer.",
            );
        } else {
            for i in 0..self.shares.len().min(4) {
                let ry = CONTENT_Y + (i as i32) * 98;
                let is_sel = i == self.selected_share_idx;
                let share = self.shares[i].clone();
                let devs_summary = self.share_devices_summary(&share);

                self.draw_card(CONTENT_X, ry, CONTENT_W, 90, is_sel);

                let lx = CONTENT_X + 18;
                self.gfx.rrect(lx, ry + 19, 52, 52, 14, self.th.accent_bg);
                self.gfx.icon(
                    Icon::Folder,
                    (lx + 26) as f32,
                    (ry + 45) as f32,
                    28.0,
                    if share.enabled {
                        self.th.accent_hi
                    } else {
                        self.th.dim
                    },
                );

                // Action buttons on the right: [X Devices] [SQ Pause] [TRI Folder]
                let w_folder = self.action_width(PadButton::Triangle, "Folder");
                let w_toggle = self.action_width(
                    PadButton::Square,
                    if share.enabled { "Pause" } else { "Enable" },
                );
                let w_devs = self.action_width(PadButton::Cross, "Devices");

                let bx_folder = CONTENT_R - 14 - w_folder;
                let bx_toggle = bx_folder - 8 - w_toggle;
                let bx_devs = bx_toggle - 8 - w_devs;

                self.draw_action(bx_devs, ry + 56, PadButton::Cross, "Devices");
                self.draw_action(
                    bx_toggle,
                    ry + 56,
                    PadButton::Square,
                    if share.enabled { "Pause" } else { "Enable" },
                );
                self.draw_action(bx_folder, ry + 56, PadButton::Triangle, "Folder");

                let status_col = if share.enabled { C_GOOD } else { C_WARN };
                let status_txt = if share.enabled { "Active" } else { "Paused" };
                let chip_w = self.draw_chip(CONTENT_R - 14, ry + 22, status_txt, status_col);

                let tx = lx + 66;
                let title = format!("{}  ·  {}", share.label, share.folder_id);
                self.gfx.text_fit(
                    tx,
                    ry + 22,
                    CONTENT_R - 24 - chip_w - tx,
                    17,
                    FontWeight::Bold,
                    self.th.text,
                    Align::Left,
                    &title,
                );

                let max_sub_w = bx_devs - tx - 12;
                self.gfx.text_fit(
                    tx,
                    ry + 46,
                    max_sub_w,
                    13,
                    FontWeight::Medium,
                    self.th.accent_hi,
                    Align::Left,
                    &share.target_path,
                );

                let dev_col = if share.shared_devices.is_empty() {
                    C_WARN
                } else {
                    self.th.dim
                };
                self.gfx.text_fit(
                    tx,
                    ry + 68,
                    max_sub_w,
                    13,
                    FontWeight::Regular,
                    dev_col,
                    Align::Left,
                    &devs_summary,
                );
            }
        }

        self.draw_hints(&[
            (PadButton::Circle, None, "+ New Share"),
            (PadButton::Cross, None, "Add/Edit Devices"),
            (PadButton::Square, None, "Pause/Enable"),
            (PadButton::Triangle, None, "Folder Path"),
            (PadButton::L, Some(PadButton::R), "Tabs"),
        ]);
    }

    fn render_folder_picker_tab(&mut self) {
        let (page_title, share_name) = if self.picker_mode == PickerMode::CreateNewShare {
            (
                "Create Share from Folder",
                "Pick any ux0: folder".to_owned(),
            )
        } else {
            let s = self
                .shares
                .get(self.selected_share_idx)
                .map(|s| format!("For {} ({})", s.label, s.folder_id))
                .unwrap_or_else(|| "Default share".to_owned());
            ("Choose Target Folder", s)
        };

        self.draw_page_title(page_title, Some(&share_name));

        // Top Breadcrumb bar card
        self.draw_card(CONTENT_X, 80, CONTENT_W, 42, false);
        self.gfx.icon(
            Icon::Folder,
            (CONTENT_X + 26) as f32,
            101.0,
            22.0,
            self.th.accent_hi,
        );

        let uw = self.action_width(PadButton::Circle, "Up");
        self.draw_action(CONTENT_R - 10 - uw, 101, PadButton::Circle, "Up");
        let mut right = CONTENT_R - 18 - uw;

        if !self.picker_items.is_empty() {
            let cnt = format!(
                "{} / {}",
                self.selected_picker_idx + 1,
                self.picker_items.len()
            );
            right -= self.gfx.text(
                right,
                101,
                14,
                FontWeight::Regular,
                self.th.dim,
                Align::Right,
                &cnt,
            ) + 14;
        }

        let cur_dir = self.picker_current_dir.clone();
        self.gfx.text_fit(
            CONTENT_X + 50,
            101,
            right - CONTENT_X - 58,
            16,
            FontWeight::Medium,
            self.th.text,
            Align::Left,
            &cur_dir,
        );

        self.clamp_picker_scroll();
        let top = self.picker_top_idx;
        let total = self.picker_items.len();

        for i in 0..BROWSER_ROWS {
            let idx = top + i;
            if idx >= total {
                break;
            }
            let y = BROWSER_TOP + (i as i32) * BROWSER_STEP;
            let cy = y + 19;
            let item = self.picker_items[idx].clone();
            let cur = idx == self.selected_picker_idx;

            if cur {
                self.gfx
                    .rrect(CONTENT_X, y, CONTENT_W - 14, 38, 10, self.th.accent_bg);
                self.gfx.rrect(CONTENT_X, y + 8, 4, 22, 2, self.th.accent);
            }

            let tx = CONTENT_X + 22;
            let ic = if item.is_preset {
                Icon::Plus
            } else {
                Icon::Folder
            };
            let ic_col = if item.is_preset {
                C_GOOD
            } else {
                self.th.accent_hi
            };
            self.gfx.icon(ic, (tx + 12) as f32, cy as f32, 22.0, ic_col);

            let badge = if item.is_preset { "Select" } else { "Directory" };
            let bw = self.draw_chip(CONTENT_R - 26, cy, badge, ic_col);

            self.gfx.text_fit(
                tx + 34,
                cy,
                CONTENT_W - 60 - bw,
                16,
                if cur {
                    FontWeight::Medium
                } else {
                    FontWeight::Regular
                },
                self.th.text,
                Align::Left,
                &item.label,
            );
        }

        self.draw_scrollbar(
            CONTENT_R - 6,
            BROWSER_TOP,
            (BROWSER_ROWS as i32) * BROWSER_STEP - 4,
            top,
            BROWSER_ROWS,
            total,
        );

        self.draw_hints(&[
            (PadButton::DpadUd, None, "Move"),
            (
                PadButton::Cross,
                None,
                if self.picker_mode == PickerMode::CreateNewShare {
                    "Create Share Here"
                } else {
                    "Select Target"
                },
            ),
            (PadButton::Triangle, None, "Open Subfolder"),
            (PadButton::Circle, None, "Up / Back"),
            (PadButton::L, Some(PadButton::R), "Tabs"),
        ]);
    }

    fn render_settings_tab(&mut self) {
        self.draw_page_title("Settings & Log", Some("Display & BEP/1.0"));

        // 1. Display & Theme Settings Card
        self.draw_card(CONTENT_X, CONTENT_Y, CONTENT_W, 118, true);
        let lx = CONTENT_X + 24;
        self.gfx.text(
            lx,
            CONTENT_Y + 22,
            12,
            FontWeight::Bold,
            self.th.accent_hi,
            Align::Left,
            "DISPLAY AND OLED SAVER",
        );

        let theme_desc = match self.theme_mode {
            ThemeMode::Dark => "Dark — VitaSend slate blue-grey look with teal accent",
            ThemeMode::Oled => "OLED — Pure #000000 black background (saves PS Vita OLED battery)",
        };
        self.gfx.text(
            lx,
            CONTENT_Y + 52,
            16,
            FontWeight::Medium,
            self.th.text,
            Align::Left,
            "Colour Theme",
        );
        self.gfx.text_fit(
            lx,
            CONTENT_Y + 76,
            CONTENT_W - 220,
            14,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            theme_desc,
        );

        let w_theme = self.action_width(PadButton::Cross, "Switch Theme");
        let w_qr = self.action_width(PadButton::Circle, "QR / ID");
        let bx_theme = CONTENT_R - 20 - w_theme;
        let bx_qr = bx_theme - 10 - w_qr;
        self.draw_action(bx_qr, CONTENT_Y + 62, PadButton::Circle, "QR / ID");
        self.draw_action(bx_theme, CONTENT_Y + 62, PadButton::Cross, "Switch Theme");

        self.gfx.text(
            lx,
            CONTENT_Y + 102,
            13,
            FontWeight::Regular,
            self.th.faint,
            Align::Left,
            "OLED Screensaver activates automatically after 12s during sync (or press SELECT anytime).",
        );

        // 2. Full Protocol Event Log Card
        let ly = CONTENT_Y + 132;
        let lh = CONTENT_B - ly;
        self.draw_card(CONTENT_X, ly, CONTENT_W, lh, false);
        self.gfx.text(
            lx,
            ly + 22,
            12,
            FontWeight::Bold,
            self.th.accent_hi,
            Align::Left,
            "BEP/1.0 & TLS 1.3 PROTOCOL LOG",
        );

        for (i, line) in self.logs.iter().rev().take(10).enumerate() {
            let y = ly + 48 + (i as i32) * 22;
            let col = if line.contains("Error") || line.contains("Failed") {
                C_BAD
            } else if line.contains("Synced")
                || line.contains("Connected")
                || line.contains("Sync OK")
                || line.contains("up-to-date")
            {
                C_GOOD
            } else {
                self.th.dim
            };
            self.gfx.text_fit(
                lx,
                y,
                CONTENT_W - 48,
                13,
                FontWeight::Regular,
                col,
                Align::Left,
                line,
            );
        }

        self.draw_hints(&[
            (PadButton::Cross, None, "Switch Theme (Dark/OLED)"),
            (PadButton::Circle, None, "Device ID / QR"),
            (PadButton::Select, None, "OLED Screensaver"),
            (PadButton::L, Some(PadButton::R), "Tabs"),
            (PadButton::Start, None, "Quit"),
        ]);
    }

    // ============================================================== VitaSend Modal Dialogs

    /// Modal Dialog 1: Create a new share on PS Vita (from template or custom `ux0:` folder)
    fn render_create_share_modal(&mut self, selected_idx: usize) {
        self.gfx.dim(175);
        let w = 680;
        let h = 408;
        let dx = (GFX_W - w) / 2;
        let dy = (HINT_Y - h) / 2;

        self.gfx.shadow(dx, dy, w, h, 18);
        self.gfx.rrect(dx, dy, w, h, 18, self.th.card);
        self.gfx.rrect_outline(dx, dy, w, h, 18, 1, self.th.border);

        let lx = dx + 28;
        self.gfx
            .circle((lx + 20) as f32, (dy + 40) as f32, 20.0, self.th.accent_bg);
        self.gfx.icon(
            Icon::Plus,
            (lx + 20) as f32,
            (dy + 40) as f32,
            22.0,
            self.th.accent_hi,
        );
        self.gfx.text(
            lx + 52,
            dy + 32,
            21,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            "Create New Share",
        );
        self.gfx.text(
            lx + 52,
            dy + 54,
            14,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            "Choose a PS Vita folder preset or browse ux0: to create a new Syncthing share",
        );
        self.gfx.line(
            (dx + 24) as f32,
            (dy + 76) as f32,
            (dx + w - 24) as f32,
            (dy + 76) as f32,
            1.0,
            self.th.border,
        );

        for (i, &(fid, label, path)) in NEW_SHARE_TEMPLATES.iter().enumerate() {
            let ry = dy + 90 + (i as i32) * 50;
            let cy = ry + 21;
            let is_sel = i == selected_idx;

            if is_sel {
                self.gfx
                    .rrect(lx, ry, w - 56, 44, 10, self.th.accent_bg);
                self.gfx.rrect(lx, ry + 9, 4, 26, 2, self.th.accent);
            }

            let is_browse = fid == "__browse__";
            let ic = if is_browse {
                Icon::Receive
            } else {
                Icon::Folder
            };
            self.gfx.icon(
                ic,
                (lx + 24) as f32,
                cy as f32,
                22.0,
                if is_browse { C_GOOD } else { self.th.accent_hi },
            );

            self.gfx.text(
                lx + 48,
                cy - 7,
                16,
                if is_sel {
                    FontWeight::Bold
                } else {
                    FontWeight::Medium
                },
                self.th.text,
                Align::Left,
                label,
            );
            let sub = if is_browse {
                "Open the interactive ux0: directory browser".to_owned()
            } else {
                format!("ID: {}  ·  Path: {}", fid, path)
            };
            self.gfx.text(
                lx + 48,
                cy + 12,
                13,
                FontWeight::Regular,
                self.th.dim,
                Align::Left,
                &sub,
            );
        }

        self.draw_hints(&[
            (PadButton::DpadUd, None, "Select"),
            (PadButton::Cross, None, "Create & Pick Devices"),
            (PadButton::Circle, None, "Cancel"),
        ]);
    }

    /// Modal Dialog 2: Add or remove Syncthing devices for a share (`shares[share_idx]`)
    fn render_manage_devices_modal(&mut self, share_idx: usize, selected_dev_idx: usize) {
        self.gfx.dim(175);
        let w = 680;
        let h = 390;
        let dx = (GFX_W - w) / 2;
        let dy = (HINT_Y - h) / 2;

        self.gfx.shadow(dx, dy, w, h, 18);
        self.gfx.rrect(dx, dy, w, h, 18, self.th.card);
        self.gfx.rrect_outline(dx, dy, w, h, 18, 1, self.th.border);

        let share_title = self
            .shares
            .get(share_idx)
            .map(|s| format!("{} ({})", s.label, s.folder_id))
            .unwrap_or_else(|| "Share".to_owned());
        let shared_list = self
            .shares
            .get(share_idx)
            .map(|s| s.shared_devices.clone())
            .unwrap_or_default();

        let lx = dx + 28;
        self.gfx
            .circle((lx + 20) as f32, (dy + 40) as f32, 20.0, self.th.accent_bg);
        self.gfx.icon(
            Icon::Desktop,
            (lx + 20) as f32,
            (dy + 40) as f32,
            22.0,
            self.th.accent_hi,
        );
        self.gfx.text_fit(
            lx + 52,
            dy + 32,
            w - 110,
            20,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            &format!("Devices for {}", share_title),
        );
        self.gfx.text(
            lx + 52,
            dy + 54,
            14,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            "Tick the remote Syncthing devices that should synchronize this folder",
        );
        self.gfx.line(
            (dx + 24) as f32,
            (dy + 76) as f32,
            (dx + w - 24) as f32,
            (dy + 76) as f32,
            1.0,
            self.th.border,
        );

        let devices_snapshot = self.devices.clone();
        if devices_snapshot.is_empty() {
            let (headline, col) = if self.sync_in_progress {
                (
                    "Scanning LAN (UDP 21027 + TCP 22000) for Syncthing peers...",
                    self.th.accent_hi,
                )
            } else {
                ("No remote Syncthing devices discovered yet.", C_WARN)
            };
            self.gfx
                .text(lx, dy + 130, 16, FontWeight::Medium, col, Align::Left, headline);
            let sub_hint = if self.sync_in_progress {
                self.status_line.clone()
            } else {
                "Press Triangle (Scan & Sync) while Syncthing is open on your PC/Mac/Phone."
                    .to_owned()
            };
            self.gfx.text(
                lx,
                dy + 160,
                14,
                FontWeight::Regular,
                self.th.dim,
                Align::Left,
                &sub_hint,
            );
        } else {
            for (i, dev) in devices_snapshot.iter().take(5).enumerate() {
                let ry = dy + 92 + (i as i32) * 52;
                let cy = ry + 23;
                let is_sel = i == selected_dev_idx;
                let is_checked = shared_list.contains(&dev.device_id.to_hex());

                if is_sel {
                    self.gfx
                        .rrect(lx, ry, w - 56, 46, 10, self.th.accent_bg);
                    self.gfx.rrect(lx, ry + 10, 4, 26, 2, self.th.accent);
                }

                self.draw_row_tick(lx + 24, cy, is_checked);
                self.gfx.icon(
                    Icon::Laptop,
                    (lx + 54) as f32,
                    cy as f32,
                    22.0,
                    if is_checked {
                        self.th.accent_hi
                    } else {
                        self.th.dim
                    },
                );

                let badge_txt = if is_checked { "Linked" } else { "Not linked" };
                let badge_col = if is_checked { C_GOOD } else { self.th.dim };
                self.draw_chip(dx + w - 36, cy, badge_txt, badge_col);

                self.gfx.text(
                    lx + 78,
                    cy - 7,
                    16,
                    FontWeight::Bold,
                    self.th.text,
                    Align::Left,
                    &dev.name,
                );
                let sub = format!(
                    "ID: {}  ·  Address: {}",
                    dev.device_id.to_syncthing_string(),
                    dev.address
                );
                self.gfx.text_fit(
                    lx + 78,
                    cy + 12,
                    w - 240,
                    12,
                    FontWeight::Regular,
                    self.th.dim,
                    Align::Left,
                    &sub,
                );
            }
        }

        // Bottom modal action buttons
        let w_sync = self.action_width(PadButton::Triangle, "Apply & Sync");
        let w_close = self.action_width(PadButton::Circle, "Done");
        self.draw_action(
            dx + w - 28 - w_sync - 12 - w_close,
            dy + h - 32,
            PadButton::Triangle,
            "Apply & Sync",
        );
        self.draw_action(
            dx + w - 28 - w_close,
            dy + h - 32,
            PadButton::Circle,
            "Done",
        );

        self.draw_hints(&[
            (PadButton::DpadUd, None, "Select Device"),
            (PadButton::Cross, None, "Toggle Device"),
            (PadButton::Triangle, None, "Apply & Sync Now"),
            (PadButton::Circle, None, "Done"),
        ]);
    }

    /// Modal Dialog 3: Display the PS Vita's Syncthing Device ID + scannable QR code
    fn render_device_id_modal(&mut self) {
        self.gfx.dim(180);
        let w = 720;
        let h = 414;
        let dx = (GFX_W - w) / 2;
        let dy = (HINT_Y - h) / 2;

        self.gfx.shadow(dx, dy, w, h, 18);
        self.gfx.rrect(dx, dy, w, h, 18, self.th.card);
        self.gfx.rrect_outline(dx, dy, w, h, 18, 1, self.th.border);

        let lx = dx + 28;
        self.gfx
            .circle((lx + 20) as f32, (dy + 40) as f32, 20.0, self.th.accent_bg);
        self.gfx.icon(
            Icon::Vita,
            (lx + 20) as f32,
            (dy + 40) as f32,
            26.0,
            self.th.accent_hi,
        );
        self.gfx.text(
            lx + 52,
            dy + 31,
            21,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            "PS Vita Syncthing Device ID",
        );
        self.gfx.text(
            lx + 52,
            dy + 54,
            14,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            "Scan this QR code in Syncthing (Add Remote Device) or enter the 56-character ID",
        );
        self.gfx.line(
            (dx + 24) as f32,
            (dy + 76) as f32,
            (dx + w - 24) as f32,
            (dy + 76) as f32,
            1.0,
            self.th.border,
        );

        // Left Column: Crisp white QR code card (`246 x 246`)
        let qr_box_x = lx;
        let qr_box_y = dy + 94;
        let qr_box_size = 246;
        self.gfx
            .rrect(qr_box_x, qr_box_y, qr_box_size, qr_box_size, 14, rgb(255, 255, 255));
        self.gfx.rrect_outline(
            qr_box_x,
            qr_box_y,
            qr_box_size,
            qr_box_size,
            14,
            1,
            self.th.border,
        );

        let dev_id = self.vita_device_id.clone();
        if let Ok(qr) = QrCode::encode_text(&dev_id, QrCodeEcc::Medium) {
            let n = qr.size();
            let scale = ((qr_box_size - 26) / n).max(3);
            let total_px = n * scale;
            let ox = qr_box_x + (qr_box_size - total_px) / 2;
            let oy = qr_box_y + (qr_box_size - total_px) / 2;
            let dark = rgb(10, 14, 18);

            for my in 0..n {
                for mx in 0..n {
                    if qr.get_module(mx, my) {
                        self.gfx.rect(
                            ox + mx * scale,
                            oy + my * scale,
                            scale,
                            scale,
                            dark,
                        );
                    }
                }
            }
        }

        self.gfx.text(
            qr_box_x + qr_box_size / 2,
            qr_box_y + qr_box_size + 22,
            13,
            FontWeight::Medium,
            self.th.accent_hi,
            Align::Center,
            "Scan QR in Syncthing Web / Mobile UI",
        );

        // Right Column: Formatted 8-group Syncthing Device ID + Certificate details
        let rx = qr_box_x + qr_box_size + 24;
        let rw = dx + w - 28 - rx;

        self.gfx.text(
            rx,
            dy + 104,
            12,
            FontWeight::Bold,
            self.th.accent_hi,
            Align::Left,
            "SYNCTHING DEVICE ID (8 GROUPS OF 7 CHARS)",
        );

        let groups: Vec<&str> = dev_id.split('-').collect();
        let row1 = if groups.len() >= 4 {
            format!("{}  -  {}  -  {}  -  {}", groups[0], groups[1], groups[2], groups[3])
        } else {
            dev_id.clone()
        };
        let row2 = if groups.len() >= 8 {
            format!("{}  -  {}  -  {}  -  {}", groups[4], groups[5], groups[6], groups[7])
        } else {
            String::new()
        };

        // Row 1 box (Groups 1..4)
        let box1_y = dy + 120;
        self.gfx.rrect(rx, box1_y, rw, 50, 10, self.th.accent_bg);
        self.gfx
            .rrect_outline(rx, box1_y, rw, 50, 10, 1, self.th.accent);
        self.gfx.text(
            rx + 14,
            box1_y + 15,
            11,
            FontWeight::Medium,
            self.th.dim,
            Align::Left,
            "GROUPS 1 - 4",
        );
        self.gfx.text_fit(
            rx + 14,
            box1_y + 35,
            rw - 28,
            16,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            &row1,
        );

        // Row 2 box (Groups 5..8)
        let box2_y = dy + 180;
        self.gfx.rrect(rx, box2_y, rw, 50, 10, self.th.accent_bg);
        self.gfx
            .rrect_outline(rx, box2_y, rw, 50, 10, 1, self.th.accent);
        self.gfx.text(
            rx + 14,
            box2_y + 15,
            11,
            FontWeight::Medium,
            self.th.dim,
            Align::Left,
            "GROUPS 5 - 8",
        );
        self.gfx.text_fit(
            rx + 14,
            box2_y + 35,
            rw - 28,
            16,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            &row2,
        );

        // Device metadata card
        let meta_y = dy + 244;
        self.gfx.rrect(rx, meta_y, rw, 96, 10, self.th.bg);
        self.gfx
            .rrect_outline(rx, meta_y, rw, 96, 10, 1, self.th.border);

        let short_id = groups.first().copied().unwrap_or("VITA");
        self.draw_chip(rx + rw - 14, meta_y + 22, "TLS 1.3 · ECDSA P-256", C_GOOD);
        self.gfx.text(
            rx + 14,
            meta_y + 22,
            14,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            &format!("Short ID: {}", short_id),
        );
        self.gfx.text(
            rx + 14,
            meta_y + 48,
            13,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            "Device Name: PlayStation Vita (vita-syncthing)",
        );
        self.gfx.text(
            rx + 14,
            meta_y + 72,
            13,
            FontWeight::Regular,
            self.th.faint,
            Align::Left,
            "Identity Cert: ux0:data/syncthing/cert.der",
        );

        // Close button at bottom-right of modal
        let w_close = self.action_width(PadButton::Circle, "Close");
        self.draw_action(
            dx + w - 28 - w_close,
            dy + h - 32,
            PadButton::Circle,
            "Close",
        );

        self.draw_hints(&[
            (PadButton::Circle, Some(PadButton::Cross), "Close QR / Device ID"),
        ]);
    }

    /// Modal Dialog 4: Confirmation before exiting VitaSync (`START` button)
    fn render_confirm_quit_modal(&mut self) {
        self.gfx.dim(180);
        let w = 520;
        let h = 236;
        let dx = (GFX_W - w) / 2;
        let dy = (HINT_Y - h) / 2;

        self.gfx.shadow(dx, dy, w, h, 18);
        self.gfx.rrect(dx, dy, w, h, 18, self.th.card);
        self.gfx.rrect_outline(dx, dy, w, h, 18, 1, self.th.border);

        let lx = dx + 28;
        self.gfx
            .circle((lx + 20) as f32, (dy + 40) as f32, 20.0, rgba(239, 83, 80, 42));
        self.gfx
            .icon(Icon::Close, (lx + 20) as f32, (dy + 40) as f32, 22.0, C_BAD);
        self.gfx.text(
            lx + 52,
            dy + 31,
            21,
            FontWeight::Bold,
            self.th.text,
            Align::Left,
            "Quit VitaSync?",
        );
        self.gfx.text(
            lx + 52,
            dy + 54,
            14,
            FontWeight::Regular,
            self.th.dim,
            Align::Left,
            "Confirm whether to close the application and return to LiveArea",
        );
        self.gfx.line(
            (dx + 24) as f32,
            (dy + 76) as f32,
            (dx + w - 24) as f32,
            (dy + 76) as f32,
            1.0,
            self.th.border,
        );

        if self.sync_in_progress {
            self.gfx.text(
                lx,
                dy + 110,
                15,
                FontWeight::Medium,
                C_WARN,
                Align::Left,
                "A BEP/1.0 synchronization is currently in progress.",
            );
            self.gfx.text(
                lx,
                dy + 136,
                14,
                FontWeight::Regular,
                self.th.dim,
                Align::Left,
                "Quitting now will interrupt active file transfers.",
            );
        } else {
            self.gfx.text(
                lx,
                dy + 112,
                15,
                FontWeight::Medium,
                self.th.text,
                Align::Left,
                "Are you sure you want to exit VitaSync?",
            );
            self.gfx.text(
                lx,
                dy + 138,
                14,
                FontWeight::Regular,
                self.th.dim,
                Align::Left,
                "All share configurations and device settings are saved on ux0:.",
            );
        }

        let w_quit = self.action_width(PadButton::Cross, "Quit App");
        let w_cancel = self.action_width(PadButton::Circle, "Cancel");
        self.draw_action(
            dx + w - 28 - w_quit - 12 - w_cancel,
            dy + h - 34,
            PadButton::Circle,
            "Cancel",
        );
        self.draw_action(
            dx + w - 28 - w_quit,
            dy + h - 34,
            PadButton::Cross,
            "Quit App",
        );

        self.draw_hints(&[
            (PadButton::Cross, None, "Confirm Quit"),
            (PadButton::Circle, None, "Cancel"),
        ]);
    }

    fn present(&self) {
        #[cfg(target_os = "vita")]
        unsafe {
            if !self.fb_ptr.is_null() {
                std::ptr::copy_nonoverlapping(
                    self.gfx.pixels.as_ptr(),
                    self.fb_ptr,
                    SCREEN_W * SCREEN_H,
                );
                let _ = display_ffi::sceDisplayWaitVblankStart();
            }
        }
        let _ = (FB_ALIGNED_SIZE, Path::new(""));
    }
}

impl Drop for VitaDashboard {
    fn drop(&mut self) {
        #[cfg(target_os = "vita")]
        let _ = self.mem_uid;
    }
}
