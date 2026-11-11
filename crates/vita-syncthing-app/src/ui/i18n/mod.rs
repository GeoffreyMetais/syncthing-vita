//! Internationalization (i18n) and multi-language catalog for PS Vita Syncthing.
//!
//! Provides zero-allocation static string tables for English, French, Spanish,
//! German, and Italian, as well as automatic system language detection from VitaSDK.

pub mod de;
pub mod en;
pub mod es;
pub mod fr;
pub mod it;

use crate::domain::Language;

pub use de::STRINGS_DE;
pub use en::STRINGS_EN;
pub use es::STRINGS_ES;
pub use fr::STRINGS_FR;
pub use it::STRINGS_IT;

/// Strongly-typed string catalog containing all UI labels, actions, hints, and screen titles.
#[derive(Debug, Clone, Copy)]
pub struct Strings {
    pub language: Language,

    // Common Actions
    pub action_cancel: &'static str,
    pub action_save: &'static str,
    pub action_delete: &'static str,
    pub action_done: &'static str,
    pub action_close: &'static str,
    pub action_confirm: &'static str,
    pub action_quit: &'static str,
    pub action_quit_app: &'static str,
    pub action_back: &'static str,
    pub action_accept: &'static str,
    pub action_decline: &'static str,
    pub action_later: &'static str,
    pub action_up: &'static str,
    pub action_open: &'static str,
    pub action_share: &'static str,
    pub action_shared: &'static str,
    pub action_linked: &'static str,
    pub action_not_linked: &'static str,
    pub action_pause: &'static str,
    pub action_enable: &'static str,
    pub action_devices: &'static str,
    pub action_folder: &'static str,
    pub action_folder_path: &'static str,
    pub action_add_by_id: &'static str,
    pub action_add_device: &'static str,
    pub action_add_device_id: &'static str,
    pub action_add_remote_device: &'static str,
    pub action_apply_sync: &'static str,
    pub action_scan_qr: &'static str,
    pub action_new_share: &'static str,
    pub action_sync_now: &'static str,
    pub action_qr_id: &'static str,
    pub action_switch_theme: &'static str,
    pub action_clear_declined: &'static str,
    pub action_virtual_keyboard: &'static str,
    pub action_hide_keyboard: &'static str,
    pub action_backspace: &'static str,

    // Status Badges & Chips
    pub status_ready: &'static str,
    pub status_syncing: &'static str,
    pub status_scanning: &'static str,
    pub status_active: &'static str,
    pub status_paused: &'static str,

    // Navigation & Sidebar
    pub tab_overview: &'static str,
    pub tab_shares: &'static str,
    pub tab_files: &'static str,
    pub tab_settings: &'static str,
    pub app_title: &'static str,
    pub app_version_label: &'static str,
    pub sidebar_syncing: &'static str,
    pub sidebar_last_sync: &'static str,
    pub protocol_tls: &'static str,
    pub protocol_discovery: &'static str,

    // Controller Hints
    pub hint_navigate: &'static str,
    pub hint_select: &'static str,
    pub hint_toggle: &'static str,
    pub hint_back: &'static str,
    pub hint_tabs: &'static str,
    pub hint_quit: &'static str,
    pub hint_move: &'static str,
    pub hint_screensaver: &'static str,
    pub hint_folder: &'static str,
    pub hint_folder_path: &'static str,
    pub hint_share_folder: &'static str,
    pub hint_select_target: &'static str,
    pub hint_open_subfolder: &'static str,
    pub hint_up_one_level: &'static str,
    pub hint_up_back: &'static str,
    pub hint_back_to_device_id: &'static str,
    pub hint_pause_enable: &'static str,
    pub hint_add_by_id: &'static str,
    pub hint_scan_qr: &'static str,
    pub hint_apply_sync: &'static str,
    pub hint_confirm_delete: &'static str,
    pub hint_confirm_quit: &'static str,
    pub hint_create_pick_devices: &'static str,
    pub hint_add_remote_device_id: &'static str,
    pub hint_close_qr_device_id: &'static str,
    pub hint_accept_share: &'static str,
    pub hint_confirm_add: &'static str,
    pub hint_type_key: &'static str,

    // Screen 1: Dashboard (Overview)
    pub dashboard_title: &'static str,
    pub hero_this_device: &'static str,
    pub hero_device_name: &'static str,
    pub hero_device_id_label: &'static str,
    pub section_recent_activity: &'static str,
    pub msg_no_sync_events: &'static str,

    // Screen 2: Shared Folders
    pub shares_title: &'static str,
    pub msg_no_shares: &'static str,
    pub msg_no_shares_hint_1: &'static str,
    pub msg_no_shares_hint_2: &'static str,

    // Screen 3: Files & Folder Picker
    pub picker_title_create: &'static str,
    pub picker_subtitle_create: &'static str,
    pub picker_title_choose: &'static str,
    pub picker_default_share: &'static str,

