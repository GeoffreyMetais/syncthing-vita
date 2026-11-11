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
syncthing-vita
├── Cargo.toml                        # Workspace manifest (Edition 2021, LTO + opt-level = "z")
├── Justfile                          # Task runner (test-host, check-vita, build-vita, vpk)
├── scripts/build_vpk.sh              # Full VPK build pipeline (vita-elf-create, vita-make-fself, vita-mksfoex, vita-pack-vpk)
└── crates/
    ├── syncthing-core/               # Platform-independent Syncthing BEP/1.0 & Discovery engine
    │   ├── src/
    │   │   ├── lib.rs                # Module declarations & public API re-exports ONLY (no logic, no tests)
    │   │   ├── proto.rs              # Protobuf wire primitives (varint, fields, WireIter)
    │   │   ├── device_id/            # DeviceId: mod.rs (format, hex, short-ID matching), parse.rs (user input), base32.rs (Base32 + Luhn mod 32)
    │   │   ├── cert.rs               # Deterministic ECDSA P-256 X.509 identity (VitaIdentity) & DER builder
    │   │   ├── version_vector.rs     # Version vector arithmetic (ge / merge / bump, short device ID)
    │   │   ├── bep/                  # BEP/1.0 pure codecs: header, framing, lz4, hello, cluster_config, index, request, close
    │   │   ├── discovery/            # announce.rs (codec), addresses.rs, local_ip.rs, udp.rs (broadcast cycle), lan_scan.rs (TCP 22000 + TLS probe)
    │   │   ├── storage/              # FsSyncStorage: mod.rs (folder registry & paths), blocks.rs, download.rs (temp file assembly), vita_fs.rs (sceIo quirks)
    │   │   ├── local_db/             # Persistent folder DB: mod.rs (LocalEntry, FolderDb), codec.rs (text format), cache.rs (process-wide cache)
    │   │   ├── folder_sync/          # Reconciliation: scan.rs (disk scan -> local Index), pull.rs (pull planning, conflicts, deletions)
    │   │   ├── tls/                  # TLS 1.3 transport only: verifier.rs, client.rs (BEP streams, ID probe), polling.rs
    │   │   └── session/              # BEP sync engine, one file per phase (see §3.3)
    │   └── tests/                    # Integration tests, one file per module (+ common/ helpers)
    └── vita-syncthing-app/           # Native PlayStation Vita GUI application (Title ID: SYNCTHING, Clean Architecture)
        ├── assets/fonts/             # Embedded Poppins TrueType fonts (Regular, Medium, Bold — SIL OFL 1.1)
        ├── tests/                    # Integration tests, one file per use case / component (+ common/ helpers)
        └── src/
            ├── lib.rs                # Library target: declares the four layers ONLY (no logic, no tests)
            ├── main.rs               # Composition root (binary), 60 FPS event loop, and PS Vita kernel symbols
            ├── domain/               # Layer 1: Pure Domain Entities, Value Objects & Application State
            │   ├── mod.rs
            │   ├── models.rs         # KnownDevice, ShareConfigEntry, PickerItem, ActiveTab, PickerMode, ModalDialog, ThemeMode
            │   └── app_state.rs      # AppState (pure state & view-model queries, zero I/O or rendering dependencies)
            ├── application/          # Layer 2: Use Cases, Input Controller & Sync Event Coordinator
            │   ├── mod.rs
            │   ├── share_service.rs  # ShareService (create_share, upsert_device, toggle_device_for_share, refresh_picker_items)
            │   ├── share_creation.rs # ShareCreation (new share kept only once a device is linked)
            │   ├── sync_coordinator.rs # SyncCoordinator (maps WorkerMsg & SyncEvent into AppState & ShareService)
            │   └── input_controller.rs # InputController (routes InputFrame -> AppState + ShareService -> UserCommand)
            ├── infrastructure/       # Layer 3: Persistence, OS Filesystem, PS Vita Hardware & BEP Worker Thread
            │   ├── mod.rs
            │   ├── config_repo.rs    # ConfigRepository (devices.cfg, shares.cfg, syncthing.log, ux0: directory listing)
            │   ├── identity_store.rs # IdentityStore (sceKernelGetRandomNumber, device_seed.bin, cert.der)
            │   ├── vita_system.rs    # init_vita_network (SceSysmodule + SceSysmem + SceNet + SceNetCtl)
            │   ├── sync_worker.rs    # spawn_sync_worker (background thread for fast-path BEP + UDP discovery + TLS 1.3)
            │   ├── framebuffer.rs    # VitaFramebuffer (SceDisplay + SceSysmem 960x544 RGBA8888 hardware driver)
            │   └── input.rs          # VitaInput & InputFrame (SceCtrl + SceTouch hardware driver)
            └── ui/                   # Layer 4: Pure Presentation & Rendering Layer (reads &AppState, zero business logic)
                ├── mod.rs
                ├── gfx.rs            # 960x544 RGBA8888 2D anti-aliased vector & TrueType software renderer
                ├── theme.rs          # UiTheme (Dark & OLED palettes) and layout constants
                ├── widgets.rs        # Reusable VitaSend UI components (cards, chips, actions, checkboxes, chrome, toast)
                ├── screens/          # Feature-split screen views (Overview, Shares, Files, Settings, OLED Screensaver)
                │   ├── mod.rs
                │   ├── dashboard.rs
                │   ├── shares.rs
                │   ├── folder_picker.rs
                │   ├── settings.rs
                │   └── screensaver.rs
                ├── modals/           # Modal dialog views & fallback keypad
                │   ├── mod.rs
                │   ├── create_share.rs
                │   ├── manage_devices.rs
                │   ├── add_device.rs
                │   ├── fallback_keyboard.rs
                │   ├── device_id.rs
                │   ├── confirm_quit.rs
                │   └── confirm_delete_share.rs
                └── renderer.rs       # VitaUiRenderer (coordinates Gfx + VitaFramebuffer presentation)
