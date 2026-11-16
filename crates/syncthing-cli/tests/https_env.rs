//! HTTPS client settings from the environment: proxy variables (in order of precedence) and
//! the extra trusted CA file. Tests change environment variables, so they run one at a time.

use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard};

use syncthing_cli::https_env::https_client_from_env;

use crate::common::temp_home;

mod common;

static ENV_LOCK: Mutex<()> = Mutex::new(());

const VARS: [&str; 5] = [
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "SSL_CERT_FILE",
];

/// Holds the lock; every variable read by `https_env` is cleared, then `values` are set.
struct Env {
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<OsString>)>,
}

fn set_env(values: &[(&str, &str)]) -> Env {
    let guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let saved = VARS
        .iter()
        .map(|name| (*name, std::env::var_os(name)))
        .collect();
    for name in VARS {
        std::env::remove_var(name);
    }
    for (name, value) in values {
        std::env::set_var(name, value);
    }
    Env {
        _guard: guard,
        saved,
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }
}

#[test]
fn without_environment_there_is_no_proxy_and_no_note() {
    let _env = set_env(&[]);
    let (client, notes) = https_client_from_env();
    assert!(client.proxy.is_none());
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn an_http_proxy_is_used_and_described_without_credentials() {
    let _env = set_env(&[("HTTPS_PROXY", "http://proxy.example:3128")]);
    let (client, notes) = https_client_from_env();
    let proxy = client.proxy.expect("proxy configured");
    assert_eq!(proxy.host, "proxy.example");
    assert_eq!(proxy.port, 3128);
    assert_eq!(
        notes,
        vec!["HTTPS proxy proxy.example:3128 (from HTTPS_PROXY)".to_owned()]
    );
}

#[test]
fn the_proxy_variables_are_read_in_order_of_precedence() {
    // Each case sets every variable from the first one down to the expected winner.
    let cases: [(&[(&str, &str)], &str, &str, u16); 4] = [
        (
            &[
                ("HTTPS_PROXY", "http://a:1"),
                ("https_proxy", "http://b:2"),
                ("ALL_PROXY", "http://c:3"),
                ("all_proxy", "http://d:4"),
            ],
            "HTTPS_PROXY",
            "a",
            1,
        ),
        (
            &[
                ("https_proxy", "http://b:2"),
                ("ALL_PROXY", "http://c:3"),
                ("all_proxy", "http://d:4"),
            ],
            "https_proxy",
            "b",
            2,
        ),
        (
            &[("ALL_PROXY", "http://c:3"), ("all_proxy", "http://d:4")],
            "ALL_PROXY",
            "c",
            3,
        ),
        (&[("all_proxy", "http://d:4")], "all_proxy", "d", 4),
    ];
    for (env, var, host, port) in cases {
        let _env = set_env(env);
        let (client, notes) = https_client_from_env();
        let proxy = client.proxy.expect("proxy configured");
        assert_eq!((proxy.host.as_str(), proxy.port), (host, port), "{var}");
        assert_eq!(notes.len(), 1);
        assert!(notes[0].ends_with(&format!("(from {var})")), "{notes:?}");
    }
}

#[test]
fn an_empty_proxy_variable_is_skipped() {
    let _env = set_env(&[
        ("HTTPS_PROXY", ""),
        ("https_proxy", "http://fallback.example:8080"),
    ]);
    let (client, notes) = https_client_from_env();
    assert_eq!(client.proxy.expect("proxy").host, "fallback.example");
    assert!(notes[0].ends_with("(from https_proxy)"), "{notes:?}");
}

#[test]
fn a_secure_proxy_is_refused_with_a_note() {
    let _env = set_env(&[("HTTPS_PROXY", "https://proxy.example:443")]);
    let (client, notes) = https_client_from_env();
    assert!(client.proxy.is_none());
    assert_eq!(
        notes,
        vec!["HTTPS_PROXY ignored: only http:// proxies are supported".to_owned()]
    );
}

#[test]
fn an_unreadable_ca_file_is_reported_with_its_path() {
    let _env = set_env(&[("SSL_CERT_FILE", "/nonexistent/syncthing-cli/ca.pem")]);
    let (client, notes) = https_client_from_env();
    assert!(client.proxy.is_none());
    assert_eq!(notes.len(), 1);
    assert!(
        notes[0].starts_with("SSL_CERT_FILE /nonexistent/syncthing-cli/ca.pem unreadable: "),
        "{notes:?}"
    );
}

#[test]
fn a_ca_file_without_certificates_adds_none() {
    let dir = temp_home("https-env-ca");
    let path = dir.join("ca.pem");
    std::fs::write(&path, "this is not a certificate\n").expect("write CA file");
    let path_text = path.to_str().expect("utf-8 path").to_owned();
    let _env = set_env(&[("SSL_CERT_FILE", &path_text)]);
    let (_client, notes) = https_client_from_env();
    assert_eq!(
        notes,
        vec![format!("0 extra trusted certificate(s) from {path_text}")]
    );
}

#[test]
fn proxy_note_comes_before_the_ca_note() {
    let dir = temp_home("https-env-both");
    let path = dir.join("missing.pem");
    let path_text = path.to_str().expect("utf-8 path").to_owned();
    let _env = set_env(&[
        ("HTTPS_PROXY", "http://p.example:3128"),
        ("SSL_CERT_FILE", &path_text),
    ]);
    let (client, notes) = https_client_from_env();
    assert!(client.proxy.is_some());
    assert_eq!(notes.len(), 2);
    assert!(notes[0].starts_with("HTTPS proxy"), "{notes:?}");
    assert!(notes[1].starts_with("SSL_CERT_FILE"), "{notes:?}");
}
