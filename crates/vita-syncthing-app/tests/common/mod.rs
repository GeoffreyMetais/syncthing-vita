//! Shared helpers for `vita-syncthing-app` integration tests.

/// Creates an empty, unique directory under `/tmp` for one test.
pub fn temp_test_dir(suffix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = format!("/tmp/vita-syncthing-test-{}-{}", suffix, nanos);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
