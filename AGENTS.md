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
├── scripts/gen_livearea.py           # Generates the LiveArea images (assets/sce_sys/: icon, background, gate — PS Vita + Syncthing emblem)
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
    │   │   ├── storage/              # FsSyncStorage: mod.rs (folder registry & paths), blocks.rs, download.rs (temp file assembly), atomic.rs (crash-safe replacement), vita_fs.rs (sceIo quirks)
    │   │   ├── local_db/             # Persistent folder DB: mod.rs (LocalEntry, FolderDb), codec.rs (text format), cache.rs (process-wide cache)
    │   │   ├── folder_sync/          # Reconciliation: scan.rs (disk scan -> local Index), pull.rs (pull planning, conflicts, deletions)
    │   │   ├── tls/                  # TLS 1.3 transport only: verifier.rs, client.rs (BEP streams, ID probe), polling.rs
    │   │   └── session/              # BEP sync engine, one file per phase (see §3.3)
    │   └── tests/                    # Integration tests, one file per module (+ common/ helpers)
    └── vita-syncthing-app/           # Native PlayStation Vita GUI application (Title ID: SYNCTHING, Clean Architecture)
        ├── assets/fonts/             # Embedded Poppins TrueType fonts (Regular, Medium, Bold — SIL OFL 1.1)
        ├── assets/sce_sys/           # Packed into the VPK: icon0.png (128x128), livearea/contents/{bg.png (840x500), startup.png (280x158), template.xml}; 8-bit indexed PNGs
        ├── tests/                    # Integration tests, one file per use case / component (+ common/ helpers)
        └── src/
            ├── lib.rs                # Library target: declares the four layers ONLY (no logic, no tests)
            ├── main.rs               # Composition root (binary), VBlank-paced event loop, and PS Vita kernel symbols
            ├── domain/               # Layer 1: Pure Domain Entities, Value Objects & Application State
            │   ├── mod.rs
            │   ├── navigation.rs     # NavFocus / NavState (D-pad focus), tab order, buttons per row
            │   ├── models.rs         # KnownDevice, ShareConfigEntry, PickerItem, ActiveTab, PickerMode, ModalDialog, ThemeMode
            │   ├── app_state.rs      # AppState (pure state & view-model queries, zero I/O or rendering dependencies)
            │   ├── interaction.rs    # HitMap / Hotspot / UiAction / FocusSlot: interactive elements declared by the frame on screen
            │   ├── defaults.rs       # Every tunable value of the app: sync / discovery / power / display timings, limits, share presets
            │   ├── initial_sync.rs   # InitialSyncPeers (peers in their initial sync phase; full CPU speed meanwhile)
            │   └── ui_text.rs        # UiText: translatable messages (status line, labels, toasts) chosen by inner layers
            ├── application/          # Layer 2: Use Cases, Input Controller & Sync Event Coordinator
            │   ├── mod.rs
            │   ├── share_service.rs  # ShareService (create_share, upsert_device, toggle_device_for_share, refresh_picker_items)
            │   ├── dashboard_controller.rs / shares_controller.rs / files_controller.rs / settings_controller.rs # One controller per page: D-pad focus movement, execution of the page's UiActions
            │   ├── share_creation.rs # ShareCreation (new share kept only once a device is linked)
            │   ├── share_mode.rs     # ShareModeService (share read & write <-> read only, revert of local changes)
            │   ├── folder_offers.rs  # FolderOfferService (folders offered by peers need consent: accept / decline / later)
            │   ├── frame_pacing.rs   # FramePacer (dynamic frame rate: render only when the screen changes)
            │   ├── devices_scan.rs   # DevicesScan (LAN scan of the ManageDevices dialog: immediate or delayed, cancelled on close)
            │   ├── dpad_navigation.rs # DpadNavigation (sidebar <-> page focus, Cross on focused element)
            │   ├── qr_scan.rs        # QrScanFlow (scanner events -> AddDeviceById confirmation), extract_device_id
            │   ├── sync_coordinator.rs # SyncCoordinator (maps WorkerMsg & SyncEvent into AppState & ShareService)
            │   ├── upload_activity.rs # UploadActivity ("Uploading" state ends UPLOAD_DISPLAY_GRACE after the last block served)
            │   ├── input_controller.rs # InputController (resolves touch / Cross / shortcuts against the frame's HitMap, executes UiActions)
            │   └── modal_input/      # ModalInput: resolves touches on the open modal, one key / row handler per modal (create_share, manage_devices, add_device, scan_qr, dialogs)
            ├── infrastructure/       # Layer 3: Persistence, OS Filesystem, PS Vita Hardware & BEP Worker Thread
            │   ├── mod.rs
            │   ├── config_repo.rs    # ConfigRepository (devices.cfg, shares.cfg, ignored_folders.cfg, theme.cfg, language.cfg, syncthing.log, ux0: directory listing)
            │   ├── config_fields.rs  # escape_field / unescape_field (peer-supplied text can never forge `.cfg` fields or lines)
            │   ├── worker_slot.rs    # spawn_after_previous / wait_for_worker (a new sync worker starts only once the previous one ended)
            │   ├── identity_store.rs # IdentityStore (sceKernelGetRandomNumber, device_seed.bin, cert.der)
            │   ├── vita_system.rs    # init_vita_network (SceSysmodule + SceSysmem + SceNet + SceNetCtl) & detect_system_language (SceAppUtil)
            │   ├── ui_script.rs      # Opt-in `ui-script` feature: scripted input replay + frame dumps for Vita3K
            │   ├── qr_camera.rs      # QrScanner (SceCamera rear camera thread: 640x360 preview + QR decoding)
            │   ├── qr_decoder.rs     # QrDecoder (pure-Rust `quircs` QR decoder on greyscale frames, host-testable)
            │   ├── sync_worker.rs    # spawn_sync_worker (background thread for fast-path BEP + UDP discovery + TLS 1.3)
            │   ├── framebuffer.rs    # VitaFramebuffer (SceDisplay + SceSysmem 960x544 RGBA8888 hardware driver)
            │   └── input.rs          # VitaInput & InputFrame (SceCtrl + SceTouch hardware driver)
            └── ui/                   # Layer 4: Pure Presentation & Rendering Layer (reads &AppState, zero business logic)
                ├── mod.rs
                ├── i18n/             # Multi-language string catalogs (en, fr, es, de, it) with zero allocation in 60 FPS loop
                │   ├── mod.rs        # Strings struct, dynamic formatters, strings(Language) selector
                │   ├── en.rs         # English static catalog (STRINGS_EN)
                │   ├── fr.rs         # French static catalog (STRINGS_FR)
                │   ├── es.rs         # Spanish static catalog (STRINGS_ES)
                │   ├── de.rs         # German static catalog (STRINGS_DE)
                │   ├── it.rs         # Italian static catalog (STRINGS_IT)
                │   └── messages.rs   # Messages templates, Strings::text(&UiText), device_id_error
                ├── image.rs          # Greyscale camera frame blit (QR viewfinder)
                ├── gfx.rs            # 960x544 RGBA8888 2D anti-aliased vector & TrueType software renderer
                ├── theme.rs          # UiTheme (Dark & OLED palettes) and layout constants
                ├── strings.rs        # Factorized UI string resources, labels, controller hints & formatting helpers
                ├── widgets.rs        # Reusable VitaSend UI components (cards, chips, actions, checkboxes, chrome, toast)
                ├── screens/          # Feature-split screen views (Overview + its share list, Files, Settings, OLED Screensaver)
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
                │   ├── scan_qr.rs
                │   ├── fallback_keyboard.rs
                │   ├── device_id.rs
                │   ├── confirm_quit.rs
                │   ├── confirm_delete_share.rs
                │   ├── confirm_revert_share.rs
                │   └── folder_offer.rs
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
   - **Responsibility**: Pure domain models (`models.rs`), application state container (`app_state.rs`), translatable messages (`ui_text.rs`), invariants, and query functions.
   - **Strict Isolation**: **Zero external dependencies**. Absolutely no I/O, no filesystem, no sockets, no rendering, and no VitaSDK syscalls.
   - **Host Testability**: Must be 100% testable on standard x86_64 host machines without any mocks or emulators.

