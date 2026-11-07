---
name: syncthing-bep-protocol
description: >-
  Use this skill when modifying or debugging the Syncthing Block Exchange Protocol
  (BEP/1.0), UDP Local Discovery (port 21027), TLS 1.3 mutual authentication,
  LZ4 framing, or delta-sync logic in crates/syncthing-core.
---

# Syncthing BEP/1.0 & Local Discovery Protocol Skill

`crates/syncthing-core` implements Syncthing's **Block Exchange Protocol (`bep/1.0`)** and **Local Discovery (`v4`)** from scratch in pure Rust with zero external Protobuf codegen dependencies.

## Steps for Modifying Protocol Logic

1. **Consult the Wire Format Reference**:
   Read [bep_wire_spec.md](./references/bep_wire_spec.md) for the exact Protobuf field tags, wire types, pre-authentication `Hello` magic (`0x2EA7D90B`), and post-authentication LZ4 framing used by Syncthing v1.20+ / v2.x.

2. **Preserve Streaming Memory Bounds**:
   - Never buffer an entire file in memory. Syncthing blocks are capped at **128 KiB (`131,072` bytes)**.
   - Use `SyncStorage::write_block(folder, name, total_size, offset, data)` and `SyncStorage::read_block(folder, name, offset, size)` in `crates/syncthing-core/src/storage.rs`.

3. **Respect the Two-Step `ClusterConfig` Handshake (`crates/syncthing-core/src/tls.rs`)**:
   - **Step 1**: Upon receiving the peer's `ClusterConfig`, merge any locally created shares (`storage.local_folders()`) that target `peer_device_id`, and send an initial `ClusterConfig` with `index_id = 0, max_sequence = 0` so the remote peer immediately transmits its `Index` / `IndexUpdate` messages.
   - **Step 2**: After downloading missing blocks and scanning local folders (`storage.scan_local_folder`), send an updated `ClusterConfig` with the PS Vita's local `index_id` and `max_sequence` followed by `BepMessageType::Index` so the remote peer can issue `BepMessageType::Request` messages to upload modified Vita files.

4. **Run Unit Tests**:
   Always add or update roundtrip tests in `crates/syncthing-core/src/lib.rs` and run:
   ```bash
   cargo test --package syncthing-core
   ```