```

---

## 3. Clean Architecture & Modularity Guidelines (Preventing Monolithic Files)

VitaSyncthing enforces a strict **Clean Architecture** to maintain high maintainability, testability on host platforms, and prevent monolithic "god files".

### 3.1 The Four Clean Architecture Layers & Dependency Rules

Dependencies strictly point **inward**. Inner layers never import or depend on outer layers.

```text
       ┌────────────────────────────────────────────────────────┐
       │   UI / Presentation Layer (crates/.../ui)              │
       │   Screens, Modals, Widgets, Gfx Engine                 │
       │   (Reads &AppState, ZERO business logic or I/O)        │
       └───────────────────────────┬────────────────────────────┘
                                   │
                                   ▼
       ┌────────────────────────────────────────────────────────┐
       │   Application Layer (crates/.../application)           │
       │   Services, Coordinators, Input Controller             │
       │   (Orchestrates use cases, routes commands)            │
       └──────────────┬─────────────────────────┬───────────────┘
                      │                         │
                      ▼                         ▼
┌───────────────────────────────┐     ┌─────────────────────────┐
│ Infrastructure Layer          │     │ Domain Layer            │
│ (crates/.../infrastructure)   │────▶│ (crates/.../domain)     │
│ Persistence, Vita Hardware,   │     │ Entities, Models,       │
│ Syscalls, Network Worker      │     │ AppState, Invariants    │
└───────────────────────────────┘     └─────────────────────────┘
```

1. **Layer 1: Pure Domain Layer (`crates/vita-syncthing-app/src/domain/`)**
   - **Responsibility**: Pure domain models (`models.rs`), application state container (`app_state.rs`), invariants, and query functions.
   - **Strict Isolation**: **Zero external dependencies**. Absolutely no I/O, no filesystem, no sockets, no rendering, and no VitaSDK syscalls.
   - **Host Testability**: Must be 100% testable on standard x86_64 host machines without any mocks or emulators.

2. **Layer 2: Application Layer (`crates/vita-syncthing-app/src/application/`)**
   - **Responsibility**: Use cases, business workflow orchestration, command routing, and event coordination.
   - **Key Modules**:
     - `share_service.rs`: Share creation, device linkage, path validation.
     - `share_creation.rs`: Share creation workflow — a new share is discarded if `ManageDevices` closes with no linked device.
     - `sync_coordinator.rs`: Maps background `WorkerMsg` protocol events into `AppState` updates.
     - `input_controller.rs`: Translates user button/touch inputs into actions, modal triggers, and commands.
   - **Strict Rule**: No UI rendering code or direct hardware drivers.

3. **Layer 3: Infrastructure Layer (`crates/vita-syncthing-app/src/infrastructure/`)**
   - **Responsibility**: Hardware drivers, OS filesystem persistence, background threads, and Vita system modules.
   - **Key Modules**:
     - `config_repo.rs`: Disk persistence (`devices.cfg`, `shares.cfg`, `syncthing.log`) and `ux0:` browsing.
     - `identity_store.rs`: Device seed PRNG (`sceKernelGetRandomNumber`) and X.509 certificate persistence.
     - `framebuffer.rs`: PS Vita hardware framebuffer management (`SceDisplay`, `SceSysmem`).
     - `input.rs`: Controller & touchscreen hardware drivers (`SceCtrl`, `SceTouch`).
     - `sync_worker.rs`: Background sync thread (UDP discovery, BEP protocol, TLS 1.3).
     - `vita_system.rs`: Network stack and sysmodule initialization.
   - **Strict Rule**: All platform-specific `#[cfg(target_os = "vita")]` hardware syscalls (`sce*`) must be encapsulated here. Never leak them into UI or Domain.

