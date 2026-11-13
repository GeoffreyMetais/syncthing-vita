//! Upload activity: uploads have no end event (a peer simply stops requesting blocks), so the
//! "Uploading" state shown by the sidebar card and the screensaver ends after a quiet period.

use std::time::Instant;

pub use crate::domain::defaults::UPLOAD_DISPLAY_GRACE;
use crate::domain::AppState;

/// Clears `AppState::uploading` once no block was served for `UPLOAD_DISPLAY_GRACE`.
#[derive(Debug, Default)]
pub struct UploadActivity {
    seen_blocks: usize,
    last_block_at: Option<Instant>,
}

impl UploadActivity {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records blocks served since the previous call and ends the upload state after the grace
    /// period. Returns `true` if `state` changed (the screen must be redrawn).
    pub fn update(&mut self, state: &mut AppState, now: Instant) -> bool {
        if state.blocks_uploaded != self.seen_blocks {
            self.seen_blocks = state.blocks_uploaded;
            self.last_block_at = Some(now);
        }
        let quiet = self
            .last_block_at
            .is_none_or(|t| now.saturating_duration_since(t) > UPLOAD_DISPLAY_GRACE);
        if state.uploading && quiet {
            state.uploading = false;
            return true;
        }
        false
    }
}
