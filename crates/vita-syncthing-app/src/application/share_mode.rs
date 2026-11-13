//! Share access mode: read & write (Syncthing "Send & Receive") or read only ("Receive Only":
//! the PS Vita receives its peers' changes but never sends its own), and revert of a read-only
//! share's local changes.

use super::{ShareService, UserCommand};
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

    /// Asks the sync worker to revert the local changes of the read-only `shares[share_idx]`
    /// (Syncthing "Revert Local Changes"): files added on the PS Vita are deleted, edited or
    /// deleted ones are downloaded again. The request waits in `pending_reverts` until a sync
    /// worker runs; a sync is started if none is.
    pub fn request_revert(
        state: &mut AppState,
        repo: &ConfigRepository,
        share_idx: usize,
    ) -> UserCommand {
        let Some(share) = state.shares.get(share_idx) else {
            return UserCommand::None;
        };
        if share.mode != ShareMode::ReadOnly {
            return UserCommand::None;
        }
        let id = share.folder_id.clone();
        if !state.pending_reverts.contains(&id) {
            state.pending_reverts.push(id.clone());
        }
        state.auto_reconnect_enabled = true;
        ShareService::log_event(
            state,
            repo,
            &format!("Revert of local changes requested for {}", id),
        );
        state.show_toast(UiText::RevertRequested { id });
        if state.sync_in_progress || state.session_connected {
            UserCommand::None
        } else {
            UserCommand::StartSync
        }
    }
}
