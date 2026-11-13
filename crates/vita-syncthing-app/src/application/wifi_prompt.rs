//! Wi-Fi prompt (`WifiPrompt`): asks the user to turn Wi-Fi on when the console is offline.
//!
//! A userland application cannot switch the PS Vita's Wi-Fi on by itself, so the next best
//! thing is to tell the user, as soon as the app starts, how to do it. The `WifiOff` modal
//! closes by itself once the system reports a connection, and the sync worker is restarted so
//! that discovery and sessions use the new address.

use std::time::Instant;

use crate::domain::defaults::WIFI_PROMPT_DELAY;
use crate::domain::{AppState, ModalDialog, UiText, WifiLink};

/// What changed after a Wi-Fi link update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiChange {
    /// Nothing visible changed.
    None,
    /// The link state or the prompt changed (redraw).
    Updated,
    /// The console got connected after having been offline: restart the sync worker.
    Restored,
}

/// Tracks how long the console has been offline and opens / closes the `WifiOff` modal.
#[derive(Debug, Default)]
pub struct WifiPrompt {
    /// When the current disconnection was first observed.
    offline_since: Option<Instant>,
    /// The console was seen offline since the last connection.
    was_offline: bool,
}

impl WifiPrompt {
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies the link state reported by the system at `now`.
    pub fn update(&mut self, state: &mut AppState, link: WifiLink, now: Instant) -> WifiChange {
        let mut change = if state.wifi.link != link {
            state.wifi.link = link;
            WifiChange::Updated
        } else {
            WifiChange::None
        };
        match link {
            WifiLink::Disconnected => {
                self.was_offline = true;
                let since = *self.offline_since.get_or_insert(now);
                if !state.wifi.prompt_dismissed
                    && state.modal == ModalDialog::None
                    && now.duration_since(since) >= WIFI_PROMPT_DELAY
                {
                    state.modal = ModalDialog::WifiOff;
                    change = WifiChange::Updated;
                }
            }
            WifiLink::Connected => {
                self.offline_since = None;
                state.wifi.prompt_dismissed = false;
                if state.modal == ModalDialog::WifiOff {
                    state.modal = ModalDialog::None;
                    state.show_toast(UiText::WifiConnected);
                }
                if std::mem::take(&mut self.was_offline) {
                    change = WifiChange::Restored;
                }
            }
            // Associating with an access point: wait for the outcome.
            WifiLink::Connecting => self.offline_since = None,
            // The system could not tell: never bother the user.
            WifiLink::Unknown => {}
        }
        change
    }
}
