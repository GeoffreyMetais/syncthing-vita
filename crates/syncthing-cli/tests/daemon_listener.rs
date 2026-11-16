//! The daemon's listener: an idle listening socket is not an error, and a connection that
//! fails its handshake is reported without stopping the listener.

mod common;

use std::net::TcpStream;
use std::time::Duration;

use common::servers::isolate_proxy_env;
use common::{free_port, temp_home, Cli, LOCAL_ONLY};

#[test]
fn idle_listener_logs_no_accept_error() {
    isolate_proxy_env();
    let home = temp_home("listener-idle");
    let listen = format!("127.0.0.1:{}", free_port());
    let mut args = vec!["--listen", listen.as_str(), "--exit-after", "3"];
    args.extend_from_slice(LOCAL_ONLY);
    let mut daemon = Cli::new(&home).spawn_daemon(&args);
    daemon.wait_for_log_line("Listening on", Duration::from_secs(10));

    // Nothing connects: the accept loop polls the listener for the whole time.
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        !daemon.stdout().contains("Accept failed"),
        "stdout:\n{}",
        daemon.stdout()
    );
    assert!(daemon.wait_for_exit(Duration::from_secs(20)).success());
    assert!(
        !daemon.stdout().contains("Accept failed"),
        "stdout:\n{}",
        daemon.stdout()
    );
}

#[test]
fn listener_keeps_accepting_after_a_connection_that_is_not_tls() {
    isolate_proxy_env();
    let home = temp_home("listener-plain");
    let listen = format!("127.0.0.1:{}", free_port());
    let mut args = vec!["--listen", listen.as_str(), "--exit-after", "4"];
    args.extend_from_slice(LOCAL_ONLY);
    let mut daemon = Cli::new(&home).spawn_daemon(&args);
    daemon.wait_for_log_line("Listening on", Duration::from_secs(10));

    // A plain TCP client that sends garbage and hangs up: the handshake fails.
    {
        use std::io::Write;
        let mut tcp = TcpStream::connect(&listen).expect("connect");
        let _ = tcp.write_all(b"GET / HTTP/1.0\r\n\r\n");
    }
    daemon.wait_for_log_line("TLS handshake with 127.0.0.1", Duration::from_secs(10));
    assert!(
        !daemon.stdout().contains("Accept failed"),
        "stdout:\n{}",
        daemon.stdout()
    );
    assert!(daemon.wait_for_exit(Duration::from_secs(20)).success());
}

/// An accept failure other than "no connection yet" is logged, and the listener goes on: the
/// daemon runs with a file-descriptor limit that the held connections exhaust.
#[test]
fn listener_reports_an_accept_failure_and_keeps_listening() {
    isolate_proxy_env();
    let home = temp_home("listener-emfile");
    let listen = format!("127.0.0.1:{}", free_port());
    let log_path = home.join("daemon-output.txt");
    let log = std::fs::File::create(&log_path).expect("create output file");
    // `sh` lowers the descriptor limit of the daemon only.
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("ulimit -n 24 && exec \"$0\" --home \"$1\" run --listen \"$2\" --exit-after 5 --no-local-discovery --no-global-discovery --no-relays")
        .arg(env!("CARGO_BIN_EXE_syncthing-cli"))
        .arg(&home)
        .arg(&listen)
        .stdout(log.try_clone().expect("clone output file"))
        .stderr(log)
        .spawn()
        .expect("spawn daemon through sh");

    // Connections the daemon cannot accept any more wait in its backlog.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !std::fs::read_to_string(&log_path).is_ok_and(|s| s.contains("Listening on")) {
        assert!(
            std::time::Instant::now() < deadline,
            "the daemon never listened"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let held: Vec<TcpStream> = (0..40)
        .filter_map(|_| TcpStream::connect(&listen).ok())
        .collect();
    let status = child.wait().expect("daemon exit");
    drop(held);

    let output = std::fs::read_to_string(&log_path).expect("read daemon output");
    assert!(status.success(), "exit: {status}\n{output}");
    assert!(output.contains("Accept failed"), "output:\n{output}");
    assert!(output.contains("Stopped"), "output:\n{output}");
}
