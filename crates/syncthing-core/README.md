# syncthing-core

A platform-independent, pure-Rust implementation of the [Syncthing](https://syncthing.net)
protocols: **BEP/1.0** (Block Exchange Protocol) over **TLS 1.3**, **Local Discovery v4**,
**Global Discovery v2** and the **Relay Protocol v1**.

It is the engine behind two front ends of this workspace:

- `vita-syncthing-app`, the PlayStation Vita application (`armv7-sony-vita-newlibeabihf`);
- `syncthing-cli`, a command-line client for Linux and macOS.

Both interoperate with the official Syncthing (v1.x and v2.x).

This README gives an overview of the library and how to use it. Every public module, type and
function is described in the [**API reference**](API.md).

## What it provides

| Capability | Entry points |
| :--- | :--- |
| Device identity (ECDSA P-256 certificate, Syncthing Device ID) | `DeviceIdentity`, `DeviceId` |
| Synchronizing folders with a peer (dialing or accepting) | `connect_and_sync`, `connect_and_sync_stream`, `accept_and_sync` |
| Declaring folders, where they live and who may sync them | `FsSyncStorage` |
| Controlling a long-running session from another thread | `SyncControl`, `SyncTuning` |
| Observing a session (connections, transfers, errors) | `SyncEvent` callback |
| Finding peers on the LAN | `run_discovery_cycle`, `scan_lan_bep_peers` |
| Finding peers on the Internet | `lookup_peer_on`, `lookup_global_peer`, `announce_on` |
| Going through NAT with relays | `connect_via_relay`, `RelayListener`, `fetch_relay_pool` |
| Accepting incoming connections | `accept_bep_tls_stream` + `accept_and_sync` |
| Low-level BEP codecs | `bep::*` (`BepHello`, `BepClusterConfig`, `BepIndex`, ...) |

What it does **not** do (yet): ignore patterns (`.stignore`), file versioning, symlinks
(never pulled), send-only / receive-encrypted folders, NAT-PMP / UPnP, QUIC. There is no
listener loop or scheduler either: the application owns sockets, threads and retry policies.

## Design principles

- **Pure Rust, portable.** No C or assembly crypto (`ring`, `aws-lc-rs`, `openssl` are
  forbidden): TLS is `rustls` with the `rustls-rustcrypto` provider, keys are `p256`, hashes
  `sha2`. The crate contains no `#[cfg(target_os = "vita")]`; PS Vita quirks (`sceIo` paths,
  `SceNet` timeout codes) are handled portably.
- **Blocking I/O, no async runtime.** Every network call takes an explicit `timeout`. A
  session runs on the caller's thread; run one thread per peer.
- **Consent is enforced by the library.** A folder is synchronized with a peer only if the
  application declared that peer for it (`FsSyncStorage::set_folder_devices`). A folder a peer
  merely announces is reported (`SyncEvent::ClusterConfigReceived`), never synchronized.
- **Crash safety.** Downloads are assembled in a hidden temp file and moved into place with a
  backup; folder databases are written the same way. A scan error never becomes a deletion.
- **Bounded input.** Sizes, names and hashes coming from the network are validated before
  anything is allocated or written.
- **Errors are `std::io::Result`.** Failures carry an `io::ErrorKind` and a readable message.

## Architecture

The crate is layered; dependencies only point downwards.

| Layer | Modules | I/O |
| :--- | :--- | :--- |
| 4. Sync engine | `session` | orchestrates the layers below |
| 3. Transport | `tls`, `discovery` (UDP, LAN scan, HTTP), `relay` | sockets |
| 2. Local persistence | `storage`, `local_db`, `folder_sync` | filesystem |
| 1. Wire codecs & identity | `proto`, `device_id`, `bep`, `discovery::announce`, `cert`, `version_vector` | none |

Most applications only need the items re-exported at the crate root
(`use syncthing_core::*`); the submodules expose building blocks for custom transports,
tools and tests.

## Core concepts

**Identity.** A device is an ECDSA P-256 key and a self-signed X.509 certificate.
`DeviceIdentity::from_seed(&[u8; 32])` derives both deterministically from a 32-byte secret
seed, so only the seed needs to be stored. The `DeviceId` is the SHA-256 of the certificate,
displayed as `XXXXXXX-XXXXXXX-...` (8 groups of 7 Base32 characters with Luhn check
characters).

**Storage.** `FsSyncStorage` is the registry of shared folders: for each folder ID, its
directory, the devices it is shared with, whether it is paused, its type (send & receive or
receive only), and whether it is announced in our `ClusterConfig`. It also owns the on-disk
operations (scans, block hashing, crash-safe downloads, the per-folder database).

**Session.** A session is one authenticated BEP connection to one peer:
TLS handshake → `Hello` → `ClusterConfig` exchange → indexes → delta download / upload of
blocks. Without a `SyncControl` it returns after one sync cycle; with one, it stays connected,
applies live `IndexUpdate`s, serves requests and rescans periodically until stopped or
disconnected. When both sides support it, the session opens two secondary data connections
(Syncthing's `1+2` mode).

**Events.** Every session entry point takes an `FnMut(SyncEvent)` callback, called on the
session thread for connections, per-file progress, completed cycles and errors.

**Discovery.** Peers are found on the LAN with UDP broadcasts on port 21027 (with a TCP 22000
subnet scan as a fallback), and on the Internet through Global Discovery servers. A peer
announcing a `relay://` address is reached through a relay.

## Quick start

Add the dependency (path dependency inside this workspace):

```toml
[dependencies]
syncthing-core = { path = "../syncthing-core" }
```

### 1. Create an identity

```rust
use syncthing_core::DeviceIdentity;

// Generate the seed ONCE from a cryptographically secure source and store it privately
// (the seed is the device's private key).
let seed: [u8; 32] = load_or_create_seed()?;
let identity = DeviceIdentity::from_seed(&seed)?;
println!("My Device ID: {}", identity.device_id); // Display = Syncthing string form
```

### 2. Declare the shared folders

```rust
use syncthing_core::{BepFolder, DeviceId, FolderType, FsSyncStorage};

let peer = DeviceId::parse_user_id("MFZWI3D-BONSGYC-...")?; // full ID as shown by Syncthing

// Default root: folders without a custom path live in `<root>/<folder id>`.
// Folder databases are always stored in this root (see `FsSyncStorage::db_path`).
let mut storage = FsSyncStorage::new("/home/me/.local/share/my-app/sync");

storage.set_folder_path("photos", "/home/me/Pictures");      // where the folder lives
storage.set_folder_devices("photos", vec![peer]);            // authorization gate
storage.set_folder_type("photos", FolderType::SendReceive);  // or ReceiveOnly
storage.add_local_folder(BepFolder {                          // announce it in ClusterConfig
    id: "photos".into(),
    label: "Photos".into(),
    device_ids: vec![peer],
});
```

### 3. Synchronize with a peer

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use syncthing_core::{
    connect_and_sync, BepHello, SessionParams, SyncControl, SyncEvent, SyncSessionStatus,
};

let hello = BepHello::new("My Device", "my-app", "0.1.0");
let control = SyncControl::new(); // Arc<SyncControl>, share it with the UI thread

let params = SessionParams {
    identity: &identity,
    hello: &hello,
    storage: &storage,
    expected_device_id: Some(peer),   // the TLS handshake fails for any other device
    timeout: Duration::from_secs(10),
    control: Some(Arc::clone(&control)), // None = a single sync cycle
};

let addr: SocketAddr = "192.168.1.20:22000".parse()?;
let result = connect_and_sync(&params, addr, |event| match event {
    SyncEvent::FileSynced { folder, name, .. } => println!("{folder}: {name} downloaded"),
    SyncEvent::ClusterConfigReceived { folders, .. } => {
        // Folders the peer shares; unknown ones need the user's consent.
        for f in folders { println!("peer offers {} ({})", f.id, f.label); }
    }
    SyncEvent::SessionEnded { reason, .. } => println!("disconnected: {reason}"),
    _ => {}
})?;

match result.session_status {
    SyncSessionStatus::AwaitingPairing => println!("the peer has not accepted us yet"),
    SyncSessionStatus::ClosedByPeer(reason) => println!("closed by peer: {reason}"),
    SyncSessionStatus::Active { files_downloaded, .. } => println!("{files_downloaded} files"),
}
```

From another thread: `control.request_rescan()` rescans now, `control.request_stop()` ends
the session cleanly.

### 4. Find the peer

```rust
use std::time::Duration;
use syncthing_core::{
    lookup_global_peer, resolve_peer_tcp_addrs, run_discovery_cycle, Announce,
    DISCOVERY_BROADCAST_ADDR,
};
use syncthing_core::discovery::PeerEndpoint;

// LAN: broadcast our announcement and listen for answers.
let me = Announce { id: identity.device_id, addresses: vec![], instance_id: 1 };
let cycle = run_discovery_cycle(&me, DISCOVERY_BROADCAST_ADDR, Duration::from_secs(2))?;
for (src, announce) in &cycle.discovered_peers {
    if announce.id == peer {
        let addrs = resolve_peer_tcp_addrs(*src, &announce.addresses); // try in this order
    }
}

// Internet: Global Discovery returns direct and relay endpoints.
for endpoint in lookup_global_peer(peer, Duration::from_secs(10))? {
    match endpoint {
        PeerEndpoint::Direct(addr) => { /* connect_and_sync(&params, addr, ..) */ }
        PeerEndpoint::Relay { addr, device_id } => { /* see "Relays" below */ }
    }
}
```

### 5. Accept incoming connections (optional)

The PS Vita app only dials; a desktop client usually also listens:

```rust
use std::net::TcpListener;
use syncthing_core::{accept_and_sync, accept_bep_tls_stream};

let listener = TcpListener::bind("0.0.0.0:22000")?;
for tcp in listener.incoming() {
    let tcp = tcp?;
    let addr = tcp.peer_addr()?;
    let incoming = accept_bep_tls_stream(&identity, tcp, Duration::from_secs(10))?;
    if incoming.peer_device_id() != peer {
        continue; // unknown device: drop the connection
    }
    // Usually on its own thread:
    accept_and_sync(&params, incoming, addr, |_event| {})?;
}
```

### Relays

```rust
use syncthing_core::{connect_and_sync_stream, connect_via_relay};

// Dialing a peer that announced `relay://...`:
let tcp = connect_via_relay(&identity, relay_addr, relay_id, peer, timeout)?;
let result = connect_and_sync_stream(&params, tcp, relay_addr, |_e| {})?;
```

A device behind NAT that wants to be *reachable* joins a relay permanently with
`RelayListener::join`, announces `listener.relay_uri()` with `announce_on`, and handles each
`SessionInvitation` (see [Relays](API.md#7-relays-relay) in the API reference).

## Threading model

- Every function blocks the calling thread; none spawns threads, except the LAN scan
  (`scan_lan_bep_peers`, 16 short-lived workers).
- Run one session per peer thread. `FsSyncStorage` is shared by reference (wrap it in an
  `Arc` if needed); folder databases live in a process-wide cache protected by a mutex, so
  concurrent sessions on the same folder see the same database.
- `SyncControl` is `Send + Sync` and handed out as `Arc<SyncControl>`; one handle can drive
  several sessions.
- To change the configuration (folders, devices, types), stop the sessions
  (`request_stop`), wait for them to return, build a new `FsSyncStorage` and start again.
  Never let two generations of sessions write the same folder concurrently.

## Security model in brief

- Peers are identified only by the SHA-256 of their TLS certificate. Pass
  `expected_device_id` when dialing, check `IncomingBepStream::peer_device_id()` when
  accepting.
- `FsSyncStorage::is_folder_shared_with` is the single authorization gate: indexes, block
  requests and downloads are refused for any folder not shared with the peer, or paused.
- LAN announcements are unauthenticated: treat their device IDs as hints until a TLS session
  proved them.
- Names sent by peers are validated (`storage::names::is_syncable_name`): no absolute paths,
  `..`, backslashes, control characters or Syncthing-internal names.
- Every downloaded block is checked against its SHA-256; block sizes are bounded by
  `BEP_MAX_BLOCK_SIZE` (16 MiB) and LZ4 output by its announced length.
- HTTPS to infrastructure servers accepts either the `?id=` certificate pin of the URL or a
  certificate chaining to the public roots with a matching host name — never an unverified one.

## API reference

See [`API.md`](API.md): identity, version vectors, BEP codecs, TLS, discovery, relays,
storage, folder reconciliation, folder database, sessions and constants, module by module.

## Testing

```bash
cargo test -p syncthing-core
```

Tests live in `tests/` (one file per module; none inside `src/`) and run on the host:

- codecs and identity: `bep_codec.rs`, `device_id.rs`, `identity.rs`, `version_vector.rs`, ...;
- storage and reconciliation, each in its own temp directory (`common::fresh_temp_dir`):
  `storage.rs`, `local_db.rs`, `folder_sync.rs`, ...;
- sessions against `tests/common/fake_peer.rs`, a scriptable local TLS 1.3 + BEP server
  (`FakePeer::spawn(seed, |conn| ...)`) that announces folders, sends indexes, serves blocks and
  records every message received: `session.rs`, `session_inbound.rs`, ...;
- Global Discovery, relays and proxies against local stand-in servers (never the Internet):
  `https.rs`, `global_discovery.rs`, `relay.rs`, `http_proxy.rs`.

Mutation testing covers the crate: `cargo mutants -p syncthing-core` (see the repository's
`AGENTS.md`). The crate must also build for the PS Vita: `just check-vita`.

## License

MPL-2.0, as the rest of the workspace.
