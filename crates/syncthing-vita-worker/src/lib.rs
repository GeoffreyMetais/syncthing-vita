//! `syncthing-vita-worker`: stage 2 of the PS Vita background sync plugin
//! (`syncthing_worker.suprx`), loaded into `SceShell` by the launcher
//! (`plugin/launcher`, stage 1) to run **one** bounded sync cycle, then unloaded.
//!
//! The cycle is a normal host-tested library ([`cycle::run_cycle`]); the [`vita`] module
//! (PS Vita only) holds the module entry points, the newlib runtime set-up and the link with
//! the launcher. See AGENTS.md §8 for the plugin architecture.
//!
//! This file only declares modules and re-exports the public API.

pub mod abort;
pub mod config;
pub mod cycle;
pub mod defaults;
pub mod endpoints;
pub mod link;
pub mod log;
pub mod notification;
pub mod offers;
pub mod peer_session;
pub mod plan;
pub mod plugin_cfg;
#[cfg(target_os = "vita")]
pub mod vita;

pub use abort::{never_abort, AbortSignal};
pub use cycle::{run_cycle, CycleOptions, CycleOutcome, CycleReport};
pub use peer_session::{PeerOutcome, PeerStats};
pub use plugin_cfg::PluginSettings;