4. **Layer 4: UI / Presentation Layer (`crates/vita-syncthing-app/src/ui/`)**
   - **Responsibility**: Visual presentation and 2D vector/font rendering.
   - **Key Modules**:
     - `screens/`: Independent full-screen views (`dashboard.rs`, `shares.rs`, `folder_picker.rs`, `settings.rs`, `screensaver.rs`).
     - `modals/`: Isolated modal dialogs (`create_share.rs`, `manage_devices.rs`, `add_device.rs`, `device_id.rs`, `confirm_quit.rs`, `confirm_delete_share.rs`, `fallback_keyboard.rs`).
     - `widgets.rs`: Reusable UI elements (cards, chips, checkboxes, buttons, chrome, toast).
     - `gfx.rs`: Software 2D rendering primitives, anti-aliased geometry, and TrueType rasterization.
     - `renderer.rs`: Main UI coordinator invoking active screen and modal renderers.
   - **Strict Rule**: Pure read-only view. Receives `&AppState` and visual context. **Never mutate domain state directly, and never perform disk I/O or network calls.**

5. **Composition Root (`crates/vita-syncthing-app/src/main.rs`)**
   - **Responsibility**: Initializes subsystems, wires dependencies, executes the 60 FPS event loop, and exports required VitaSDK entrypoint symbols (`_newlib_heap_size_user`, `sceUserMainThreadStackSize`).
   - Must remain concise and free of monolithic rendering or business logic.
   - Imports the layers from the crate's library (`use vita_syncthing_app::...`). `src/lib.rs` only declares `domain`, `application`, `infrastructure` and `ui`; the VitaSDK entrypoint symbols stay in `main.rs`.

6. **Tests (`crates/vita-syncthing-app/tests/`)**
   - **No `#[cfg(test)] mod tests` inside `src/`** (neither in `main.rs` nor in layer modules). Tests are host integration tests against the library's public API, one file per use case / component (`share_service.rs`, `input_controller.rs`, `power_policy.rs`), with shared helpers in `tests/common/mod.rs` (e.g. `temp_test_dir`).
   - Test domain and application logic through public functions; never through Vita hardware drivers (`sce*` code is `#[cfg(target_os = "vita")]` and absent on host).

