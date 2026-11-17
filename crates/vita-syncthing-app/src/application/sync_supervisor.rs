//! Life cycle of the background sync worker (`SyncSupervisor`): when it starts, is replaced
//! after a configuration change, is asked for a rescan or a discovery, and is restarted after
//! it ended. The composition root only forwards user commands and calls `tick` once per loop.
//!
//! The first worker waits for a background worker of the plugin to stop
//! (`BackgroundHandover`); folder offers the plugin recorded meanwhile are imported once that
//! wait is over, so the plugin can no longer add any.

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use syncthing_core::{DeviceIdentity, SyncControl};

use crate::application::background_handover::{BackgroundHandover, HandoverStep, ReleaseReason};
use crate::application::{FolderOfferService, ShareService, SyncCoordinator};
use crate::domain::defaults::{AUTO_RECONNECT_DELAY, QUIT_WORKER_WAIT};
use crate::domain::{AppState, KnownDevice, ModalDialog, UiText};
use crate::infrastructure::{
    spawn_sync_worker, wait_for_worker, BackgroundWorkerProbe, ConfigRepository,
    NoBackgroundWorker, WorkerMsg,
};

/// Owns the running sync worker (its message channel and control handle), if any.
pub struct SyncSupervisor {
    identity: DeviceIdentity,
    worker_rx: Option<Receiver<WorkerMsg>>,
    control: Option<Arc<SyncControl>>,
    last_finished: Instant,
    /// Wait for a background worker of the plugin before the first worker.
    handover: BackgroundHandover,
    background: Box<dyn BackgroundWorkerProbe>,
    /// Peers of a worker start postponed by the hand-over.
    deferred: Option<Vec<KnownDevice>>,
}

impl SyncSupervisor {
    /// Supervisor of a system without the background sync plugin.
    pub fn new(identity: DeviceIdentity) -> Self {
        Self::with_background_probe(identity, Box::new(NoBackgroundWorker))
    }

    /// Supervisor whose first worker waits while `background` reports a running background
    /// worker (at most `BACKGROUND_WORKER_WAIT`).
    pub fn with_background_probe(
        identity: DeviceIdentity,
        background: Box<dyn BackgroundWorkerProbe>,
    ) -> Self {
        Self {
            identity,
            worker_rx: None,
            control: None,
            last_finished: Instant::now(),
            handover: BackgroundHandover::new(),
            background,
            deferred: None,
        }
    }

    /// True while a worker start waits for the background worker to stop.
    pub fn is_waiting_for_background(&self) -> bool {
        self.deferred.is_some()
    }

    /// Polls the hand-over at `now`. On release: logs it, imports the background worker's
    /// folder offers and starts the postponed worker. Returns `true` if that changed the screen
    /// (a postponed worker started, offers were imported).
    fn poll_handover(
        &mut self,
        state: &mut AppState,
        repo: &ConfigRepository,
        now: Instant,
    ) -> bool {
        let running = !self.handover.is_released() && self.background.worker_running();
        let HandoverStep::Released(reason) = self.handover.check(running, now) else {
            return false;
        };
        let line = match reason {
            ReleaseReason::NoWorker => None,
            ReleaseReason::Stopped(waited) => Some(format!(
                "Background sync worker stopped after {} ms.",
                waited.as_millis()
            )),
            ReleaseReason::TimedOut => {
                Some("Background sync worker still running: starting anyway.".to_owned())
            }
        };
        if let Some(line) = line {
            ShareService::log_event(state, repo, &line);
        }
        let imported = FolderOfferService::import_pending(state, repo);
        let deferred = self.deferred.take();
        let resumed = deferred.is_some();
        if let Some(peers) = deferred {
            state.peer_label = UiText::ReadyManualPairing;
            state.status_line = UiText::StartingSync;
            self.spawn(state, repo, peers);
        }
        imported > 0 || resumed
    }

    /// True while a worker runs (connected or not).
    pub fn is_running(&self) -> bool {
        self.worker_rx.is_some()
    }

    /// Application start: connects to the devices linked to an enabled share (or to every
    /// approved device while no share exists yet), if any.
    pub fn start_at_launch(&mut self, state: &mut AppState, repo: &ConfigRepository) {
        self.poll_handover(state, repo, Instant::now());
        let peers = state.sync_peers();
        if peers.is_empty() {
            state.peer_label = UiText::NoPeerLinked;
            state.status_line = UiText::SelectDeviceToConnect;
        } else {
            self.spawn(state, repo, peers);
        }
    }

