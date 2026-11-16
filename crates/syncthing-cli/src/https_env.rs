//! HTTPS client settings from the environment: the proxy (`HTTPS_PROXY`, `https_proxy`,
//! `ALL_PROXY`, `all_proxy`, like Syncthing) and extra trusted roots (`SSL_CERT_FILE`, e.g.
//! the CA of a TLS-inspecting proxy). They apply to Global Discovery and the relay pool;
//! connections to peers and relays are always direct.

use syncthing_core::{HttpProxy, HttpsClient};

const PROXY_VARS: [&str; 4] = ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"];
const CA_FILE_VAR: &str = "SSL_CERT_FILE";

/// The HTTPS client described by the environment, with a description of what was used (for
/// the log; proxy credentials are never included).
pub fn https_client_from_env() -> (HttpsClient, Vec<String>) {
    let mut client = HttpsClient::default();
    let mut notes = Vec::new();
    if let Some((var, value)) = PROXY_VARS.iter().find_map(|v| {
        std::env::var(v)
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| (*v, s))
    }) {
        match HttpProxy::parse(&value) {
            Some(proxy) => {
                notes.push(format!(
                    "HTTPS proxy {}:{} (from {var})",
                    proxy.host, proxy.port
                ));
                client.proxy = Some(proxy);
            }
            None => notes.push(format!("{var} ignored: only http:// proxies are supported")),
        }
    }
    if let Ok(path) = std::env::var(CA_FILE_VAR) {
        match std::fs::read(&path) {
            Ok(pem) => {
                let added = client.add_pem_roots(&pem);
                notes.push(format!("{added} extra trusted certificate(s) from {path}"));
            }
            Err(e) => notes.push(format!("{CA_FILE_VAR} {path} unreadable: {e}")),
        }
    }
    (client, notes)
}
