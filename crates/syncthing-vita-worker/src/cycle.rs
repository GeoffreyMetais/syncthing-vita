//! One bounded background sync cycle: read the app's configuration, reach every peer linked to
//! an enabled share, run one single-cycle session with each (one after the other), then end.
//!
//! The cycle never outlives `CycleOptions::max_duration`, stops promptly when the abort signal
//! fires (a game or the app starts), keeps the app's folder databases consistent (sessions save
//! them as they go; a stop removes the download in progress, see `syncthing-core`'s crash-safe
//! downloads) and logs what it did in `syncthing.log`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use syncthing_core::{
    lookup_global_peer, run_discovery_cycle, Announce, BepFolder, BepHello, CycleDrain,
    SyncControl, SyncTuning, DISCOVERY_BROADCAST_ADDR,
};

use crate::abort::{AbortSignal, CycleWatcher, StopCause};
use crate::config::{DeviceRecord, WorkerConfig};
use crate::defaults::{
    CLIENT_NAME, CYCLE_DRAIN_IDLE, CYCLE_DRAIN_MAX, CYCLE_MAX_DURATION, DEVICE_NAME,
    DISCOVERY_LISTEN, GLOBAL_DISCOVERY_TIMEOUT, KEEPALIVE_INTERVAL, MAX_PENDING_OFFERS,
    WORKER_CONNECTIONS,
};
use crate::endpoints::KnownEndpoints;
use crate::log::WorkerLog;
use crate::offers::{NewOffer, OfferBook, OfferVerdict};
use crate::peer_session::{sync_with_peer, PeerOutcome, SessionShared};
use crate::plan::{SkipReason, SyncPlan};
use crate::plugin_cfg::PluginSettings;

/// How a cycle runs. [`CycleOptions::background`] is what the PS Vita worker uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleOptions {
    /// Look for peers with a LAN discovery round (UDP 21027) when their configured address
    /// does not answer.
    pub lan_discovery: bool,
    /// Ask Global Discovery (HTTPS) for peers still unreachable after the LAN.
    pub global_discovery: bool,
    /// End of each peer session once our index is published.
    pub drain: CycleDrain,
    /// The cycle is stopped once it has run this long.
    pub max_duration: Duration,
    pub keepalive_interval: Duration,
}

impl CycleOptions {
    pub fn background() -> Self {
        Self {
            lan_discovery: true,
            global_discovery: true,
            drain: CycleDrain {
                idle: CYCLE_DRAIN_IDLE,
                max: CYCLE_DRAIN_MAX,
            },
            max_duration: CYCLE_MAX_DURATION,
            keepalive_interval: KEEPALIVE_INTERVAL,
        }
    }
}

/// How a cycle ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleOutcome {
    /// Every peer was tried.
    Completed,
    /// The abort signal fired.
    Aborted,
    /// `max_duration` elapsed.
    DeadlineReached,
    /// `plugin.cfg` is missing or says `enabled=0`.
    Disabled,
    /// No enabled share is linked to a known device.
    NothingToSync,
    /// The app's identity could not be read (the app has never run).
    NotConfigured(String),
}

/// What a cycle did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleReport {
    pub outcome: CycleOutcome,
    /// Each peer tried, by name, with what came of it.
    pub peers: Vec<(String, PeerOutcome)>,
    /// Folder offers recorded in `pending_offers.cfg` by this cycle (what to notify).
    pub new_offers: Vec<NewOffer>,
}

impl CycleReport {
    fn ended(outcome: CycleOutcome) -> Self {
        Self {
            outcome,
            peers: Vec::new(),
            new_offers: Vec::new(),
        }
    }
}

/// Runs one cycle with the configuration of `data_dir` (the app's data directory).
pub fn run_cycle(data_dir: &Path, abort: AbortSignal, options: &CycleOptions) -> CycleReport {
    let log = WorkerLog::new(data_dir);
    if !PluginSettings::load(data_dir).is_some_and(|s| s.enabled) {
        return CycleReport::ended(CycleOutcome::Disabled);
    }
    if abort() {
        return CycleReport::ended(CycleOutcome::Aborted);
    }
    let config = match WorkerConfig::load(data_dir) {
        Ok(config) => config,
        Err(e) => {
            log.line(&format!("not configured ({e}): open the app once"));
            return CycleReport::ended(CycleOutcome::NotConfigured(e.to_string()));
        }
    };
    let plan = SyncPlan::from_config(&config);
    for (folder, reason) in &plan.skipped {
        if *reason == SkipReason::ProtectedPath {
            log.line(&format!(
                "share {folder} skipped: its folder holds the app's data"
            ));
        }
    }
    if plan.is_empty() {
        return CycleReport::ended(CycleOutcome::NothingToSync);
    }
    if plan.peers_left_out > 0 {
        log.line(&format!(
            "{} more device(s) linked: only the first {} are synchronized in the background",
            plan.peers_left_out,
            plan.peers.len()
        ));
    }

    let control = SyncControl::single_cycle(
        SyncTuning {
            // A single cycle never rescans: the interval only has to exceed the cycle.
            rescan_interval: options.max_duration,
            keepalive_interval: options.keepalive_interval,
        },
        options.drain,
    );
    let watcher = CycleWatcher::start(abort, Arc::clone(&control), options.max_duration);
    log.line(&format!("cycle started ({} device(s))", plan.peers.len()));
    let mut offers = OfferBook::load(data_dir);
    let peers = sync_all_peers(&config, &plan, &control, &log, options, &mut offers);
    let outcome = match watcher.finish() {
        Some(StopCause::Aborted) => CycleOutcome::Aborted,
        Some(StopCause::DeadlineReached) => CycleOutcome::DeadlineReached,
        None => CycleOutcome::Completed,
    };
    // Sessions save their databases as they go; this only writes what a stop left pending.
    for share in &config.shares {
        plan.storage.flush_folder_db(&share.folder_id);
    }
    log.line(&summary(&outcome, &peers));
    if let Err(e) = offers.save() {
        log.line(&format!("cannot record the folder offers for the app: {e}"));
    }
    CycleReport {
        outcome,
        peers,
        new_offers: offers.new_offers().to_vec(),
    }
}

