//! Helpers shared by the integration tests.

#![allow(dead_code)]

pub mod servers;

use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use syncthing_core::DeviceIdentity;

/// Poll interval of every wait helper.
const POLL: Duration = Duration::from_millis(100);

/// A fresh, empty directory unique to this process and call.
pub fn temp_home(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before 1970")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "syncthing-cli-test-{name}-{}-{nanos}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp home");
    dir
}

/// Canonical full device ID derived from a constant seed.
pub fn valid_device_id(seed: u8) -> String {
    DeviceIdentity::from_seed(&[seed; 32])
        .expect("valid seed")
        .device_id
        .to_syncthing_string()
}

/// A free TCP port on 127.0.0.1 (the listener is dropped before the port is returned).
pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("bind a free port")
        .port()
}

/// Wrapper around the `syncthing-cli` binary using one `--home` directory.
#[derive(Debug, Clone)]
pub struct Cli {
    pub home: PathBuf,
}

impl Cli {
    pub fn new(home: &Path) -> Self {
        Self {
            home: home.to_path_buf(),
        }
    }

    /// A command with `--home` already set; add the subcommand arguments to it.
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_syncthing-cli"));
        command.arg("--home").arg(&self.home);
        command
    }

    /// Runs a command to completion and returns its output.
    pub fn run(&self, args: &[&str]) -> Output {
        self.command()
            .args(args)
            .output()
            .expect("run syncthing-cli")
    }

    /// Runs a command that must succeed and returns its stdout.
    pub fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "`syncthing-cli {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// This device's ID.
    pub fn id(&self) -> String {
        self.ok(&["id"]).trim().to_owned()
    }

    /// Starts `syncthing-cli run` with `args` (the caller passes the run flags, including a
    /// timeout). Stdout and stderr are read in the background.
    pub fn spawn_daemon(&self, args: &[&str]) -> Daemon {
        let mut command = self.command();
        command
            .arg("run")
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn syncthing-cli run");
        let stdout = Arc::new(Mutex::new(Vec::new()));
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let readers = vec![
            collect_lines(
                child.stdout.take().expect("piped stdout"),
                Arc::clone(&stdout),
            ),
            collect_lines(
                child.stderr.take().expect("piped stderr"),
                Arc::clone(&stderr),
            ),
        ];
        Daemon {
            child,
            stdout,
            stderr,
            readers,
            exit: None,
        }
    }
}

fn collect_lines<R: Read + Send + 'static>(
    reader: R,
    sink: Arc<Mutex<Vec<String>>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            sink.lock().unwrap_or_else(|p| p.into_inner()).push(line);
        }
    })
}

/// A running daemon. Killed on drop, so a failing test never leaks child processes.
pub struct Daemon {
    child: Child,
    stdout: Arc<Mutex<Vec<String>>>,
    stderr: Arc<Mutex<Vec<String>>>,
    readers: Vec<JoinHandle<()>>,
    exit: Option<ExitStatus>,
}

impl Daemon {
    /// Every stdout line so far.
    pub fn stdout_lines(&self) -> Vec<String> {
        self.stdout
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Everything printed on stdout so far, one line per line.
    pub fn stdout(&self) -> String {
        self.stdout_lines().join("\n")
    }

    /// Everything printed on stderr so far.
    pub fn stderr(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .join("\n")
    }

    /// Waits until a stdout line contains `needle`, or panics after `timeout` with the log.
    pub fn wait_for_log_line(&self, needle: &str, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            if self.stdout_lines().iter().any(|l| l.contains(needle)) {
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "no log line containing {needle:?} within {timeout:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                    self.stdout(),
                    self.stderr()
                );
            }
            thread::sleep(POLL);
        }
    }

    /// Waits for the daemon to exit by itself and returns its status. Panics after `timeout`
    /// (the daemon is then killed).
    pub fn wait_for_exit(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.exit {
                return status;
            }
            match self.child.try_wait().expect("poll daemon") {
                Some(status) => {
                    self.exit = Some(status);
                    for reader in self.readers.drain(..) {
                        let _ = reader.join();
                    }
                    return status;
                }
                None if Instant::now() >= deadline => {
                    let _ = self.child.kill();
                    panic!(
                        "daemon still running after {timeout:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                        self.stdout(),
                        self.stderr()
                    );
                }
                None => thread::sleep(POLL),
            }
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if self.exit.is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// One device of a test: its directory (holding the `home` and the folders), its CLI, its
/// device ID and a free listening port.
pub struct Node {
    pub root: PathBuf,
    pub cli: Cli,
    pub id: String,
    pub port: u16,
}

impl Node {
    pub fn new(name: &str) -> Self {
        let root = temp_home(name);
        let cli = Cli::new(&root.join("home"));
        std::fs::create_dir_all(&cli.home).expect("create home");
        let id = cli.id();
        Self {
            root,
            cli,
            id,
            port: free_port(),
        }
    }

    /// `root/<name>`: a folder directory of this device (created on demand by `folder add`).
    pub fn dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn listen_addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// Sets `rescan-interval` (seconds) in this device's configuration.
    pub fn set_rescan_interval(&self, secs: u64) {
        self.cli
            .ok(&["config", "set", "rescan-interval", &secs.to_string()]);
    }
}

/// Flags of a daemon that must never touch the network beyond 127.0.0.1.
pub const LOCAL_ONLY: &[&str] = &[
    "--no-local-discovery",
    "--no-global-discovery",
    "--no-relays",
];

/// Writes `content` to `path`, creating the parent directories.
pub fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent directories");
    }
    std::fs::write(path, content).expect("write file");
}

/// Polls until `path` holds exactly `content`, or panics after `timeout`.
pub fn wait_for_file(path: &Path, content: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let current = std::fs::read_to_string(path).ok();
        if current.as_deref() == Some(content) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "{} did not reach the expected content within {timeout:?} (last: {current:?}, expected {content:?})",
                path.display()
            );
        }
        thread::sleep(POLL);
    }
}

/// Polls until `path` no longer exists, or panics after `timeout`.
pub fn wait_for_absent(path: &Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while path.exists() {
        if Instant::now() >= deadline {
            panic!("{} still exists after {timeout:?}", path.display());
        }
        thread::sleep(POLL);
    }
}

/// Lets the daemons run for `duration`. Used only before a negative assertion (a file must
/// *not* arrive), where there is no event to wait for.
pub fn settle(duration: Duration) {
    thread::sleep(duration);
}

/// Every file named `name` below `root` (recursively).
pub fn files_named(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|n| n == name) {
                found.push(path);
            }
        }
    }
    found
}
