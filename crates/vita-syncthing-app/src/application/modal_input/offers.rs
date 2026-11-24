//! Input for the offer modals (`FolderOffer` / `DeviceOffer`): `Cross` accepts, `Square`
//! declines for good, `Circle` postpones the offer until the app restarts.

use crate::application::{DeviceOfferService, FolderOfferService, UserCommand};
use crate::domain::AppState;
use crate::infrastructure::{ConfigRepository, InputFrame, BTN_CIRCLE, BTN_CROSS, BTN_SQUARE};

/// Which kind of offer the open modal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OfferKind {
    Folder,
    Device,
}

enum Choice {
    Accept,
    Decline,
    Later,
}

/// Handles input in an offer modal for the offer at `offer_idx` of its list.
pub(crate) fn handle(
    state: &mut AppState,
    repo: &ConfigRepository,
    input: InputFrame,
    offer_idx: usize,
    kind: OfferKind,
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
    match (kind, choice) {
        (_, None) => {}
        (OfferKind::Folder, Some(Choice::Accept)) => {
            FolderOfferService::accept(state, repo, offer_idx)
        }
        (OfferKind::Folder, Some(Choice::Decline)) => {
            FolderOfferService::decline(state, repo, offer_idx)
        }
        (OfferKind::Folder, Some(Choice::Later)) => FolderOfferService::later(state, offer_idx),
        (OfferKind::Device, Some(Choice::Accept)) => {
            DeviceOfferService::accept(state, repo, offer_idx)
        }
        (OfferKind::Device, Some(Choice::Decline)) => {
            DeviceOfferService::decline(state, repo, offer_idx)
        }
        (OfferKind::Device, Some(Choice::Later)) => DeviceOfferService::later(state, offer_idx),
    }
    UserCommand::None
}
