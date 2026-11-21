//! Input for the `DeviceOffer` modal: `Cross` accepts, `Square` declines for good,
//! `Circle` postpones the offer until the app restarts.

use crate::application::{DeviceOfferService, UserCommand};
use crate::domain::AppState;
use crate::infrastructure::{ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE};

enum Choice {
    Accept,
    Decline,
    Later,
}

/// Handles input in the `DeviceOffer` modal for `device_offers[offer_idx]`.
pub fn handle(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
    offer_idx: usize,
) -> UserCommand {
    let choice = if (input.pressed & BTN_CROSS) != 0 {
        Some(Choice::Accept)
    } else if (input.pressed & BTN_SQUARE) != 0 {
        Some(Choice::Decline)
    } else if (input.pressed & BTN_CIRCLE) != 0 {
        Some(Choice::Later)
    } else {
        None
    };
    match choice {
        Some(Choice::Accept) => DeviceOfferService::accept(state, repo, offer_idx),
        Some(Choice::Decline) => DeviceOfferService::decline(state, repo, offer_idx),
        Some(Choice::Later) => DeviceOfferService::later(state, offer_idx),
        None => {}
    }
    UserCommand::None
}
