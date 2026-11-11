//! Modal dialog views for `vita-syncthing-app`:
//! - `create_share`: Template & custom folder selection modal (`CreateShare`)
//! - `manage_devices`: Device link/unlink checklist modal (`ManageDevices`)
//! - `add_device`: Manual Short/Full ID entry modal (`AddDeviceById`)
//! - `fallback_keyboard`: Dedicated fallback Base32 touch/gamepad keypad component
//! - `scan_qr`: Rear-camera QR code viewfinder modal (`ScanDeviceQr`)
//! - `device_id`: QR code & 8-group Device ID modal (`ShowDeviceId`)
//! - `confirm_quit`: Application exit confirmation modal (`ConfirmQuit`)
//! - `confirm_delete_share`: Share deletion confirmation modal (`ConfirmDeleteShare`)
//! - `folder_offer`: Consent dialog for a folder offered by a remote device (`FolderOffer`)

pub mod add_device;
pub mod confirm_delete_share;
pub mod confirm_quit;
pub mod create_share;
pub mod device_id;
pub mod fallback_keyboard;
pub mod folder_offer;
pub mod manage_devices;
pub mod scan_qr;

pub use add_device::render_add_device_modal;
pub use confirm_delete_share::render_confirm_delete_share_modal;
pub use confirm_quit::render_confirm_quit_modal;
pub use create_share::render_create_share_modal;
pub use device_id::render_device_id_modal;
pub use folder_offer::render_folder_offer_modal;
pub use manage_devices::render_manage_devices_modal;
pub use scan_qr::render_scan_qr_modal;