---

### 3.2 Modularity & File Size Guidelines (Preventing Monoliths)

To prevent code degradation into large, unmaintainable files:

- **Target File Size**: Aim for **200 to 400 lines of code** per file. A file approaching **500+ lines** is a strong code smell indicating multiple responsibilities and must be decomposed.
- **Single Responsibility Principle (SRP)**:
  - Each module must have one, and only one, reason to change.
  - Separate rendering from logic: UI files only draw; application services only coordinate; repositories only persist.
- **Decomposition Standards**:
  - **New Screens**: Always create a separate file under `ui/screens/<feature>.rs` and expose it via `ui/screens/mod.rs`.
  - **New Modal Dialogs**: Always create a separate file under `ui/modals/<modal_name>.rs` and expose it via `ui/modals/mod.rs`.
  - **New Services**: Split distinct domain workflows into dedicated services in `application/` rather than adding methods to an existing unrelated service.
  - **Avoid "Omnibus" Files**: Never create catch-all files like `display.rs` or `utils.rs`. Group code by specific domain responsibility.

These size and SRP rules apply to **both crates** (`vita-syncthing-app` and `syncthing-core`).

---

### 3.3 `syncthing-core` Library Architecture

`syncthing-core` is the platform-independent protocol library. It must stay as clean as the app: one responsibility per file, strict inward dependencies, no Vita syscalls.

#### Layers (dependencies point downwards only)

| Layer | Modules | Allowed I/O |
| :--- | :--- | :--- |
| 1. Wire codecs & identity | `proto`, `device_id/`, `bep/`, `discovery/announce.rs`, `discovery/addresses.rs`, `cert`, `version_vector` | **None** (pure functions on bytes / values) |
| 2. Local persistence | `storage/`, `local_db/`, `folder_sync/` | Filesystem only |
| 3. Transport | `tls/`, `discovery/udp.rs`, `discovery/lan_scan.rs`, `discovery/local_ip.rs` | Sockets only |
| 4. Sync engine | `session/` | Orchestrates layers 1-3 |

- A lower layer never imports a higher one (e.g. `bep/` never uses `tls/` or `storage/`; `tls/` never touches folders, indexes or `FsSyncStorage`).
- `lib.rs` only declares modules and re-exports the public API used by the app. **No logic and no tests in `lib.rs`.** Keep the root re-exports stable: the app imports from `syncthing_core::*`.
- No `#[cfg(target_os = "vita")]` and no `sce*` calls in this crate. PS Vita filesystem / socket quirks are handled portably (`storage/vita_fs.rs`, `tls/polling.rs`).

#### Module responsibilities

- **`bep/`**: one file per BEP message family (`header`, `framing`, `lz4`, `hello`, `cluster_config`, `index`, `request`, `close`). Encode/decode only — never read files or sockets beyond the generic `Read`/`Write` in `framing.rs` / `hello.rs`.
- **`device_id/`**: `DeviceId` value object; parsing of user input (`parse.rs`) and Base32/Luhn primitives (`base32.rs`) are separate.
- **`discovery/`**: `Announce` codec, address ordering, local IP detection, UDP broadcast cycle, LAN TCP-scan fallback — one file each.
- **`storage/`**: `FsSyncStorage` split by concern — registry & paths (`mod.rs`), block hashing / delta / block reads (`blocks.rs`), crash-safe download temp files (`download.rs`), `sceIo` path quirks (`vita_fs.rs`). Extend `FsSyncStorage` with a new `impl` block in the file owning that concern instead of growing `mod.rs`.
- **`local_db/`**: in-memory model (`mod.rs`), on-disk text format (`codec.rs`), process-wide cache + atomic save (`cache.rs`). Version arithmetic lives in `version_vector.rs`.
- **`folder_sync/`**: disk scan → local `Index` (`scan.rs`) vs. pull planning, conflicts and deletions (`pull.rs`).
- **`tls/`**: TLS transport only — certificate verifier, client connection setup / ID probe, frame polling. Nothing about sync.
- **`session/`**: the BEP sync engine, **one file per phase or concern**:
  - `mod.rs` — `connect_and_sync*` orchestrator (connect → `Hello` → phases), must stay a thin sequence of phase calls
  - `cluster_exchange.rs` — initial `ClusterConfig` exchange + initial remote `Index`
  - `secondary.rs` — `+2` secondary connections (open, promotion, polling, keepalive)
  - `initial_sync.rs` — initial download, local `Index` publication, request draining
  - `persistent.rs` — persistent keepalive / rescan loop
  - `dispatch.rs` — routing of primary-connection messages in an established session
  - `index_exchange.rs` — local index publication and runtime `ClusterConfig` / `Index` handling
  - `download.rs` / `upload.rs` — block transfers (pull / serve)
  - `folders.rs` — per-session folder state (pure); `context.rs` — `SessionContext` (identity, storage, stats, event sink); `control.rs`, `events.rs`, `outcome.rs` — public API types

