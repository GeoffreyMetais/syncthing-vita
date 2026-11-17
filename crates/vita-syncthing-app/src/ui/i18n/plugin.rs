//! Strings of the background sync plugin (`Settings` card, `ConfirmPlugin` modal, toasts).
//! Each language file defines its `PluginStrings` catalog (`PLUGIN_EN`, `PLUGIN_FR`...).

use crate::domain::{PluginNotice, PluginRequest, PluginStatus};

/// Background sync plugin strings of one language.
#[derive(Debug)]
pub struct PluginStrings {
    /// Section header of the `Settings` card.
    pub section: &'static str,
    /// Status chips.
    pub status_not_installed: &'static str,
    pub status_reboot: &'static str,
    pub status_active: &'static str,
    pub status_disabled: &'static str,
    /// Card line per status.
    pub desc_not_installed: &'static str,
    /// Uninstalled while the launcher is still loaded.
    pub desc_unload_pending: &'static str,
    pub desc_reboot: &'static str,
    pub desc_active: &'static str,
    pub desc_disabled: &'static str,
    /// Card buttons.
    pub action_install: &'static str,
    pub action_uninstall: &'static str,
    pub action_enable: &'static str,
    pub action_disable: &'static str,
    /// `ConfirmPlugin` (install).
    pub install_title: &'static str,
    pub install_subtitle: &'static str,
    pub install_lines: [&'static str; 2],
    pub install_reboot: &'static str,
    /// `ConfirmPlugin` (uninstall).
    pub uninstall_title: &'static str,
    pub uninstall_subtitle: &'static str,
    pub uninstall_lines: [&'static str; 2],
    pub uninstall_reboot: &'static str,
    /// Toasts (`UiText::Plugin`).
    pub notice_installed: &'static str,
    pub notice_installed_reboot: &'static str,
    pub notice_uninstalled: &'static str,
    pub notice_uninstalled_reboot: &'static str,
    pub notice_enabled: &'static str,
    pub notice_disabled: &'static str,
    pub notice_bundle_missing: &'static str,
    pub notice_tai_missing: &'static str,
    pub notice_write_failed: &'static str,
}

/// Texts of the `ConfirmPlugin` modal for one request.
pub struct ConfirmPluginText {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub confirm: &'static str,
    pub lines: [&'static str; 2],
    pub reboot: &'static str,
}

impl PluginStrings {
    /// Toast text of `notice`.
    pub fn notice(&self, notice: PluginNotice) -> &'static str {
        match notice {
            PluginNotice::Installed => self.notice_installed,
            PluginNotice::InstalledRebootNeeded => self.notice_installed_reboot,
            PluginNotice::Uninstalled => self.notice_uninstalled,
            PluginNotice::UninstalledRebootNeeded => self.notice_uninstalled_reboot,
            PluginNotice::Enabled => self.notice_enabled,
            PluginNotice::Disabled => self.notice_disabled,
            PluginNotice::BundleMissing => self.notice_bundle_missing,
            PluginNotice::TaiConfigMissing => self.notice_tai_missing,
            PluginNotice::WriteFailed => self.notice_write_failed,
        }
    }

    /// Status chip text.
    pub fn status(&self, status: PluginStatus) -> &'static str {
        match status {
            PluginStatus::NotInstalled => self.status_not_installed,
            PluginStatus::InstalledNeedsReboot => self.status_reboot,
            PluginStatus::Enabled => self.status_active,
            PluginStatus::Disabled => self.status_disabled,
        }
    }

    /// Card line for `status` (`unload_pending`: uninstalled, launcher still loaded).
    pub fn description(&self, status: PluginStatus, unload_pending: bool) -> &'static str {
        match status {
            PluginStatus::NotInstalled if unload_pending => self.desc_unload_pending,
            PluginStatus::NotInstalled => self.desc_not_installed,
            PluginStatus::InstalledNeedsReboot => self.desc_reboot,
            PluginStatus::Enabled => self.desc_active,
            PluginStatus::Disabled => self.desc_disabled,
        }
    }

    /// `ConfirmPlugin` texts (`SetEnabled` is never confirmed: uninstall texts).
    pub fn confirm(&self, request: PluginRequest) -> ConfirmPluginText {
        match request {
            PluginRequest::Install => ConfirmPluginText {
                title: self.install_title,
                subtitle: self.install_subtitle,
                confirm: self.action_install,
                lines: self.install_lines,
                reboot: self.install_reboot,
            },
            _ => ConfirmPluginText {
                title: self.uninstall_title,
                subtitle: self.uninstall_subtitle,
                confirm: self.action_uninstall,
                lines: self.uninstall_lines,
                reboot: self.uninstall_reboot,
            },
        }
    }
}
