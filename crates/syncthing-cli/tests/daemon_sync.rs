//! Integration tests of the sync daemon (`syncthing-cli run`): real processes talking over
//! 127.0.0.1 with LAN discovery, Global Discovery and relays disabled.

mod common;

use std::path::Path;
use std::time::Duration;

use common::{
    files_named, settle, wait_for_absent, wait_for_file, write_file, Daemon, Node, LOCAL_ONLY,
};

/// Upper bound of every wait on a remote effect (a sync, a log line, an exit).
const WAIT: Duration = Duration::from_secs(30);
/// How long a negative assertion (a file must not arrive) is given to show up.
const NEGATIVE_SETTLE: Duration = Duration::from_secs(3);

/// Starts the daemon of `node` listening on its port, and waits until it does.
fn start_listening(node: &Node, extra: &[&str]) -> Daemon {
    let listen = node.listen_addr();
    let mut args = vec!["--listen", listen.as_str()];
    args.extend_from_slice(LOCAL_ONLY);
    args.extend_from_slice(extra);
    let daemon = node.cli.spawn_daemon(&args);
    daemon.wait_for_log_line("Listening on", WAIT);
    daemon
}

/// Starts the daemon of `node` without a listener (it only dials the devices it knows).
fn spawn_dialer(node: &Node, extra: &[&str]) -> Daemon {
    let mut args: Vec<&str> = LOCAL_ONLY.to_vec();
    args.extend_from_slice(extra);
    node.cli.spawn_daemon(&args)
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("UTF-8 test path")
}

/// `a` does not dial (it has no address for `b`); `b` dials `a` at its listening port. The
/// folder `docs` is shared between them: `a-docs` on `a`, `b-docs` on `b` (receive-only when
/// `b_receive_only`). Rescan every second.
fn pair(a: &Node, b: &Node, b_receive_only: bool) {
    a.cli.ok(&["device", "add", &b.id, "--name", "bob"]);
    b.cli.ok(&[
        "device",
        "add",
        &a.id,
        "--name",
        "alice",
        "--address",
        &a.listen_addr(),
    ]);
    a.cli.ok(&[
        "folder",
        "add",
        "docs",
        path_str(&a.dir("a-docs")),
        "--device",
        &b.id,
    ]);
    let b_dir = b.dir("b-docs");
    let a_id = a.id.as_str();
    let mut args = vec!["folder", "add", "docs", path_str(&b_dir), "--device", a_id];
    if b_receive_only {
        args.push("--receive-only");
    }
    b.cli.ok(&args);
    a.set_rescan_interval(1);
    b.set_rescan_interval(1);
}

#[test]
fn two_devices_sync_a_folder_both_ways_and_the_receiver_exits_when_synced() {
    let a = Node::new("sync-both-a");
    let b = Node::new("sync-both-b");
    write_file(&a.dir("a-docs/a-root.txt"), "from alice");
    write_file(&a.dir("a-docs/sub/a-nested.txt"), "nested from alice");
    write_file(&b.dir("b-docs/b-root.txt"), "from bob");
    write_file(&b.dir("b-docs/sub/b-nested.txt"), "nested from bob");
    pair(&a, &b, false);

    let _alice = start_listening(&a, &["--exit-after", "90"]);
    let mut bob_daemon = spawn_dialer(&b, &["--exit-when-synced", "--exit-after", "90"]);

    let status = bob_daemon.wait_for_exit(WAIT);
    assert!(
        status.success(),
        "bob exited with {status}\n{}",
        bob_daemon.stdout()
    );
    assert!(
        bob_daemon.stdout().contains("Every device is in sync"),
        "bob's log:\n{}",
        bob_daemon.stdout()
    );

    wait_for_file(&b.dir("b-docs/a-root.txt"), "from alice", WAIT);
    wait_for_file(&b.dir("b-docs/sub/a-nested.txt"), "nested from alice", WAIT);
    wait_for_file(&a.dir("a-docs/b-root.txt"), "from bob", WAIT);
    wait_for_file(&a.dir("a-docs/sub/b-nested.txt"), "nested from bob", WAIT);
}

#[test]
fn modified_and_deleted_files_on_one_side_reach_the_other() {
    let a = Node::new("sync-change-a");
    let b = Node::new("sync-change-b");
    write_file(&a.dir("a-docs/a-root.txt"), "alice v1");
    write_file(&a.dir("a-docs/sub/a-nested.txt"), "nested v1");
    pair(&a, &b, false);

    let _alice = start_listening(&a, &["--exit-after", "120"]);
    let _bob = spawn_dialer(&b, &["--exit-after", "120"]);

    // Initial sync first, so the changes below are changes to an already synchronized folder.
    wait_for_file(&b.dir("b-docs/a-root.txt"), "alice v1", WAIT);
    wait_for_file(&b.dir("b-docs/sub/a-nested.txt"), "nested v1", WAIT);

    write_file(
        &a.dir("a-docs/a-root.txt"),
        "alice v2, with a longer content",
    );
    wait_for_file(
        &b.dir("b-docs/a-root.txt"),
        "alice v2, with a longer content",
        WAIT,
    );

    std::fs::remove_file(a.dir("a-docs/sub/a-nested.txt")).expect("delete on alice");
    wait_for_absent(&b.dir("b-docs/sub/a-nested.txt"), WAIT);
}

