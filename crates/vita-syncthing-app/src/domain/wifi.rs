//! Wi-Fi link state (`WifiLink`, `WifiState`): the app cannot sync without a network, so the user
//! is asked to turn Wi-Fi on when the console is offline (`ModalDialog::WifiOff`).

/// State of the console's Wi-Fi link, as reported by the system (`SceNetCtl` on PS Vita).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WifiLink {
    /// Not known yet, or the system could not report it: never prompts the user.
    #[default]
    Unknown,
    /// Wi-Fi off, Airplane Mode on, or no access point in range.
    Disconnected,
    /// Associating with an access point or obtaining an IP address.
    Connecting,
    /// Connected with an IP address.
    Connected,
}

/// Wi-Fi state shown by the UI and the user's answer to the `WifiOff` prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WifiState {
    pub link: WifiLink,
    /// The user chose to continue offline: the prompt does not come back until the next time
    /// the console gets connected and then loses the connection again.
    pub prompt_dismissed: bool,
}

impl WifiState {
    /// True when the console is known to be offline.
    pub fn is_offline(&self) -> bool {
        self.link == WifiLink::Disconnected
    }
}