/// Synchronizes with each peer of `plan` in turn, until a stop is requested.
fn sync_all_peers(
    config: &WorkerConfig,
    plan: &SyncPlan,
    control: &Arc<SyncControl>,
    log: &WorkerLog,
    options: &CycleOptions,
    offers: &mut OfferBook,
) -> Vec<(String, PeerOutcome)> {
    let mut hello = BepHello::new(DEVICE_NAME, CLIENT_NAME, env!("CARGO_PKG_VERSION"));
    hello.num_connections = WORKER_CONNECTIONS;
    let shared = SessionShared {
        identity: &config.identity,
        hello: &hello,
        storage: &plan.storage,
        control,
        log,
    };
    let mut endpoints = KnownEndpoints::default();
    let mut lan_searched = false;
    let mut full_logged = false;
    let mut results = Vec::new();
    for peer in &plan.peers {
        if control.is_stop_requested() {
            break;
        }
        let mut on_offer = |device: syncthing_core::DeviceId, folders: Vec<BepFolder>| {
            for folder in folders {
                let verdict =
                    offers.consider(config, device, &peer.name, &folder.id, &folder.label);
                match verdict {
                    OfferVerdict::Recorded => log.line(&format!(
                        "{} offers folder {}: open the app to accept it",
                        peer.name, folder.id
                    )),
                    OfferVerdict::Full if !full_logged && offers.newly_full() => {
                        full_logged = true;
                        log.line(&format!(
                            "{MAX_PENDING_OFFERS} folder offers are already waiting for the app: \
                             further offers are dropped until it is opened"
                        ));
                    }
                    _ => {}
                }
            }
        };
        let mut outcome = sync_with_peer(&shared, peer, &endpoints, &mut on_offer);
        if outcome == PeerOutcome::Unreachable && options.lan_discovery && !lan_searched {
            lan_searched = true;
            search_lan(config, &mut endpoints, log);
            outcome = sync_with_peer(&shared, peer, &endpoints, &mut on_offer);
        }
        if outcome == PeerOutcome::Unreachable
            && options.global_discovery
            && search_global(peer, &mut endpoints, log)
        {
            outcome = sync_with_peer(&shared, peer, &endpoints, &mut on_offer);
        }
        log.line(&format!("{}: {}", peer.name, describe(&outcome)));
        results.push((peer.name.clone(), outcome));
    }
    results
}

/// One LAN discovery round: announces this device and records the peers' announcements.
fn search_lan(config: &WorkerConfig, endpoints: &mut KnownEndpoints, log: &WorkerLog) {
    // The PS Vita never listens, so it announces no address (as the app does).
    let announce = Announce {
        id: config.identity.device_id,
        addresses: Vec::new(),
        instance_id: instance_id(),
    };
    match run_discovery_cycle(&announce, DISCOVERY_BROADCAST_ADDR, DISCOVERY_LISTEN) {
        Ok(result) => endpoints.add_lan(&result),
        Err(e) => log.line(&format!("LAN discovery failed: {e}")),
    }
}

/// Looks `peer` up on Global Discovery. Returns `true` if endpoints were found.
fn search_global(peer: &DeviceRecord, endpoints: &mut KnownEndpoints, log: &WorkerLog) -> bool {
    if peer.device_id.is_short_placeholder() {
        return false;
    }
    match lookup_global_peer(peer.device_id, GLOBAL_DISCOVERY_TIMEOUT) {
        Ok(found) if !found.is_empty() => {
            endpoints.add_global(peer.device_id, found);
            true
        }
        Ok(_) => false,
        Err(e) => {
            log.line(&format!(
                "Global Discovery lookup of {} failed: {e}",
                peer.name
            ));
            false
        }
    }
}

/// Instance ID of our LAN announcements (changes at every cycle, as at every app start).
fn instance_id() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
        ^ 0x5649_5441_0000_0007
}

fn describe(outcome: &PeerOutcome) -> String {
    match outcome {
        PeerOutcome::Synced(stats) => format!(
            "synchronized ({} file(s) pulled, {} block(s) sent)",
            stats.files_downloaded, stats.blocks_uploaded
        ),
        PeerOutcome::AwaitingPairing => "has not accepted this device yet".to_owned(),
        PeerOutcome::ClosedByPeer(reason) => format!("closed the session: {reason}"),
        PeerOutcome::Unreachable => "unreachable".to_owned(),
        PeerOutcome::Stopped => "skipped (cycle stopped)".to_owned(),
    }
}

fn summary(outcome: &CycleOutcome, peers: &[(String, PeerOutcome)]) -> String {
    let synced = peers
        .iter()
        .filter(|(_, o)| matches!(o, PeerOutcome::Synced(_)))
        .count();
    let end = match outcome {
        CycleOutcome::Aborted => "aborted (a game or the app started)",
        CycleOutcome::DeadlineReached => "stopped at its time limit",
        _ => "completed",
    };
    format!(
        "cycle {end}: {synced}/{} device(s) synchronized",
        peers.len()
    )
}