2. **Layer 2: Application Layer (`crates/vita-syncthing-app/src/application/`)**
   - **Responsibility**: Use cases, business workflow orchestration, command routing, and event coordination.
   - **Key Modules**:
     - `share_service.rs`: Share creation, device linkage, path validation.
     - `share_creation.rs`: Share creation workflow — a new share is discarded if `ManageDevices` closes with no linked device.
     - `share_mode.rs`: `ShareModeService::toggle` switches a share between `ShareMode::ReadWrite` and `ShareMode::ReadOnly`, saves it and restarts the worker (`config_dirty`). `ShareModeService::request_revert` queues a read-only share in `AppState::pending_reverts`; `main.rs` hands it to the running worker (`SyncControl::request_revert`) once no configuration change is pending, and a sync is started if none runs.
     - `folder_offers.rs`: Consent for folders offered by peers (`FolderOffer` modal); declined offers persisted.
     - `sync_coordinator.rs`: Maps background `WorkerMsg` protocol events into `AppState` updates.
     - `controllers/`: `input.rs` (`InputController`: translates main-screen button/touch inputs into actions, modal triggers, and commands) and one controller per page (`dashboard.rs` with the share list's `shares.rs`, `files.rs`, `settings.rs`; see §3.4).
     - `modal_input/`: Input handling for open modals, one file per modal (mirrors `ui/modals/`).
     - `dpad_navigation.rs`: D-pad focus between the sidebar and the page elements.
     - **No screen coordinate and no `crate::ui` import in this layer**: what is interactive, and where, is declared by the views (§3.4).
     - `frame_pacing.rs`: `FramePacer` dynamic frame rate (see §7).
     - `power_policy.rs`: automatic standby blocking and CPU clock level (`cpu_clock`: 333 MHz during the initial sync phase (`domain::InitialSyncPeers`, from `PeerConnected` to that peer's first `SyncCycleCompleted` / `SessionEnded`) and transfers + 10 s, 222 MHz for 3 s after input or while scanning a QR code, 111 MHz otherwise; applied by `set_cpu_clock_mhz` in `vita_system.rs`).
   - **Strict Rule**: No UI rendering code or direct hardware drivers.

3. **Layer 3: Infrastructure Layer (`crates/vita-syncthing-app/src/infrastructure/`)**
   - **Responsibility**: Hardware drivers, OS filesystem persistence, background threads, and Vita system modules.
   - **Key Modules**:
     - `config_repo.rs`: Disk persistence (`devices.cfg`, `shares.cfg`, `ignored_folders.cfg`, `theme.cfg`, `language.cfg`, `syncthing.log`) and `ux0:` browsing.
     - `config_fields.rs`: Percent-escaping of every text field of the `.cfg` files (see §5).
     - `identity_store.rs`: Device seed PRNG (`sceKernelGetRandomNumber`) and X.509 certificate persistence.
     - `framebuffer.rs`: PS Vita hardware framebuffer management (`SceDisplay`, `SceSysmem`).
     - `input.rs`: Controller & touchscreen hardware drivers (`SceCtrl`, `SceTouch`).
     - `sync_worker.rs`: Background sync thread (UDP discovery, BEP protocol, TLS 1.3).
     - `worker_slot.rs`: Sequential hand-over between sync workers. Never spawn a sync worker thread directly: two generations of sessions must not write the same files concurrently.
     - `vita_system.rs`: Network stack, sysmodule initialization, and automatic system language detection (`detect_system_language` via `SceAppUtil` on Vita and `LANG`/`LC_ALL` on host).
   - **Strict Rule**: All platform-specific `#[cfg(target_os = "vita")]` hardware syscalls (`sce*`) must be encapsulated here. Never leak them into UI or Domain.

4. **Layer 4: UI / Presentation Layer (`crates/vita-syncthing-app/src/ui/`)**
   - **Responsibility**: Visual presentation and 2D vector/font rendering.
   - **Key Modules**:
     - `i18n/`: Zero-allocation multi-language catalogs (`Strings`) supporting English (`en`), French (`fr`), Spanish (`es`), German (`de`), and Italian (`it`).
     - `strings.rs`: Centralized user-facing strings, action constants, button hints, and zero-allocation formatting helpers re-exporting `ui::i18n`.
     - `screens/`: Independent full-screen views (`dashboard.rs` drawing the share list of `shares.rs`, `folder_picker.rs`, `settings.rs`, `screensaver.rs`).
     - `modals/`: Isolated modal dialogs (`create_share.rs`, `manage_devices.rs`, `add_device.rs`, `device_id.rs`, `confirm_quit.rs`, `confirm_delete_share.rs`, `fallback_keyboard.rs`).
     - `widgets.rs`: Reusable UI elements (cards, chips, checkboxes, buttons, chrome, toast).
     - `gfx.rs`: Software 2D rendering primitives, anti-aliased geometry, and TrueType rasterization.
     - `renderer.rs`: Main UI coordinator invoking active screen and modal renderers.
   - **Strict Rule**: Pure read-only view. Receives `&AppState` and visual context. **Never mutate domain state directly, and never perform disk I/O or network calls.**

5. **Composition Root (`crates/vita-syncthing-app/src/main.rs`)**
   - **Responsibility**: Initializes subsystems, wires dependencies, executes the VBlank-paced event loop, and exports required VitaSDK entrypoint symbols (`_newlib_heap_size_user`, `sceUserMainThreadStackSize`).
   - Must remain concise and free of monolithic rendering or business logic.
   - Imports the layers from the crate's library (`use vita_syncthing_app::...`). `src/lib.rs` only declares `domain`, `application`, `infrastructure` and `ui`; the VitaSDK entrypoint symbols stay in `main.rs`.

6. **Tests (`crates/vita-syncthing-app/tests/`)**
   - **No `#[cfg(test)] mod tests` inside `src/`** (neither in `main.rs` nor in layer modules). Tests are host integration tests against the library's public API, one file per use case / component (`share_service.rs`, `input_controller.rs`, `power_policy.rs`), with shared helpers in `tests/common/mod.rs` (`temp_test_dir`, `press`, `tap`). A test belongs to the file of the component it exercises, not to `input_controller.rs` just because it goes through `InputController::handle_input`.
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
  - **Messages from inner layers**: the domain, application and infrastructure layers never build display strings. They set a `UiText` variant (status line, peer / folder labels, toasts, `Files` rows) and `ui::i18n::messages` renders it with each language's `Messages` templates (`{placeholder}` syntax). Only the protocol log (`syncthing.log`) stays in English.
  - **Factorized UI Copy & Multi-Language Support**: Never scatter hardcoded user-facing strings or button labels across screens and modals. Place all reusable actions, tab titles, hints, status badges, and formatting helpers in `ui/i18n/` (with static language catalogs in `en.rs`, `fr.rs`, `es.rs`, `de.rs`, `it.rs`) and `ui/strings.rs` so copy is deduplicated and i18n-ready without per-frame heap churn.
  - **Avoid "Omnibus" Files**: Never create catch-all files like `display.rs` or `utils.rs`. Group code by specific domain responsibility.

These size and SRP rules apply to **both crates** (`vita-syncthing-app` and `syncthing-core`).

---

### 3.4 Input Pattern: Elements Declared by the Screen (`HitMap`)

An interactive element is declared **once, where it is drawn**. Never hit-test with pixel coordinates in the application layer: button positions depend on the label widths of the current language.

- **Declaration (UI)**: the view declares the element where it draws it; `ui::renderer::draw_frame` records the frame's elements in `AppState::hit_map` (the one piece of state the renderer writes).
  - *Pages*: `widgets::draw_action_hotspot` (or `HitMap::add`) with a `UiAction`, a D-pad `FocusSlot` and, through the `PadButton` it displays, a face-button shortcut. The sidebar (`draw_chrome`) declares the tabs and the transfer card.
  - *Hint bar*: `widgets::draw_hint_bar` with `Hint::action(button, label, action)`: touching the hint, or pressing the button it shows, triggers the action (`Hint::info` for purely informative entries). A hint that does not fit on the bar is left out, never cut off.
  - *Modals*: `declare_modal` right after the card (what a touch outside / inside does), `draw_press_button` and `draw_press_hints` (touching them is the same as pressing the pad button they show: `UiAction::Press`), `declare_touch` with `UiAction::ModalRow(idx)` for list rows and keypad keys. Page elements are not interactive while a modal is open.
- **Resolution (application)**: `InputController` resolves touch (`HitMap::action_at`), `Cross` on the focused element (`HitMap::focused_action(state.focus_slot())`) and face buttons (`HitMap::shortcut_action`), then `InputController::execute` runs the `UiAction` (global ones itself, page ones through the page's controller). `ModalInput` turns a touch into the pad button or the row it stands for, so modal handlers only know buttons and rows. Executing an action must tolerate a stale frame (the state may have changed since it was drawn): validate indexes.
- **Adding an element**: add a `UiAction` variant if needed, declare the element in the view, execute the action in the page's controller. Never add a coordinate to the application layer.
- **Tests** draw the frame before feeding input (`tests/common`: `draw`, `press`, `tap`, `feed`) and locate elements through the hit map (`rect_of`, `rects_of`, `tap_action`, `tap_rect`), never with literal coordinates. `main_screens.rs` and `modal_touch.rs` check in all five languages that every element is on screen and that hints / buttons do not overlap: run them after any layout or label change.

### 3.5 Tunable Values (`domain/defaults.rs`)

Timings, limits and presets that govern the app's behaviour (rescan interval, retry delays, discovery period, maximum concurrent devices, power grace periods, frame rates, screensaver timeouts, share presets) live in `domain/defaults.rs`. Never write such a value inline in `sync_worker.rs`, `main.rs` or a policy: add a documented constant there. The values `syncthing-core` lets an application tune are passed through `SyncTuning` (`SyncControl::with_tuning`); purely internal protocol timings stay as named constants in the file that uses them.

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
- **Folder authorization**: `FsSyncStorage::is_folder_shared_with(folder_id, peer)` is the single gate of every session. A folder is only announced, indexed, downloaded or served for the devices declared with `set_folder_devices`; a folder a peer merely announces in its `ClusterConfig` is reported through `SyncEvent::ClusterConfigReceived` (so the app can ask for consent) but never synchronized. Any new message handler in `session/` must check `FolderSyncState::is_shared_folder` or `is_folder_shared_with` before touching a folder.
- **Folder types (read-only shares)**: `FsSyncStorage::set_folder_type(folder_id, FolderType)`. A `FolderType::ReceiveOnly` folder (the app's read-only share) is announced with `type = RECEIVE_ONLY` (BEP `Folder` field 3) in our `ClusterConfig` and applies the peer's changes like any folder, but its local changes are never offered: `index_exchange::publish_index` passes its index through `folder_sync::mask_local_changes`, which flags `invalid` every entry whose version the peer has not seen (local creation, modification or deletion), as Syncthing does. Any new code path that publishes an index must go through `publish_index`. *Revert Local Changes*: `SyncControl::request_revert(folder)` is taken once by a persistent session sharing that receive-only folder (`session/revert.rs`, step F of the persistent loop): `FsSyncStorage::revert_local_changes` deletes entries the peer does not have and resets the version of locally modified / deleted ones, then the peer's version is pulled and published, and `SyncEvent::LocalChangesReverted` is emitted.
- **`storage/`**: `FsSyncStorage` split by concern — registry & paths (`mod.rs`), block hashing / delta / block reads (`blocks.rs`), crash-safe download temp files (`download.rs`), `sceIo` path quirks (`vita_fs.rs`). Extend `FsSyncStorage` with a new `impl` block in the file owning that concern instead of growing `mod.rs`.
- **Crash-safe writes**: a file is never deleted before its replacement is in place. Use `storage::atomic::{write_file_synced, replace_file, recover_replaced_file}` (temp file, previous version kept as a backup, recovery on the next load); downloads, folder databases and the app's `.cfg` files all go through them.
- **Scans never turn an error into a deletion**: an entry that exists but cannot be inspected (`DiskEntryKind::Unreadable`, failing `stat()`) keeps its database entry, as does everything below it. Only an entry absent from a successful directory listing becomes a tombstone.
- **Sizes from the network are bounded before allocating**: block sizes above `BEP_MAX_BLOCK_SIZE`, negative sizes / offsets and LZ4 output beyond the announced length are rejected.
- **`local_db/`**: in-memory model (`mod.rs`), on-disk text format (`codec.rs`), process-wide cache + atomic save (`cache.rs`). Version arithmetic lives in `version_vector.rs`.
- **`folder_sync/`**: disk scan → local `Index` (`scan.rs`) vs. pull planning, conflicts and deletions (`pull.rs`).
- **`tls/`**: TLS transport only — certificate verifier, client connection setup / ID probe, frame polling. Nothing about sync.
- **`session/`**: the BEP sync engine, **one file per phase or concern**:
  - `mod.rs` — `connect_and_sync*` orchestrator (connect → `Hello` → phases), must stay a thin sequence of phase calls
  - `cluster_exchange.rs` — initial `ClusterConfig` exchange + initial remote `Index`
  - `secondary.rs` — `+2` secondary connections (open, promotion, polling, keepalive)
  - `initial_sync.rs` — initial download, local `Index` publication, request draining
  - `persistent.rs` — persistent keepalive / rescan loop
  - `revert.rs` — revert of a receive-only folder's local changes (UI-requested through `SyncControl`)
  - `dispatch.rs` — routing of primary-connection messages in an established session
  - `index_exchange.rs` — local index publication and runtime `ClusterConfig` / `Index` handling
  - `download.rs` / `upload.rs` — block transfers (pull / serve)
  - `folders.rs` — per-session folder state (pure); `context.rs` — `SessionContext` (identity, storage, stats, event sink, deferred messages); `control.rs`, `events.rs`, `outcome.rs` — public API types
  - `keepalive.rs` — `Ping` when the keepalive interval elapsed; `tuning.rs` — `SyncTuning` (rescan / keepalive intervals chosen by the app, passed through `SyncControl::with_tuning`)
- **Session invariants** (covered by `tests/session.rs` with the fake peer):
  - *No message is dropped during a download*: while waiting for a block `Response`, `Index` / `IndexUpdate` / `ClusterConfig` / `Close` are deferred (`SessionContext::defer_message`) and the session loops read through `dispatch::next_primary_message`, never `try_read_bep_message` directly.
  - *Index publication is incremental*: always publish through `index_exchange::publish_index` (`FolderSyncState::take_unpublished`): one full `Index` per folder and session, then `IndexUpdate`s holding only entries with a newer sequence, in batches of `INDEX_BATCH_FILES`.
  - *Long local work keeps the connection alive*: scan through `index_exchange::scan_local_index` / `plan_pull_with_progress`, whose progress callback sends keepalive `Ping`s. After a pull, publish from `local_index_snapshot` (no second scan).

#### Rules for new code

- **No long parameter lists**: pass `SessionContext` (identity, storage, peer, stats, `emit`) and `FolderSyncState` instead of threading counters and maps through functions. A new `#[allow(clippy::too_many_arguments)]` in `syncthing-core` is a design smell (the public `connect_and_sync_with_control` signature is the only accepted exception).
- **Named constants for timings**: protocol timeouts and intervals are `const` items at the top of the file that uses them, never inline magic `Duration`s.
- **New BEP message** → new file in `bep/`; **new session phase** → new file in `session/`; **new storage concern** → new file in `storage/` with its own `impl FsSyncStorage` block.

#### Tests

- **No `#[cfg(test)] mod tests` inside `src/`.** All `syncthing-core` tests live in `crates/syncthing-core/tests/`, one file per module (`bep_codec.rs`, `device_id.rs`, `discovery.rs`, `identity.rs`, `storage.rs`, `local_db.rs`, `folder_sync.rs`, `version_vector.rs`, `session.rs`), with shared helpers in `tests/common/mod.rs` (e.g. `fresh_temp_dir`).
- Tests exercise the public API. If something important can only be tested through a private item, it is usually a sign the item deserves its own module with a small public surface (as `storage::vita_fs::with_device_root_slash` or `FolderDb::{serialize, parse}`).
- Session tests run against `tests/common/fake_peer.rs`: a scriptable local TLS 1.3 + BEP/1.0 server (`FakePeer::spawn(seed, |conn| ...)`) that announces folders, sends indexes, serves blocks and records every message received. Use it for any change in `session/`.
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
   - Consent is enforced by `syncthing-core`, not only by the UI: `spawn_sync_worker` declares each share's linked devices with `FsSyncStorage::set_folder_devices`, and sessions ignore every other folder (see §3.3 **Folder authorization**).
   - Remote folders received via `ClusterConfig` never become shares on their own: `FolderOfferService::receive` queues them as `FolderOffer`s and the `FolderOffer` modal asks for consent. An accepted folder defaults to `enabled: false` (**Paused**) until the user selects a target folder and enables it in the **Overview** share list; a declined one is saved in `ignored_folders.cfg`.

---

## 5. Runtime Configuration Files on PS Vita (`ux0:data/syncthing/`)

All persistent application state lives in `ux0:data/syncthing/` (or `/tmp/syncthing-vita` on host builds).

Text fields of the `.cfg` files (device names, addresses, folder IDs, labels, paths) are written with `config_fields::escape_field` (`%` -> `%25`, `|` -> `%7C`, line breaks -> `%0A` / `%0D`) and read with `unescape_field`: names, folder IDs and labels come from remote peers and must never be written verbatim. Always go through these helpers when adding a field or a file.

| File | Purpose | Format |
| :--- | :--- | :--- |
| `device_seed.bin` | 32-byte random seed for deterministic ECDSA P-256 keypair | Raw 32 bytes |
| `cert.der` | Self-signed X.509 DER certificate whose SHA-256 is the Syncthing `DeviceId` | ASN.1 DER |
| `devices.cfg` | Known/discovered remote Syncthing peers | `<device_id_hex>\|<name>\|<ip:port>\n` |
| `shares.cfg` | Configured shared folders, linked devices and access mode (`rw` read & write, `ro` read only; missing means `rw`) | `<folder_id>\|<label>\|<target_path>\|<0\|1>\|<dev_hex1,dev_hex2>\|<rw\|ro>\n` |
| `ignored_folders.cfg` | Folder offers declined by the user or deleted shares (not prompted again until `Settings` · `Clear Declined`) | `<folder_id>\|<device_id_hex>\n` |
| `theme.cfg` | Persistent colour theme mode (`dark` or `oled`) | `<dark\|oled>\n` |
| `language.cfg` | UI language chosen in `Settings` (`system` follows the PS Vita language; a missing file means `system`) | `<system\|en\|fr\|es\|de\|it>\n` |
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
- `storage.rs`: streaming download, delta sync, `ux0:` device-root path variants, folder types
- `local_db.rs`: persistent folder database text format roundtrip
- `folder_sync.rs`: version convergence across peers, local deletions become tombstones, receive-only masking of local changes, revert of local changes
- `version_vector.rs`: version vector ordering / merge / bump
- `bep_codec.rs` / `storage.rs` / `folder_sync.rs` also cover the hardening rules: bounded LZ4 output, bounded block sizes, interrupted download commit recovery, unreadable entries never recorded as deleted
- `session.rs`: folder authorization (per-device sharing gate, block requests refused for unlinked peers, unconfigured or paused folders) and end-to-end sessions against the fake peer (only shared folders are announced / pulled, an `IndexUpdate` received during a download is applied, a local change is published as an incremental `IndexUpdate`, batched publication, scan progress for keepalives, a receive-only folder is announced as such, pulls the peer's changes and publishes its local changes as invalid, a revert request restores it once)

`vita-syncthing-app` tests live in `crates/vita-syncthing-app/tests/` (`cargo test -p vita-syncthing-app`):
- `share_creation.rs`: a new share without a linked device is discarded, a linked one is kept, `Files` tab folder selection, already-shared folders are not selectable
- `folder_offers.rs`: offered folders need consent, accept / decline / later, a crafted folder label cannot inject a share, declined offers persisted, deleted shares are declined and do not come back, `Settings` · `Clear Declined` (D-pad and touch)
- `share_service.rs`: share deletion, share creation, device linking, paused accepted folders, `ConfigRepository` roundtrip, manual LAN pairing
- `input_controller.rs`: global routing — an active screensaver swallows input and wakes on a button press or touch only
- `main_screens.rs`: sidebar tabs and transfer card, `Overview` device card / share list / hints, `Files` rows and `Open` chip, `Settings` hints; geometry of every page in all 5 languages (elements on screen, hints not overlapping)
- `modal_touch.rs`: modal touch resolved from declared elements (template rows, device rows, pause / done buttons, `Scan QR` button in every language, `Triangle` LAN scan without devices, text box -> system keyboard, keypad keys, folder offer buttons, outside / inside touches); geometry of every modal in all 5 languages (on screen, hints and buttons not overlapping)
- `dialogs.rs`: `START` -> `ConfirmQuit` -> `Cross` quits, `ConfirmQuit` touch targets, `ConfirmDeleteShare` (`Square` on the Overview, cancel / confirm)
- `add_device.rs`: add-device-by-short-ID via system keyboard (placeholder upgraded on discovery), virtual keyboard hidden until requested
- `theme_settings.rs`: theme toggle (D-pad `Cross`, `Square`, touch) and `theme.cfg` persistence
- `folder_picker.rs`: `Files` tab `Circle` walks up device-root paths, then leaves the tab
- `dpad_navigation.rs`: sidebar <-> page focus, Overview device card buttons / share rows / buttons, Files rows / Up button
- `qr_scan.rs`: Device ID extraction from QR payloads, `quircs` decoding of a rendered QR code, scanner modal navigation, adding the scanned device
- `power_policy.rs`: standby blocking while syncing (charging / battery / transfer grace period), CPU clock level (full speed only during the initial sync phase and transfers), initial sync phase tracked per peer from `SyncEvent`s
- `ui_text.rs`: every `UiText` message renders in all 5 languages with its placeholders filled, translated Device ID errors
- `shares_screen.rs`: `Overview` share list input resolved from the drawn frame (touch targets in all 5 languages, card selection, face-button shortcuts, `Cross` on the focused element, stale element ignored, nothing declared behind the screensaver)
- `frame_pacing.rs`: dynamic frame rate (input -> next VBlank, animations 30 FPS, background updates 10 FPS, static 1 FPS), toast expiry and screensaver speed independent of the frame rate
- `worker_slot.rs`: a new worker starts only after the previous one ended, bounded wait on quit
- `config_fields.rs`: interrupted `.cfg` replacement restored from its backup, `.cfg` field escaping roundtrip, legacy `%` text kept, hostile names / labels through `devices.cfg` / `shares.cfg` / `ignored_folders.cfg`, invalid linked-device entries dropped
- `share_mode.rs`: read & write / read only toggle (`L` and touch in `ManageDevices`), `shares.cfg` persistence (legacy lines without a mode are read & write), mode / revert buttons geometry in all 5 languages, revert of local changes (`START` in `ManageDevices` + `ConfirmRevertShare`, pad and touch, read-only shares only), `LocalChangesReverted` toast
- `devices_scan.rs`: `ManageDevices` LAN scan — immediate without a linked device, after the delay with one, cancelled on close (and re-armed on reopening), also while another device is connected
- `language_settings.rs`: language preference codec / cycle, `language.cfg` persistence (defaults to the system language), `Settings` language card (D-pad and touch)

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
No Sony firmware is needed (the app only uses HLE modules and embeds its fonts). `scripts/vita3k_launch.sh` runs Vita3K under `xvfb-run` when there is no X display.
```bash
just run-vita3k
# or, with an already built VPK:
mkdir -p ~/.local/share/Vita3K/Vita3K/ux0/app/SYNCTHING
unzip -qo target/armv7-sony-vita-newlibeabihf/release/SYNCTHING.vpk -d ~/.local/share/Vita3K/Vita3K/ux0/app/SYNCTHING
timeout 10s ./scripts/vita3k_launch.sh -z --frontend sdl -l 2 -r SYNCTHING || true; pkill -x Vita3K || true
tail -n 5 ~/.local/share/Vita3K/Vita3K/ux0/data/syncthing/syncthing.log
```

### Scripted UI Smoke Tests in Vita3K (`ui-script` feature)
The opt-in `ui-script` Cargo feature (never in release builds) replays inputs from `ux0:data/syncthing/ui_script.txt` and dumps frames on request (`infrastructure/ui_script.rs`). Steps, one per line: `wait <frames>`, `press <BUTTON>` (`CROSS`, `CIRCLE`, `SQUARE`, `TRIANGLE`, `UP`/`DOWN`/`LEFT`/`RIGHT`, `L`, `R`, `START`, `SELECT`), `tap <x> <y>`, `ime <text>`, `shot <name>`.
```bash
printf 'wait 60\npress R\nwait 10\nshot shares\n' > /tmp/shares.txt
just ui-scenario /tmp/shares.txt
```
PNGs land in `target/ui-shots/`; the Vita3K app data directory is backed up and restored around the run. It needs neither firmware nor a display (see above). `wait` frames count as activity (the automatic screensaver never starts during a script) but do not wake a screensaver started with `press SELECT`.

---

## 7. UI Layout & Controls Reference (`VitaSend` Design System)

The UI (`gfx.rs` + `ui/screens/*` + `ui/modals/*` + `ui/renderer.rs`) renders at native `960x544` with a `212px` left sidebar (`SIDEBAR_W = 212`), main content area (`x = 236..936`, `y = 86..488`), and bottom button hint bar (`y = 500..544`):

- **Dynamic frame rate** (`application/frame_pacing.rs`): the main loop wakes up on every display VBlank (60 Hz) to poll input, but a frame is only rendered when needed — on the next VBlank after user input (press, tap, IME, dialog, toast expiry), at 30 FPS for animations (OLED screensaver, QR viewfinder), at most 10 FPS for background updates (sync progress, logs), and once per second when static. When adding a time-based animation, set `FrameSignals::animating` in `main.rs` and scale its motion by the elapsed VBlanks passed to `VitaUiRenderer::render`; never count time in rendered frames.
- **D-pad focus** (`domain/navigation.rs` + `application/dpad_navigation.rs`): focus is on the sidebar or on the page. Sidebar: `Up`/`Down` switch tabs, `Right`/`Cross` enter the page. Page: `Up`/`Down` move between rows (including the top row: the `Overview` device card buttons, the `Files` `Up` button), `Left`/`Right` between a row's buttons, `Left` on the first one returns to the sidebar, `Cross` activates the focused element (drawn with `draw_focus_ring`). On `Files`, `Right` opens the folder. Face buttons remain direct shortcuts.
- **Left Sidebar Tabs** (`L` / `R` shoulder buttons, D-pad from the sidebar, or touch tap):
  1. `Overview` (`ActiveTab::Dashboard`): compact device status card (the page header row: `QR / ID` modal (`Circle`) and `New Share` modal (`Triangle`) buttons), then the list of shared folders (4 visible, scrolling with the selection): `Devices` assignment modal (`Cross`), `Delete` with confirmation (`Square`) and `Folder` path picker (`Triangle`) on each card. A row's face buttons answer only while that row has the D-pad focus (`HitMap::shortcut_action` prefers the focused row, then page-level elements): `Triangle` is `Folder` on a share row and `New Share` anywhere else, and the hint bar shows which. A sync is started by tapping the sidebar transfer card. The protocol log is only on `Settings`.
  2. `Files` (`ActiveTab::FolderPicker`): Interactive `ux0:` directory browser. Selecting a folder (`Cross` or tap) starts creating a share of it (folders already shared are greyed out and cannot be selected or opened); `Triangle` (or tapping the `Open` chip) enters a subfolder, `Circle` goes up/back. The `Overview` share `Folder` action reuses this browser to change a share's target path.
  3. `Settings` (`ActiveTab::Settings`): Dark / Pure-Black OLED theme switcher (`Cross`), `QR / ID` modal (`Circle`), `Language` card (D-pad `Down` + `Cross` or tap: System -> English -> Français -> Español -> Deutsch -> Italiano, saved in `language.cfg`), `Declined folders` card (D-pad `Down` + `Cross` or tap `Clear Declined` to forget declined folder offers so peers can offer them again), and BEP/1.0 protocol log viewer.
- **Modal Dialogs** (`ModalDialog` in `domain/models.rs`, rendered by `ui/modals/*`):
  - `CreateShare`: Create a share from PS Vita folder presets (`ux0:user/00/savedata`, `ux0:picture`, `ux0:music`, RetroArch saves) or browse `ux0:`. The share is only kept once a device is linked in the `ManageDevices` modal that follows.
  - `ManageDevices`: Link/unlink discovered Syncthing peers to a share with circular checkboxes (`draw_row_tick`), add one by ID (`Square`) or by QR code (`R · Scan QR`); changes apply when the dialog closes (`Circle · Done`: `config_dirty` restarts the worker, which advertises the folder in its `ClusterConfig`); opening the dialog scans the LAN (`DevicesScan`): at once for a share with no linked device, after `DEVICES_DIALOG_SCAN_DELAY` (3 s, counted in VBlanks by `main.rs`, cancelled when the dialog closes) otherwise, also while another device is connected (`UserCommand::ScanLan`: `main.rs` asks the running worker for a discovery cycle now with `SyncControl::request_discovery`, or starts a sync when none runs); while no device is known, `Triangle` (hint `Scan LAN`) scans again; `SELECT` pauses / enables the share; `L` (header button showing the current mode) switches it between **Read & write** and **Read only** (Syncthing "Receive Only": the Vita receives its peers' changes but never sends its own). On a read-only share, `START · Revert` (header) opens `ConfirmRevertShare`. Read-only shares carry a lock badge on their `Overview` card.
  - `ConfirmRevertShare`: Syncthing's *Revert Local Changes* for a read-only share: files added on the PS Vita are deleted, edited or deleted files are downloaded again from the peers. `Cross` confirms, `Circle` cancels; both return to `ManageDevices`. Runs in the next connected session; a toast reports the result.
  - `AddDeviceById`: Enter a remote Short / Full Device ID with the PS Vita system keyboard (opened on entry or by tapping the text box). The fallback Base32 keypad is hidden by default; `Triangle · Virtual Keyboard` shows / hides it.
  - `ScanDeviceQr`: Rear-camera (`SceCamera`) viewfinder opened with `R · Scan QR` from `AddDeviceById` or from `ManageDevices` (next to `+ Add by ID`). The camera runs only while the modal is open; a decoded full Device ID is added like a typed one, `Circle` goes back to the ID form. Vita3K's emulated camera only delivers blank frames, so decoding must be tested on hardware.
  - `ShowDeviceId`: Displays a scannable QR code (`qrcodegen`) alongside the 8-group Syncthing Device ID.
  - `ConfirmDeleteShare`: Triggered by `Square` on the `Overview` share list; removes the share from `shares.cfg` (files on `ux0:` are kept) and registers the folder as declined for its linked devices (`ignored_folders.cfg`), so peers do not offer it back until `Settings` · `Clear Declined`.
  - `FolderOffer`: Opens when a peer's `ClusterConfig` lists a folder that is not shared with it yet (including a deleted share). `Cross · Accept` creates a paused share and opens `Files` to choose its target, `Square · Decline` ignores it (`ignored_folders.cfg`, cleared from `Settings` · `Clear Declined`), `Circle · Later` postpones it until the app restarts.
  - `ConfirmQuit`: Triggered by `START` (or tapping `Quit`), warns if a sync is in progress before exiting to LiveArea.
- **Sync activity** (`AppState::sync_activity`: `Downloading` / `Uploading` / `Ready` / `Idle`): the single source for the sidebar transfer card and the OLED screensaver status line, so they never disagree.
- **OLED Screensaver**: Activates automatically after 12s of inactivity during active sync (or 45s when idle, or immediately via `SELECT`), rendering a pure `#000000` black background with a bouncing progress widget to protect PS Vita 1000 OLED screens.
