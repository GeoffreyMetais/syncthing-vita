# syncthing-core — API reference

Detailed reference of every public module of `syncthing-core`, grouped by layer (lowest
first). For an overview of the library, its concepts and a quick start, see the
[README](README.md).

Items marked **(root)** are re-exported at the crate root. All fallible functions return
`std::io::Result` unless stated otherwise.

## Contents

1. [Identity: `cert`, `device_id`](#1-identity-cert-device_id)
2. [Version vectors: `version_vector`](#2-version-vectors-version_vector)
3. [Protobuf primitives: `proto`](#3-protobuf-primitives-proto)
4. [BEP codecs: `bep`](#4-bep-codecs-bep)
5. [TLS transport: `tls`](#5-tls-transport-tls)
6. [Discovery: `discovery`](#6-discovery-discovery)
7. [Relays: `relay`](#7-relays-relay)
8. [Folder storage: `storage`](#8-folder-storage-storage)
9. [Folder reconciliation: `folder_sync`](#9-folder-reconciliation-folder_sync)
10. [Folder database: `local_db`](#10-folder-database-local_db)
11. [Sync sessions: `session`](#11-sync-sessions-session)
12. [Constants](#12-constants)

## 1. Identity: `cert`, `device_id`

### `DeviceIdentity` (root)

```rust
pub struct DeviceIdentity {
    pub cert_der: Vec<u8>,      // self-signed X.509 certificate (DER)
    pub key_pkcs8_der: Vec<u8>, // private key (PKCS#8 DER)
    pub device_id: DeviceId,    // SHA-256 of cert_der
}
impl DeviceIdentity {
    pub fn from_seed(seed: &[u8; 32]) -> io::Result<Self>;
}
```

`from_seed` is deterministic: the same seed always yields the same key, certificate and
`DeviceId`. It fails if the seed is not a valid P-256 scalar (probability ~2⁻¹²⁸ for random
bytes). The library never generates seeds: use the platform's secure RNG and keep the seed
private.

### `DeviceId` (root)

`pub struct DeviceId(pub [u8; 32]);` — `Copy`, `Eq`, `Hash`. `Display` and `Debug` print the
Syncthing string form.

| Method | Description |
| :--- | :--- |
| `to_syncthing_string(&self) -> String` | 56 Base32 characters with 4 Luhn check characters, grouped `XXXXXXX-...` (8 groups). |
| `short(&self) -> String` | First 7 characters (Syncthing's "short ID"), for logs. |
| `to_hex(&self) -> String` / `from_hex(&str) -> Option<Self>` | 64-char lowercase hex, for configuration files. |
| `parse_user_id(&str) -> Result<Self, DeviceIdParseError>` | Parses user input: full 56-char ID (hyphens / spaces optional, Luhn-checked, `0`/`1`/`8` look-alikes accepted), 52-char raw Base32, 64-char hex, or a **short prefix** of 4 to 51 Base32 characters. |
| `is_short_placeholder(&self) -> bool` | `true` for an ID parsed from a prefix (it is not a real certificate hash). |
| `syncthing_prefix(&self) -> String` | The entered prefix of a placeholder, or all 56 characters (no hyphens). |
| `matches_peer(&self, other: &Self) -> bool` | Equality, or prefix match when one side is a placeholder. Used by every authorization check, so a device configured by short ID is recognized once it connects. |

A placeholder can be passed as `expected_device_id` or in `set_folder_devices`; replace it
with the full ID observed in `PeerHandshakeResult::peer_device_id` once a session proved it.

### `DeviceIdParseError` (root)

`Empty`, `InvalidChar(char)`, `TooShort`, `TooLong(usize)`,
`ChecksumMismatch { first_group: usize }` (1-based group pair with the typo), `DecodeFailed`.
Implements `Display` (English) and `std::error::Error`; match on the variants to translate.

`device_id::base32` exposes the RFC 4648 Base32 / Luhn mod 32 primitives used above.

## 2. Version vectors: `version_vector`

Syncthing decides which side of a change is newer with version vectors
(`Vec<BepVersionCounter>`), not timestamps.

| Function | Description |
| :--- | :--- |
| `short_device_id(id: DeviceId) -> u64` | The 64-bit counter key of a device (first 8 bytes, big-endian). |
| `version_ge(a, b) -> bool` | `a` is equal to or newer than `b` for every counter. Two vectors where neither is `ge` the other are concurrent (conflict). |
| `version_merge(a, b) -> Vec<_>` | Element-wise maximum, sorted by device. |
| `version_bump(v, short_id) -> Vec<_>` | Records a local change: our counter becomes `max(current + 1, now in seconds)`, as Syncthing does. |

## 3. Protobuf primitives: `proto`

Minimal protobuf wire helpers shared by every codec: `encode_varint`, `decode_varint`
(returns `(value, bytes read)`), `encode_bytes_field`, `encode_varint_field`, and `WireIter`,
an iterator over the `(field number, wire value)` pairs of a message. Only needed to write new
codecs.

## 4. BEP codecs: `bep`

Pure encode / decode functions on bytes (no sockets, no filesystem), one file per message
family. Specification: <https://docs.syncthing.net/specs/bep-v1.html>.

### Framing

| Item | Description |
| :--- | :--- |
| `write_bep_message(stream, msg_type, payload)` (root) | Writes one uncompressed post-`Hello` frame `[u16 header len][Header][u32 len][payload]` and flushes. |
| `read_bep_message(stream) -> (BepMessageType, Vec<u8>)` (root) | Reads one frame, decompressing LZ4 payloads. |
| `read_bep_message_after_first_byte(stream, first_byte)` | Same, when the first byte was already consumed (polling). |
| `decompress_bep_lz4(input) -> Vec<u8>` (root) | LZ4 block decompression of a compressed frame (4-byte big-endian length prefix, output bounded by that length). |
| `BepHeader { msg_type, compression }` (root) | Frame header; `encode()` / `decode()`. |
| `BepMessageType` (root) | `ClusterConfig`, `Index`, `IndexUpdate`, `Request`, `Response`, `DownloadProgress`, `Ping`, `Close`, `Unknown(u8)`; `From<u64>` and `as_u64()`. |
| `BepCompression` (root) | `None`, `Lz4`. |

### `BepHello` (root)

Pre-authentication handshake message, exchanged right after TLS.

```rust
pub struct BepHello {
    pub device_name: String,
    pub client_name: String,
    pub client_version: String,
    pub num_connections: i32, // BEP_DEFAULT_NUM_CONNECTIONS (3) = 1 primary + 2 secondary
    pub timestamp: i64,       // 0 = set by `exchange` (nanoseconds, unique per Hello)
}
```

`new(device_name, client_name, client_version)`, `encode_frame()`, `decode_frame(&[u8])`,
`decode_payload(&[u8])`, and `exchange(&self, stream) -> io::Result<BepHello>` (sends ours,
returns the peer's).

### `ClusterConfig`

| Item | Description |
| :--- | :--- |
| `BepFolder { id, label, device_ids: Vec<DeviceId> }` (root) | A folder as announced in `ClusterConfig`. Also used to declare local folders (`FsSyncStorage::add_local_folder`). |
| `BepClusterConfig { folders, secondary }` (root) | Decoded message; `secondary` marks a secondary connection's config. `decode(payload)`, `encode_secondary()`, `encode_for_peers(folders: &[(BepFolder, FolderIndexState)], own_id, own_name, peer_id)`. |
| `FolderIndexState { index_id, max_sequence, paused, folder_type }` (root) | Our per-folder index metadata announced to the peer. |
| `FolderType` (root) | `SendReceive` (default) or `ReceiveOnly`; `wire_value()` gives Syncthing's numbers (0, 2). |

### `Index` / `IndexUpdate`

```rust
pub struct BepIndex { pub folder: String, pub files: Vec<BepFileInfo> }
pub struct BepFileInfo {
    pub name: String,        // relative path with '/' separators
    pub is_dir: bool,
    pub size: i64,
    pub modified_s: i64,
    pub deleted: bool,
    pub invalid: bool,       // "do not pull this" (also used for receive-only local changes)
    pub sequence: i64,
    pub versions: Vec<BepVersionCounter>,
    pub blocks: Vec<BepBlockInfo>,
}
pub struct BepBlockInfo { pub offset: i64, pub size: i32, pub hash: Vec<u8> /* SHA-256 */ }
pub struct BepVersionCounter { pub short_device_id: u64, pub value: u64 }
```

All root re-exports. `BepIndex::decode(payload)` / `encode()`. Entries of any type other than
file or directory (symlinks) are decoded as `invalid`.

### Block transfer and close

- `BepRequest { id, folder, name, offset, size, hash }` (root) — `encode()` / `decode()`.
- `BepResponse { id, data, code }` (root) — `code`: 0 no error, 1 generic, 2 no such file.
- `BepClose { reason }` (root) — `encode()` / `decode()`.

## 5. TLS transport: `tls`

TLS 1.3 with mutual certificate authentication and ALPN `bep/1.0` (`BEP_ALPN`, root). The
identity of a peer is the SHA-256 of its certificate; certificate chains, names and dates
are not checked (as in Syncthing).

### Streams

`BepTlsStream` is an enum `Client(..)` / `Server(..)` implementing `Read + Write`, with
`get_ref()` / `get_mut()` (the underlying `TcpStream`, e.g. to set timeouts) and
`is_server()`. Session code does not care which side dialed.

### Client side (`tls::client`)

| Function | Description |
| :--- | :--- |
| `open_bep_tls_stream(identity, addr, expected: Option<DeviceId>, timeout)` | TCP connect + TLS handshake. Fails if the peer's ID does not match `expected`. Returns `(BepTlsStream, ObservedDeviceId)`; the second value (`Arc<Mutex<Option<DeviceId>>>`) receives the peer's ID during the handshake. |
| `wrap_bep_tls_stream(identity, tcp, expected, timeout)` | Same on an already connected `TcpStream` (relay tunnels). |
| `open_tls_stream_with_alpn` / `wrap_tls_stream_with_alpn` | Same with another ALPN (e.g. `b"bep-relay"`). |
| `probe_bep_peer_device_id(addr, timeout) -> Option<DeviceId>` (root) | Anonymous handshake that only reads the server certificate: tells whether a Syncthing device listens there, and which one. |

### Server side (`tls::server`)

| Item | Description |
| :--- | :--- |
| `accept_bep_tls_stream(identity, tcp, timeout) -> IncomingBepStream` (root) | Server handshake requiring a client certificate. `timeout` bounds the **whole** handshake (a trickling client cannot hold the connection); it then becomes the socket's read / write timeout. |
| `IncomingBepStream` (root) | An authenticated, not yet used connection. `peer_device_id()` returns the proven ID; pass the stream to `accept_and_sync` or drop it. |

### Polling (`tls::polling`)

- `try_read_bep_message(stream, poll_timeout) -> io::Result<Option<(BepMessageType, Vec<u8>)>>`:
  `Ok(None)` if no frame started within `poll_timeout`; once a frame starts, it is read
  completely (10 s completion timeout).
- `is_socket_timeout_or_would_block(&io::Error) -> bool`: recognizes timeouts on Unix and PS
  Vita (`SceNet` error codes).

### HTTPS to infrastructure servers (`tls::https`, `tls::proxy`)

| Item | Description |
| :--- | :--- |
| `HttpsServer { host, port, path, pinned_id }` (root) | `HttpsServer::parse("https://host[:port]/path?id=...")`. `id=` pins the certificate (its SHA-256 must match); the client-only flags `nolookup`, `noannounce`, `insecure` are removed from `path`. `host_header()`. |
| `HttpsClient { proxy: Option<HttpProxy>, extra_roots }` (root) | How requests are made. `Default` = direct, public roots (`webpki-roots`) only. `add_pem_roots(pem) -> usize` adds trusted CAs (e.g. a TLS-inspecting proxy's). |
| `HttpProxy` (root) | `HttpProxy::parse("http://[user:pass@]host[:port]")` (HTTP only, default port 80); `connect(host, port, timeout) -> TcpStream` opens a `CONNECT` tunnel, sending `Proxy-Authorization` when the URL has credentials. |
| `open_https_stream(server, client, client_identity: Option<&DeviceIdentity>, timeout)` | TLS stream to the server, presenting our certificate when given (Global Discovery announcements). |

Only infrastructure requests (Global Discovery, relay pool) use `HttpsClient`; connections to peers and relays are always
direct.

`tls::verifier` exposes the `rustls` verifiers (`SyncthingServerVerifier`,
`SyncthingClientVerifier`, `PinnedOrWebPkiVerifier`) for custom TLS setups.

## 6. Discovery: `discovery`

### Local Discovery v4 (LAN)

| Item | Description |
| :--- | :--- |
| `Announce { id, addresses: Vec<String>, instance_id }` (root) | Announcement packet; `encode_packet()` (magic + protobuf), `decode_packet(&[u8])`. Addresses are URIs such as `tcp://192.168.1.20:22000` or `tcp://0.0.0.0:22000` ("my source address"). |
| `run_discovery_cycle(own: &Announce, target_addr: &str, listen: Duration) -> DiscoveryCycleResult` (root) | Binds UDP 21027 (or an ephemeral port if busy), broadcasts to `target_addr` (usually `DISCOVERY_BROADCAST_ADDR`) and to the local `/24` broadcast, then collects announcements for `listen`. Our own announcements are ignored. |
| `DiscoveryCycleResult { local_port, bytes_sent, discovered_peers: Vec<(SocketAddr, Announce)> }` (root) | `SocketAddr` is the UDP source of each announcement. |
| `resolve_peer_tcp_addrs(udp_src, addresses) -> Vec<SocketAddr>` (root) | Turns announced URIs into dialable addresses, replacing unspecified hosts with the announcement's source IP, best candidates first. |
| `scan_lan_bep_peers(own_id) -> Vec<(SocketAddr, Announce)>` (root) | Fallback when UDP is blocked: scans TCP 22000 on the local `/24` and identifies each open port with `probe_bep_peer_device_id`. Takes a few seconds. |
| `detect_local_ipv4() -> Option<Ipv4Addr>` (root), `detect_local_ipv4_from(targets)` | The LAN address of the default route (no packet is sent). |

### Global Discovery v2 (Internet)

| Item | Description |
| :--- | :--- |
| `lookup_peer_on(server, client, device_id, timeout) -> Vec<PeerEndpoint>` (root) | Asks a lookup server for a device's addresses. Empty if unknown (HTTP 404). |
| `lookup_global_peer(device_id, timeout)` (root) | Same on `DEFAULT_LOOKUP_SERVER`, direct connection. |
| `PeerEndpoint` | `Direct(SocketAddr)` or `Relay { addr, device_id }` (the relay's address and ID). |
| `resolve_endpoint_uri(uri) -> Option<PeerEndpoint>` (root) | Parses `tcp://` / `relay://...?id=` URIs. Host names are resolved through DNS (may block). |
| `parse_global_discovery_json(json) -> Vec<String>` (root) | The `addresses` array of a lookup answer. |
| `announce_on(server, client, identity, addresses, timeout) -> AnnounceOutcome` (root) | Announces our addresses (mutual TLS: the server learns our ID from our certificate). Network / TLS errors are `Err`. |
| `AnnounceOutcome` (root) | `Accepted { reannounce_after }` or `Refused { status, retry_after }`, from the server's headers or `DEFAULT_REANNOUNCE_INTERVAL` (30 min) / `DEFAULT_ANNOUNCE_RETRY` (5 min). |
| `global_announce::encode_announcement(addresses) -> String` | JSON body of an announcement. |

A device that accepts connections announces `tcp://0.0.0.0:<port>` (the server substitutes
the source IP) and/or its relay URI.

### Minimal HTTP client (`discovery::http`)

`https_request(server, client, method, path_and_query, body, client_identity, timeout) -> HttpResponse`
(`HttpResponse { status, headers, body }`, `header(name)`), `parse_http_response(raw)`, and
tiny JSON extractors `json_string_array(json, key)` / `json_string_values(json, key)`.
Responses are capped at 1 MiB.

## 7. Relays: `relay`

Syncthing Relay Protocol v1, for devices that cannot reach each other directly.

### Dialing a peer through a relay

| Function | Description |
| :--- | :--- |
| `connect_via_relay(identity, relay_addr, relay_id, target_id, timeout) -> TcpStream` (root) | Requests a session with `target_id` and joins it. The returned stream is a raw tunnel: run `connect_and_sync_stream` on it. |
| `request_relay_session(...) -> SessionInvitation` | First step only (protocol mode, TLS with ALPN `bep-relay`). |
| `join_relay_session(relay_addr, &invitation, timeout) -> TcpStream` (root) | Second step (session mode, plain TCP). |

### Being reachable through a relay

```rust
let mut relay = RelayListener::join(&identity, relay_addr, relay_id, timeout)?;
announce_on(&server, &client, &identity, &[relay.relay_uri()], timeout)?;
loop {
    if let Some(inv) = relay.poll_invitation(Duration::from_secs(1))? {
        // inv.from = the peer's device ID bytes: check it is a known device first.
        let tcp = join_relay_session(relay.relay_addr(), &inv, timeout)?;
        if inv.server_socket {
            let incoming = accept_bep_tls_stream(&identity, tcp, timeout)?;
            // check incoming.peer_device_id() == inv.from, then accept_and_sync(..)
        } else {
            // connect_and_sync_stream(&params, tcp, relay.relay_addr(), ..)
        }
    }
}
```

`RelayListener` (root): `join(identity, relay_addr, relay_id, timeout)`, `relay_addr()`,
`relay_uri()` (`relay://ip:port/?id=...`), `poll_invitation(poll) -> Option<SessionInvitation>`
(answers the relay's pings; an `Err` means the connection is lost: join again).

### Relay pool

- `fetch_relay_pool(pool: &HttpsServer, client, timeout) -> Vec<String>` (root): `relay://`
  URLs of a pool (`DEFAULT_RELAY_POOL`, root, is Syncthing's public one).
- `parse_relay_pool_json(json)`.

### Wire codec (`relay::wire`)

`RelayMessage` (root): `Ping`, `Pong`, `Response(RelayResponse { code, message })`,
`SessionInvitation(SessionInvitation { from, key, address, port, server_socket })`,
`RelayFull`. `SessionInvitation::session_ip(fallback)` resolves the session address.
Encoders: `encode_connect_request`, `encode_join_session_request`, `encode_join_relay_request`,
`encode_ping`, `encode_pong`; decoders: `read_relay_message`, `read_relay_message_after_header`,
`decode_header`, `decode_payload`. Constants `RELAY_MAGIC`, `RELAY_ALPN`.

## 8. Folder storage: `storage`

### `FsSyncStorage` (root) — folder registry

| Method | Description |
| :--- | :--- |
| `new(default_root_dir)` | Folders without a custom path live in `<root>/<sanitized folder id>`. |
| `set_folder_path(folder_id, path)` | Custom directory of a folder (`device:path` forms such as `ux0:music` are normalized). |
| `folder_root(folder_id) -> PathBuf` | Effective directory. |
| `set_folder_devices(folder_id, Vec<DeviceId>)` | **Authorization**: the devices the folder is shared with (replaces the previous list). A folder never declared here is shared with nobody. |
| `is_folder_shared_with(folder_id, &peer) -> bool` | The gate checked by every session (supports short-ID placeholders). |
| `add_local_folder(BepFolder)` / `local_folders()` | Folders we announce in our `ClusterConfig`, to the devices listed in `BepFolder::device_ids` (keep that list equal to `set_folder_devices`). Announcing is separate from authorizing: a folder that is authorized for a peer but not announced is still synchronized once the peer announces it. |
| `set_folder_paused(folder_id, bool)` / `is_folder_paused` | A paused folder is announced as paused and neither pulled nor served. |
| `set_folder_type(folder_id, FolderType)` / `folder_type` / `is_folder_receive_only` | `ReceiveOnly`: the peer's changes are applied, local changes are published as `invalid` and never pulled by peers. |
| `resolve_safe_path(folder_id, rel_path) -> PathBuf` | Local path of an entry; `.`, `..` and empty components are dropped. |
| `db_path(folder_id) -> PathBuf` | Path of the folder database (`<default root>/.syncthing_db_<folder>_<hash of the folder's directory>.txt`): pointing a folder at another directory starts a fresh database. |
| `root_dir()` | The default root. |

### Blocks (`storage::blocks`)

- `find_outdated_blocks(folder_id, &BepFileInfo) -> Vec<BepBlockInfo>`: compares the local
  file with the remote block list (128 KiB buffer) and returns the blocks to download. Rejects
  invalid block lists (negative / oversized blocks, blocks beyond the file size, non-SHA-256
  hashes).
- `read_block_for_peer(folder_id, rel_path, offset, size) -> Vec<u8>`: reads a block to serve
  a peer's `Request` (size bounded by `BEP_MAX_BLOCK_SIZE`).

### Crash-safe downloads (`storage::download`)

Sequence: `prepare_download` → `open_download` + `write_block_at` (or
`write_block_streaming`) for each block → `commit_download(folder_id, rel_path, final_size)`,
or `abort_download`.

- `prepare_download(folder_id, rel_path)` seeds the temp file with the current content (delta
  sync); `prepare_download_seeded(.., reuse_local_content: bool)` lets you skip the copy.
- `commit_download` truncates to `final_size` and moves the file into place, keeping the
  previous version as a backup until the move succeeded; an interrupted commit is repaired on
  the next scan.
- `download_temp_path` (`.syncthing.<name>.tmp`) / `download_backup_path`
  (`.syncthing.<name>.bak`).

### Helpers

- `storage::atomic`: `write_file_synced(path, bytes)`, `replace_file(tmp, dest, backup)` (at
  every instant either `dest` or `backup` is complete), `recover_replaced_file(dest, backup)`.
  Use them for any file whose loss matters (the app's configuration files do).
- `storage::names`: `is_internal_name(component)` (`.syncthing*`, `~syncthing~*`, `.stfolder`,
  `.stignore`, `.stversions`: never scanned or pulled) and `is_syncable_name(peer_name)`.
- `storage::vita_fs`: `with_device_root_slash(path)` (`ux0:music` → `ux0:/music`).

## 9. Folder reconciliation: `folder_sync`

These `FsSyncStorage` methods connect the disk, the folder database and a peer's index.
Sessions call them; use them directly only to build a custom engine or tool.

| Method | Description |
| :--- | :--- |
| `build_local_index(folder_id, own_id, remote_files) -> (BepIndex, FolderIndexState)` | Scans the folder, records changes in the database (new versions, tombstones for deleted entries) and returns our index. Entries that cannot be read keep their previous state: an error never becomes a deletion. |
| `build_local_index_with_progress(.., on_progress: &mut dyn FnMut())` | Same, calling `on_progress` regularly (to send keepalives). |
| `local_index_snapshot(folder_id, own_id, remote_files)` | Our index from the database, without scanning. |
| `plan_pull(folder_id, own_id, remote) -> PullPlan` / `plan_pull_with_progress` | Compares the peer's index with ours, **applies** everything that needs no transfer (directory creation, deletions, conflict copies `name.sync-conflict-...`, version merges for identical content) and returns what must be downloaded. Invalid entries and non-syncable names are ignored. |
| `record_downloaded(folder_id, own_id, &BepFileInfo)` | Records a completed download (deferred save); call `flush_folder_db(folder_id)` after the batch. |
| `revert_local_changes(folder_id, own_id, remote, on_progress) -> usize` | Syncthing's *Revert Local Changes* for a receive-only folder: deletes local-only entries and resets locally changed ones so the next pull restores the peer's version. |

- `PullPlan { downloads, up_to_date, applied }` (root): files to download, files already
  identical, and human-readable descriptions of applied changes.
- `mask_local_changes(&BepIndex, remote) -> BepIndex`: flags `invalid` every entry whose
  version the peer has not seen (used to publish receive-only folders).

## 10. Folder database: `local_db`

One persistent database per folder: version vector, sequence number, blocks and tombstones
per entry, a stable `index_id` and the folder's `max_sequence`. It is what lets peers agree on
versions across restarts.

- `LocalEntry { name, is_dir, deleted, size, modified_s, disk_mtime, sequence, version, blocks }`
  — `to_file_info()`, `content_equals(&BepFileInfo)`.
- `FolderDb { index_id, max_sequence, entries }` — `new(folder_id, own_id)`, `put(entry)`
  (assigns the next sequence), `touch_disk_mtime(name, mtime)`, `sorted_entries()`;
  `serialize()` / `parse(text)` (text format `#vita-db-v1`, tab-separated).
- Process-wide cache: `with_folder_db(db_path, folder_id, own_id, |db| ..)` loads on first use
  and saves after a change; `with_folder_db_deferred` saves at most every 5 s (for many small
  changes in a row); `flush_folder_db(db_path)` writes pending changes. Saves use
  `storage::atomic`.

## 11. Sync sessions: `session`

### Entry points (root)

```rust
pub fn connect_and_sync<F: FnMut(SyncEvent)>(
    params: &SessionParams<'_>, peer_addr: SocketAddr, on_event: F,
) -> io::Result<PeerHandshakeResult>;

pub fn connect_and_sync_stream<F: FnMut(SyncEvent)>(
    params: &SessionParams<'_>, tcp: TcpStream, peer_addr: SocketAddr, on_event: F,
) -> io::Result<PeerHandshakeResult>;

pub fn accept_and_sync<F: FnMut(SyncEvent)>(
    params: &SessionParams<'_>, incoming: IncomingBepStream, peer_addr: SocketAddr, on_event: F,
) -> io::Result<PeerHandshakeResult>;
```

- `connect_and_sync` dials `peer_addr`, then opens up to two secondary connections when both
  `Hello`s announce them.
- `connect_and_sync_stream` runs over an existing tunnel (relay): single connection.
- `accept_and_sync` runs on an accepted stream: it announces a single connection and sends its
  `ClusterConfig` first. It fails if `expected_device_id` does not match the peer.

Phases: `Hello` → `ClusterConfig` exchange → initial remote `Index` → secondary connections →
pull (delta download, conflicts, deletions) → scan and publication of our `Index` → serving the
peer's requests → with a `SyncControl`, the persistent loop (live `IndexUpdate`s, `Ping`s,
periodic rescans, revert requests) until `request_stop` or disconnection.

`Err` means the connection could not be established or failed (TCP, TLS, wrong peer,
protocol error). A peer that closes the session normally is an `Ok` result.

### `SessionParams<'a>` (root)

| Field | Description |
| :--- | :--- |
| `identity: &DeviceIdentity` | Our certificate and key. |
| `hello: &BepHello` | Our `Hello`; its `device_name` is also our name in `ClusterConfig`. |
| `storage: &FsSyncStorage` | Folders and authorization. |
| `expected_device_id: Option<DeviceId>` | Peer to accept (`None`: any peer, reported in the result). |
| `timeout: Duration` | TCP connect, TLS handshake and blocking socket operations. |
| `control: Option<Arc<SyncControl>>` | `Some`: persistent session; `None`: one sync cycle. |

`Clone`; one value can be reused for every attempt with a peer.

### `PeerHandshakeResult` and `SyncSessionStatus` (root)

```rust
pub struct PeerHandshakeResult {
    pub peer_addr: SocketAddr,
    pub peer_device_id: DeviceId,      // proven by the TLS certificate
    pub peer_hello: Option<BepHello>,  // None: the peer refused BEP (it does not know us)
    pub session_status: SyncSessionStatus,
}
pub enum SyncSessionStatus {
    AwaitingPairing,          // TLS succeeded but the peer does not accept us yet
    ClosedByPeer(String),     // the peer sent Close (reason)
    Active { shared_folders, files_up_to_date, files_downloaded, bytes_downloaded,
             blocks_uploaded, bytes_uploaded },
}
```

### `SyncControl` (root)

Thread-safe handle (`Arc<SyncControl>`) between the application and persistent sessions.

| Method | Description |
| :--- | :--- |
| `new()` / `with_tuning(SyncTuning)` | Create a handle (returns `Arc<Self>`). |
| `request_stop()` / `is_stop_requested()` | Ends every session using the handle at its next checkpoint. Create a new handle for the next generation of sessions. |
| `request_rescan()` | Immediate rescan of the local folders. `take_rescan_request()` and `rescan_epoch()` are used by the sessions. |
| `request_revert(folder_id)` | Asks the next session sharing that receive-only folder to revert its local changes (`SyncEvent::LocalChangesReverted` follows). `take_revert_requests(filter)` is used by the sessions. |
| `request_discovery()` / `take_discovery_request()` | A flag for the **application's** discovery loop (not used by the library): "discover now". |
| `tuning()` | The handle's `SyncTuning`. |

### `SyncTuning` (root)

`{ rescan_interval: Duration, keepalive_interval: Duration }`, defaults 60 s and 20 s.
Syncthing drops a connection after 5 minutes of silence; keepalive `Ping`s are also sent
during long scans.

### `SyncEvent` (root)

Called synchronously on the session thread: keep the callback short (e.g. forward to a
channel).

| Variant | When |
| :--- | :--- |
| `PeerConnected { peer_addr, peer_device_id, peer_name, client_version }` | `Hello` exchanged. |
| `ClusterConfigReceived { peer_device_id, folders }` | The peer announced its folders, including ones not shared with it yet (ask the user for consent). |
| `FileUpToDate { folder, name, size }` | A remote file is already identical locally. |
| `BlockDownloaded { folder, name, offset, size, total_size }` | A block was downloaded and verified. |
| `FileSynced { folder, name, blocks_downloaded, bytes_downloaded, total_size }` | A file download completed. |
| `LocalChangeApplied { folder, description }` | A change applied without transfer (directory, deletion, conflict copy). |
| `LocalIndexPublished { folder, file_count }` | Our index (or an update) was sent. |
| `BlockUploaded { folder, name, offset, size }` | A block was served to the peer. |
| `SyncCycleCompleted { peer_device_id, active_connections, files_up_to_date, files_downloaded, bytes_downloaded, blocks_uploaded, bytes_uploaded }` | A sync cycle ended (the first one ends the initial sync). |
| `FolderUnavailable { folder, error }` | The folder's directory cannot be used; skipped for this session. |
| `FileError { folder, name, error }` | One file failed; a persistent session pulls its folder again before the next rescan. |
| `LocalChangesReverted { folder, reverted }` | A revert request was carried out. |
| `SessionEnded { peer_device_id, reason }` | A persistent session ended. |

### Building blocks (`session::context`, `session::folders`, `session::upload`)

For custom transports and tools:

- `SessionContext<'a>`: identity, `Hello`, storage, peer ID, statistics (`SyncStats`), control
  and tuning, event sink. `new(identity, hello, storage, peer_id, &mut on_event)`, `own_id()`,
  `emit(event)`, `defer_message` / `take_deferred_message`, `mark_folder_incomplete` /
  `take_incomplete_folders`, `keepalive_due()`, `next_request_id()`.
- `FolderSyncState`: per-session folder state — remote indexes (`merge_remote_index`,
  `remote_files`, `remote_folders`), which folders are shared (`is_shared_folder`,
  `is_peer_folder`), our published indexes (`upsert_local_index`, `local_index`,
  `take_unpublished`: a full `Index` once, then incremental `IndexUpdate`s in batches of
  `INDEX_BATCH_FILES` = 1000). `published_file_count(&BepIndex)`.
- `serve_block_request(stream, ctx, payload)`: answers a `Request` (refused with code 1 for a
  paused or unshared folder, code 2 for a missing file).

## 12. Constants

| Constant | Value | Meaning |
| :--- | :--- | :--- |
| `BEP_DEFAULT_PORT` | `22000` | Syncthing's TCP port. |
| `BEP_MAGIC` / `DISCOVERY_MAGIC` | `0x2EA7D90B` | `Hello` and Local Discovery magic. |
| `BEP_BLOCK_SIZE` | 128 KiB | Block size of files we index. |
| `BEP_MAX_BLOCK_SIZE` | 16 MiB | Largest block accepted from a peer. |
| `BEP_BLOCK_HASH_LEN` | `32` | SHA-256 length; other hash lengths are rejected. |
| `BEP_DEFAULT_NUM_CONNECTIONS` | `3` | `1+2` connections announced in `Hello`. |
| `BEP_ALPN` | `b"bep/1.0"` | TLS ALPN of BEP. |
| `DISCOVERY_PORT` | `21027` | Local Discovery UDP port. |
| `DISCOVERY_BROADCAST_ADDR` | `"255.255.255.255:21027"` | Broadcast target. |
| `DEFAULT_LOOKUP_SERVER` | `https://discovery-lookup.syncthing.net/v2/?noannounce&id=...` | Public Global Discovery lookup (pinned). |
| `DEFAULT_ANNOUNCE_SERVERS` | IPv4 and IPv6 announce servers | Public Global Discovery announce (pinned). |
| `DEFAULT_RELAY_POOL` | `https://relays.syncthing.net/endpoint` | Public relay list. |
| `relay::wire::RELAY_ALPN` | `b"bep-relay"` | TLS ALPN of relay protocol mode. |