    // Screen 4: Settings & Protocol Log
    pub settings_title: &'static str,
    pub settings_subtitle: &'static str,
    pub settings_sec_display: &'static str,
    pub settings_label_colour_theme: &'static str,
    pub settings_desc_theme_dark: &'static str,
    pub settings_desc_theme_oled: &'static str,
    pub settings_saver_auto_hint: &'static str,
    pub settings_sec_log: &'static str,
    pub settings_sec_declined: &'static str,
    pub settings_no_declined: &'static str,

    // Screensaver
    pub saver_wake_hint: &'static str,

    // Modals: Create Share
    pub modal_create_share_title: &'static str,
    pub modal_create_share_subtitle: &'static str,
    pub modal_create_share_browse_sub: &'static str,

    // Modals: Manage Devices
    pub modal_manage_devices_default_title: &'static str,
    pub modal_manage_devices_subtitle_incomplete: &'static str,
    pub modal_manage_devices_subtitle: &'static str,
    pub modal_manage_devices_scanning: &'static str,
    pub modal_manage_devices_empty: &'static str,
    pub modal_manage_devices_empty_hint: &'static str,

    // Modals: Add Device
    pub modal_add_device_title: &'static str,
    pub modal_add_device_subtitle: &'static str,
    pub modal_add_device_example: &'static str,
    pub modal_add_device_tip: &'static str,
    pub modal_add_device_placeholder: &'static str,
    pub modal_add_device_name_label: &'static str,
    pub modal_add_device_name_empty: &'static str,
    pub modal_add_device_confirm: &'static str,

    // Modals: Confirm Delete
    pub modal_delete_share_title: &'static str,
    pub modal_delete_share_subtitle: &'static str,
    pub modal_delete_share_decline_hint: &'static str,

    // Modals: Confirm Quit
    pub modal_quit_title: &'static str,
    pub modal_quit_subtitle: &'static str,
    pub modal_quit_syncing_warn: &'static str,
    pub modal_quit_syncing_hint: &'static str,
    pub modal_quit_idle_ready: &'static str,
    pub modal_quit_idle_hint: &'static str,

    // Modals: Device ID
    pub modal_device_id_title: &'static str,
    pub modal_device_id_subtitle: &'static str,
    pub modal_device_id_scan_web: &'static str,
    pub modal_device_id_dev_name: &'static str,
    pub modal_device_id_cert_path: &'static str,
    pub modal_device_id_short_label: &'static str,
    pub modal_device_id_full_label: &'static str,
    pub modal_device_id_scan_hint: &'static str,

    // Modals: Folder Offer
    pub modal_folder_offer_title: &'static str,
    pub modal_folder_offer_hint: &'static str,

    // Modals: Scan QR
    pub modal_scan_qr_title: &'static str,
    pub modal_scan_qr_subtitle: &'static str,
}

impl Strings {
    /// Formats the overview subtitle: `"{active} active share(s) · {devices} known device(s)"`.
    pub fn fmt_active_shares_devices(&self, active: usize, devices: usize) -> String {
        match self.language {
            Language::French => format!("{active} partage(s) actif(s) · {devices} appareil(s) connu(s)"),
            Language::Spanish => format!("{active} carpeta(s) activa(s) · {devices} dispositivo(s) conocido(s)"),
            Language::German => format!("{active} aktive Freigabe(n) · {devices} bekannte(s) Gerät(e)"),
            Language::Italian => format!("{active} condivisione/i attiva/e · {devices} dispositivo/i noto/i"),
            Language::English => format!("{active} active share(s) · {devices} known device(s)"),
        }
    }

    /// Formats the peer and folder label string for the hero card.
    pub fn fmt_peer_and_folders(&self, peer: &str, folders: &str) -> String {
        match self.language {
            Language::French => format!("Pair : {peer} · Dossiers : {folders}"),
            Language::Spanish => format!("Par: {peer} · Carpetas: {folders}"),
            Language::German => format!("Peer: {peer} · Ordner: {folders}"),
            Language::Italian => format!("Peer: {peer} · Cartelle: {folders}"),
            Language::English => format!("Peer: {peer} · Folders: {folders}"),
        }
    }

    /// Formats the recent statistics chip on the dashboard.
    pub fn fmt_recent_stats(&self, ok: usize, dl: usize, ul: usize, bytes_str: &str) -> String {
        format!("Delta OK: {ok}  ·  DL: {dl}  ·  UL: {ul}  ·  {bytes_str}")
    }

    /// Formats a share card title: `"{label}  ·  {folder_id}"`.
    pub fn fmt_share_title(&self, label: &str, folder_id: &str) -> String {
        format!("{label}  ·  {folder_id}")
    }

    /// Formats the template subtitle: `"ID: {fid}  ·  Path: {path}"`.
    pub fn fmt_template_sub(&self, fid: &str, path: &str) -> String {
        match self.language {
            Language::French => format!("ID : {fid}  ·  Chemin : {path}"),
            Language::Spanish => format!("ID: {fid}  ·  Ruta: {path}"),
            Language::German => format!("ID: {fid}  ·  Pfad: {path}"),
            Language::Italian => format!("ID: {fid}  ·  Percorso: {path}"),
            Language::English => format!("ID: {fid}  ·  Path: {path}"),
        }
    }