#### Rules for new code

- **No long parameter lists**: pass `SessionContext` (identity, storage, peer, stats, `emit`) and `FolderSyncState` instead of threading counters and maps through functions. A new `#[allow(clippy::too_many_arguments)]` in `syncthing-core` is a design smell (the public `connect_and_sync_with_control` signature is the only accepted exception).
- **Named constants for timings**: protocol timeouts and intervals are `const` items at the top of the file that uses them, never inline magic `Duration`s.
- **New BEP message** → new file in `bep/`; **new session phase** → new file in `session/`; **new storage concern** → new file in `storage/` with its own `impl FsSyncStorage` block.

#### Tests

- **No `#[cfg(test)] mod tests` inside `src/`.** All `syncthing-core` tests live in `crates/syncthing-core/tests/`, one file per module (`bep_codec.rs`, `device_id.rs`, `discovery.rs`, `identity.rs`, `storage.rs`, `local_db.rs`, `folder_sync.rs`, `version_vector.rs`), with shared helpers in `tests/common/mod.rs` (e.g. `fresh_temp_dir`).
- Tests exercise the public API. If something important can only be tested through a private item, it is usually a sign the item deserves its own module with a small public surface (as `storage::vita_fs::with_device_root_slash` or `FolderDb::{serialize, parse}`).
- Filesystem tests use a unique `common::fresh_temp_dir("<name>")` per test so test binaries can run in parallel.

---

## 4. Critical PlayStation Vita Hardware Constraints (`C2-12828-1` Prevention)

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

## 5. Runtime Configuration Files on PS Vita (`ux0:data/syncthing/`)

All persistent application state lives in `ux0:data/syncthing/` (or `/tmp/syncthing-vita` on host builds):

| File | Purpose | Format |
| :--- | :--- | :--- |
| `device_seed.bin` | 32-byte random seed for deterministic ECDSA P-256 keypair | Raw 32 bytes |
| `cert.der` | Self-signed X.509 DER certificate whose SHA-256 is the Syncthing `DeviceId` | ASN.1 DER |
| `devices.cfg` | Known/discovered remote Syncthing peers | `<device_id_hex>\|<name>\|<ip:port>\n` |
| `shares.cfg` | Configured shared folders and linked devices | `<folder_id>\|<label>\|<target_path>\|<0\|1>\|<dev_hex1,dev_hex2>\n` |
| `syncthing.log` | Persistent protocol & UI event log | Plain text lines |
| `sync/.syncthing_db_<folder>_<root hash>.txt` | Persistent folder database (`local_db/`): version vectors, sequences, blocks and tombstones per entry | `#vita-db-v1` tab-separated text |

---

## 6. Build, Test & Verification Commands

