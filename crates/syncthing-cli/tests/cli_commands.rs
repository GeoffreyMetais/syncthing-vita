//! End-to-end runs of the `syncthing-cli` binary against a temporary home directory.

mod common;

use std::path::Path;
use std::process::{Command, Output};

use syncthing_core::DeviceId;

use common::{temp_home, valid_device_id};

fn run(home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_syncthing-cli"))
        .arg("--home")
        .arg(home)
        .args(args)
        .output()
        .expect("run syncthing-cli")
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn id_prints_a_stable_valid_device_id() {
    let home = temp_home("cli-id");
    let first = stdout(&run(&home, &["id"])).trim().to_owned();
    let second = stdout(&run(&home, &["id"])).trim().to_owned();
    assert_eq!(first, second);
    assert!(DeviceId::parse_user_id(&first).is_ok());
    assert!(!first.contains(char::is_whitespace));
}

#[test]
fn device_add_then_list_shows_it() {
    let home = temp_home("cli-device");
    let peer = valid_device_id(20);
    stdout(&run(
        &home,
        &[
            "device",
            "add",
            &peer,
            "--name",
            "laptop",
            "--address",
            "192.168.1.5:22000",
        ],
    ));
    let listing = stdout(&run(&home, &["device", "list"]));
    assert!(listing.contains(&peer), "listing: {listing}");
    assert!(listing.contains("laptop"));
    assert!(listing.contains("tcp://192.168.1.5:22000"));
}

#[test]
fn folder_add_list_and_device_remove_unshares() {
    let home = temp_home("cli-folder");
    let peer = valid_device_id(21);
    let folder_dir = home.join("shared");
    stdout(&run(&home, &["device", "add", &peer]));
    stdout(&run(
        &home,
        &[
            "folder",
            "add",
            "docs",
            folder_dir.to_str().unwrap(),
            "--label",
            "Documents",
            "--device",
            &peer,
        ],
    ));
    assert!(folder_dir.is_dir(), "folder directory created");

    let listing = stdout(&run(&home, &["folder", "list"]));
    let short = DeviceId::parse_user_id(&peer).unwrap().short();
    assert!(listing.contains("docs"), "listing: {listing}");
    assert!(listing.contains("Documents"));
    assert!(listing.contains(&short), "listing: {listing}");

    stdout(&run(&home, &["device", "remove", &peer]));
    let listing = stdout(&run(&home, &["folder", "list"]));
    assert!(listing.contains("devices: none"), "listing: {listing}");
}

#[test]
fn invalid_device_id_fails_with_error_on_stderr() {
    let home = temp_home("cli-invalid");
    let output = run(&home, &["device", "add", "NOT-A-DEVICE-ID"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("error:"), "stderr: {stderr}");
}

#[test]
fn config_set_then_show_reports_the_option() {
    let home = temp_home("cli-config");
    stdout(&run(&home, &["config", "set", "listen", "0.0.0.0:22001"]));
    let shown = stdout(&run(&home, &["config", "show"]));
    assert!(
        shown.contains("listen = \"0.0.0.0:22001\""),
        "show: {shown}"
    );
    assert!(shown.starts_with("# device id: "), "show: {shown}");
}
