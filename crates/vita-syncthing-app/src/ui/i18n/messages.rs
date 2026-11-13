//! Translation of domain `UiText` messages (status line, peer / folder labels, toasts, Files rows)
//! and of the parameterised dialog labels.
//!
//! Each language file defines a `Messages` catalog of templates; `{name}` placeholders are filled
//! with the message parameters by `Strings::text`.

use std::borrow::Cow;
use std::fmt::Display;

use syncthing_core::DeviceIdParseError;

use super::Strings;
use crate::domain::UiText;

/// Message templates of one language. Placeholders are documented on each field.
#[derive(Debug)]
pub struct Messages {
    /// Initial status line.
    pub ready_tap_sync_card: &'static str,
    /// Initial peer label.
    pub ready_manual_pairing: &'static str,
    /// Peer label with no linked device.
    pub no_peer_linked: &'static str,
    /// Status line with no linked device.
    pub select_device_to_connect: &'static str,
    /// Folder label before the first ClusterConfig.
    pub no_folders_synced: &'static str,
    /// Folder label when the peer shares nothing.
    pub no_remote_folders: &'static str,
    pub starting_sync: &'static str,
    pub scanning_lan: &'static str,
    /// `{name}`, `{addr}`.
    pub connecting_to: &'static str,
    pub exchanging_index: &'static str,
    /// `{file}`.
    pub file_up_to_date: &'static str,
    /// `{file}`, `{pct}`.
    pub downloading: &'static str,
    /// `{files}`.
    pub index_published: &'static str,
    /// `{file}`.
    pub uploading: &'static str,
    /// Current file once the worker has finished.
    pub sync_finished: &'static str,
    /// One BEP connection.
    pub connections_one: &'static str,
    /// `{secondary}` secondary connections.
    pub connections_many: &'static str,
    /// `{connections}` (rendered `connections_*`), `{up_to_date}`, `{downloaded}`, `{kb}`, `{uploaded}`, `{bytes}`.
    pub sync_summary: &'static str,
    /// `{count}`.
    pub found_lan_peers: &'static str,
    pub no_peer_add_device: &'static str,
    /// `{error}`.
    pub discovery_error: &'static str,
    pub syncing_with_peers: &'static str,
    pub theme_dark: &'static str,
    pub theme_oled: &'static str,
    /// `{name}` (native language name).
    pub language_changed: &'static str,
    /// `{device}`, `{share}`.
    pub device_linked: &'static str,
    /// `{device}`, `{share}`.
    pub device_unlinked: &'static str,
    pub link_device_first: &'static str,
    /// `{prefix}`.
    pub added_short_id: &'static str,
    /// `{name}`, `{short}`.
    pub added_device: &'static str,
    pub own_device_id: &'static str,
    pub qr_not_device_id: &'static str,
    /// `{label}`.
    pub share_deleted: &'static str,
    /// `{label}`.
    pub link_device_to_share: &'static str,
    /// `{label}`.
    pub share_created: &'static str,
    /// `{label}`.
    pub share_not_created: &'static str,
    /// `{path}`.
    pub saved_in: &'static str,
    /// `{id}`.
    pub share_active: &'static str,
    /// `{id}`.
    pub share_paused: &'static str,
    /// `{label}`, `{id}`.
    pub offer_accepted: &'static str,
    /// `{label}`.
    pub choose_where_to_store: &'static str,
    /// `{label}`, `{id}`.
    pub offer_declined: &'static str,
    pub no_declined_folders: &'static str,
    /// `{count}`.
    pub declined_cleared: &'static str,
    pub id_empty: &'static str,
    /// `{ch}`.
    pub id_invalid_char: &'static str,
    pub id_too_short: &'static str,
    /// `{len}`.
    pub id_too_long: &'static str,
    /// `{first}`, `{second}` (group numbers).
    pub id_checksum: &'static str,
    pub id_decode_failed: &'static str,
    pub no_devices_linked: &'static str,
    /// `{devices}`.
    pub shared_with: &'static str,
    /// `{path}`.
    pub picker_share_this_folder: &'static str,
    /// `{path}`.
    pub picker_keep_current: &'static str,
    /// `{path}`.
    pub picker_save_data: &'static str,
    /// `{path}`.
    pub picker_pictures: &'static str,
    /// `{path}`.
    pub picker_music: &'static str,
    /// `{path}`.
    pub picker_retroarch: &'static str,
    /// `{path}`.
    pub picker_default_folder: &'static str,
    /// `{path}`.
    pub picker_browsed_folder: &'static str,
    /// QR / ID modal: section header.
    pub device_id_section: &'static str,
    /// QR / ID modal.
    pub device_id_groups_1_4: &'static str,
    /// QR / ID modal.
    pub device_id_groups_5_8: &'static str,
    /// `{id}`.
    pub short_id_label: &'static str,
    /// QR scanner.
    pub camera_starting: &'static str,
    /// `{error}` (camera driver message).
    pub camera_error: &'static str,
    /// `{name}`, `{short}`, `{addr}`.
    pub add_device_matches_peer: &'static str,
    /// `{len}`, `{short}`.
    pub add_device_valid_short: &'static str,
    /// `{short}`.
    pub add_device_valid_full: &'static str,
    /// Manage Devices row: `{prefix}`, `{addr}`.
    pub device_row_short: &'static str,
    /// Manage Devices row: `{id}`, `{addr}`.
    pub device_row_full: &'static str,
    /// Folder offer from an unknown device: `{short}`.
    pub unknown_device: &'static str,
}

