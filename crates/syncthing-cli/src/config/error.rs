//! Errors of configuration edits and validation.

use std::fmt;
use std::io;

/// A rejected configuration edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// The text is not a full device ID (`detail` holds the input and the reason).
    InvalidDeviceId(String),
    /// A device with this ID is already configured.
    DeviceExists(String),
    /// No device with this ID is configured.
    UnknownDevice(String),
    /// A folder with this ID already exists.
    FolderExists(String),
    /// No folder with this ID is configured.
    UnknownFolder(String),
    /// The value is not valid for the option or field `key`.
    InvalidValue { key: String, value: String },
    /// No option with this name exists.
    UnknownKey(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDeviceId(detail) => write!(f, "invalid device ID {detail}"),
            Self::DeviceExists(id) => write!(f, "device {id} is already configured"),
            Self::UnknownDevice(id) => write!(f, "device {id} is not configured"),
            Self::FolderExists(id) => write!(f, "folder {id} already exists"),
            Self::UnknownFolder(id) => write!(f, "folder {id} does not exist"),
            Self::InvalidValue { key, value } => {
                write!(f, "invalid value `{value}` for `{key}`")
            }
            Self::UnknownKey(key) => write!(f, "unknown option `{key}`"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<ConfigError> for io::Error {
    fn from(err: ConfigError) -> Self {
        io::Error::new(io::ErrorKind::InvalidInput, err)
    }
}
