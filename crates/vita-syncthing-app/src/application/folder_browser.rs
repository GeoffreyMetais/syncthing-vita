//! `Files` tab folder browser (`FolderBrowser`): lists the presets and subfolders of the
//! browsed directory, walks up and down, and applies the selection (new share or new target
//! folder of the selected share).

use crate::application::{ShareCreation, ShareService};
use crate::domain::{
    ActiveTab, AppState, PickerItem, PickerMode, UiText, BROWSE_TEMPLATE_ID, NEW_SHARE_TEMPLATES,
};
use crate::infrastructure::ConfigRepository;

pub struct FolderBrowser;

impl FolderBrowser {
    /// Refreshes presets and `ux0:` subdirectories for the currently selected share or new share creation.
    pub fn refresh(state: &mut AppState, repo: &ConfigRepository) {
        let folder_id = if state.picker_mode == PickerMode::CreateNewShare {
            let tail = state
                .picker_current_dir
                .trim_end_matches('/')
                .rsplit(['/', ':'])
                .next()
                .unwrap_or("vita-folder");
            if tail.is_empty() || tail == "ux0" {
                "vita-shared".to_owned()
            } else {
                tail.to_lowercase()
            }
        } else {
            state
                .shares
                .get(state.selected_share_idx)
                .map(|s| s.folder_id.clone())
                .unwrap_or_else(|| "default".to_owned())
        };

        let current_target = state
            .shares
            .get(state.selected_share_idx)
            .map(|s| s.target_path.clone())
            .unwrap_or_else(|| "ux0:user/00/savedata".to_owned());

        let mut items = if state.picker_mode == PickerMode::CreateNewShare {
            Self::unshared_template_items(state)
        } else {
            vec![
                preset(
                    UiText::PickerKeepCurrent {
                        path: current_target.clone(),
                    },
                    current_target,
                ),
                preset(
                    UiText::PickerSaveData {
                        path: "ux0:user/00/savedata".to_owned(),
                    },
                    "ux0:user/00/savedata".to_owned(),
                ),
                preset(
                    UiText::PickerPictures {
                        path: "ux0:picture".to_owned(),
                    },
                    "ux0:picture".to_owned(),
                ),
                preset(
                    UiText::PickerMusic {
                        path: "ux0:music".to_owned(),
                    },
                    "ux0:music".to_owned(),
                ),
                preset(
                    UiText::PickerRetroArch {
                        path: "ux0:data/retroarch/savefiles".to_owned(),
                    },
                    "ux0:data/retroarch/savefiles".to_owned(),
                ),
                preset(
                    UiText::PickerDefaultFolder {
                        path: repo.default_share_dir(&folder_id),
                    },
                    repo.default_share_dir(&folder_id),
                ),
                preset(
                    UiText::PickerBrowsedFolder {
                        path: state.picker_current_dir.trim_end_matches('/').to_owned(),
                    },
                    state.picker_current_dir.trim_end_matches('/').to_owned(),
                ),
            ]
        };

        for sub in repo.list_subdirectories(&state.picker_current_dir) {
            let full = if state.picker_current_dir.ends_with(':')
                || state.picker_current_dir.ends_with('/')
            {
                format!("{}{}", state.picker_current_dir, sub)
            } else {
                format!("{}/{}", state.picker_current_dir, sub)
            };
            items.push(PickerItem {
                label: UiText::Raw(format!("{}/", sub)),
                path: full,
                is_preset: false,
                template: None,
                is_shared: false,
            });
        }

        // In `ChangeShareTarget` mode the selected share may keep its own folder.
        let own_share = (state.picker_mode == PickerMode::ChangeShareTarget)
            .then_some(state.selected_share_idx);
        for item in &mut items {
            let path = item.path.trim_end_matches('/');
            item.is_shared =
                state.shares.iter().enumerate().any(|(i, s)| {
                    Some(i) != own_share && s.target_path.trim_end_matches('/') == path
                });
        }

        state.picker_items = items;
        if state.selected_picker_idx >= state.picker_items.len() {
            state.selected_picker_idx = 0;
        }
        state.clamp_picker_scroll();
    }

