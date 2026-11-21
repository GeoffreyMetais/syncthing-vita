//! Incoming unknown-device offers (`DeviceOfferService`): an unknown remote device attempting
//! to connect (via TCP 22000 or Relay) is never approved automatically; it is queued as a
//! `DeviceOffer` until the user accepts, declines (persisted in `ignored_devices.cfg`), or
//! postpones (`Later`) it.

use syncthing_core::DeviceId;

use crate::application::{DeviceRegistry, ShareService};
use crate::domain::defaults::MAX_DEVICE_OFFERS;
use crate::domain::{AppState, DeviceOffer, ModalDialog, UiText};
use crate::infrastructure::ConfigRepository;

pub struct DeviceOfferService;

impl DeviceOfferService {
    /// Queues an unknown remote device that attempted to connect to this PS Vita.
    ///
    /// Already-approved devices, declined devices (`ignored_devices.cfg`), and duplicates are
    /// ignored. The queue is bounded by `MAX_DEVICE_OFFERS`.
    pub fn receive(
        state: &mut AppState,
        repo: &ConfigRepository,
        device_id: DeviceId,
        address: &str,
        via_relay: bool,
    ) {
        if state
            .approved_devices()
            .iter()
            .any(|d| d.device_id.matches_peer(&device_id))
        {
            return;
        }
        if state.is_device_ignored(&device_id) {
            return;
        }
        if let Some(offer) = state
            .device_offers
            .iter_mut()
            .find(|o| o.device_id.matches_peer(&device_id))
        {
            if !address.is_empty() {
                offer.address = address.to_owned();
            }
            offer.via_relay = via_relay;
            return;
        }
        if state.device_offers.len() >= MAX_DEVICE_OFFERS {
            return;
        }
        state.device_offers.push(DeviceOffer {
            device_id,
            address: address.to_owned(),
            via_relay,
            snoozed: false,
        });
        let via = if via_relay {
            "via relay".to_owned()
        } else if address.is_empty() {
            "over TCP".to_owned()
        } else {
            format!("from {address}")
        };
        ShareService::log_event(
            state,
            repo,
            &format!(
                "Device {} ({}) wants to connect: waiting for approval",
                device_id.short(),
                via
            ),
        );
    }

    /// Opens the `DeviceOffer` modal for the next pending offer when no other modal is open.
    pub fn prompt_next(state: &mut AppState) {
        if state.modal != ModalDialog::None {
            return;
        }
        if let Some(offer_idx) = state.next_device_offer_to_prompt() {
            state.modal = ModalDialog::DeviceOffer { offer_idx };
        }
    }

    /// Accepts `device_offers[offer_idx]`: approves and persists the device, then marks the
    /// configuration dirty so the background worker reloads and accepts/connects to it.
    pub fn accept(state: &mut AppState, repo: &ConfigRepository, offer_idx: usize) {
        state.modal = ModalDialog::None;
        if offer_idx >= state.device_offers.len() {
            return;
        }
        let offer = state.device_offers.remove(offer_idx);
        let short = offer.device_id.short();
        // Use `"dynamic"` so LAN discovery / Global Discovery resolves the peer's actual
        // listening port (or the peer reconnects inbound immediately once the worker reloads).
        DeviceRegistry::upsert_device(state, repo, offer.device_id, "", "dynamic");
        state.config_dirty = true;
        state.auto_reconnect_enabled = true;
        state.show_toast(UiText::DeviceOfferAccepted {
            short: short.clone(),
        });
        ShareService::log_event(
            state,
            repo,
            &format!("Accepted incoming device {short} ({})", offer.device_id),
        );
    }

    /// Declines `device_offers[offer_idx]` for good (persisted in `ignored_devices.cfg`).
    pub fn decline(state: &mut AppState, repo: &ConfigRepository, offer_idx: usize) {
        state.modal = ModalDialog::None;
        if offer_idx >= state.device_offers.len() {
            return;
        }
        let offer = state.device_offers.remove(offer_idx);
        if !state.is_device_ignored(&offer.device_id) {
            state.ignored_devices.push(offer.device_id);
            repo.save_ignored_devices(&state.ignored_devices);
        }
        let short = offer.device_id.short();
        state.show_toast(UiText::DeviceOfferDeclined {
            short: short.clone(),
        });
        ShareService::log_event(state, repo, &format!("Declined incoming device {short}"));
    }

    /// Postpones `device_offers[offer_idx]` until the app restarts.
    pub fn later(state: &mut AppState, offer_idx: usize) {
        state.modal = ModalDialog::None;
        if let Some(offer) = state.device_offers.get_mut(offer_idx) {
            offer.snoozed = true;
        }
    }
}
