//! General options settable with `config set`.

use std::net::SocketAddr;

use super::error::ConfigError;
use super::model::Config;

impl Config {
    /// Sets a general option: `name`, `listen`, `local-discovery`, `global-discovery`,
    /// `relays` or `rescan-interval` (seconds).
    pub fn set_option(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        let value = value.trim();
        match key {
            "name" => {
                if value.is_empty() {
                    return Err(invalid(key, value));
                }
                self.device_name = value.to_owned();
            }
            "listen" => {
                value
                    .parse::<SocketAddr>()
                    .map_err(|_| invalid(key, value))?;
                self.listen = value.to_owned();
            }
            "local-discovery" => self.local_discovery = parse_bool(key, value)?,
            "global-discovery" => self.global_discovery = parse_bool(key, value)?,
            "relays" => self.relays = parse_bool(key, value)?,
            "rescan-interval" => {
                self.rescan_interval_secs = value
                    .parse::<u64>()
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or_else(|| invalid(key, value))?;
            }
            _ => return Err(ConfigError::UnknownKey(key.to_owned())),
        }
        Ok(())
    }
}

fn parse_bool(key: &str, value: &str) -> Result<bool, ConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Ok(true),
        "false" | "off" | "no" | "0" => Ok(false),
        _ => Err(invalid(key, value)),
    }
}

fn invalid(key: &str, value: &str) -> ConfigError {
    ConfigError::InvalidValue {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}