    /// Default share presets (`NEW_SHARE_TEMPLATES`) listed first at the root of the `Files`
    /// browser, except those already shared (same folder ID or same target folder).
    fn unshared_template_items(state: &AppState) -> Vec<PickerItem> {
        if parent_dir(&state.picker_current_dir).is_some() {
            return Vec::new();
        }
        NEW_SHARE_TEMPLATES
            .iter()
            .enumerate()
            .filter(|(_, (fid, _, _))| *fid != BROWSE_TEMPLATE_ID)
            .filter(|(_, (fid, _, path))| {
                !state.shares.iter().any(|s| {
                    s.folder_id == *fid
                        || s.target_path.trim_end_matches('/') == path.trim_end_matches('/')
                })
            })
            .map(|(idx, &(_, label, path))| PickerItem {
                label: UiText::PickerTemplate {
                    label: label.to_owned(),
                    path: path.to_owned(),
                },
                path: path.to_owned(),
                is_preset: true,
                template: Some(idx),
                is_shared: false,
            })
            .collect()
    }

    /// Navigates one directory level up in the `ux0:` picker, or returns to the `Overview` tab if at root.
    pub fn navigate_up(state: &mut AppState, repo: &ConfigRepository) {
        match parent_dir(&state.picker_current_dir) {
            Some(parent) => {
                state.picker_current_dir = parent;
                state.selected_picker_idx = 0;
                Self::refresh(state, repo);
            }
            None => state.active_tab = ActiveTab::Dashboard,
        }
    }

    /// Applies selection in the `ux0:` directory browser (folders already shared are ignored):
    /// - If `enter_subdir == true` and item is a directory, descends into it.
    /// - If `state.picker_mode == PickerMode::CreateNewShare`, creates a new share and opens device assignment.
    /// - Otherwise, assigns the chosen target path to the selected share.
    pub fn apply_selection(state: &mut AppState, repo: &ConfigRepository, enter_subdir: bool) {
        let Some(item) = state.picker_items.get(state.selected_picker_idx).cloned() else {
            return;
        };
        if item.is_shared {
            return;
        }

        if enter_subdir && !item.is_preset {
            state.picker_current_dir = item.path;
            state.selected_picker_idx = 0;
            Self::refresh(state, repo);
            return;
        }
        // Browsing through the app's data is fine, sharing it is not.
        if repo.is_protected_share_path(&item.path) {
            state.show_toast(UiText::FolderProtected);
            return;
        }

        if state.picker_mode == PickerMode::CreateNewShare {
            match item.template {
                Some(idx) => ShareCreation::begin_from_template(state, repo, idx),
                None => ShareCreation::begin_from_path(state, repo, &item.path),
            };
        } else {
            ShareService::update_selected_share_target(state, repo, &item.path);
        }
    }
}

/// A preset row: selecting it picks `path` as is (it cannot be entered).
fn preset(label: UiText, path: String) -> PickerItem {
    PickerItem {
        label,
        path,
        is_preset: true,
        template: None,
        is_shared: false,
    }
}

/// Parent of a browsed directory, handling `sceIo` device roots (`ux0:data` -> `ux0:`).
/// Returns `None` at a root (`ux0:` or `/`).
fn parent_dir(dir: &str) -> Option<String> {
    let trimmed = dir.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) if trimmed.len() > 1 => Some("/".to_owned()),
        Some(0) => None,
        Some(i) => Some(trimmed[..i].to_owned()),
        None => match trimmed.find(':') {
            Some(i) if i + 1 < trimmed.len() => Some(trimmed[..=i].to_owned()),
            _ => None,
        },
    }
}
