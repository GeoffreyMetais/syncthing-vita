//! Home directory resolution: `--home`, then `$SYNCTHING_CLI_HOME`, then the platform default
//! (`$XDG_CONFIG_HOME` or `$HOME/.config`). Tests that change environment variables run in
//! this binary only, one at a time.

mod common;

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use syncthing_cli::paths::{db_dir, default_home, resolve_home};

use common::temp_home;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Holds the environment lock and sets the three variables read by `paths`
/// (`None` removes one). The previous values are restored on drop.
struct Env {
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<OsString>)>,
}

const VARS: [&str; 3] = ["SYNCTHING_CLI_HOME", "HOME", "XDG_CONFIG_HOME"];

fn set_env(values: [Option<&str>; 3]) -> Env {
    let guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let saved = VARS
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    for (name, value) in VARS.iter().zip(values) {
        match value {
            // The lock above serializes the environment changes of this test binary.
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
    }
    Env {
        _guard: guard,
        saved,
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }
}

#[test]
fn explicit_home_wins_over_the_environment() {
    let _env = set_env([
        Some("/nonexistent/from-env"),
        Some("/nonexistent/home"),
        None,
    ]);
    let explicit = temp_home("paths-explicit").join("given");
    let home = resolve_home(Some(explicit.clone())).expect("explicit home");
    assert_eq!(home, explicit);
    assert!(home.is_dir(), "the home directory is created");
}

#[test]
fn syncthing_cli_home_is_used_when_no_explicit_home() {
    let target = temp_home("paths-env").join("from-env");
    let _env = set_env([
        Some(target.to_str().expect("utf-8 path")),
        Some("/nonexistent/home"),
        None,
    ]);
    let home = resolve_home(None).expect("env home");
    assert_eq!(home, target);
    assert!(home.is_dir());
}

#[test]
fn an_empty_syncthing_cli_home_falls_back_to_the_default() {
    let root = temp_home("paths-empty-env");
    let _env = set_env([Some(""), Some(root.to_str().expect("utf-8 path")), None]);
    let home = resolve_home(None).expect("default home");
    assert_eq!(home, root.join(".config").join("syncthing-cli"));
    assert!(home.is_dir());
}

#[test]
fn default_home_is_under_home_dot_config_without_xdg() {
    let root = PathBuf::from("/home/tester");
    let _env = set_env([None, Some("/home/tester"), None]);
    assert_eq!(
        default_home().expect("default home"),
        root.join(".config").join("syncthing-cli")
    );
}

#[test]
fn xdg_config_home_takes_precedence_when_set() {
    let _env = set_env([None, Some("/home/tester"), Some("/srv/xdg")]);
    assert_eq!(
        default_home().expect("default home"),
        PathBuf::from("/srv/xdg").join("syncthing-cli")
    );
}

#[test]
fn an_empty_xdg_config_home_is_ignored() {
    let _env = set_env([None, Some("/home/tester"), Some("")]);
    assert_eq!(
        default_home().expect("default home"),
        PathBuf::from("/home/tester/.config/syncthing-cli")
    );
}

#[test]
fn default_home_needs_a_home_directory() {
    {
        let _env = set_env([None, None, None]);
        assert_eq!(
            default_home().expect_err("no HOME").kind(),
            ErrorKind::NotFound
        );
    }
    let _env = set_env([None, Some(""), None]);
    assert_eq!(
        default_home().expect_err("empty HOME").kind(),
        ErrorKind::NotFound
    );
}

#[test]
fn folder_databases_live_in_a_db_subdirectory() {
    assert_eq!(
        db_dir(&PathBuf::from("/var/lib/sc")),
        PathBuf::from("/var/lib/sc/db")
    );
}
