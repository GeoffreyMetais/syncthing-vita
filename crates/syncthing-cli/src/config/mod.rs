//! Persistent configuration (`config.toml`): devices, folders and general options.

mod devices;
mod error;
mod folders;
mod model;
mod options;
mod store;

pub use error::ConfigError;
pub use model::{parse_device_id, Config, DeviceConfig, FolderConfig};
pub use store::{load_config, save_config};
