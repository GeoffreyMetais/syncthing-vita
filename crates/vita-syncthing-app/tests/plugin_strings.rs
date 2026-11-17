//! `PluginStrings` (`ui::i18n::plugin`): status chips, card descriptions, `ConfirmPlugin`
//! texts and toasts are picked per status / request / notice, in every language.

use vita_syncthing_app::domain::{Language, PluginNotice, PluginRequest, PluginStatus, UiText};
use vita_syncthing_app::ui::strings;

const STATUSES: [PluginStatus; 4] = [
    PluginStatus::NotInstalled,
    PluginStatus::InstalledNeedsReboot,
    PluginStatus::Enabled,
    PluginStatus::Disabled,
];

#[test]
fn test_status_chips_and_descriptions_follow_the_status() {
    for lang in Language::ALL {
        let p = strings::strings(lang).plugin;
        assert_eq!(p.status(PluginStatus::NotInstalled), p.status_not_installed);
        assert_eq!(
            p.status(PluginStatus::InstalledNeedsReboot),
            p.status_reboot
        );
        assert_eq!(p.status(PluginStatus::Enabled), p.status_active);
        assert_eq!(p.status(PluginStatus::Disabled), p.status_disabled);
        assert_eq!(
            p.description(PluginStatus::NotInstalled, false),
            p.desc_not_installed
        );
        assert_eq!(
            p.description(PluginStatus::NotInstalled, true),
            p.desc_unload_pending
        );
        assert_eq!(
            p.description(PluginStatus::InstalledNeedsReboot, false),
            p.desc_reboot
        );
        assert_eq!(p.description(PluginStatus::Enabled, false), p.desc_active);
        assert_eq!(
            p.description(PluginStatus::Disabled, false),
            p.desc_disabled
        );
        // Every status reads differently.
        for (i, a) in STATUSES.iter().enumerate() {
            for b in &STATUSES[i + 1..] {
                assert_ne!(p.status(*a), p.status(*b), "{lang:?}");
                assert_ne!(
                    p.description(*a, false),
                    p.description(*b, false),
                    "{lang:?}"
                );
            }
        }
    }
}

#[test]
fn test_confirm_texts_per_request() {
    for lang in Language::ALL {
        let p = strings::strings(lang).plugin;
        let install = p.confirm(PluginRequest::Install);
        assert_eq!(install.title, p.install_title);
        assert_eq!(install.subtitle, p.install_subtitle);
        assert_eq!(install.confirm, p.action_install);
        assert_eq!(install.lines, p.install_lines);
        assert_eq!(install.reboot, p.install_reboot);
        let uninstall = p.confirm(PluginRequest::Uninstall);
        assert_eq!(uninstall.title, p.uninstall_title);
        assert_eq!(uninstall.subtitle, p.uninstall_subtitle);
        assert_eq!(uninstall.confirm, p.action_uninstall);
        assert_eq!(uninstall.lines, p.uninstall_lines);
        assert_eq!(uninstall.reboot, p.uninstall_reboot);
        assert_ne!(install.title, uninstall.title);
    }
}

#[test]
fn test_notices_are_rendered_as_toasts() {
    let pairs = |p: &'static vita_syncthing_app::ui::i18n::PluginStrings| {
        [
            (PluginNotice::Installed, p.notice_installed),
            (
                PluginNotice::InstalledRebootNeeded,
                p.notice_installed_reboot,
            ),
            (PluginNotice::Uninstalled, p.notice_uninstalled),
            (
                PluginNotice::UninstalledRebootNeeded,
                p.notice_uninstalled_reboot,
            ),
            (PluginNotice::Enabled, p.notice_enabled),
            (PluginNotice::Disabled, p.notice_disabled),
            (PluginNotice::BundleMissing, p.notice_bundle_missing),
            (PluginNotice::TaiConfigMissing, p.notice_tai_missing),
            (PluginNotice::WriteFailed, p.notice_write_failed),
        ]
    };
    for lang in Language::ALL {
        let s = strings::strings(lang);
        let all = pairs(s.plugin);
        for (i, (notice, text)) in all.iter().enumerate() {
            assert_eq!(s.plugin.notice(*notice), *text);
            assert_eq!(s.text(&UiText::Plugin(*notice)), *text);
            for (_, other) in &all[i + 1..] {
                assert_ne!(text, other, "{lang:?} {notice:?}");
            }
        }
    }
    assert_eq!(
        strings::strings(Language::English).text(&UiText::Plugin(PluginNotice::BundleMissing)),
        "This build has no plugin files (app0:plugins)"
    );
}
