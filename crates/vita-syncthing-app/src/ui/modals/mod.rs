//! Modal dialog views for `vita-syncthing-app`:
//! - `create_share`: Template & custom folder selection modal (`CreateShare`)
//! - `manage_devices`: Device link/unlink checklist modal (`ManageDevices`)
//! - `add_device`: Manual Short/Full ID entry modal (`AddDeviceById`)
//! - `fallback_keyboard`: Dedicated fallback Base32 touch/gamepad keypad component
//! - `device_id`: QR code & 8-group Device ID modal (`ShowDeviceId`)
//! - `confirm_quit`: Application exit confirmation modal (`ConfirmQuit`)

pub mod add_device;
pub mod confirm_quit;
pub mod create_share;
pub mod device_id;
pub mod fallback_keyboard;
pub mod manage_devices;

pub use add_device::render_add_device_modal;
pub use confirm_quit::render_confirm_quit_modal;
pub use create_share::render_create_share_modal;
pub use device_id::render_device_id_modal;
pub use manage_devices::render_manage_devices_modal;
