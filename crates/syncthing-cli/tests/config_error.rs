//! Messages of configuration errors, as printed to the user.

use syncthing_cli::config::ConfigError;

#[test]
fn every_error_names_its_subject() {
    let cases = [
        (
            ConfigError::InvalidDeviceId("ABC".into()),
            "invalid device ID ABC",
        ),
        (
            ConfigError::DeviceExists("D1".into()),
            "device D1 is already configured",
        ),
        (
            ConfigError::UnknownDevice("D2".into()),
            "device D2 is not configured",
        ),
        (
            ConfigError::FolderExists("docs".into()),
            "folder docs already exists",
        ),
        (
            ConfigError::UnknownFolder("pics".into()),
            "folder pics does not exist",
        ),
        (
            ConfigError::InvalidValue {
                key: "listen".into(),
                value: "nope".into(),
            },
            "invalid value `nope` for `listen`",
        ),
        (
            ConfigError::UnknownKey("colour".into()),
            "unknown option `colour`",
        ),
    ];
    for (err, text) in cases {
        assert_eq!(err.to_string(), text);
    }
}

#[test]
fn config_errors_convert_to_invalid_input_io_errors() {
    let io: std::io::Error = ConfigError::UnknownKey("x".into()).into();
    assert_eq!(io.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(io.to_string(), "unknown option `x`");
}
