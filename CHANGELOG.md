# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). A release is cut
from a `vX.Y.Z` tag matching the workspace version in `Cargo.toml`; the tag also publishes a
GitHub Release carrying the `SYNCTHING.vpk` asset.

## [Unreleased]

## [0.2.0] - 2026-11-24

### Added

- The PS Vita app is now **reachable inbound**: TCP listener on port 22000, permanent relay
  listener and Global Discovery announcements, active even with zero paired devices or shares
  ([#38](https://github.com/GeoffreyMetais/syncthing-vita/pull/38)).
- `DeviceOffer` consent modal when an unknown device connects inbound: accept, decline or decide
  later; declined devices are persisted and can be reviewed and cleared in *Settings*
  ([#38](https://github.com/GeoffreyMetais/syncthing-vita/pull/38)).
- `Send Only` share mode (3-way ReadWrite / ReadOnly / SendOnly toggle in the device modal) with
  *Override Changes*, alongside the existing *Receive Only* / *Revert Local Changes*
  ([#40](https://github.com/GeoffreyMetais/syncthing-vita/pull/40)).
- `.stignore` pattern filtering in folder scans and pulls, with the same syntax as desktop
  Syncthing (glob wildcards, `!` negation, `(?i)`, `#include`, ...)
  ([#40](https://github.com/GeoffreyMetais/syncthing-vita/pull/40)).
- "Vita Games Saves" (`ux0:user/00/savedata`) share preset, first in the one-tap presets
  ([#36](https://github.com/GeoffreyMetais/syncthing-vita/pull/36)).
- Touch-drag scrolling of the share list on the Overview and of the full-screen log viewer
  ([#50](https://github.com/GeoffreyMetais/syncthing-vita/pull/50)).
- `syncthing-core` protocol features:
  - BEP LZ4 send compression and `DownloadProgress` messages
    ([#39](https://github.com/GeoffreyMetais/syncthing-vita/pull/39)).
  - Inbound secondary 1+2 connections and Adler-32 `weak_hash` shifted-block reuse during
    downloads ([#41](https://github.com/GeoffreyMetais/syncthing-vita/pull/41)).
  - Extended `FileInfo` metadata, Unix symlink sync, and ClusterConfig `introducer` support
    (devices introduced by a trusted peer still require consent on the Vita)
    ([#42](https://github.com/GeoffreyMetais/syncthing-vita/pull/42)).
  - UPnP/NAT-PMP port mapping, `.stversions` file versioning, and `ReceiveEncrypted` folder
    support ([#43](https://github.com/GeoffreyMetais/syncthing-vita/pull/43)).
- Connection-lifecycle E2E coverage: host integration tests plus Vita3K ↔ `syncthing-cli`
  scenarios for connect, sync, live updates, graceful disconnect, reconnection and shutdown
  ([#49](https://github.com/GeoffreyMetais/syncthing-vita/pull/49)).

### Fixed

- `ClusterConfig` disconnects from real Syncthing servers: a single `ClusterConfig` per
  connection, corrected protobuf field tags (`introducer` on field 7, invalid varint field 10
  removed), inbound handshake ordering, no disk scan or `Ping` during the initial state, and
  clean offline state after early disconnects
  ([#49](https://github.com/GeoffreyMetais/syncthing-vita/pull/49)).
- Global Discovery lookup now runs even while a saved LAN address is being dialed, and starts
  immediately at launch when no local peer is present
  ([#46](https://github.com/GeoffreyMetais/syncthing-vita/pull/46),
  [#48](https://github.com/GeoffreyMetais/syncthing-vita/pull/48)).
- LAN addresses are preferred over public IPs when peering on the same network, and WAN/relay
  addresses are no longer persisted into `devices.cfg`
  ([#37](https://github.com/GeoffreyMetais/syncthing-vita/pull/37)).
- Local Discovery advertises the explicit LAN IPv4 (never `0.0.0.0`, which made peers behind
  Docker/NAT dial the bridge gateway); behind NAT without a port mapping, Global Discovery
  advertises relay-only
  ([#45](https://github.com/GeoffreyMetais/syncthing-vita/pull/45)).
- IPv6 endpoints are filtered out on the Vita's IPv4-only network stack
  ([#46](https://github.com/GeoffreyMetais/syncthing-vita/pull/46),
  [#48](https://github.com/GeoffreyMetais/syncthing-vita/pull/48)).

### Security

- All `cargo audit` vulnerabilities resolved: upgraded `p256`/`p384`/`ecdsa`, `rsa` and
  `rustls-webpki`, removed `paste` and the duplicate `rustls-webpki 0.102`, and replaced
  `rustls-rustcrypto` with an in-crate pure-Rust `CryptoProvider` so the Vita target keeps zero
  C/ASM crypto dependencies
  ([#34](https://github.com/GeoffreyMetais/syncthing-vita/pull/34)).
- TLS 1.2 ChaCha20-Poly1305 decryption now bounds-checks short/malformed records instead of
  risking integer underflow
  ([#35](https://github.com/GeoffreyMetais/syncthing-vita/pull/35)).

### Internal

- CI now runs `cargo fmt --check` and `cargo clippy`, a CLI ↔ CLI E2E suite (LAN discovery and
  Internet via Global Discovery + relay), and expanded Vita3K ↔ CLI suites (LAN, Internet,
  inbound TCP with `DeviceOffer` accept/decline, connection lifecycle)
  ([#35](https://github.com/GeoffreyMetais/syncthing-vita/pull/35),
  [#47](https://github.com/GeoffreyMetais/syncthing-vita/pull/47),
  [#49](https://github.com/GeoffreyMetais/syncthing-vita/pull/49)).
- `AGENTS.md` and workspace skills updated for inbound reachability, Part 2 features,
  `cargo audit` and E2E validation; agents no longer read `tests/` during design and
  implementation ([#33](https://github.com/GeoffreyMetais/syncthing-vita/pull/33),
  [#44](https://github.com/GeoffreyMetais/syncthing-vita/pull/44)).

## [0.1.0] - 2026-11-17

First public release: a native PS Vita Syncthing client (BEP/1.0 over TLS 1.3) that pairs with
the official Syncthing apps.

### Added

- On-demand frame rendering with a dynamic frame rate instead of a fixed 60 FPS loop
  ([#3](https://github.com/GeoffreyMetais/syncthing-vita/pull/3)).
- Folder consent enforced in the sync engine; `.cfg` fields escaped
  ([#4](https://github.com/GeoffreyMetais/syncthing-vita/pull/4)).
- Share access mode read & write / read only, with *Revert Local Changes*
  ([#13](https://github.com/GeoffreyMetais/syncthing-vita/pull/13)).
- Tag-triggered GitHub release workflow; `APP_VER` in `param.sfo` set from the Cargo workspace
  version ([#15](https://github.com/GeoffreyMetais/syncthing-vita/pull/15)).
- Full-screen scrollable log viewer from the Settings log card
  ([#17](https://github.com/GeoffreyMetais/syncthing-vita/pull/17)); progress bars only while a
  transfer runs ([#16](https://github.com/GeoffreyMetais/syncthing-vita/pull/16)).
- Screensaver showing the real sync state, `START · Quit` hint, 10 s / 30 s screensaver delays
  ([#12](https://github.com/GeoffreyMetais/syncthing-vita/pull/12),
  [#18](https://github.com/GeoffreyMetais/syncthing-vita/pull/18)).
- **Scan QR** with the rear camera to pair devices, including a shortcut in the share modal
  ([#20](https://github.com/GeoffreyMetais/syncthing-vita/pull/20)).
- Files tab: presets listed first, hold/drag scrolling
  ([#19](https://github.com/GeoffreyMetais/syncthing-vita/pull/19)); screenshot and picture
  folder presets ([#26](https://github.com/GeoffreyMetais/syncthing-vita/pull/26)).
- Global Discovery v2 + Relay v1 ([#21](https://github.com/GeoffreyMetais/syncthing-vita/pull/21)).
- [`syncthing-cli`](crates/syncthing-cli), a command-line Syncthing client for Linux and macOS
  built on the same core library ([#25](https://github.com/GeoffreyMetais/syncthing-vita/pull/25)).
- "Sync now" button on the idle sidebar transfer card
  ([#32](https://github.com/GeoffreyMetais/syncthing-vita/pull/32)).
- LiveArea icon ([#11](https://github.com/GeoffreyMetais/syncthing-vita/pull/11)).
- Merged Overview and Shares screens ([#9](https://github.com/GeoffreyMetais/syncthing-vita/pull/9));
  sidebar "Ready" state with share cards listing devices
  ([#10](https://github.com/GeoffreyMetais/syncthing-vita/pull/10)); dedicated Devices & Pairing
  card ([#8](https://github.com/GeoffreyMetais/syncthing-vita/pull/8)).
- Mutation testing with `cargo mutants` for the three crates
  ([#27](https://github.com/GeoffreyMetais/syncthing-vita/pull/27)).
- READMEs: the PS Vita app README ([#30](https://github.com/GeoffreyMetais/syncthing-vita/pull/30))
  and developer READMEs for `syncthing-core` and `syncthing-cli`
  ([#29](https://github.com/GeoffreyMetais/syncthing-vita/pull/29)).

[Unreleased]: https://github.com/GeoffreyMetais/syncthing-vita/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/GeoffreyMetais/syncthing-vita/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/GeoffreyMetais/syncthing-vita/releases/tag/v0.1.0
