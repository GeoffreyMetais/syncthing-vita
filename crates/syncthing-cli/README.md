# syncthing-cli

A command-line [Syncthing](https://syncthing.net) client for **Linux and macOS**, built on
[`syncthing-core`](../syncthing-core/README.md).

It keeps folders in sync with other Syncthing devices — the official Syncthing, the PS Vita
app of this workspace, or another `syncthing-cli` — over the LAN, directly over the Internet,
or through Syncthing's public relays. There is no web GUI and no REST API: devices and folders
are configured with commands (or by editing `config.toml`), and `syncthing-cli run` is the sync
daemon, logging one line per event on stdout.

This document has two parts:

1. [**Overview**](#part-1--overview): what the tool does, its concepts and a walkthrough.
2. [**Reference**](#part-2--reference): every command, option, configuration field,
   environment variable, daemon behaviour, and the crate's internal modules.

---

# Part 1 — Overview

## Features

- Two-way sync of folders with configured devices, block-level delta transfers, conflict
  copies (`name.sync-conflict-<date>-<device>.ext`), deletions.
- **Receive-only** folders (local changes are never sent) and **paused** folders.
- **LAN discovery** (UDP 21027): announces itself, listens continuously and answers
  announcements directly.
- **Incoming connections** on TCP 22000 (configurable) — the PS Vita app always dials, so it
  needs a device that listens.
- **Global Discovery**: looks up configured devices and announces its own addresses.
- **Relays**: dials devices through their relay, and joins a relay itself so that devices
  behind NAT can reach it.
- **HTTP proxy** support for Global Discovery and the relay pool (`HTTPS_PROXY`, ...).
- **Explicit consent**: only configured devices get a session, and a folder is only synced
  with the devices it is shared with. Offers from peers are logged with the command that
  accepts them.
- Script-friendly: `run --exit-after <s>` and `run --exit-when-synced`.

Not supported: ignore patterns (`.stignore`), file versioning, symlinks, filesystem watching
(changes are found by periodic rescans), introducers, auto-accept, encrypted folders, NAT-PMP /
UPnP, QUIC, a GUI or a REST API.

## Concepts

| Concept | Description |
| :--- | :--- |
| **Home directory** | Holds the identity, `config.toml` and the folder databases. `--home <DIR>`, else `$SYNCTHING_CLI_HOME`, else `~/.config/syncthing-cli` (Linux, honours `$XDG_CONFIG_HOME`) or `~/Library/Application Support/syncthing-cli` (macOS). |
| **Identity** | `device_seed.bin`, 32 random bytes created on first use (`0600`). It *is* the private key: the Device ID is derived from it. Back it up to keep the same ID; never share it. |
| **Device** | A remote device, known by its full Device ID, with a name and addresses (`dynamic` = found by discovery, or `tcp://host:port`). Both sides must add each other. |
| **Folder** | A folder ID (identical on every device), a local path, a label and the devices it is shared with. |
| **Daemon** | `syncthing-cli run`: listens, discovers, dials, syncs, until Ctrl-C. Configuration changes are read at start: restart the daemon after editing. |

## Walkthrough

Build and install (from the workspace root):

```bash
cargo install --path crates/syncthing-cli
```

**1. Get this device's ID** (the identity is created on first use):

```bash
syncthing-cli id
```

```text
6SS647G-R6LCDQ6-MPLFGKK-ZQ2THNX-QBZTDYN-VA35CTX-LW5NVLH-ESQZPQ5
```

Add this ID on the other device (in Syncthing: *Add Remote Device*; on the PS Vita: *Add by ID*
or scan its QR code).

**2. Add the other device** (its full ID; a 7-character short ID is refused):

```bash
syncthing-cli device add HYDCJXY-TXXCMAK-7GVM3IA-AM4657A-34QOS6D-5MNHTXI-F5OKI75-OKNGSAP --name laptop
```

Without `--address`, the device is `dynamic`: found through LAN or Global Discovery, or by
connecting to us.

**3. Share a folder with it** (same folder ID as on the other device):

```bash
syncthing-cli folder add photos ~/Pictures --label Photos --device HYDCJXY-TXXCMAK-7GVM3IA-AM4657A-34QOS6D-5MNHTXI-F5OKI75-OKNGSAP
```

**4. Run the daemon:**

```bash
syncthing-cli run
```

```text
[12:58:19] syncthing-cli 0.1.0 — device OJGZ2LE-AXXG5DJ-... (bob)
[12:58:19] Folder docs at /home/bob/docs shared with 1 device(s)
[12:58:19] Listening on 0.0.0.0:22000
[12:58:20] Connected to alice (alice, 0.1.0) at 192.168.1.12:49170
[12:58:20] alice offers folder "extra" (extra). To accept: syncthing-cli folder add extra <path> --device TPFK56N-SOGIVHW-...
[12:58:21] docs: received hello.txt (6 B)
[12:58:22] In sync with alice (session: 1 file(s), 6 B received, 0 B sent)
```

Stop it with Ctrl-C (sessions are closed cleanly, folder databases flushed).

### Accepting a device or folder offered by a peer

Nothing is accepted automatically. The daemon logs what it turned away, with the command to
run:

```text
Connection from unknown device TPFK56N-... at 192.168.1.12:50311. To accept it: syncthing-cli device add TPFK56N-...
alice offers folder "Photos" (photos). To accept: syncthing-cli folder add photos <path> --device TPFK56N-...
```

Run the command, then restart the daemon.

### Syncing with the PS Vita app

The PS Vita app never listens: it dials. Keep `listen` enabled (the default) and local
discovery on; the app finds the computer by LAN discovery. Add the Vita's ID with
`device add` (address `dynamic`), share the folder, run the daemon. The Vita shows the computer
as a discovered device and asks for consent to the folders it offers.

### Two daemons on one machine (tests, scripts)

Each daemon needs its own home (`--home`) and its own listen port:

```bash
syncthing-cli --home /tmp/b config set listen 127.0.0.1:22002
syncthing-cli --home /tmp/a device add <B_ID> --name bob --address 127.0.0.1:22002
syncthing-cli --home /tmp/b device add <A_ID> --name alice
syncthing-cli --home /tmp/a folder add docs /tmp/a/docs --device <B_ID>
syncthing-cli --home /tmp/b folder add docs /tmp/b/docs --device <A_ID>
syncthing-cli --home /tmp/b run --no-local-discovery --no-global-discovery --no-relays --exit-when-synced &
syncthing-cli --home /tmp/a run --listen none --no-local-discovery --no-global-discovery --no-relays --exit-when-synced
```

A only dials (`--listen none`), B listens on `127.0.0.1:22002`; both exit once in sync.

---

# Part 2 — Reference

## Contents

1. [Command line](#1-command-line)
2. [Configuration file](#2-configuration-file-configtoml)
3. [Home directory layout](#3-home-directory-layout)
4. [Environment variables](#4-environment-variables)
5. [Daemon behaviour](#5-daemon-behaviour)
6. [Log messages](#6-log-messages)
7. [Timings and limits](#7-timings-and-limits)
8. [Crate structure (for contributors)](#8-crate-structure-for-contributors)
9. [Testing](#9-testing)

## 1. Command line

```text
syncthing-cli [--home <DIR>] <COMMAND>
```

`--home` is accepted before or after the command. Every command exits with status `0` on
success; on failure it prints `error: <message>` on stderr and exits with `1`. Device IDs are
accepted with or without hyphens, in any case, and must be **full** IDs (56 characters, or 52
Base32 / 64 hex characters).

### `id`

Prints this device's ID, creating the identity on first use.

### `device`

| Command | Description |
| :--- | :--- |
| `device add <ID> [--name <NAME>] [--address <ADDR>]...` | Adds a device. `--name` defaults to the 7-character short ID. `--address` is repeatable: `tcp://host:port`, `host:port` (stored as `tcp://host:port`) or `dynamic` (default when none is given). Host names are resolved at each dial. Fails if the device exists. |
| `device list` | One line per device: `<ID>  <name>  <addresses, comma-separated>`. |
| `device remove <ID>` | Removes the device and unshares it from every folder. |

### `folder`

| Command | Description |
| :--- | :--- |
| `folder add <ID> <PATH> [--label <LABEL>] [--device <ID>]... [--receive-only]` | Adds a folder. `PATH` is created if missing and stored as an absolute path. `--label` defaults to the folder ID. Every `--device` must already be configured. A folder with no device is configured but synced with nobody. |
| `folder list` | One line per folder: `<ID>  <label>  <path>  [receive-only]  [paused]  devices: <short IDs, comma-separated, or none>`. |
| `folder remove <ID>` | Removes the folder from the configuration; its files are kept. |
| `folder share <FOLDER> <DEVICE>` | Shares a folder with a configured device (no-op if already shared). |
| `folder unshare <FOLDER> <DEVICE>` | Stops sharing (no-op if not shared). |
| `folder pause <FOLDER>` / `folder resume <FOLDER>` | A paused folder is announced as paused; nothing is pulled or served. |

A **receive-only** folder applies the other devices' changes, but its own changes (files
added, edited or deleted locally) are announced as invalid and never pulled by the others, as
Syncthing's *Receive Only* folders. The flag can be changed by editing `receive_only` in
`config.toml`.

### `config`

| Command | Description |
| :--- | :--- |
| `config show` | Prints `# device id: <ID>` then the whole configuration as TOML. |
| `config set <KEY> <VALUE>` | Changes a general option (below). Server lists are edited in `config.toml`. |

#### Options (`config set`)

| Key | Value | Field |
| :--- | :--- | :--- |
| `name` | non-empty text | `device_name` (sent to peers in `Hello` and `ClusterConfig`) |
| `listen` | `ip:port` (to disable listening, set `listen = "none"` in `config.toml` or use `run --listen none`) | `listen` |
| `local-discovery` | boolean | `local_discovery` |
| `global-discovery` | boolean | `global_discovery` |
| `relays` | boolean | `relays` |
| `rescan-interval` | seconds, `> 0` | `rescan_interval_secs` |

Booleans: `true` / `on` / `yes` / `1`, `false` / `off` / `no` / `0` (case-insensitive).

### `run`

Runs the daemon until Ctrl-C (or an exit condition). Flags override the configuration for
this run only:

| Flag | Effect |
| :--- | :--- |
| `--listen <ADDR>` | Listen address, e.g. `0.0.0.0:22000`; `none` (or an empty value) disables listening: outgoing connections only. |
| `--no-local-discovery` | No UDP 21027. |
| `--no-global-discovery` | No Global Discovery lookups or announcements. |
| `--no-relays` | Relays are neither dialed nor joined. |
| `--exit-after <SECONDS>` | Stops after this delay. |
| `--exit-when-synced` | Stops once every device sharing an active (not paused) folder completed a sync cycle and no data moved for 3 s. Never true if no such device exists. |

If the listen address cannot be bound, the daemon logs it and runs with outgoing connections
only.

### `discover [--seconds <N>]`

Listens to LAN announcements for `N` seconds (default 10) and prints each device once:
`<ID>  <addresses>  (configured as "<name>" | not configured)`. A device announcing no address
(the PS Vita app) is shown as `no address (heard from <ip>)`. Does not need the daemon (they
can run at the same time: the UDP port is shared).

### `lookup <ID>`

Queries every configured Global Discovery lookup server for a device and prints its addresses
(`tcp://...`, `relay://...?id=...`) or `<server>: <ID> is not announced`. Fails if no server
answered.

### `relays`

Prints the `relay://` URLs of the configured relay pool, then `<n> relay(s) in <host>` on
stderr.

## 2. Configuration file (`config.toml`)

TOML, in the home directory. Missing fields take their defaults; a missing file is the default
configuration. Every command that changes it writes it atomically (temporary file, then
rename). The daemon reads it only at start.

```toml
device_name = "syncthing-cli"
listen = "0.0.0.0:22000"
local_discovery = true
global_discovery = true
relays = true
rescan_interval_secs = 60
global_announce_servers = [
    "https://discovery-announce-v4.syncthing.net/v2/?nolookup&id=LYXKCHX-VI3NYZR-ALCJBHF-WMZYSPK-QG6QJA3-MPFYMSO-U56GTUK-NA2MIAW",
    "https://discovery-announce-v6.syncthing.net/v2/?nolookup&id=LYXKCHX-VI3NYZR-ALCJBHF-WMZYSPK-QG6QJA3-MPFYMSO-U56GTUK-NA2MIAW",
]
global_lookup_servers = ["https://discovery-lookup.syncthing.net/v2/?noannounce&id=LYXKCHX-VI3NYZR-ALCJBHF-WMZYSPK-QG6QJA3-MPFYMSO-U56GTUK-NA2MIAW"]
relay_pool_url = "https://relays.syncthing.net/endpoint"
static_relays = []

[[devices]]
id = "HYDCJXY-TXXCMAK-7GVM3IA-AM4657A-34QOS6D-5MNHTXI-F5OKI75-OKNGSAP"
name = "laptop"
addresses = ["tcp://192.168.1.20:22000"]

[[folders]]
id = "photos"
label = "Photos"
path = "/home/me/Pictures"
devices = ["HYDCJXY-TXXCMAK-7GVM3IA-AM4657A-34QOS6D-5MNHTXI-F5OKI75-OKNGSAP"]
receive_only = false
paused = false
```

| Field | Default | Description |
| :--- | :--- | :--- |
| `device_name` | `"syncthing-cli"` | Name shown to peers. |
| `listen` | `"0.0.0.0:22000"` | TCP listen address; `"none"` or `""`: do not listen. |
| `local_discovery` | `true` | UDP 21027 announcements and listening. |
| `global_discovery` | `true` | Lookups of configured devices and announcements of our addresses. |
| `relays` | `true` | Dial devices through relays and join one ourselves. |
| `rescan_interval_secs` | `60` | Interval between scans of the local folders (no filesystem watcher). |
| `global_announce_servers` | Syncthing's v4 and v6 announce servers | Where our addresses are announced. `?id=` pins the server certificate; otherwise it must chain to a public root. |
| `global_lookup_servers` | Syncthing's lookup server | Where devices are looked up. |
| `relay_pool_url` | `https://relays.syncthing.net/endpoint` | Relay list used when `static_relays` is empty. |
| `static_relays` | `[]` | Relays to join instead of the pool (`relay://host:port/?id=<relay ID>`), tried in order. |
| `devices[].id` | — | Full Device ID (stored in canonical form). |
| `devices[].name` | short ID | Display name (logs). |
| `devices[].addresses` | `["dynamic"]` | `dynamic` and / or `tcp://host:port`. |
| `folders[].id` | — | Folder ID, identical on every device. |
| `folders[].label` | folder ID | Label announced to peers. |
| `folders[].path` | — | Local directory. |
| `folders[].devices` | `[]` | Device IDs the folder is shared with. Entries that are not valid full IDs are ignored. |
| `folders[].receive_only` | `false` | Receive-only folder. |
| `folders[].paused` | `false` | Paused folder. |

To run without any Internet service (LAN only): `global_discovery = false` and
`relays = false`. To use private infrastructure: point `global_*_servers`, `relay_pool_url`
or `static_relays` at your own `stdiscosrv` / `strelaysrv`.

## 3. Home directory layout

| Path | Content |
| :--- | :--- |
| `device_seed.bin` | Identity seed: 32 bytes from `/dev/urandom`, mode `0600`, written atomically. There is no fallback random source. A file of another size is an error. |
| `config.toml` | Configuration. |
| `db/.syncthing_db_<folder>_<hash>.txt` | Folder databases (versions, sequence numbers, blocks, deletions). The hash is that of the folder's path: pointing a folder at another directory starts a fresh database. |

Inside each synced folder, the daemon creates hidden temporary files during downloads
(`.syncthing.<name>.tmp`, `.syncthing.<name>.bak`); they are never synced. Files named
`.syncthing*`, `~syncthing~*`, `.stfolder`, `.stignore` and `.stversions` are ignored.

## 4. Environment variables

| Variable | Effect |
| :--- | :--- |
| `SYNCTHING_CLI_HOME` | Home directory when `--home` is not given. |
| `XDG_CONFIG_HOME` | Base of the default home directory on Linux. |
| `HTTPS_PROXY`, `https_proxy`, `ALL_PROXY`, `all_proxy` | First one set: HTTP proxy (`http://[user:password@]host[:port]`, default port 80) tunnelling Global Discovery and relay pool requests (`CONNECT`). Other schemes (`socks5://`) are ignored with a log line. |
| `SSL_CERT_FILE` | PEM bundle of extra trusted CAs for those requests (e.g. a TLS-inspecting proxy's CA). |

Connections to devices and relays are always direct. Proxy credentials never appear in logs.
Behind a TLS-inspecting proxy, Global Discovery *announcements* fail (the proxy cannot forward
our client certificate); lookups and the relay pool work.

## 5. Daemon behaviour

`run` starts one thread per feature, plus one dialer thread per configured device:

| Thread | Role |
| :--- | :--- |
| **listener** | Accepts TCP connections, runs the TLS handshake (10 s in total) and starts a session if the peer is a configured device; unknown devices are logged and dropped. |
| **lan** | Binds UDP 21027 with `SO_REUSEADDR` / `SO_REUSEPORT` (a Syncthing on the same computer does not make it deaf), broadcasts `tcp://0.0.0.0:<port>` every 30 s (no address when not listening), records the addresses configured devices announce, and answers each announcement directly (the PS Vita only listens for a moment after its own broadcast). |
| **announce** | Announces `tcp://0.0.0.0:<listen port>` and our relay URI to every announce server, again when the server asks (`Reannounce-After`) or when our addresses change. |
| **relay** | Joins a relay (`static_relays` in order, or the pool in random order), announces it, and runs the sessions it invites us to — only for configured devices without a session. Moves to the next relay when the connection is lost. |
| **dialer** (per device) | While the device has no session: looks it up on Global Discovery (every 60 s), then dials, in order, its configured addresses, its LAN addresses, its global direct addresses, then its relays. Retries after 3 s (session ended), 10 s (failure) or 30 s (the device does not accept us yet), plus up to 2.5 s of jitter. |

Rules:

- **One session per device**, whatever the link (dialed, accepted, relayed): a second link to
  a connected device is dropped.
- A session stays open: live index updates, block requests, keepalive pings every 30 s,
  rescans every `rescan_interval_secs`.
- **Consent**: only configured devices get a session. A folder is synced with a device only if
  it is shared with it here; folders a peer offers are logged once with the command that
  accepts them, never synced.
- Configuration is read once at start; restart the daemon to apply changes.
- Ctrl-C asks every session to stop, waits up to 5 s for the threads, flushes the folder
  databases and logs `Stopped`.

## 6. Log messages

Every line is `[HH:MM:SS] <message>` (UTC) on stdout, written atomically (lines of concurrent
threads never interleave). Text received from peers (names, labels, file names) has its
control characters replaced with `?`. Repetitive reports are logged once (per device, per
error until it changes).

| Message | Meaning |
| :--- | :--- |
| `syncthing-cli <version> — device <ID> (<name>)` | Start. |
| `Folder <id> at <path> shared with <n> device(s)[, receive only][, paused]` | Configured folders. |
| `Listening on <addr>` / `Cannot listen on <addr> (...): outgoing connections only` | Listener. |
| `No device configured: add one with ...` | Nothing to sync. |
| `No address known for <name> yet, waiting for discovery` | The dialer has no address; the device may still connect to us. |
| `LAN: <name> announces <addrs>` | A configured device was heard on the LAN. |
| `LAN: unknown device <ID> at <ip>. To add it: ...` | Unconfigured device on the LAN. |
| `Connection from unknown device <ID> at <addr>. To accept it: ...` | Unconfigured device connected to us. |
| `Relay: unknown device <ID> wants to connect. To accept it: ...` | Same through a relay. |
| `Reachable through relay <addr>` / `Lost relay <addr>: ...` | Relay mode. |
| `Connected to <name> (<peer name>, <version>) at <addr>` | Session established. |
| `<name> offers folder "<label>" (<id>). To accept: syncthing-cli folder add <id> <path> --device <ID>` | Folder offered by a peer, not shared with it here. |
| `<folder>: received <file> (<size>)` | File downloaded. |
| `<folder>: <description>` | Change applied without transfer (directory, deletion, conflict copy). |
| `<folder>: <file>: <error>` | A file could not be synced; retried later. |
| `<folder>: folder unavailable: <error>` | The folder's directory cannot be used. |
| `In sync with <name> (session: <n> file(s), <size> received, <size> sent)` | A sync cycle completed (logged the first time, then when data moved). |
| `Cannot sync with <name> at <endpoint>: <error>` | Dial failure; `this device is not accepted by the peer yet` means the peer has not added us. |
| `Disconnected from <name>: <reason>` | Session ended. |
| `Every device is in sync` / `Exit time reached` / `Stopped` | Exit. |

## 7. Timings and limits

All defined in `src/daemon/timing.rs`.

| Value | Default | |
| :--- | :--- | :--- |
| `CONNECT_TIMEOUT` | 10 s | TCP connect, TLS handshake, blocking socket operations. |
| `KEEPALIVE_INTERVAL` | 30 s | Ping interval of established sessions. |
| `RETRY_AFTER_SESSION` / `RETRY_AFTER_ERROR` / `RETRY_AWAITING_PAIRING` | 3 s / 10 s / 30 s | Redial delays (+ up to `RETRY_JITTER` = 2.5 s). |
| `LAN_ANNOUNCE_INTERVAL` | 30 s | LAN broadcast interval (also the minimum delay between two direct answers to a device). |
| `GLOBAL_LOOKUP_INTERVAL` | 60 s | Lookup interval of a disconnected device. |
| `HTTPS_TIMEOUT` | 15 s | Global Discovery and relay pool requests. |
| `RELAY_RETRY` / `RELAY_POOL_RETRY` | 20 s / 5 min | Next relay after a loss / next pool fetch after a failure. |
| `SYNCED_QUIET_PERIOD` | 3 s | `--exit-when-synced` quiet period. |
| `MAX_UNAUTHENTICATED_CONNECTIONS` | 32 | Incoming connections in their TLS handshake (or being turned away) at once; more are closed immediately. |
| `MAX_PENDING_RELAY_INVITATIONS` | 16 | Relay invitations being joined at once (separate pool). |
| `MAX_UNKNOWN_DEVICE_REPORTS` | 256 | Unknown devices reported individually, then one line and silence. |
| `MAX_REPORTS_PER_SUBJECT` | 32 | Distinct reports kept per device / server. |
| `MAX_LAN_REPLIES_TO_UNKNOWN` | 64 | Unknown devices answered on the LAN per reply interval (configured devices are always answered). |

These bounds exist because LAN announcements, connections and relay invitations are
unauthenticated: they can carry any number of device IDs, and nothing they feed may grow
without limit.

## 8. Crate structure (for contributors)

The crate is a library (`src/lib.rs`, used by the integration tests) plus a thin binary
(`src/main.rs`: parse, run, print the error). Same rules as the rest of the workspace (see
`AGENTS.md`): English, zero warnings, one responsibility per 200–400-line file, no
`#[cfg(test)]` in `src/`, no `ring` / `aws-lc-rs` / `openssl`. It is never built for the PS
Vita.

| Module | Content |
| :--- | :--- |
| `cli` | `clap` definitions: `Cli`, `Command`, `DeviceCommand`, `FolderCommand`, `ConfigCommand`, `DiscoverArgs`, `RunArgs`. |
| `commands` | One file per command (`id`, `device`, `folder`, `config_cmd`, `discover`, `lookup`, `relays`, `run`); `commands::run(Cli)` dispatches. `run::daemon_options(&Config, &RunArgs)` merges the configuration and the flags. |
| `paths` | `resolve_home`, `default_home`, `db_dir`, `CONFIG_FILE`, `SEED_FILE`. |
| `identity` | `load_or_create_identity(home) -> DeviceIdentity`. |
| `config` | `Config`, `DeviceConfig`, `FolderConfig` (serde model with defaults), edits (`add_device`, `remove_device`, `device`, `add_folder`, `remove_folder`, `share_folder`, `unshare_folder`, `set_folder_paused`, `set_option`), `parse_device_id` (full IDs only), `ConfigError`, `load_config` / `save_config` (atomic). |
| `https_env` | `https_client_from_env() -> (HttpsClient, notes)`: proxy and extra roots. |
| `daemon` | `run_daemon(home, &Config, identity, DaemonOptions, ExitPolicy)` and its threads: `context` (`DaemonContext`: identity, `Hello`, `FsSyncStorage`, `SyncControl`, configured peers, shared `PeerTable`), `sessions` (`run_session` over a `Link`: `Dial`, `RelayClient`, `Incoming`; `SessionEnd` chooses the retry delay), `listener`, `dialer` (`targets`, `run_dialer`), `lan` (`LanBeacon`, `ReplySchedule`), `global` (`announced_addresses`, `run_global_announcer`), `relay` (`candidate_relays`, `triage_invitation`, `run_relay`), `events` (`SyncEvent` → log line), `activity` (`--exit-when-synced`), `reports` (`ReportMemory`), `connection_slots` (`ConnectionSlots`, RAII slots), `log` (`log`, `printable`), `timing`. |

The sync itself is `syncthing-core`: `DaemonContext::new` builds an `FsSyncStorage` from the
configuration (`set_folder_path`, `set_folder_devices`, `set_folder_paused`,
`set_folder_type`, `add_local_folder` for folders with devices) and a
`SyncControl::with_tuning` (rescan interval, keepalive); sessions call `connect_and_sync`,
`connect_and_sync_stream` or `accept_and_sync`.

Adding a command: a variant in `cli.rs`, a file in `commands/`, a line in `commands::run`, a
test in `tests/`. Adding a timing or a limit: a documented constant in `daemon/timing.rs`.

## 9. Testing

```bash
cargo test -p syncthing-cli
```

Tests live in `tests/`, with helpers in `tests/common/` (temporary homes, local stand-ins for
Global Discovery and relays in `servers.rs` — tests never reach the Internet):

- configuration: `config.rs`, `config_error.rs`, `config_options.rs`, `config_store.rs`,
  `identity.rs`, `paths.rs`, `https_env.rs`, `run_options.rs`;
- the real binary: `cli_commands.rs`, `diagnostic_commands.rs` (`lookup`, `relays`);
- daemon pieces: `lan.rs`, `lan_beacon.rs`, `dialer_targets.rs`, `relay_candidates.rs`,
  `announced_addresses.rs`, `events_format.rs`, `log_output.rs`, `sync_activity.rs`,
  `daemon_limits.rs`, `daemon_context.rs`, `daemon_dialer.rs`, `daemon_listener.rs`,
  `daemon_global.rs`, `daemon_relay.rs`, `daemon_lifecycle.rs`;
- end to end: `daemon_sync.rs` — two daemons on `127.0.0.1` with discovery and relays off: sync
  both ways, live changes and deletions, unknown device refused, offered folder not synced,
  receive-only folder.

Wider checks, from the workspace root: `just vita3k-cli-sync` (PS Vita app in Vita3K ↔ CLI)
and the `syncthing-e2e-validation` skill (LAN, Vita3K and Internet paths). Mutation testing:
`cargo mutants -p syncthing-cli`.

## License

MPL-2.0, as the rest of the workspace.
