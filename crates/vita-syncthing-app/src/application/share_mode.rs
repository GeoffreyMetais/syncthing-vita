//! Share access mode: read & write (Syncthing "Send & Receive") or read only ("Send Only",
//! peers never modify the PS Vita's files).

use super::ShareService;
use crate::domain::{AppState, ShareMode, UiText};
use crate::infrastructure::ConfigRepository;

/// Switches a share between read & write and read only.
pub struct ShareModeService;

impl ShareModeService {
    /// Switches `shares[share_idx]` to the other mode and saves it. The running sync worker is
    /// restarted (`config_dirty`) so the new mode applies to the next session.
    pub fn toggle(state: &mut AppState, repo: &ConfigRepository, share_idx: usize) {
        let Some(share) = state.shares.get_mut(share_idx) else {
            return;
        };
        share.mode = share.mode.toggled();
        let (id, mode) = (share.folder_id.clone(), share.mode);
        state.config_dirty = true;
        state.auto_reconnect_enabled = true;
        repo.save_shares(&state.shares);
        let log = match mode {
            ShareMode::ReadOnly => format!("Share {} is now read only", id),
            ShareMode::ReadWrite => format!("Share {} is now read & write", id),
        };
        state.show_toast(match mode {
            ShareMode::ReadOnly => UiText::ShareReadOnly { id },
            ShareMode::ReadWrite => UiText::ShareReadWrite { id },
        });
        ShareService::log_event(state, repo, &log);
    }
}
