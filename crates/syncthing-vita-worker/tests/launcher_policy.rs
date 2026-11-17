//! The launcher's decision logic (`plugin/launcher/src/policy.c`, plain C without system
//! calls) built with the host C compiler and run against `plugin/launcher/tests/policy_test.c`:
//! when cycles start, what aborts them, and `plugin.cfg` parsed like `plugin_cfg.rs` does.

use std::path::PathBuf;
use std::process::Command;

fn plugin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugin")
}

#[test]
fn test_launcher_policy_in_c() {
    let plugin = plugin_dir();
    let out = std::env::temp_dir().join(format!("stp_policy_test-{}", std::process::id()));
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_owned());
    let build = Command::new(&cc)
        .args(["-std=c99", "-Wall", "-Wextra", "-Werror", "-O1", "-o"])
        .arg(&out)
        .arg(plugin.join("launcher/src/policy.c"))
        .arg(plugin.join("launcher/tests/policy_test.c"))
        .output()
        .unwrap_or_else(|e| panic!("host C compiler `{cc}` not available: {e}"));
    assert!(
        build.status.success(),
        "policy.c does not build:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let run = Command::new(&out).output().expect("run policy_test");
    let _ = std::fs::remove_file(&out);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{stdout}");
    assert!(stdout.contains("all checks passed"), "{stdout}");
}

#[test]
fn test_launcher_and_worker_agree_on_plugin_cfg() {
    // The C parser's defaults are the Rust ones (the C checks use the same literal inputs as
    // `plugin_cfg.rs`).
    let header = std::fs::read_to_string(plugin_dir().join("launcher/src/policy.h")).unwrap();
    let define = |name: &str| {
        header
            .lines()
            .find_map(|l| l.strip_prefix(&format!("#define {name} ")))
            .unwrap_or_else(|| panic!("{name}"))
            .trim()
            .trim_end_matches('u')
            .to_owned()
    };
    use syncthing_vita_worker::plugin_cfg::*;
    assert_eq!(
        define("STP_DEFAULT_ENABLED"),
        u8::from(DEFAULT_ENABLED).to_string()
    );
    assert_eq!(
        define("STP_DEFAULT_SYNC_ON_GAME_EXIT"),
        u8::from(DEFAULT_SYNC_ON_GAME_EXIT).to_string()
    );
    assert_eq!(
        define("STP_DEFAULT_INTERVAL_SECS"),
        DEFAULT_INTERVAL_SECS.to_string()
    );
}