### Run Host Unit Tests
```bash
cargo test --workspace
```
`syncthing-core` tests live in `crates/syncthing-core/tests/` (run only them with `just test-host`):
- `bep_codec.rs`: BEP `Hello` magic & framing, secondary `ClusterConfig`, LZ4 block decompression
- `device_id.rs`: Luhn-mod-32 `DeviceId` formatting, full / short / hex user-input parsing
- `discovery.rs`: Protobuf `Announce` roundtrip, advertised TCP address resolution
- `identity.rs`: X.509 certificate & `DeviceId` derivation
- `storage.rs`: streaming download, delta sync, `ux0:` device-root path variants
- `local_db.rs`: persistent folder database text format roundtrip
- `folder_sync.rs`: version convergence across peers, local deletions become tombstones
- `version_vector.rs`: version vector ordering / merge / bump

`vita-syncthing-app` tests live in `crates/vita-syncthing-app/tests/` (`cargo test -p vita-syncthing-app`):
- `share_creation.rs`: a new share without a linked device is discarded, a linked one is kept
- `share_service.rs`: share deletion, share creation, device linking, paused discovered folders, `ConfigRepository` roundtrip, manual LAN pairing
- `input_controller.rs`: `START` quit dialog, theme toggle, add-device-by-short-ID via system keyboard, share deletion confirmation, `Files` up navigation
- `power_policy.rs`: standby blocking while syncing (charging / battery / transfer grace period)

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

## 7. UI Layout & Controls Reference (`VitaSend` Design System)

The UI (`gfx.rs` + `ui/screens/*` + `ui/modals/*` + `ui/renderer.rs`) renders at native `960x544` @ 60 FPS with a `212px` left sidebar (`SIDEBAR_W = 212`), main content area (`x = 236..936`, `y = 86..488`), and bottom button hint bar (`y = 500..544`):

- **Left Sidebar Tabs** (`L` / `R` shoulder buttons or touch tap):
  1. `Overview` (`ActiveTab::Dashboard`): Device status card, `QR / ID` modal button (`Circle`), `Sync Now` button (`Cross`), transfer statistics, and recent activity feed.
  2. `Shares` (`ActiveTab::Shares`): List of shared folders, `+ New Share` modal (`Circle`), `Devices` assignment modal (`Cross`), `Delete` with confirmation (`Square`), and `Folder` path picker (`Triangle`).
  3. `Files` (`ActiveTab::FolderPicker`): Interactive `ux0:` directory browser. Selecting a folder (`Cross` or tap) starts creating a share of it; `Triangle` (or tapping the `Open` chip) enters a subfolder, `Circle` goes up/back. The `Shares` tab `Folder` action reuses this browser to change a share's target path.
  4. `Settings` (`ActiveTab::Settings`): Dark / Pure-Black OLED theme switcher (`Cross`), `QR / ID` modal (`Circle`), and BEP/1.0 protocol log viewer.
- **Modal Dialogs** (`ModalDialog` in `domain/models.rs`, rendered by `ui/modals/*`):
  - `CreateShare`: Create a share from PS Vita folder presets (`ux0:user/00/savedata`, `ux0:picture`, `ux0:music`, RetroArch saves) or browse `ux0:`. The share is only kept once a device is linked in the `ManageDevices` modal that follows.
  - `ManageDevices`: Link/unlink discovered Syncthing peers to a share with circular checkboxes (`draw_row_tick`) and trigger immediate `ClusterConfig` advertising (`Triangle · Apply & Sync`); `SELECT` pauses / enables the share.
  - `ShowDeviceId`: Displays a scannable QR code (`qrcodegen`) alongside the 8-group Syncthing Device ID.
  - `ConfirmDeleteShare`: Triggered by `Square` on the `Shares` tab; removes the share from `shares.cfg` (files on `ux0:` are kept).
  - `ConfirmQuit`: Triggered by `START` (or tapping `Quit`), warns if a sync is in progress before exiting to LiveArea.
- **OLED Screensaver**: Activates automatically after 12s of inactivity during active sync (or 45s when idle, or immediately via `SELECT`), rendering a pure `#000000` black background with a bouncing progress widget to protect PS Vita 1000 OLED screens.
