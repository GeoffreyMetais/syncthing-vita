//! Infrastructure Layer: Persistence, OS filesystem, PS Vita hardware drivers (`SceSysmodule`,
//! `SceNet`, `SceDisplay`, `SceCtrl`, `SceTouch`, `SceLibKernel`), and background BEP worker thread.

pub mod config_repo;
pub mod framebuffer;
pub mod identity_store;
pub mod input;
pub mod sync_worker;
pub mod vita_system;

pub use config_repo::ConfigRepository;
pub use framebuffer::VitaFramebuffer;
pub use identity_store::IdentityStore;
pub use input::{
    InputFrame, VitaInput, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_L, BTN_LEFT, BTN_R, BTN_RIGHT,
    BTN_SELECT, BTN_SQUARE, BTN_START, BTN_TRIANGLE, BTN_UP,
};
pub use sync_worker::{spawn_sync_worker, WorkerMsg};
pub use vita_system::init_vita_network;
