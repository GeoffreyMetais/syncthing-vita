//! User-facing messages (`UiText`) chosen by the domain, application and infrastructure layers.
//!
//! Inner layers never build display strings: they pick a `UiText` variant with its parameters,
//! and the UI layer renders it in the current language (`ui::i18n::messages`).

use syncthing_core::DeviceIdParseError;

use crate::domain::models::Language;

/// A translatable message with its parameters (status line, peer / folder labels, toasts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiText {
    /// Text shown as-is: data such as file names, peer addresses or folder lists.
    Raw(String),

    // Overview status line, peer and folder labels
    /// Initial status line before any sync.
    ReadyTapSyncCard,
    /// Initial peer label before any sync.
    ReadyManualPairing,
    /// Peer label while no device is linked to a share.
    NoPeerLinked,
    /// Status line while no device is linked to a share.
    SelectDeviceToConnect,
    /// Folder label before the first `ClusterConfig`.
    NoFoldersSynced,
    /// Folder label when the peer shares no folder.
    NoRemoteFolders,
    StartingSync,
    ScanningLan,
    ConnectingTo {
        name: String,
        addr: String,
    },
    ExchangingIndex,
    FileUpToDate {
        file: String,
    },
    Downloading {
        file: String,
        pct: u32,
    },
    IndexPublished {
        files: usize,
    },
    Uploading {
        file: String,
    },
    /// Current file once the worker thread has finished.
    SyncFinished,
    /// Current file while connected: number of BEP connections (primary + secondaries).
    Connections {
        count: usize,
    },
    /// Status line after a completed sync cycle.
    SyncSummary {
        connections: usize,
        up_to_date: usize,
        downloaded: usize,
        downloaded_kb: usize,
        uploaded: usize,
        uploaded_bytes: usize,
    },
    FoundLanPeers {
        count: usize,
    },
    NoPeerAddDevice,
    DiscoveryError {
        error: String,
    },

    // Toasts
    SyncingWithPeers,
    ThemeDark,
    ThemeOled,
    LanguageChanged(Language),
    DeviceLinked {
        device: String,
        share: String,
    },
    DeviceUnlinked {
        device: String,
        share: String,
    },
    LinkDeviceFirst,
    AddedShortId {
        prefix: String,
    },
    AddedDevice {
        name: String,
        short: String,
    },
    OwnDeviceId,
    InvalidDeviceId(DeviceIdParseError),
    QrNotDeviceId,
    ShareDeleted {
        label: String,
    },
    LinkDeviceToShare {
        label: String,
    },
    ShareCreated {
        label: String,
    },
    ShareNotCreated {
        label: String,
    },
    SavedIn {
        path: String,
    },
    ShareActive {
        id: String,
    },
    SharePaused {
        id: String,
    },
    ShareReadOnly {
        id: String,
    },
    ShareReadWrite {
        id: String,
    },
    RevertRequested {
        id: String,
    },
    LocalChangesReverted {
        id: String,
        count: usize,
    },
    OfferAccepted {
        label: String,
        id: String,
    },
    ChooseWhereToStore {
        label: String,
    },
    OfferDeclined {
        label: String,
        id: String,
    },
    NoDeclinedFolders,
    DeclinedCleared {
        count: usize,
    },

    // Overview share list
    /// Share card line when no device is linked yet.
    NoDevicesLinked,
    /// Share card line listing the linked devices (`name (short ID)`, comma-separated).
    SharedWith {
        devices: String,
    },

    // Files tab rows (`PickerItem::label`)
    PickerShareThisFolder {
        path: String,
    },
    PickerKeepCurrent {
        path: String,
    },
    PickerSaveData {
        path: String,
    },
    PickerPictures {
        path: String,
    },
    PickerMusic {
        path: String,
    },
    PickerRetroArch {
        path: String,
    },
    PickerDefaultFolder {
        path: String,
    },
    PickerBrowsedFolder {
        path: String,
    },
}

impl UiText {
    /// Wraps data that is displayed as-is.
    pub fn raw(s: impl Into<String>) -> Self {
        Self::Raw(s.into())
    }
}
