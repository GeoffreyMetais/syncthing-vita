//! Life cycle of the background sync worker (`SyncSupervisor`): when it starts, is replaced
//! after a configuration change, is asked for a rescan or a discovery, and is restarted after
//! it ended. The composition root only forwards user commands and calls `tick` once per loop.

use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Instant;

use syncthing_core::{DeviceIdentity, SyncControl};

use crate::application::{DeviceOfferService, FolderOfferService, ShareService, SyncCoordinator};
use crate::domain::defaults::{AUTO_RECONNECT_DELAY, QUIT_WORKER_WAIT};
use crate::domain::{AppState, KnownDevice, ModalDialog, UiText};
use crate::infrastructure::{spawn_sync_worker, wait_for_worker, ConfigRepository, WorkerMsg};

/// Owns the running sync worker (its message channel and control handle), if any.
pub struct SyncSupervisor {
    identity: DeviceIdentity,
    worker_rx: Option<Receiver<WorkerMsg>>,
    control: Option<Arc<SyncControl>>,
    last_finished: Instant,
}

impl SyncSupervisor {
    pub fn new(identity: DeviceIdentity) -> Self {
        Self {
            identity,
            worker_rx: None,
            control: None,
            last_finished: Instant::now(),
        }
    }

    /// True while a worker runs (connected or not).
    pub fn is_running(&self) -> bool {
        self.worker_rx.is_some()
    }

    /// Application start: always starts the worker so the Vita is reachable inbound (TCP 22000,
    /// LAN announce, Relay, Global Discovery) even with 0 paired devices, while dialing out only
    /// to `state.sync_peers()`.
    pub fn start_at_launch(&mut self, state: &mut AppState, repo: &ConfigRepository) {
        let approved = state.approved_devices();
        let dial = state.sync_peers();
        if approved.is_empty() {
            state.peer_label = UiText::NoPeerLinked;
            state.status_line = UiText::SelectDeviceToConnect;
        }
        self.spawn(state, repo, approved, dial);
    }

    /// The user asked for a sync: a connected worker rescans its folders at once; otherwise
    /// the worker is (re)started with the current configuration, dialing every approved device.
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
        let approved = state.approved_devices();
        let dial = approved.clone();
        self.spawn(state, repo, approved, dial);
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
        let mut changed = false;

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
            let approved = state.approved_devices();
            let dial = state.sync_peers();
            self.spawn(state, repo, approved, dial);
        }

        let was_running = self.is_running();
        changed |= SyncCoordinator::process_worker_messages(state, repo, &mut self.worker_rx);
        DeviceOfferService::prompt_next(state);
        FolderOfferService::prompt_next(state);
        if was_running && !self.is_running() {
            self.control = None;
            self.last_finished = Instant::now();
        }

        // A worker that ended is restarted after a short delay so inbound reachability stays up.
        if !self.is_running()
            && state.auto_reconnect_enabled
            && self.last_finished.elapsed() >= AUTO_RECONNECT_DELAY
        {
            changed = true;
            let approved = state.approved_devices();
            let dial = state.sync_peers();
            self.spawn(state, repo, approved, dial);
        }
        changed
    }

    fn request_stop(&self) {
        if let Some(control) = &self.control {
            control.request_stop();
        }
    }

    /// Starts a worker accepting `approved` devices inbound and dialing `dial` peers outbound,
    /// replacing the previous one (which must have been asked to stop: `worker_slot` makes the
    /// new one wait for it).
    fn spawn(
        &mut self,
        state: &mut AppState,
        repo: &ConfigRepository,
        approved: Vec<KnownDevice>,
        dial: Vec<KnownDevice>,
    ) {
        if !dial.is_empty() {
            state.progress_pct = None;
        }
        let offered = std::mem::take(&mut state.pending_folder_offers);
        let (rx, control) = spawn_sync_worker(
            repo.base_data_dir(),
            self.identity.clone(),
            state.shares.clone(),
            approved,
            dial,
            offered,
        );
        self.worker_rx = Some(rx);
        self.control = Some(control);
    }
}
