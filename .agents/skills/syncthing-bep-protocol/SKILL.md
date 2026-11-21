---
name: syncthing-bep-protocol
description: >-
  Use this skill when modifying or debugging the Syncthing Block Exchange Protocol
  (BEP/1.0), UDP Local Discovery (port 21027), TLS 1.3 mutual authentication,
  LZ4 framing, or delta-sync logic in crates/syncthing-core.
---

# Syncthing BEP/1.0 & Local Discovery Protocol Skill

`crates/syncthing-core` implements Syncthing's **Block Exchange Protocol (`bep/1.0`)**, **Local Discovery (`v4`)**, **Global Discovery (`v2`)**, **Relay Protocol (`v1`)**, and **UPnP IGD / NAT-PMP** from scratch in pure Rust with zero external Protobuf codegen or C/ASM crypto dependencies.

## Steps for Modifying Protocol Logic

1. **Consult the Wire Format Reference**:
   Read [bep_wire_spec.md](./references/bep_wire_spec.md) for the exact Protobuf field tags, wire types, pre-authentication `Hello` magic (`0x2EA7D90B`), LZ4 framing (`compress_bep_lz4` / `decompress_bep_lz4`), `DownloadProgress` (type 5), `FolderType` (`SendReceive`, `SendOnly`, `ReceiveOnly`, `ReceiveEncrypted`), and `weak_hash` (rolling Adler-32) used by Syncthing v1.20+ / v2.x.

2. **Preserve Streaming Memory Bounds**:
   - Never buffer an entire file in memory. Syncthing blocks are capped at **128 KiB (`131,072` bytes)** (max accepted 16 MiB).
   - Use `FsSyncStorage` (`crates/syncthing-core/src/storage/`) streaming block reads/writes, rolling `weak_hash` shifted-block reuse (`storage/weak_hash.rs`), `.stignore` filtering (`folder_sync/ignore.rs`), and `.stversions` archiving (`storage/versioning.rs`).

3. **Respect the `ClusterConfig` & `1+2` Session Flow (`crates/syncthing-core/src/session/`)**:
   - Single `ClusterConfig` exchange per session (`cluster_exchange.rs`): never send a second `ClusterConfig` after transitioning to `stateReady` (fatal in Syncthing).
   - Both outbound (`secondary.rs`) and inbound (`SecondaryInbox` + `accept_and_sync`) sessions support `1+2` secondary data connections (`ClusterConfig { secondary: true }`).
   - Compression policy (`BepDeviceCompression`: `Metadata`, `Never`, `Always`) controls LZ4 send compression for `Index` / `IndexUpdate` / `Response`.
   - `introducer` peers emit `SyncEvent::DevicesIntroduced` (routed to user consent, never auto-paired silently).

4. **Run Unit & Integration Tests**:
   Always add or update integration tests in `crates/syncthing-core/tests/` (never inside `src/lib.rs`) and run:
   ```bash
   cargo test --package syncthing-core
   ```
