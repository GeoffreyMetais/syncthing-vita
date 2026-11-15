//! The sync daemon: accepts connections, dials the configured devices, announces itself on
//! the LAN and on Global Discovery, and stays reachable through a relay.
//!
//! - [`context`] — shared state; [`sessions`] — one session per device over any link
//! - [`listener`] — incoming TCP; [`dialer`] — outgoing (direct, then relays)
//! - [`lan`] — Local Discovery; [`global`] — Global Discovery announcements; [`relay`]
//! - [`events`] — session events to log lines; [`activity`] — `--exit-when-synced`
//! - [`reports`] — what was already logged once (bounded); [`connection_slots`] — connections
//!   not authenticated yet (bounded); [`timing`] — timings and limits

pub mod activity;
pub mod connection_slots;
pub mod context;
pub mod dialer;
pub mod events;
pub mod global;
pub mod lan;
pub mod listener;
pub mod log;
pub mod relay;
pub mod reports;
pub mod sessions;
pub mod timing;

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use syncthing_core::{DeviceId, DeviceIdentity};

use crate::config::Config;
use context::{DaemonContext, DaemonOptions};
use log::log;
use timing::{STOP_CHECK_STEP, SYNCED_QUIET_PERIOD};

/// Longest wait for the threads to end once a stop is requested.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// When `run` ends on its own (besides Ctrl-C).
#[derive(Debug, Clone, Default)]
pub struct ExitPolicy {
    pub after: Option<Duration>,
    pub when_synced: bool,
}

fn spawn(name: &str, handles: &mut Vec<JoinHandle<()>>, f: impl FnOnce() + Send + 'static) {
    match std::thread::Builder::new().name(name.to_owned()).spawn(f) {
        Ok(handle) => handles.push(handle),
        Err(e) => log(&format!("Cannot start the {name} thread: {e}")),
    }
}

/// Devices that must be in sync for `--exit-when-synced`: those sharing an active folder.
fn devices_to_sync(config: &Config) -> Vec<DeviceId> {
    let mut devices: Vec<DeviceId> = Vec::new();
    for folder in config.folders.iter().filter(|f| !f.paused) {
        for id in folder.device_ids() {
            if !devices.contains(&id) {
                devices.push(id);
            }
        }
    }
    devices
}

/// Runs the daemon until Ctrl-C or the exit policy says so.
pub fn run_daemon(
    home: &Path,
    config: &Config,
    identity: DeviceIdentity,
    options: DaemonOptions,
    exit: ExitPolicy,
) -> io::Result<()> {
    let ctx = Arc::new(DaemonContext::new(home, config, identity, options));
    let control = Arc::clone(&ctx.control);
    ctrlc::set_handler(move || control.request_stop())
        .map_err(|e| io::Error::other(format!("Cannot install the Ctrl-C handler: {e}")))?;

    let mut handles = Vec::new();
    if let Some(addr) = ctx.options.listen {
        match listener::bind(&ctx, addr) {
            Ok(tcp) => {
                let c = Arc::clone(&ctx);
                spawn("listener", &mut handles, move || {
                    listener::run_listener(c, tcp)
                });
            }
            Err(e) => log(&format!(
                "Cannot listen on {addr} ({e}): outgoing connections only"
            )),
        }
    }
    if ctx.options.local_discovery {
        let c = Arc::clone(&ctx);
        spawn("lan", &mut handles, move || lan::run_lan_discovery(c));
    }
    if ctx.options.relays {
        let c = Arc::clone(&ctx);
        spawn("relay", &mut handles, move || relay::run_relay(c));
    }
    if ctx.options.global_discovery {
        let c = Arc::clone(&ctx);
        spawn("announce", &mut handles, move || {
            global::run_global_announcer(c)
        });
    }
    for peer in ctx.peers.clone() {
        let c = Arc::clone(&ctx);
        spawn("dialer", &mut handles, move || dialer::run_dialer(c, peer));
    }

    let required = devices_to_sync(config);
    let started = Instant::now();
    while !ctx.is_stopping() {
        if exit.after.is_some_and(|d| started.elapsed() >= d) {
            log("Exit time reached");
            break;
        }
        if exit.when_synced
            && ctx
                .table()
                .activity
                .is_synced(&required, SYNCED_QUIET_PERIOD)
        {
            log("Every device is in sync");
            break;
        }
        std::thread::sleep(STOP_CHECK_STEP);
    }

    ctx.control.request_stop();
    let deadline = Instant::now() + SHUTDOWN_GRACE;
    while Instant::now() < deadline && handles.iter().any(|h| !h.is_finished()) {
        std::thread::sleep(STOP_CHECK_STEP);
    }
    for folder in &config.folders {
        ctx.storage.flush_folder_db(&folder.id);
    }
    log("Stopped");
    Ok(())
}
