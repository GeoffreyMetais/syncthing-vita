//! `vita-syncthing-app` library: the four Clean Architecture layers of the PS Vita application.
//!
//! - `domain`: Pure domain models (`KnownDevice`, `ShareConfigEntry`, `PickerItem`) and `AppState`.
//! - `application`: Use cases (`ShareService`), `SyncCoordinator`, and `InputController`.
//! - `infrastructure`: `ConfigRepository`, `IdentityStore`, `VitaFramebuffer`, `VitaInput`,
//!   `init_vita_network`, and background `spawn_sync_worker`.
//! - `ui`: Pure presentation & rendering layer (`VitaUiRenderer`, `screens`, `modals`, `widgets`, `theme`, `gfx`).
//!
//! The executable (`main.rs`) is only the composition root; exposing the layers as a library
//! lets host integration tests (`tests/`) exercise them. This file only declares modules.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod ui;