    /// Formats the short ID label: `"Short ID: {short_id}"`.
    pub fn fmt_short_id_label(&self, short_id: &str) -> String {
        format!("Short ID: {short_id}")
    }

    /// Formats the target folder hint when choosing folder in picker.
    pub fn fmt_picker_target(&self, label: &str, folder_id: &str) -> String {
        match self.language {
            Language::French => format!("Pour {label} ({folder_id})"),
            Language::Spanish => format!("Para {label} ({folder_id})"),
            Language::German => format!("Für {label} ({folder_id})"),
            Language::Italian => format!("Per {label} ({folder_id})"),
            Language::English => format!("For {label} ({folder_id})"),
        }
    }

    /// Formats the item counter in the breadcrumb bar: `"{current} / {total}"`.
    pub fn fmt_item_counter(&self, current: usize, total: usize) -> String {
        format!("{current} / {total}")
    }

    /// Formats the declined folders count and list.
    pub fn fmt_declined_summary(&self, count: usize, list: &str) -> String {
        match self.language {
            Language::French => format!("{count} refusé(s) : {list}"),
            Language::Spanish => format!("{count} rechazado(s): {list}"),
            Language::German => format!("{count} abgelehnt: {list}"),
            Language::Italian => format!("{count} rifiutato/i: {list}"),
            Language::English => format!("{count} declined: {list}"),
        }
    }

    /// Formats the files-kept message when confirming share deletion.
    pub fn fmt_files_kept_on_vita(&self, path: &str) -> String {
        match self.language {
            Language::French => format!("Les fichiers dans {path} sont conservés sur la PS Vita."),
            Language::Spanish => format!("Los archivos en {path} se conservan en la PS Vita."),
            Language::German => format!("Dateien in {path} verbleiben auf der PS Vita."),
            Language::Italian => format!("I file in {path} sono conservati sulla PS Vita."),
            Language::English => format!("Files in {path} are kept on the PS Vita."),
        }
    }

    /// Formats the modal title for device management of a share.
    pub fn fmt_devices_for_share(&self, title: &str) -> String {
        match self.language {
            Language::French => format!("Appareils pour {title}"),
            Language::Spanish => format!("Dispositivos para {title}"),
            Language::German => format!("Geräte für {title}"),
            Language::Italian => format!("Dispositivi per {title}"),
            Language::English => format!("Devices for {title}"),
        }
    }

    /// Formats the folder offer requester message.
    pub fn fmt_folder_offer_sub(&self, device: &str) -> String {
        match self.language {
            Language::French => format!("{device} souhaite partager un dossier avec cette PS Vita"),
            Language::Spanish => format!("{device} quiere compartir una carpeta con esta PS Vita"),
            Language::German => format!("{device} möchte einen Ordner mit dieser PS Vita teilen"),
            Language::Italian => format!("{device} vuole condividere una cartella con questa PS Vita"),
            Language::English => format!("{device} wants to share a folder with this PS Vita"),
        }
    }

    /// Formats the pending folder offers count message.
    pub fn fmt_offers_waiting(&self, count: usize) -> String {
        match self.language {
            Language::French => format!("{count} autre(s) offre(s) en attente"),
            Language::Spanish => format!("{count} oferta(s) más en espera"),
            Language::German => format!("{count} weitere(s) Angebot(e) ausstehend"),
            Language::Italian => format!("{count} ulteriore/i offerta/e in attesa"),
            Language::English => format!("{count} more offer(s) waiting"),
        }
    }

    /// Formats the screensaver active syncing status line.
    pub fn fmt_saver_syncing(&self, pct: u32, current_file: &str) -> String {
        match self.language {
            Language::French => format!("Synchronisation {pct}% · {current_file}"),
            Language::Spanish => format!("Sincronizando {pct}% · {current_file}"),
            Language::German => format!("Synchronisiere {pct}% · {current_file}"),
            Language::Italian => format!("Sincronizzazione {pct}% · {current_file}"),
            Language::English => format!("Syncing {pct}% · {current_file}"),
        }
    }

    /// Formats the screensaver idle status line.
    pub fn fmt_saver_idle(&self, pct: u32, status_line: &str) -> String {
        match self.language {
            Language::French => format!("Inactif ({pct}%) · {status_line}"),
            Language::Spanish => format!("Inactivo ({pct}%) · {status_line}"),
            Language::German => format!("Inaktiv ({pct}%) · {status_line}"),
            Language::Italian => format!("Inattivo ({pct}%) · {status_line}"),
            Language::English => format!("Idle ({pct}%) · {status_line}"),
        }
    }
}

/// Returns the static string catalog for the given user interface language.
pub fn strings(lang: Language) -> &'static Strings {
    match lang {
        Language::English => &STRINGS_EN,
        Language::French => &STRINGS_FR,
        Language::Spanish => &STRINGS_ES,
        Language::German => &STRINGS_DE,
        Language::Italian => &STRINGS_IT,
    }
}

/// Alias for `strings(lang)`.
pub fn get_strings(lang: Language) -> &'static Strings {
    strings(lang)
}
