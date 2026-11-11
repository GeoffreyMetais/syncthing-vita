//! Input for the `FolderOffer` modal: `Cross` accepts, `Square` declines for good,
//! `Circle` postpones the offer until the app restarts.

use crate::application::{FolderOfferService, UserCommand};
use crate::domain::AppState;
use crate::infrastructure::{ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE};
use crate::ui::theme::{folder_offer_modal_rect, folder_offer_slot_x, FOLDER_OFFER_SLOT_W};

enum Choice {
    Accept,
    Decline,
    Later,
}

/// Handles input in the `FolderOffer` modal for `folder_offers[offer_idx]`.
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
        input.touch_tap.and_then(|(tx, ty)| tapped_choice(tx, ty))
    };
    match choice {
        Some(Choice::Accept) => FolderOfferService::accept(state, repo, offer_idx),
        Some(Choice::Decline) => FolderOfferService::decline(state, repo, offer_idx),
        Some(Choice::Later) => FolderOfferService::later(state, offer_idx),
        None => {}
    }
    UserCommand::None
}

/// Maps a tap on the modal's action row or on the bottom hint bar to a choice.
fn tapped_choice(tx: usize, ty: usize) -> Option<Choice> {
    if ty >= 500 {
        return match tx {
            0..=169 => Some(Choice::Accept),
            170..=274 => Some(Choice::Decline),
            275..=369 => Some(Choice::Later),
            _ => None,
        };
    }
    let (_, dy, _, h) = folder_offer_modal_rect();
    let cy = dy + h - 34;
    if ty + 22 < cy || ty > cy + 22 {
        return None;
    }
    let in_slot = |slot: usize| {
        let x = folder_offer_slot_x(slot);
        tx >= x && tx < x + FOLDER_OFFER_SLOT_W
    };
    if in_slot(0) {
        Some(Choice::Decline)
    } else if in_slot(1) {
        Some(Choice::Later)
    } else if in_slot(2) {
        Some(Choice::Accept)
    } else {
        None
    }
}
