# AGENTS.md — Contributor & AI Agent Guide for VitaSyncthing (`SYNCTHING`)

This document provides essential architectural context, build workflows, and **real PlayStation Vita hardware constraints** for future coding sessions. Read this file before modifying any crate or build script.

---

## 1. Language & Code Quality Rules

- **English Only**: All source code comments, docstrings, UI labels, log messages, commit messages, and documentation **must be written in English**.
- **Zero Warnings**: Both host tests (`cargo test --workspace`) and the PS Vita release build (`just vpk`) must compile with **zero compiler warnings**. Use `#[cfg(target_os = "vita")]` and `#[cfg(not(target_os = "vita"))]` cleanly so host and Vita targets both stay warning-free.
- **No Unapproved C/ASM Crypto Dependencies**: Never add `ring`, `aws-lc-rs`, or `openssl` to the dependency tree. They do not compile for `armv7-sony-vita-newlibeabihf`. Always use pure-Rust cryptography (`rustls` with `default-features = false`, `rustls-rustcrypto`, `p256`, `sha2`, `rcgen`).

---

## 2. Workspace Architecture

```text
/mnt/projects/vita-dev
├── Cargo.toml                        # Workspace manifest (Edition 2021, LTO + opt-level = "z")
├── Justfile                          # Task runner (test-host, check-vita, build-vita, vpk)
├── scripts/build_vpk.sh              # Full VPK build pipeline (vita-elf-create, vita-make-fself, vita-mksfoex, vita-pack-vpk)
└── crates/
    ├── syncthing-core/               # Platform-independent Syncthing BEP/1.0 & Discovery engine
    │   └── src/
    │       ├── lib.rs                # Public API exports & unit test suite
    │       ├── disco.rs              # UDP Local Discovery (port 21027), Protobuf Announce, Luhn-mod-32 DeviceId
    │       ├── bep.rs                # BEP/1.0 framing, LZ4 block compression, Hello/ClusterConfig/Index/Request/Response
    │       ├── cert.rs               # Deterministic/random ECDSA P-256 X.509 certificate & DER generator
    │       ├── tls.rs                # TLS 1.3 mutual-auth handshake (ALPN "bep/1.0") + bidirectional delta sync
    │       └── storage.rs            # SyncStorage trait & FsSyncStorage (128 KiB streaming I/O, local folder registry)
    └── vita-syncthing-app/           # Native PlayStation Vita GUI application (Title ID: SYNCTHING)
        ├── assets/fonts/             # Embedded Poppins TrueType fonts (Regular, Medium, Bold — SIL OFL 1.1)
        └── src/
            ├── main.rs               # PS Vita entry point, SceNet/SceSysmodule init, worker thread, input routing
            ├── gfx.rs                # 960x544 RGBA8888 2D anti-aliased vector & TrueType software renderer
            ├── display.rs            # VitaSend-style GUI (Sidebar, Overview, Shares, Files, Settings, Modals, OLED Saver)
            └── input.rs              # SceCtrl gamepad/analog + SceTouch front touchscreen (1920x1088 -> 960x544)
```

---

## 3. Critical PlayStation Vita Hardware Constraints (`C2-12828-1` Prevention)

Emulators like Vita3K are permissive HLE environments that run with host PC RAM and pre-bound HLE syscalls. **Real PS Vita hardware will crash with `(C2-12828-1)` if any of the following rules are violated**:

1. **Always Export `_newlib_heap_size_user` (`main.rs`)**:
   ```rust
   #[cfg(target_os = "vita")]
   #[used]
   #[no_mangle]
   pub static _newlib_heap_size_user: u32 = 64 * 1024 * 1024; // 64 MB
   ```
   - *Why*: In VitaSDK's `libc.a` (`_init_vita_heap`), if `_newlib_heap_size_user` is not defined by the binary, Newlib defaults to requesting a single contiguous **128 MB (`0x08000000`)** block via `sceKernelAllocMemBlock`. On real PS Vita hardware, this allocation fails, leaving `_newlib_heap_base = NULL` and causing the very first Rust `malloc` in `_start -> main` to abort with `C2-12828-1`.

2. **Export `sceUserMainThreadStackSize` & Box Large State (`main.rs`)**:
   ```rust
   #[cfg(target_os = "vita")]
   #[used]
   #[no_mangle]
   pub static sceUserMainThreadStackSize: u32 = 2 * 1024 * 1024; // 2 MB
   ```
   - Keep large structs like `VitaDashboard` heap-allocated (`Box::new(VitaDashboard::new(...))`), and spawn background threads (`bep_worker`) with explicit `.stack_size(2 * 1024 * 1024)`.

3. **Load `SCE_SYSMODULE_NET` Before `sceNetInit` (`main.rs`)**:
   - On real hardware, `sceSysmoduleLoadModule(SCE_SYSMODULE_NET)` (`0x0001`) **must** be called before `sceNetInit` and `sceNetCtlInit`.
   - Allocate `SceNetInitParam.memory` via `sceKernelAllocMemBlock("SceNetMemory", 0x0C20D060, NET_INIT_SIZE, ptr::null())` + `sceKernelGetMemBlockBase`.

4. **Build `eboot.bin` with `vita-make-fself -c` (Not `-s`) (`scripts/build_vpk.sh`)**:
   - Never use `-s` (safe eboot). Synchronizing `ux0:user/00/savedata`, `ux0:picture`, `ux0:music`, and loading sysmodules with `ATTRIBUTE2=12` in `param.sfo` requires standard full homebrew permissions (`vita-make-fself -c`).

