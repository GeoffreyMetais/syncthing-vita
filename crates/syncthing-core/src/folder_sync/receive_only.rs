//! Receive-only folders (Syncthing "Receive Only", the app's read-only shares): the PS Vita
//! applies its peers' changes, but its own local changes are never sent to them.
//!
//! As in Syncthing, a locally changed entry is still listed in the published `Index`, flagged
//! `invalid`: peers know the device has something else there and never pull it.

use std::collections::HashMap;

use crate::bep::{BepFileInfo, BepIndex};
use crate::version_vector::version_ge;

/// Returns `index` with every local change flagged `invalid`. An entry is a local change when
/// the peer has no valid entry of that name, or when its version is not covered by the peer's
/// version (a local creation, modification or deletion the peer has not seen). Entries the
/// peer already has, or has newer, are published unchanged.
pub fn mask_local_changes(index: &BepIndex, remote: &[BepFileInfo]) -> BepIndex {
    let remote_by_name: HashMap<&str, &BepFileInfo> = remote
        .iter()
        .filter(|rf| !rf.invalid)
        .map(|rf| (rf.name.as_str(), rf))
        .collect();
    let files = index
        .files
        .iter()
        .map(|f| {
            let known_by_peer = remote_by_name
                .get(f.name.as_str())
                .is_some_and(|rf| version_ge(&rf.versions, &f.versions));
            BepFileInfo {
                invalid: f.invalid || !known_by_peer,
                ..f.clone()
            }
        })
        .collect();
    BepIndex {
        folder: index.folder.clone(),
        files,
    }
}