#[test]
fn connection_from_an_unknown_device_is_refused_and_nothing_is_transferred() {
    let a = Node::new("sync-unknown-a");
    let b = Node::new("sync-unknown-b");
    // `a` has a folder but does not know `b` at all.
    a.cli
        .ok(&["folder", "add", "docs", path_str(&a.dir("a-docs"))]);
    write_file(&a.dir("a-docs/from-alice.txt"), "alice's file");
    // `b` knows `a` and dials it.
    b.cli.ok(&[
        "device",
        "add",
        &a.id,
        "--name",
        "alice",
        "--address",
        &a.listen_addr(),
    ]);
    b.cli.ok(&[
        "folder",
        "add",
        "docs",
        path_str(&b.dir("b-docs")),
        "--device",
        &a.id,
    ]);
    write_file(&b.dir("b-docs/from-bob.txt"), "bob's file");
    a.set_rescan_interval(1);
    b.set_rescan_interval(1);

    let alice_daemon = start_listening(&a, &["--exit-after", "60"]);
    let _bob = spawn_dialer(&b, &["--exit-after", "60"]);

    alice_daemon.wait_for_log_line(&format!("unknown device {}", b.id), WAIT);
    assert!(
        alice_daemon
            .stdout()
            .contains("Connection from unknown device"),
        "alice's log:\n{}",
        alice_daemon.stdout()
    );
    assert!(
        files_named(&b.root, "from-alice.txt").is_empty(),
        "alice's file reached bob"
    );
    assert!(
        files_named(&a.root, "from-bob.txt").is_empty(),
        "bob's file reached alice"
    );
}

#[test]
fn folder_shared_by_a_known_device_is_offered_and_never_synchronized() {
    let a = Node::new("sync-offer-a");
    let b = Node::new("sync-offer-b");
    // `a` knows `b` but has no `docs` folder: `b` shares one with it.
    a.cli.ok(&["device", "add", &b.id, "--name", "bob"]);
    b.cli.ok(&[
        "device",
        "add",
        &a.id,
        "--name",
        "alice",
        "--address",
        &a.listen_addr(),
    ]);
    b.cli.ok(&[
        "folder",
        "add",
        "docs",
        path_str(&b.dir("b-docs")),
        "--device",
        &a.id,
    ]);
    write_file(&b.dir("b-docs/offered-only.txt"), "not for alice yet");
    b.set_rescan_interval(1);

    let alice_daemon = start_listening(&a, &["--exit-after", "60"]);
    let _bob = spawn_dialer(&b, &["--exit-after", "60"]);

    alice_daemon.wait_for_log_line("bob offers folder \"docs\" (docs)", WAIT);
    settle(NEGATIVE_SETTLE);
    assert!(
        files_named(&a.root, "offered-only.txt").is_empty(),
        "a folder that was never accepted was synchronized on alice's side"
    );
}

#[test]
fn receive_only_folder_receives_but_never_sends_local_files() {
    let a = Node::new("sync-recv-a");
    let b = Node::new("sync-recv-b");
    write_file(&a.dir("a-docs/a-only.txt"), "from alice");
    // Created on bob before the first sync: a receive-only folder must never offer it.
    write_file(&b.dir("b-docs/b-only.txt"), "bob's local file");
    pair(&a, &b, true);

    let alice_daemon = start_listening(&a, &["--exit-after", "90"]);
    let mut bob_daemon = spawn_dialer(&b, &["--exit-when-synced", "--exit-after", "90"]);

    let status = bob_daemon.wait_for_exit(WAIT);
    assert!(
        status.success(),
        "bob exited with {status}\n{}",
        bob_daemon.stdout()
    );
    assert!(
        bob_daemon.stdout().contains("In sync with alice"),
        "bob's log:\n{}",
        bob_daemon.stdout()
    );

    wait_for_file(&b.dir("b-docs/a-only.txt"), "from alice", WAIT);
    // Alice keeps running: give her the time to receive whatever bob would have offered.
    settle(NEGATIVE_SETTLE);
    assert!(
        files_named(&a.root, "b-only.txt").is_empty(),
        "a file created only on the receive-only device reached the other device\n{}",
        alice_daemon.stdout()
    );
    wait_for_file(&b.dir("b-docs/b-only.txt"), "bob's local file", WAIT);
}