5. **Unique Per-Installation Cryptographic Identity (`main.rs`)**:
   - Never hardcode the contents of `ux0:data/syncthing/device_seed.bin`.
   - On a clean installation (when `device_seed.bin` does not exist), generate 32 random bytes via `sceKernelGetRandomNumber` (`SceLibKernel_stub`) so every PS Vita gets a unique Syncthing `DeviceId`.
   - Remote folders discovered via `ClusterConfig` (`upsert_discovered_folder`) must default to `enabled: false` (**Paused**) until the user selects a target folder and enables them in the **Shares** tab.

---

## 4. Runtime Configuration Files on PS Vita (`ux0:data/syncthing/`)

All persistent application state lives in `ux0:data/syncthing/` (or `/tmp/syncthing-vita` on host builds):

| File | Purpose | Format |
| :--- | :--- | :--- |
| `device_seed.bin` | 32-byte random seed for deterministic ECDSA P-256 keypair | Raw 32 bytes |
| `cert.der` | Self-signed X.509 DER certificate whose SHA-256 is the Syncthing `DeviceId` | ASN.1 DER |
| `devices.cfg` | Known/discovered remote Syncthing peers | `<device_id_hex>\|<name>\|<ip:port>\n` |
| `shares.cfg` | Configured shared folders and linked devices | `<folder_id>\|<label>\|<target_path>\|<0\|1>\|<dev_hex1,dev_hex2>\n` |
| `syncthing.log` | Persistent protocol & UI event log | Plain text lines |
| `sync/.syncthing_index_<folder>.db` | Local file index cache (`size\|mtime\|sha256\|name`) to avoid re-hashing unchanged files | Pipe-delimited text |

---

## 5. Build, Test & Verification Commands

### Run Host Unit Tests
```bash
cargo test --workspace
```
Tests cover:
- Protobuf `Announce` encoding/decoding (`test_announce_protobuf_roundtrip`)
- Luhn-mod-32 `DeviceId` formatting (`test_device_id_formatting`)
- BEP `Hello` magic & framing (`test_bep_hello_roundtrip`)
- LZ4 block decompression (`test_lz4_decompression`)
- X.509 certificate & `DeviceId` derivation (`test_vita_x509_identity_generation`)
- Delta sync block comparison & local index persistence (`test_delta_sync_and_local_index_roundtrip`)

### Cross-Check for PS Vita (`armv7-sony-vita-newlibeabihf`)
```bash
just check-vita
```

### Build Release `.vpk` Package
```bash
just vpk
```
Output package:
```text
target/armv7-sony-vita-newlibeabihf/release/SYNCTHING.vpk
```

### Verify Symbol Exports in the Vita ELF
After running `just vpk`, you can verify that the required Newlib heap and thread symbols are exported in `.rodata`:
```bash
/usr/local/vitasdk/bin/arm-vita-eabi-nm -n target/armv7-sony-vita-newlibeabihf/release/vita-syncthing-app.elf | grep -E "_newlib_heap_size_user|sceUserMainThread"
```

### Install & Smoke-Test in Vita3K (Local Emulator)
```bash
unzip -o target/armv7-sony-vita-newlibeabihf/release/SYNCTHING.vpk -d ~/.local/share/Vita3K/Vita3K/ux0/app/SYNCTHING
timeout 6s ~/.local/bin/Vita3K --console 1 --frontend sdl -l 2 -r SYNCTHING || true
```

---

## 6. UI Layout & Controls Reference (`VitaSend` Design System)

The UI (`gfx.rs` + `display.rs`) renders at native `960x544` @ 60 FPS with a `212px` left sidebar (`SIDEBAR_W = 212`), main content area (`x = 236..936`, `y = 86..488`), and bottom button hint bar (`y = 500..544`):

- **Left Sidebar Tabs** (`L` / `R` shoulder buttons or touch tap):
  1. `Overview` (`ActiveTab::Dashboard`): Device status card, `QR / ID` modal button (`Circle`), `Sync Now` button (`Cross`), transfer statistics, and recent activity feed.
  2. `Shares` (`ActiveTab::Shares`): List of shared folders, `+ New Share` modal (`Circle`), `Devices` assignment modal (`Cross`), `Pause/Enable` toggle (`Square`), and `Folder` path picker (`Triangle`).
  3. `Files` (`ActiveTab::FolderPicker`): Interactive `ux0:` directory browser with presets and subfolder navigation (`Cross` select, `Triangle` open subfolder, `Circle` up/back).
  4. `Settings` (`ActiveTab::Settings`): Dark / Pure-Black OLED theme switcher (`Cross`), `QR / ID` modal (`Circle`), and BEP/1.0 protocol log viewer.
- **Modal Dialogs** (`ModalDialog` in `display.rs`):
  - `CreateShare`: Create a share from PS Vita folder presets (`ux0:user/00/savedata`, `ux0:picture`, `ux0:music`, RetroArch saves) or browse `ux0:`.
  - `ManageDevices`: Link/unlink discovered Syncthing peers to a share with circular checkboxes (`draw_row_tick`) and trigger immediate `ClusterConfig` advertising (`Triangle · Apply & Sync`).
  - `ShowDeviceId`: Displays a scannable QR code (`qrcodegen`) alongside the 8-group Syncthing Device ID.
  - `ConfirmQuit`: Triggered by `START` (or tapping `Quit`), warns if a sync is in progress before exiting to LiveArea.
- **OLED Screensaver**: Activates automatically after 12s of inactivity during active sync (or 45s when idle, or immediately via `SELECT`), rendering a pure `#000000` black background with a bouncing progress widget to protect PS Vita 1000 OLED screens.