    /// The user asked for a sync: a connected worker rescans its folders at once; otherwise
    /// the worker is (re)started with the current configuration.
    pub fn start_sync(&mut self, state: &mut AppState, repo: &ConfigRepository) {
        state.auto_reconnect_enabled = true;
        if self.is_running() && state.session_connected && !state.config_dirty {
            state.prepare_manual_sync();
            ShareService::log_event(
                state,
                repo,
                "Triggering live local rescan over persistent 1+2 connection...",
            );
            if let Some(control) = &self.control {
                control.request_rescan();
            }
            return;
        }
        self.request_stop();
        state.config_dirty = false;
        state.reset_sessions();
        state.prepare_manual_sync();
        ShareService::log_event(state, repo, "Starting BEP/1.0 synchronization...");
        let peers = state.approved_devices();
        self.spawn(state, repo, peers);
    }

    /// Asks the running worker for a LAN discovery cycle now. Returns `false` if no worker
    /// runs (a sync must be started instead: it begins with a discovery).
    pub fn request_discovery(&self) -> bool {
        match &self.control {
            Some(control) => {
                control.request_discovery();
                true
            }
            None => false,
        }
    }

    /// The app is closing: stops the worker and gives its sessions a moment to close and
    /// save their state.
    pub fn shutdown(&mut self) {
        self.request_stop();
        wait_for_worker(QUIT_WORKER_WAIT);
    }

    /// One main-loop iteration: hands over pending revert requests, replaces the worker after
    /// a configuration change, applies the worker's messages to `state` and restarts a worker
    /// that ended. Returns `true` if the screen must be redrawn.
    pub fn tick(&mut self, state: &mut AppState, repo: &ConfigRepository) -> bool {
        self.tick_at(state, repo, Instant::now())
    }

    /// `tick` at `now` (the hand-over deadline is checked against it).
    pub fn tick_at(&mut self, state: &mut AppState, repo: &ConfigRepository, now: Instant) -> bool {
        let mut changed = self.poll_handover(state, repo, now);

        // Revert requests of read-only shares go to the running worker (its sessions take
        // them), unless that worker is about to be replaced by one with the new configuration.
        if let (Some(control), false) = (&self.control, state.config_dirty) {
            for folder_id in std::mem::take(&mut state.pending_reverts) {
                control.request_revert(&folder_id);
            }
        }

        // A configuration change applies once the dialogs are closed.
        if state.config_dirty && state.modal == ModalDialog::None {
            changed = true;
            state.config_dirty = false;
            self.request_stop();
            self.worker_rx = None;
            self.control = None;
            state.reset_sessions();
            state.sync_in_progress = false;
            let peers = state.sync_peers();
            if !peers.is_empty() {
                self.spawn(state, repo, peers);
            }
        }

        let was_running = self.is_running();
        changed |= SyncCoordinator::process_worker_messages(state, repo, &mut self.worker_rx);
        FolderOfferService::prompt_next(state);
        if was_running && !self.is_running() {
            self.control = None;
            self.last_finished = Instant::now();
        }

        // A worker that ended (session dropped) is restarted after a short delay.
        if !self.is_running()
            && self.deferred.is_none()
            && state.auto_reconnect_enabled
            && self.last_finished.elapsed() >= AUTO_RECONNECT_DELAY
        {
            let peers = state.sync_peers();
            if !peers.is_empty() {
                changed = true;
                self.spawn(state, repo, peers);
            }
        }
        changed
    }

    fn request_stop(&self) {
        if let Some(control) = &self.control {
            control.request_stop();
        }
    }

    /// Starts a worker for `peers` with the current shares, replacing the previous one (which
    /// must have been asked to stop: `worker_slot` makes the new one wait for it).
    fn spawn(&mut self, state: &mut AppState, repo: &ConfigRepository, peers: Vec<KnownDevice>) {
        if !self.handover.is_released() {
            if self.deferred.is_none() {
                ShareService::log_event(
                    state,
                    repo,
                    "Waiting for the background sync worker to stop...",
                );
            }
            state.peer_label = UiText::BackgroundSyncStopping;
            state.status_line = UiText::WaitingForBackgroundSync;
            self.deferred = Some(peers);
            return;
        }
        state.progress_pct = None;
        let offered = std::mem::take(&mut state.pending_folder_offers);
        let (rx, control) = spawn_sync_worker(
            repo.base_data_dir(),
            self.identity.clone(),
            state.shares.clone(),
            peers,
            offered,
        );
        self.worker_rx = Some(rx);
        self.control = Some(control);
    }
}