/// Replaces each `{key}` of `template` with its value.
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = template.to_owned();
    for (key, value) in args {
        out = out.replace(&format!("{{{key}}}"), &value.to_string());
    }
    out
}

impl Strings {
    /// Renders a domain message in this catalog's language.
    pub fn text<'a>(&self, t: &'a UiText) -> Cow<'a, str> {
        let m = self.messages;
        let s: &'static str = match t {
            UiText::Raw(s) => return Cow::Borrowed(s),
            UiText::ReadyTapSyncCard => m.ready_tap_sync_card,
            UiText::ReadyManualPairing => m.ready_manual_pairing,
            UiText::NoPeerLinked => m.no_peer_linked,
            UiText::SelectDeviceToConnect => m.select_device_to_connect,
            UiText::NoFoldersSynced => m.no_folders_synced,
            UiText::NoRemoteFolders => m.no_remote_folders,
            UiText::StartingSync => m.starting_sync,
            UiText::ScanningLan => m.scanning_lan,
            UiText::ExchangingIndex => m.exchanging_index,
            UiText::SyncFinished => m.sync_finished,
            UiText::NoPeerAddDevice => m.no_peer_add_device,
            UiText::SyncingWithPeers => m.syncing_with_peers,
            UiText::ThemeDark => m.theme_dark,
            UiText::ThemeOled => m.theme_oled,
            UiText::LinkDeviceFirst => m.link_device_first,
            UiText::OwnDeviceId => m.own_device_id,
            UiText::QrNotDeviceId => m.qr_not_device_id,
            UiText::NoDeclinedFolders => m.no_declined_folders,
            UiText::NoDevicesLinked => m.no_devices_linked,
            UiText::InvalidDeviceId(err) => return Cow::Owned(self.device_id_error(*err)),
            UiText::Connections { count } => return Cow::Owned(self.connections(*count)),
            other => return Cow::Owned(self.text_with_params(other)),
        };
        Cow::Borrowed(s)
    }

    /// Messages carrying parameters.
    fn text_with_params(&self, t: &UiText) -> String {
        let m = self.messages;
        match t {
            UiText::ConnectingTo { name, addr } => {
                fill(m.connecting_to, &[("name", name), ("addr", addr)])
            }
            UiText::FileUpToDate { file } => fill(m.file_up_to_date, &[("file", file)]),
            UiText::Downloading { file, pct } => {
                fill(m.downloading, &[("file", file), ("pct", pct)])
            }
            UiText::IndexPublished { files } => fill(m.index_published, &[("files", files)]),
            UiText::Uploading { file } => fill(m.uploading, &[("file", file)]),
            UiText::SyncSummary {
                connections,
                up_to_date,
                downloaded,
                downloaded_kb,
                uploaded,
                uploaded_bytes,
            } => fill(
                m.sync_summary,
                &[
                    ("connections", &self.connections(*connections)),
                    ("up_to_date", up_to_date),
                    ("downloaded", downloaded),
                    ("kb", downloaded_kb),
                    ("uploaded", uploaded),
                    ("bytes", uploaded_bytes),
                ],
            ),
            UiText::FoundLanPeers { count } => fill(m.found_lan_peers, &[("count", count)]),
            UiText::DiscoveryError { error } => fill(m.discovery_error, &[("error", error)]),
            UiText::LanguageChanged(lang) => fill(m.language_changed, &[("name", &lang.name())]),
            UiText::DeviceLinked { device, share } => {
                fill(m.device_linked, &[("device", device), ("share", share)])
            }
            UiText::DeviceUnlinked { device, share } => {
                fill(m.device_unlinked, &[("device", device), ("share", share)])
            }
            UiText::AddedShortId { prefix } => fill(m.added_short_id, &[("prefix", prefix)]),
            UiText::AddedDevice { name, short } => {
                fill(m.added_device, &[("name", name), ("short", short)])
            }
            UiText::ShareDeleted { label } => fill(m.share_deleted, &[("label", label)]),
            UiText::LinkDeviceToShare { label } => {
                fill(m.link_device_to_share, &[("label", label)])
            }
            UiText::ShareCreated { label } => fill(m.share_created, &[("label", label)]),
            UiText::ShareNotCreated { label } => fill(m.share_not_created, &[("label", label)]),
            UiText::SavedIn { path } => fill(m.saved_in, &[("path", path)]),
            UiText::ShareActive { id } => fill(m.share_active, &[("id", id)]),
            UiText::SharePaused { id } => fill(m.share_paused, &[("id", id)]),
            UiText::OfferAccepted { label, id } => {
                fill(m.offer_accepted, &[("label", label), ("id", id)])
            }
            UiText::ChooseWhereToStore { label } => {
                fill(m.choose_where_to_store, &[("label", label)])
            }
            UiText::OfferDeclined { label, id } => {
                fill(m.offer_declined, &[("label", label), ("id", id)])
            }
            UiText::DeclinedCleared { count } => fill(m.declined_cleared, &[("count", count)]),
            UiText::SharedWith { devices } => fill(m.shared_with, &[("devices", devices)]),
            UiText::PickerShareThisFolder { path } => {
                fill(m.picker_share_this_folder, &[("path", path)])
            }
            UiText::PickerKeepCurrent { path } => fill(m.picker_keep_current, &[("path", path)]),
            UiText::PickerSaveData { path } => fill(m.picker_save_data, &[("path", path)]),
            UiText::PickerPictures { path } => fill(m.picker_pictures, &[("path", path)]),
            UiText::PickerMusic { path } => fill(m.picker_music, &[("path", path)]),
            UiText::PickerRetroArch { path } => fill(m.picker_retroarch, &[("path", path)]),
            UiText::PickerDefaultFolder { path } => {
                fill(m.picker_default_folder, &[("path", path)])
            }
            UiText::PickerBrowsedFolder { path } => {
                fill(m.picker_browsed_folder, &[("path", path)])
            }
            // Parameterless messages and data are handled by `text`.
            other => self.text(other).into_owned(),
        }
    }

    /// `Connected (1 connection)` / `Connected (1+N connections)`.
    fn connections(&self, count: usize) -> String {
        if count > 1 {
            fill(
                self.messages.connections_many,
                &[("secondary", &(count - 1))],
            )
        } else {
            self.messages.connections_one.to_owned()
        }
    }

    /// Translated reason why a typed Device ID was rejected.
    pub fn device_id_error(&self, err: DeviceIdParseError) -> String {
        let m = self.messages;
        match err {
            DeviceIdParseError::Empty => m.id_empty.to_owned(),
            DeviceIdParseError::InvalidChar(ch) => fill(m.id_invalid_char, &[("ch", &ch)]),
            DeviceIdParseError::TooShort => m.id_too_short.to_owned(),
            DeviceIdParseError::TooLong(len) => fill(m.id_too_long, &[("len", &len)]),
            DeviceIdParseError::ChecksumMismatch { first_group } => fill(
                m.id_checksum,
                &[("first", &first_group), ("second", &(first_group + 1))],
            ),
            DeviceIdParseError::DecodeFailed => m.id_decode_failed.to_owned(),
        }
    }
}
