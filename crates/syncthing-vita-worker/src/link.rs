//! Rust mirror of `plugin/include/syncthing_plugin.h`: the link the launcher shares with the
//! worker, and the codes both exchange through it. `tests/plugin_contract.rs` checks that the
//! values agree with the C header.

use std::sync::atomic::{AtomicI32, AtomicU32};

use crate::cycle::CycleOutcome;

pub const LAUNCHER_FILE: &str = "syncthing_launcher.suprx";
pub const WORKER_FILE: &str = "syncthing_worker.suprx";

pub const GUI_RUNNING_MUTEX: &str = "SyncthingGuiRunning";
pub const LAUNCHER_ALIVE_MUTEX: &str = "SyncthingLauncherAlive";
pub const WORKER_RUNNING_MUTEX: &str = "SyncthingWorkerRunning";

pub const LINK_MAGIC: u32 = 0x5354_4C4B;
pub const LINK_VERSION: u32 = 1;

pub const STATE_LOADING: u32 = 0;
pub const STATE_RUNNING: u32 = 1;
pub const STATE_DONE: u32 = 2;
pub const STATE_FATAL: u32 = 3;

pub const RESULT_COMPLETED: i32 = 0;
pub const RESULT_ABORTED: i32 = 1;
pub const RESULT_DEADLINE: i32 = 2;
pub const RESULT_DISABLED: i32 = 3;
pub const RESULT_NOTHING_TO_SYNC: i32 = 4;
pub const RESULT_NOT_CONFIGURED: i32 = 5;
pub const RESULT_NO_NETWORK: i32 = 6;
pub const RESULT_NO_MEMORY: i32 = 7;
pub const RESULT_BAD_LINK: i32 = 8;
pub const RESULT_NO_THREAD: i32 = 9;

/// `SyncthingWorkerLink`: owned by the launcher, alive until the worker is unloaded.
/// The atomics have the layout of the C header's `volatile` fields.
#[repr(C)]
pub struct WorkerLink {
    pub magic: u32,
    pub version: u32,
    /// Launcher -> worker: non-zero asks the cycle to stop.
    pub abort: AtomicU32,
    /// Worker -> launcher: `STATE_*`.
    pub state: AtomicU32,
    /// Worker -> launcher: `RESULT_*`, written before `STATE_DONE`.
    pub result: AtomicI32,
}

impl WorkerLink {
    /// `true` if the launcher that filled this link speaks this version of the contract.
    pub fn is_compatible(&self) -> bool {
        self.magic == LINK_MAGIC && self.version == LINK_VERSION
    }
}

/// Code of a cycle's outcome in `SyncthingWorkerLink.result`.
pub fn outcome_code(outcome: &CycleOutcome) -> i32 {
    match outcome {
        CycleOutcome::Completed => RESULT_COMPLETED,
        CycleOutcome::Aborted => RESULT_ABORTED,
        CycleOutcome::DeadlineReached => RESULT_DEADLINE,
        CycleOutcome::Disabled => RESULT_DISABLED,
        CycleOutcome::NothingToSync => RESULT_NOTHING_TO_SYNC,
        CycleOutcome::NotConfigured(_) => RESULT_NOT_CONFIGURED,
    }
}
