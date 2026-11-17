# AGENTS.md — Contributor & AI Agent Guide for VitaSyncthing (`SYNCTHING`)

This document provides essential architectural context, build workflows, and **real PlayStation Vita hardware constraints** for future coding sessions. Read this file before modifying any crate or build script.

---

## 1. Language & Code Quality Rules

- **English Only**: All source code comments, docstrings, UI labels, log messages, commit messages, and documentation **must be written in English**.
- **Zero Warnings**: Both host tests (`cargo test --workspace`) and the PS Vita release build (`just vpk`) must compile with **zero compiler warnings**. Use `#[cfg(target_os = "vita")]` and `#[cfg(not(target_os = "vita"))]` cleanly so host and Vita targets both stay warning-free.
- **Visual Changes Are Checked in the Emulator**: any change that alters what is drawn (layout, labels, translations, colours, icons, new screen or modal) must be (1) run in Vita3K, (2) captured with screenshots of every affected screen / state, and (3) shown with those screenshots in the PR description. Host tests passing is not enough. See §6 *Screenshots for Visual Changes*.
- **No Unapproved C/ASM Crypto Dependencies**: Never add `ring`, `aws-lc-rs`, or `openssl` to the dependency tree. They do not compile for `armv7-sony-vita-newlibeabihf`. Always use pure-Rust cryptography (`rustls` with `default-features = false`, `rustls-rustcrypto`, `p256`, `sha2`, `rcgen`).

---

## 2. Workspace Architecture

```text
syncthing-vita
├── Cargo.toml                        # Workspace manifest (Edition 2021, LTO + opt-level = "z")
├── Justfile                          # Task runner (test-host, check-vita, build-vita, vpk)
├── scripts/build_vpk.sh              # Full VPK build pipeline (vita-elf-create, vita-make-fself, vita-mksfoex, vita-pack-vpk); APP_VER from the Cargo version
├── scripts/build_plugin.sh           # Background sync plugin modules (syncthing_launcher.suprx, syncthing_worker.suprx), audited by scripts/audit_plugin.sh (see §8)
├── plugin/                           # Background sync plugin: include/syncthing_plugin.h (launcher <-> worker contract), launcher/ (stage 1, C), worker/exports.yml (see §8)
├── docs/screenshots/<branch>/        # Vita3K screenshots of visual changes, embedded in PR descriptions (see §6)
├── scripts/gen_livearea.py           # Generates the LiveArea images (assets/sce_sys/: icon, background, gate — PS Vita + Syncthing emblem)
└── crates/
    ├── syncthing-core/               # Platform-independent Syncthing BEP/1.0 & Discovery engine
    │   ├── src/
    │   │   ├── lib.rs                # Module declarations & public API re-exports ONLY (no logic, no tests)
    │   │   ├── proto.rs              # Protobuf wire primitives (varint, fields, WireIter)
    │   │   ├── device_id/            # DeviceId: mod.rs (format, hex, short-ID matching), parse.rs (user input), base32.rs (Base32 + Luhn mod 32)
    │   │   ├── cert.rs               # Deterministic ECDSA P-256 X.509 identity (DeviceIdentity) & DER builder
    │   │   ├── version_vector.rs     # Version vector arithmetic (ge / merge / bump, short device ID)
    │   │   ├── bep/                  # BEP/1.0 pure codecs: header, framing, lz4, hello, cluster_config, index, request, close
    │   │   ├── discovery/            # announce.rs (codec), addresses.rs, local_ip.rs, udp.rs (broadcast cycle), lan_scan.rs (TCP 22000 + TLS probe)
    │   │   ├── storage/              # FsSyncStorage: mod.rs (folder registry & paths), blocks.rs, download.rs (temp file assembly), atomic.rs (crash-safe replacement), names.rs (internal names, validation of peer-announced names), vita_fs.rs (sceIo quirks)
    │   │   ├── local_db/             # Persistent folder DB: mod.rs (LocalEntry, FolderDb), codec.rs (text format), cache.rs (process-wide cache)
    │   │   ├── folder_sync/          # Reconciliation: scan.rs (disk scan -> local Index), pull.rs (pull planning, conflicts, deletions)
    │   │   ├── tls/                  # TLS 1.3 transport only: verifier.rs, client.rs (BEP streams, ID probe), polling.rs
    │   │   └── session/              # BEP sync engine, one file per phase (see §3.3)
    │   └── tests/                    # Integration tests, one file per module (+ common/ helpers)
    ├── syncthing-cli/                # Command-line Syncthing client for Linux & macOS (see §3.6)
    │   ├── src/
    │   │   ├── lib.rs / main.rs / cli.rs  # Module declarations, entry point, `clap` command line
    │   │   ├── paths.rs / identity.rs     # Home directory (`--home`, `$SYNCTHING_CLI_HOME`), device seed
    │   │   ├── config/               # `config.toml`: model, devices, folders, options, atomic store
    │   │   ├── commands/             # One file per command: id, device, folder, config, discover, run
    │   │   └── daemon/               # `run`: listener, dialer, lan, global, relay, sessions, events, reports, connection_slots, timing
    │   └── tests/                    # Config, identity, commands, and CLI <-> CLI sync over localhost
    ├── syncthing-vita-worker/        # Stage 2 of the background sync plugin (syncthing_worker.suprx): one bounded cycle, host-tested (see §8)
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
            │   ├── folder_picker.rs  # FolderPicker (`Files` browser state: mode, browsed directory, rows, selection, scroll window)
            │   ├── screensaver.rs    # ScreensaverMotion (bouncing position of the OLED screensaver widget)
            │   ├── interaction.rs    # HitMap / Hotspot / UiAction / FocusSlot: interactive elements declared by the frame on screen
            │   ├── defaults.rs       # Every tunable value of the app: sync / discovery / power / display timings, limits, share presets
            │   ├── log_view.rs       # LogView (full-screen protocol log viewer: session lines / `Show All` history, scroll window)
            │   ├── initial_sync.rs   # PeerSet: InitialSyncPeers (initial sync phase; full CPU speed meanwhile) and ConnectedPeers (established sessions)
            │   ├── plugin.rs         # Background sync plugin: module names / tai paths, PluginStatus, PluginInfo, PluginRequest, PluginNotice
            │   ├── plugin_config.rs  # PluginConfig (`plugin.cfg` codec: enabled, sync_on_game_exit, interval_secs, unknown lines kept)
            │   ├── tai_config.rs     # Pure taiHEN `config.txt` editor (`*main` entry lookup / add / remove, CRLF kept)
            │   └── ui_text.rs        # UiText: translatable messages (status line, labels, toasts) chosen by inner layers
            ├── application/          # Layer 2: Use Cases, Input Controller & Sync Event Coordinator
            │   ├── mod.rs
            │   ├── share_service.rs  # ShareService (create_share, delete_share, toggle_device_for_share, share target / pause, log_event)
            │   ├── device_registry.rs # DeviceRegistry (LAN peers, devices added by ID, Short ID placeholder upgrade)
            │   ├── folder_browser.rs # FolderBrowser (`Files` tab: presets and subfolders, navigation, selection)
            │   ├── dashboard_controller.rs / shares_controller.rs / files_controller.rs / settings_controller.rs / logs.rs # One controller per page: D-pad focus movement, execution of the page's UiActions (`LogsController`: log viewer scrolling)
            │   ├── share_creation.rs # ShareCreation (new share kept only once a device is linked)
            │   ├── share_mode.rs     # ShareModeService (share read & write <-> read only, revert of local changes)
            │   ├── plugin_service.rs # PluginService (background sync plugin: install / uninstall / enable / status)
            │   ├── folder_offers.rs  # FolderOfferService (folders offered by peers need consent: accept / decline / later)
            │   ├── frame_pacing.rs   # FramePacer (dynamic frame rate: render only when the screen changes)
            │   ├── input_gestures.rs # InputGestures (held Up / Down repeat, touch tap on release, touch drag)
            │   ├── devices_scan.rs   # DevicesScan (LAN scan of the ManageDevices dialog: immediate or delayed, cancelled on close)
            │   ├── dpad_navigation.rs # DpadNavigation (sidebar <-> page focus, Cross on focused element)
            │   ├── qr_scan.rs        # QrScanFlow (scanner events -> AddDeviceById confirmation), extract_device_id
            │   ├── sync_supervisor.rs # SyncSupervisor (sync worker life cycle: start, restart on configuration change, rescan / discovery requests, reconnection)
            │   ├── sync_coordinator.rs # SyncCoordinator (maps WorkerMsg & SyncEvent into AppState & ShareService)
            │   ├── upload_activity.rs # UploadActivity ("Uploading" state ends UPLOAD_DISPLAY_GRACE after the last block served)
            │   ├── input_controller.rs # InputController (resolves touch / Cross / shortcuts against the frame's HitMap, executes UiActions)
            │   └── modal_input/      # ModalInput: resolves touches on the open modal, one key / row handler per modal (create_share, manage_devices, add_device, scan_qr, dialogs)
            ├── infrastructure/       # Layer 3: Persistence, OS Filesystem, PS Vita Hardware & BEP Worker Thread
            │   ├── mod.rs
            │   ├── config_repo.rs    # ConfigRepository (devices.cfg, shares.cfg, ignored_folders.cfg, theme.cfg, language.cfg, syncthing.log, ux0: directory listing)
            │   ├── config_fields.rs  # escape_field / unescape_field (peer-supplied text can never forge `.cfg` fields or lines)
            │   ├── worker_slot.rs    # spawn_after_previous / wait_for_worker (a new sync worker starts only once the previous one ended)
            │   ├── plugin_host.rs    # PluginHost (app0:plugins, ux0:/ur0:tai, plugin.cfg; host builds map device paths below a root), LauncherProbe
            │   ├── plugin_mutex.rs   # GuiRunningLock (`SyncthingGuiRunning` held while the app runs), KernelLauncherProbe (`SyncthingLauncherAlive`)
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
                │   ├── messages.rs   # Messages templates, Strings::text(&UiText), device_id_error
                │   └── plugin.rs     # PluginStrings (background sync plugin card, modal, toasts; catalogs PLUGIN_EN..IT in each language file)
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
                │   ├── settings_plugin.rs # Background sync plugin card of Settings (status chip, Install / Enable / Uninstall)
                │   ├── logs.rs       # Full-screen protocol log viewer (opened from the Settings log card)
                │   └── screensaver.rs
                ├── modals/           # Modal dialog views & fallback keypad
                │   ├── mod.rs
                │   ├── create_share.rs
                │   ├── manage_devices.rs
                │   ├── add_device.rs
                │   ├── scan_qr.rs
                │   ├── fallback_keyboard.rs
                │   ├── device_id.rs
                │   ├── confirm_frame.rs  # Card, header, buttons and hints shared by the three confirmation modals
                │   ├── confirm_quit.rs
                │   ├── confirm_delete_share.rs
                │   ├── confirm_revert_share.rs
                │   ├── confirm_plugin.rs # Install / uninstall the background sync plugin (reboot notice)
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
     - `share_service.rs`: Share creation, deletion, pause, target folder and device linkage. `device_registry.rs`: known devices (discovered, added by ID, placeholder upgrade). `folder_browser.rs`: the `Files` tab browser.
     - `share_creation.rs`: Share creation workflow — a new share is discarded if `ManageDevices` closes with no linked device.
     - `share_mode.rs`: `ShareModeService::toggle` switches a share between `ShareMode::ReadWrite` and `ShareMode::ReadOnly`, saves it and restarts the worker (`config_dirty`). `ShareModeService::request_revert` queues a read-only share in `AppState::pending_reverts`; `main.rs` hands it to the running worker (`SyncControl::request_revert`) once no configuration change is pending, and a sync is started if none runs.
     - `folder_offers.rs`: Consent for folders offered by peers (`FolderOffer` modal); declined offers persisted.
     - `sync_coordinator.rs`: Maps background `WorkerMsg` protocol events into `AppState` updates. `AppState::session_connected` follows `connected_peers` (a peer enters on `SyncCycleCompleted`, leaves on `SessionEnded`). Per-file and per-block events (`FileUpToDate`, `BlockUploaded`) are never logged.
     - `sync_supervisor.rs`: `SyncSupervisor` owns the running worker (message channel + `SyncControl`). `main.rs` forwards `UserCommand::StartSync` / `ScanLan` / quit to it and calls `tick` once per loop iteration; it never calls `spawn_sync_worker` itself.
     - `controllers/`: `input.rs` (`InputController`: translates main-screen button/touch inputs into actions, modal triggers, and commands) and one controller per page (`dashboard.rs` with the share list's `shares.rs`, `files.rs`, `settings.rs`; see §3.4).
     - `modal_input/`: Input handling for open modals, one file per modal (mirrors `ui/modals/`).
     - `dpad_navigation.rs`: D-pad focus between the sidebar and the page elements.
     - **No screen coordinate and no `crate::ui` import in this layer**: what is interactive, and where, is declared by the views (§3.4).
     - `frame_pacing.rs`: `FramePacer` dynamic frame rate (see §7).
     - `plugin_service.rs`: `PluginService` (background sync plugin). `install` copies `app0:plugins/syncthing_{launcher,worker}.suprx` into the active `tai` directory (`ux0:tai/` when `ux0:tai/config.txt` exists, else `ur0:tai/`), copies `config.txt` to `config.txt.bak`, lists only the launcher under `*main` (`domain::tai_config`: deduplicated, stale entries of the other `tai` directory and of the worker dropped, CRLF kept) and writes `enabled=1` to `plugin.cfg`; nothing is written if a module or `config.txt` is missing (toast). `uninstall` cleans both `tai` directories and leaves `enabled=0`. `Enable / Disable` only rewrites `plugin.cfg` (no reboot). Status: `NotInstalled` (no launcher file or no `*main` entry), `InstalledNeedsReboot` (launcher not loaded: `SyncthingLauncherAlive` absent), `Enabled` / `Disabled` (`enabled` flag). The settings controller returns `UserCommand::Plugin(PluginRequest)` and `main.rs` calls `PluginService::execute` with its `PluginHost`.
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
     - `plugin_host.rs`: `PluginHost` file access for the background sync plugin (device paths as is on the Vita; below `<data>/devices` on host builds, `PluginHost::rooted` in tests), crash-safe writes, `plugin.cfg`, and the `LauncherProbe` trait (`FixedLauncherProbe` on host).
     - `plugin_mutex.rs`: named openable kernel mutexes shared with the plugin. `main.rs` holds `GuiRunningLock` (`SyncthingGuiRunning`, created locked) for the app's whole lifetime: the plugin never syncs while it exists, so enabling background sync in `Settings` never starts a sync while the app is open. `KernelLauncherProbe` opens `SyncthingLauncherAlive` (created by the launcher). With the `ui-script` feature only, `ux0:data/syncthing/ui_script_launcher_alive` (`1` / `0`) forces the probe (Vita3K has no taiHEN).
   - **Strict Rule**: All platform-specific `#[cfg(target_os = "vita")]` hardware syscalls (`sce*`) must be encapsulated here. Never leak them into UI or Domain.

4. **Layer 4: UI / Presentation Layer (`crates/vita-syncthing-app/src/ui/`)**
   - **Responsibility**: Visual presentation and 2D vector/font rendering.
   - **Key Modules**:
     - `i18n/`: Zero-allocation multi-language catalogs (`Strings`) supporting English (`en`), French (`fr`), Spanish (`es`), German (`de`), and Italian (`it`).
     - `strings.rs`: Centralized user-facing strings, action constants, button hints, and zero-allocation formatting helpers re-exporting `ui::i18n`.
     - `screens/`: Independent full-screen views (`dashboard.rs` drawing the share list of `shares.rs`, `folder_picker.rs`, `settings.rs`, `logs.rs`, `screensaver.rs`).
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
- **Gestures** (`application/input_gestures.rs`, applied by `main.rs` to every polled frame): a held `Up` / `Down` repeats (`KEY_REPEAT_DELAY`, `KEY_REPEAT_INTERVAL`); a touch is a tap when the finger is lifted without moving more than `TOUCH_DRAG_THRESHOLD` (`touch_tap`, at the touch-down position), otherwise a drag (`touch_drag`). A scrollable list declares its area with `HitMap::add_scroll_area` (`ScrollTarget`, one row per `step` pixels); a drag scrolls the area it started on and never triggers a tap.
- **Adding an element**: add a `UiAction` variant if needed, declare the element in the view, execute the action in the page's controller. Never add a coordinate to the application layer.
- **Tests** draw the frame before feeding input (`tests/common`: `draw`, `press`, `tap`, `feed`) and locate elements through the hit map (`rect_of`, `rects_of`, `tap_action`, `tap_rect`), never with literal coordinates. `main_screens.rs` and `modal_touch.rs` check in all five languages that every element is on screen and that hints / buttons do not overlap: run them after any layout or label change.

### 3.5 Tunable Values (`domain/defaults.rs`)

Timings, limits and presets that govern the app's behaviour (rescan interval, retry delays, discovery period, maximum concurrent devices, power grace periods, frame rates, screensaver timeouts, share presets) live in `domain/defaults.rs`. Never write such a value inline in `sync_worker.rs`, `main.rs` or a policy: add a documented constant there. The values `syncthing-core` lets an application tune are passed through `SyncTuning` (`SyncControl::with_tuning`); purely internal protocol timings stay as named constants in the file that uses them.

---

### 3.6 `syncthing-cli` (Linux & macOS)

A host-only binary built on `syncthing-core` (never cross-compiled for the Vita; still no `ring` / `aws-lc-rs` / `openssl`). Same rules as the other crates: English, zero warnings, 200-400 line files, no `#[cfg(test)]` in `src/`, tests in `crates/syncthing-cli/tests/`.

- **Commands** (`commands/`, one file each): `id`, `device add|list|remove`, `folder add|list|remove|share|unshare|pause|resume`, `config show|set`, `discover`, `run`, and the diagnostics `lookup <ID>` (Global Discovery) and `relays` (relay pool). Configuration in `<home>/config.toml` (`config/`, saved atomically), identity in `<home>/device_seed.bin` (0600, `/dev/urandom`, no fallback), folder databases in `<home>/db/`.
- **Consent**: only configured devices get a session (unknown devices and folders a peer offers are logged with the command that accepts them); a folder is only synchronized with the devices it is shared with (`FsSyncStorage::set_folder_devices`).
- **Daemon** (`daemon/`, `run`): `listener.rs` accepts connections (the PS Vita app never listens: it always dials the CLI); `dialer.rs` keeps one thread per configured device dialing configured / LAN / Global Discovery addresses, then relays; `lan.rs` listens on UDP 21027 continuously (shared with `SO_REUSEADDR` / `SO_REUSEPORT`, so a Syncthing or another instance on the same computer does not deafen it) and answers announcements directly (the Vita only listens briefly after its broadcast); `global.rs` announces `tcp://0.0.0.0:<port>` and our relay URI; `relay.rs` joins a relay (configured `static_relays` or the public pool) and runs invited sessions. `sessions.rs` allows one session per device whatever the link; a session the peer closed (`SessionEnd::ClosedByPeer`) is redialed after `RETRY_AFTER_SESSION`, only a peer that does not accept us yet (`NotAccepted`) waits `RETRY_AWAITING_PAIRING`. Timings and limits live in `daemon/timing.rs`.
- **Bounded state from unauthenticated input**: LAN announcements, connections and relay invitations can carry any device ID or address, so nothing they feed grows without limit: `reports.rs` (`ReportMemory`: unknown devices reported once each up to `MAX_UNKNOWN_DEVICE_REPORTS`, then one line and silence; reports about configured devices / servers kept apart, `MAX_REPORTS_PER_SUBJECT` each), `lan.rs` (`ReplySchedule`: answers of the last reply interval only, configured devices always answered, at most `MAX_LAN_REPLIES_TO_UNKNOWN` others), `connection_slots.rs` (`ConnectionSlots`, RAII slots given back on every exit path): `listener.rs` keeps at most `MAX_UNAUTHENTICATED_CONNECTIONS` connections in their handshake or being turned away, `relay.rs` at most `MAX_PENDING_RELAY_INVITATIONS` invitations being joined (a separate pool, so a relay cannot keep direct connections out); a configured device's session gives its slot back. `relay::triage_invitation` drops invitations from unknown or already connected devices, and those beyond the limit (logged once), before spending a thread. `accept_bep_tls_stream`'s `timeout` bounds the whole handshake.
- **Proxy** (`https_env.rs`): Global Discovery and the relay pool go through `HTTPS_PROXY` / `https_proxy` / `ALL_PROXY` (HTTP `CONNECT`) and also trust `SSL_CERT_FILE` (a TLS-inspecting proxy's CA); connections to peers and relays are always direct.
- **Logs**: one timestamped line per event on stdout; peer-supplied text goes through `log::printable` (no control characters).

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
- **Scans and pulls agree on names** (`storage/names.rs`): only Syncthing-internal names (`is_internal_name`: `.syncthing*`, `~syncthing~*`, `.stfolder`, `.stignore`, `.stversions`) are skipped by scans, and `plan_pull` ignores every peer-announced name that is not `is_syncable_name` (empty, absolute, `.` / `..` components, backslashes, control characters, internal names). An entry that is pulled but not scanned would be announced as deleted to every peer.
- **Folder database saves are batched during downloads**: `record_downloaded` uses `local_db::with_folder_db_deferred` (saved at most every 5 s) and the pull ends with `FsSyncStorage::flush_folder_db`. Any new code that records many entries in a row must do the same; everything else uses `with_folder_db` (saved at once).
- **Failed downloads are retried**: a skipped file marks its folder with `SessionContext::mark_folder_incomplete`; the persistent loop pulls such folders again before each rescan (`index_exchange::retry_incomplete_pulls`).
- **Web servers are authenticated**: `tls::https::open_https_stream` (Global Discovery, relay pool) accepts a certificate whose hash is the URL's `?id=` pin, otherwise verifies the chain (`webpki-roots` plus the application's `HttpsClient::extra_roots`) and the host name; never an unverified certificate. Syncthing peers and relays are identified by their certificate hash.
- **Symlinks are never pulled**: an entry of any type other than file / directory is decoded as `invalid`.
- **Sizes from the network are bounded before allocating**: block sizes above `BEP_MAX_BLOCK_SIZE`, negative sizes / offsets and LZ4 output beyond the announced length are rejected.
- **Every block is verified**: a block hash must be a SHA-256 (`BEP_BLOCK_HASH_LEN` bytes). A block announced with any other hash length is rejected by `find_outdated_blocks` (the file is never kept or downloaded) and by the download's hash check; it is never treated as matching.
- **`local_db/`**: in-memory model (`mod.rs`), on-disk text format (`codec.rs`), process-wide cache + atomic save (`cache.rs`). Version arithmetic lives in `version_vector.rs`.
- **`folder_sync/`**: disk scan → local `Index` (`scan.rs`) vs. pull planning, conflicts and deletions (`pull.rs`).
- **`tls/`**: TLS transport only — certificate verifiers, client connection setup / ID probe, server side (`server.rs`: `accept_bep_tls_stream` completes the handshake within `timeout` in total (not per read) and returns an `IncomingBepStream` whose `peer_device_id` the application checks before running a session), HTTPS to infrastructure servers (`https.rs`: `HttpsServer`, `?id=` certificate pin or public roots, optional client certificate; `HttpsClient`: optional HTTP proxy tunnel (`proxy.rs`, `CONNECT`) and extra trusted roots, chosen by the application — the default is direct with public roots only), frame polling. `BepTlsStream` is a `Client` / `Server` enum: the session code never cares which side dialed. Nothing about sync.
- **Incoming sessions**: `session::accept_and_sync` runs a session on an accepted stream (TCP listener, or a relay invitation with `ServerSocket`). Its `Hello` announces a single connection (the dialer would open the secondary ones), and it sends its `ClusterConfig` first: a dialing `syncthing-core` peer (the PS Vita app) only answers the one it receives.
- **Global Discovery & relays**: `discovery/http.rs` (minimal HTTP/1.1 client), `discovery/global.rs` (`lookup_peer_on`), `discovery/global_announce.rs` (`announce_on`, mutual TLS), `relay/listen.rs` (`RelayListener`: permanent relay mode, `SessionInvitation`s), `relay/pool.rs` (public relay list). The PS Vita app only uses lookups and dials relays; announcing and relay listening are for devices that accept connections (`syncthing-cli`).
- **`session/`**: the BEP sync engine, **one file per phase or concern**:
  - `mod.rs` — `connect_and_sync*` orchestrator (connect → `Hello` → phases), must stay a thin sequence of phase calls
  - `cluster_exchange.rs` — initial `ClusterConfig` exchange + initial remote `Index`
  - `secondary.rs` — `+2` secondary connections (open, promotion, polling, keepalive)
  - `initial_sync.rs` — initial download, local `Index` publication, request draining
  - `persistent.rs` — persistent keepalive / rescan loop; `close_session` (`Close` on every connection + `SessionEnded`)
  - `revert.rs` — revert of a receive-only folder's local changes (UI-requested through `SyncControl`)
  - `dispatch.rs` — routing of primary-connection messages in an established session
  - `index_exchange.rs` — local index publication and runtime `ClusterConfig` / `Index` handling
  - `download.rs` / `upload.rs` — block transfers (pull / serve)
  - `folders.rs` — per-session folder state (pure); `context.rs` — `SessionContext` (identity, storage, stats, event sink, deferred messages); `params.rs` (`SessionParams`), `control.rs`, `events.rs`, `outcome.rs` — public API types
  - `keepalive.rs` — `Ping` when the keepalive interval elapsed; `tuning.rs` — `SyncTuning` (rescan / keepalive intervals chosen by the app, passed through `SyncControl::with_tuning`) and `CycleDrain`
- **Single-cycle sessions** (`SyncControl::single_cycle(tuning, CycleDrain)`, used by the background worker, §8): the session runs its initial cycle (download, local `Index` publication), then drains — answering the peer's `Request`s / `Ping`s / late `Index`es until the peer has been silent for `CycleDrain::idle`, never longer than `CycleDrain::max` — and ends with `Close` + `SessionEnded { reason: "sync cycle completed" }` instead of entering the persistent loop. Unlike a session without a control handle (fixed 4 s drain), it honours `request_stop` at every phase: a stop during the download, after it or during the scan / drain ends the cycle with `InitialSync::Stopped` (`Close`, `SessionEnded "stopped locally"`, no `SyncCycleCompleted`). Persistent and handle-less sessions keep their behaviour (only the download notices a stop; a persistent session reports its initial cycle, then leaves its loop).
- **Session invariants** (covered by `tests/session.rs` with the fake peer):
  - *No message is dropped during a download*: while waiting for a block `Response`, `Index` / `IndexUpdate` / `ClusterConfig` / `Close` are deferred (`SessionContext::defer_message`) and the session loops read through `dispatch::next_primary_message`, never `try_read_bep_message` directly.
  - *Index publication is incremental*: always publish through `index_exchange::publish_index` (`FolderSyncState::take_unpublished`): one full `Index` per folder and session, then `IndexUpdate`s holding only entries with a newer sequence, in batches of `INDEX_BATCH_FILES`.
  - *Long local work keeps the connection alive*: scan through `index_exchange::scan_local_index` / `plan_pull_with_progress`, whose progress callback sends keepalive `Ping`s. After a pull, publish from `local_index_snapshot` (no second scan).

#### Rules for new code

- **No long parameter lists**: pass `SessionContext` (identity, our `Hello`, storage, peer, stats, `emit`) and `FolderSyncState` instead of threading counters and maps through functions. The public entry points (`connect_and_sync`, `connect_and_sync_stream`) take a `SessionParams` (`session/params.rs`). `#[allow(clippy::too_many_arguments)]` is a design smell in both crates.
- **Nothing application-specific in the crate**: no device or client name (the app passes its `BepHello`), no fabricated values in results (`PeerHandshakeResult::peer_hello` is `None` when the peer refused BEP).
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
   - A LAN announcement is not authenticated: `DeviceRegistry::register_discovered_peer` only lists the peer (at most `MAX_DISCOVERED_PEERS`), and a Short ID placeholder is upgraded to a full ID only by `upsert_device`, once a TLS session proved the identity.
   - There is no fallback random source: `IdentityStore::generate_random_seed` fails rather than derive a key from guessable values.
   - The app's data directory and its parents can never be a share's folder (`ConfigRepository::is_protected_share_path`: refused by the `Files` browser with a toast, skipped by `spawn_sync_worker`). A share's default directory comes from `ConfigRepository::default_share_dir`, which sanitizes the peer-chosen folder ID.
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
| `plugin.cfg` | Background sync plugin settings, written by `Settings` (`enabled=1` at install, `0` on uninstall), re-read by the launcher before each cycle; missing keys take their defaults (`PLUGIN_DEFAULT_*` in `defaults.rs`: enabled, sync on game exit, 1800 s), unknown lines are kept | `enabled=<0\|1>\nsync_on_game_exit=<0\|1>\ninterval_secs=<u32>\n` |
| `syncthing.log`, `syncthing.log.1` | Persistent protocol & UI event log, rotated above `LOG_MAX_BYTES` (512 KiB) to `syncthing.log.1` | Plain text lines |
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
- `bep_codec.rs` / `storage.rs` / `folder_sync.rs` also cover the hardening rules: bounded LZ4 output, bounded block sizes, interrupted download commit recovery, unreadable entries never recorded as deleted, a pulled `.st*` dot-file is not announced as deleted, invalid peer-announced names are ignored
- `session_inbound.rs`: a listening and a dialing `syncthing-core` device sync a folder both ways (`accept_and_sync`), a peer other than the expected one is refused, `accept_bep_tls_stream` gives up on a silent or trickling client within its timeout
- `https.rs` / `global_discovery.rs`: `HttpsServer` URLs and certificate pins, HTTP parsing, Global Discovery lookups and announcements against local HTTPS servers (`tests/common/tls_server.rs`)
- `http_proxy.rs`: proxy URLs, a request tunnelled through a local `CONNECT` proxy (with `Proxy-Authorization`), refused `CONNECT`, extra PEM roots
- `relay.rs`: relay wire codec, relay pool list, `RelayListener` (join, ping / pong, invitations, refusals)
- `session_single_cycle.rs`: `SyncControl::single_cycle` (pull, publish, idle drain, `Close`), a drain extended while the peer keeps requesting and bounded by its maximum, stop requests before the download and during the drain, persistent handles unchanged
- `session.rs`: folder authorization (per-device sharing gate, block requests refused for unlinked peers, unconfigured or paused folders) and end-to-end sessions against the fake peer (only shared folders are announced / pulled, an `IndexUpdate` received during a download is applied, a local change is published as an incremental `IndexUpdate`, batched publication, scan progress for keepalives, a receive-only folder is announced as such, pulls the peer's changes and publishes its local changes as invalid, a revert request restores it once)

`vita-syncthing-app` tests live in `crates/vita-syncthing-app/tests/` (`cargo test -p vita-syncthing-app`):
- `share_creation.rs`: a new share without a linked device is discarded, a linked one is kept, `Files` tab folder selection, already-shared folders are not selectable, default share presets listed first at the `Files` root unless already shared (selecting one creates it with its preset folder ID)
- `folder_offers.rs`: offered folders need consent, accept / decline / later, a crafted folder label cannot inject a share, declined offers persisted, deleted shares are declined and do not come back, `Settings` · `Clear Declined` (D-pad and touch)
- `share_service.rs`: share deletion, share creation, device linking, paused accepted folders, `ConfigRepository` roundtrip, manual LAN pairing
- `input_controller.rs`: global routing — an active screensaver swallows input and wakes on a button press or touch only
- `main_screens.rs`: sidebar tabs and transfer card (progress bar only while downloading / uploading), `Overview` device card / share list / hints, `Files` rows and `Open` chip, `Settings` hints; geometry of every page in all 5 languages (elements on screen, hints not overlapping)
- `modal_touch.rs`: modal touch resolved from declared elements (template rows, device rows, pause / done buttons, `Scan QR` button in every language, `Triangle` LAN scan without devices, text box -> system keyboard, keypad keys, folder offer buttons, outside / inside touches); geometry of every modal in all 5 languages (on screen, hints and buttons not overlapping)
- `dialogs.rs`: `START` -> `ConfirmQuit` -> `Cross` quits, `ConfirmQuit` touch targets, `ConfirmDeleteShare` (`Square` on the Overview, cancel / confirm)
- `add_device.rs`: add-device-by-short-ID via system keyboard (placeholder upgraded on discovery), virtual keyboard hidden until requested
- `theme_settings.rs`: theme toggle (D-pad `Cross`, `Square`, touch) and `theme.cfg` persistence
- `folder_picker.rs`: `Files` tab `Circle` walks up device-root paths, then leaves the tab; the folder list scrolls with a held `Down` and with touch drags (within bounds, selection kept visible, a drag never selects), a tap still selects
- `input_gestures.rs`: held `Up` / `Down` repeat (delay, interval, release, other buttons never repeat), tap on release within the drag threshold, drag beyond it (no tap), injected taps pass through
- `dpad_navigation.rs`: sidebar <-> page focus, Overview device card buttons / share rows / buttons, Files rows / Up button
- `qr_scan.rs`: Device ID extraction from QR payloads, `quircs` decoding of a rendered QR code, scanner modal navigation (from `AddDeviceById`, `ManageDevices` and `ShowDeviceId`), adding the scanned device
- `power_policy.rs`: standby blocking while syncing (charging / battery / transfer grace period), CPU clock level (full speed only during the initial sync phase and transfers), initial sync phase tracked per peer from `SyncEvent`s
- `ui_text.rs`: every `UiText` message renders in all 5 languages with its placeholders filled, translated Device ID errors
- `shares_screen.rs`: `Overview` share list input resolved from the drawn frame (touch targets in all 5 languages, card selection, face-button shortcuts, `Cross` on the focused element, stale element ignored, nothing declared behind the screensaver)
- `screensaver.rs`: the OLED screensaver progress bar is drawn only while downloading / uploading
- `frame_pacing.rs`: dynamic frame rate (input -> next VBlank, animations 30 FPS, background updates 10 FPS, static 1 FPS), toast expiry and screensaver speed independent of the frame rate
- `worker_slot.rs`: a new worker starts only after the previous one ended, bounded wait on quit
- `config_fields.rs`: interrupted `.cfg` replacement restored from its backup, `.cfg` field escaping roundtrip, legacy `%` text kept, hostile names / labels through `devices.cfg` / `shares.cfg` / `ignored_folders.cfg`, invalid linked-device entries dropped
- `share_mode.rs`: read & write / read only toggle (`L` and touch in `ManageDevices`), `shares.cfg` persistence (legacy lines without a mode are read & write), mode / revert buttons geometry in all 5 languages, revert of local changes (`START` in `ManageDevices` + `ConfirmRevertShare`, pad and touch, read-only shares only), `LocalChangesReverted` toast
- `devices_scan.rs`: `ManageDevices` LAN scan — immediate without a linked device, after the delay with one, cancelled on close (and re-armed on reopening), also while another device is connected
- `log_viewer.rs`: log viewer opened from the `Settings` log card (touch, D-pad `Open`), closed with `Circle`, scrolling by line / page (D-pad, `L` / `R`, touch) within bounds, new lines followed at the bottom and a scrolled-back window kept, `Show All` loading `syncthing.log` and back, session buffer size, geometry in all 5 languages
- `language_settings.rs`: language preference codec / cycle, `language.cfg` persistence (defaults to the system language), `Settings` language card (D-pad and touch)
- `tai_config.rs`: taiHEN `config.txt` editing (`*main` lookup, insertion at the end of the section or a new section, removal, stale / duplicate entries dropped, idempotency, comments and CRLF kept)
- `plugin_config.rs`: `plugin.cfg` codec (exact serialized form, defaults, invalid values, unknown lines kept)
- `plugin_status.rs`: plugin module names / `tai` paths, active `tai` directory, status resolution, card buttons per status
- `plugin_host.rs`: `PluginHost` device path mapping, crash-safe writes and recovery, `plugin.cfg` persistence, probe
- `plugin_service.rs`: install (`ux0:` vs `ur0:`, `config.txt.bak`, idempotent, stale entries, `enabled=1`, settings kept), failures (no bundled modules, no taiHEN, unwritable `tai`), uninstall from both directories, status transitions and toasts
- `settings_plugin.rs`: `Settings` plugin card (D-pad, touch, `ConfirmPlugin` confirm / cancel, stale elements, end-to-end with `PluginService`); `main_screens.rs` checks the card's buttons and texts in every status and language, `modal_touch.rs` the `ConfirmPlugin` modal

`syncthing-cli` tests live in `crates/syncthing-cli/tests/` (`cargo test -p syncthing-cli`): `config.rs`, `identity.rs`, `cli_commands.rs` (the real binary), `lan.rs` (UDP 21027 shared, bounded reply schedule), `daemon_limits.rs` (bounded "reported once" memory, configured-device reports never crowded out, connection slots given back, relay invitations triaged and capped), `daemon_sync.rs` (two daemons on `127.0.0.1` with discovery and relays off: sync both ways, live changes and deletions, unknown device refused, offered folder not synchronized, receive-only folder).

### Sync Test Between the Vita App (Vita3K) and `syncthing-cli`
```bash
just vita3k-cli-sync
```
`scripts/vita3k_cli_sync.sh` pairs a fresh Vita identity with a fresh CLI home by writing both configurations (the Vita knows the CLI as `dynamic`: it is found by LAN discovery), shares `ux0:data/synctest` with a CLI folder, runs both and checks each side received the other's files. Logs (`vita.log`, `cli.log`) and screenshots land in `target/vita3k-cli-sync/`.

### End-to-End Validation (skill `syncthing-e2e-validation`)
```bash
./.agents/skills/syncthing-e2e-validation/scripts/validate.sh
```
Runs every stage — host tests, Vita build + ELF audit, CLI <-> CLI LAN sync, Vita3K <-> CLI sync, Internet (relay pool, Global Discovery announce + lookup, relay, CLI <-> CLI sync through Global Discovery + relay) — and prints `PASS` / `FAIL` / `BLOCKED` / `SKIP` per stage (see `.agents/skills/syncthing-e2e-validation/SKILL.md`). Run it before proposing any change to sync behaviour; report `BLOCKED` stages as not validated.

### Mutation Testing (`cargo-mutants`)
```bash
just mutants                   # whole workspace (hours on 4 cores)
just mutants syncthing-core    # one crate
cargo mutants -p vita-syncthing-app -f crates/vita-syncthing-app/src/domain/log_view.rs   # one file
```
`.cargo/mutants.toml` sets the scope: `syncthing-core`, `syncthing-cli` and the app's domain / application / persistence / i18n logic. Pixel rendering (`ui/gfx.rs`, screens, modals, widgets, theme), the PS Vita hardware drivers, the `ui-script` feature and the `main.rs` entry points are excluded (host tests cannot observe them; visual changes are checked in Vita3K). Builds use the `mutants` profile (no debug info). Outcomes land in `mutants.out/` (`missed.txt`, `caught.txt`, `timeout.txt`).
- **New code comes with tests that kill its mutants.** Before proposing a change to a covered file, run `cargo mutants -f <file>` on it and kill every `MISSED` mutant, or explain in the PR why it is equivalent (no observable difference: `|` vs `^` on disjoint bits, a `<` vs `<=` on values that are never equal, a clamp that undoes an overshoot) or not testable on the host (Vita-only branch, real Internet, OS-level `EINTR`).
- Prefer exact expected values over round trips (a symmetric encode / decode mutation survives a round trip), and test boundaries on both sides (limit, limit + 1).
- While iterating, restrict the run to the test targets you touch (`cargo mutants -f <file> -- --test <target>`); the final run uses the whole crate suite.
- Test files written for mutation coverage follow the usual rules (one file per component, named after it): e.g. `syncthing-core/tests/{proto_wire,bep_frames,bep_messages,bep_index_codec,lz4_codec,device_id_codec,block_storage,download_staging,folder_db_codec,folder_registry,pull_planning,pull_deletions,scan_changes,revert_local_changes,session_state,session_transfers,session_index,session_control,session_secondary,session_reconnect,tls_client,https_request,http_proxy_tunnel,relay_wire_xdr,relay_join,relay_pool,discovery_udp,lan_scan,local_ip}.rs` (sessions are driven through `tests/common/session_driver.rs`), `syncthing-cli/tests/{daemon_*,lan_beacon,diagnostic_commands,events_format,paths,https_env,...}.rs` (local Global Discovery / relay stand-ins in `tests/common/servers.rs`, never the Internet), `vita-syncthing-app/tests/{app_state,hit_map,domain_models,dashboard_controller,files_controller,settings_controller,logs_controller,manage_devices_modal,create_share_modal,add_device_keypad,confirm_dialogs,device_registry,sync_coordinator_events,power_policy_levels,config_repo_formats,log_rotation,i18n_formatters,...}.rs`.

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
`build_vpk.sh` writes the workspace version (`[workspace.package] version` in `Cargo.toml`) to `param.sfo` as `APP_VER`: major on two digits, minor and patch on one digit each (`0.1.0` -> `00.10`, `1.2.3` -> `01.23`). A minor or patch above 9 fails the build.

### Publish a Release
1. Bump `version` in the root `Cargo.toml` and commit it on `main`.
2. Push a matching tag:
   ```bash
   git tag -a v0.1.0 -m "v0.1.0" && git push origin v0.1.0
   ```
3. `.github/workflows/release.yml` checks that the tag matches the version, runs the host tests, builds the VPK, audits the ELF symbols and publishes a GitHub Release with `SYNCTHING.vpk` attached (notes generated from the merged PRs).

VitaDB catalogs (NeoVitaDB) download the `SYNCTHING.vpk` asset of the latest release: never rename it.

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
The opt-in `ui-script` Cargo feature (never in release builds) replays inputs from `ux0:data/syncthing/ui_script.txt` and dumps frames on request (`infrastructure/ui_script.rs`). Steps, one per line: `wait <frames>`, `press <BUTTON>` (`CROSS`, `CIRCLE`, `SQUARE`, `TRIANGLE`, `UP`/`DOWN`/`LEFT`/`RIGHT`, `L`, `R`, `START`, `SELECT`), `hold <BUTTON> <frames>` (pressed, then held), `tap <x> <y>`, `drag <x> <y0> <y1> <frames>` (finger moved vertically, then lifted), `ime <text>`, `shot <name>`.
```bash
printf 'wait 60\npress R\nwait 10\nshot shares\n' > /tmp/shares.txt
just ui-scenario /tmp/shares.txt
```
PNGs land in `target/ui-shots/`; the Vita3K app data directory and `ux0:tai` are backed up and restored around the run (`VITA3K_SEED=<dir>` copies a directory onto `ux0:` first, e.g. `data/syncthing/plugin.cfg` + `tai/config.txt` to start from a given background sync plugin state). It needs neither firmware nor a display (see above). `wait` frames count as activity (the automatic screensaver never starts during a script). `START` (or its hint on `Overview` and `Settings`) asks to quit.

If `target/ui-shots/` only holds `vita3k.log` and it reports `qt.qpa.wayland: There are no outputs` (desktop session with its screen off or locked), run the scenario through XWayland instead:
```bash
CARGO_FEATURES=ui-script just vpk
DISPLAY=:0 SDL_VIDEODRIVER=x11 QT_QPA_PLATFORM=xcb ./scripts/vita3k_scenario.sh /tmp/shares.txt 40
```

### Screenshots for Visual Changes (mandatory)
Every change that alters what is drawn goes through these steps before it is reported done or proposed in a PR:
1. **Run it in the emulator**: write a `ui-script` scenario reaching every affected screen / modal / state (focus rings, scrolled lists, both themes or several languages when the change depends on them) and run it with `just ui-scenario`. Host tests do not replace this.
2. **Take screenshots**: one `shot` per affected state, named in order (`1_settings`, `2_log_viewer`, ...). Look at each PNG and check the change is visible and nothing is clipped or overlapping. Show them to the user.
3. **Put them in the PR description**: commit the PNGs on the PR branch itself, in `docs/screenshots/<branch>/` (one folder per branch, `target/` is not committed):
   ```bash
   mkdir -p docs/screenshots/<branch> && cp target/ui-shots/*.png docs/screenshots/<branch>/
   git add docs/screenshots/<branch> && git commit -m "Screenshots of <change>" && git push
   git rev-parse HEAD   # full hash of the screenshots commit, used in the links
   ```
   then embed each one in a `## Screenshots` section of the PR description with a one-line caption, linking through **that commit hash, never the branch name** (a branch link breaks once the branch is merged and deleted):
   `![Settings](https://github.com/GeoffreyMetais/syncthing-vita/blob/<commit sha>/docs/screenshots/<branch>/1_settings.png?raw=true)`.
   When the PR is updated with another visual change, replace the outdated screenshots (same names) or add new ones, commit, and update the section with the new commit hash.

---

## 7. UI Layout & Controls Reference (`VitaSend` Design System)

The UI (`gfx.rs` + `ui/screens/*` + `ui/modals/*` + `ui/renderer.rs`) renders at native `960x544` with a `212px` left sidebar (`SIDEBAR_W = 212`), main content area (`x = 236..936`, `y = 86..488`), and bottom button hint bar (`y = 500..544`):

- **Dynamic frame rate** (`application/frame_pacing.rs`): the main loop wakes up on every display VBlank (60 Hz) to poll input, but a frame is only rendered when needed — on the next VBlank after user input (press, tap, IME, dialog, toast expiry), at 30 FPS for animations (OLED screensaver, QR viewfinder), at most 10 FPS for background updates (sync progress, logs), and once per second when static. When adding a time-based animation, set `FrameSignals::animating` in `main.rs` and scale its motion by the elapsed VBlanks passed to `VitaUiRenderer::render`; never count time in rendered frames.
- **D-pad focus** (`domain/navigation.rs` + `application/dpad_navigation.rs`): focus is on the sidebar or on the page. Sidebar: `Up`/`Down` switch tabs, `Right`/`Cross` enter the page. Page: `Up`/`Down` move between rows (including the top row: the `Overview` device card buttons, the `Files` `Up` button), `Left`/`Right` between a row's buttons, `Left` on the first one returns to the sidebar, `Cross` activates the focused element (drawn with `draw_focus_ring`). On `Files`, `Right` opens the folder. Face buttons remain direct shortcuts.
- **Left Sidebar Tabs** (`L` / `R` shoulder buttons, D-pad from the sidebar, or touch tap):
  1. `Overview` (`ActiveTab::Dashboard`): compact device status card (the page header row: `QR / ID` modal (`Circle`) and `New Share` modal (`Triangle`) buttons), then the list of shared folders (4 visible, scrolling with the selection): `Devices` assignment modal (`Cross`), `Delete` with confirmation (`Square`) and `Folder` path picker (`Triangle`) on each card. A row's face buttons answer only while that row has the D-pad focus (`HitMap::shortcut_action` prefers the focused row, then page-level elements): `Triangle` is `Folder` on a share row and `New Share` anywhere else, and the hint bar shows which. A sync is started by tapping the sidebar transfer card. The protocol log is only on `Settings`.
  2. `Files` (`ActiveTab::FolderPicker`): Interactive `ux0:` directory browser. At its root, the `CreateShare` presets not shared yet are listed first. Selecting a folder (`Cross` or tap) starts creating a share of it (folders already shared are greyed out and cannot be selected or opened); `Triangle` (or tapping the `Open` chip) enters a subfolder, `Circle` goes up/back. The list scrolls with a held `Up` / `Down` or by dragging a finger over it. The `Overview` share `Folder` action reuses this browser to change a share's target path.
  3. `Settings` (`ActiveTab::Settings`): Dark / Pure-Black OLED theme switcher (`Cross`), `QR / ID` modal (`Circle`), `Language` card (D-pad `Down` + `Cross` or tap: System -> English -> Français -> Español -> Deutsch -> Italiano, saved in `language.cfg`), `Declined folders` card (D-pad `Down` + `Cross` or tap `Clear Declined` to forget declined folder offers so peers can offer them again), the **Background sync plugin** card (`ui/screens/settings_plugin.rs`: status chip `Not installed` / `Reboot required` / `Active` / `Disabled` with what it means; `Install`, or `Enable / Disable` (applied at once, no reboot) + `Uninstall`; install / uninstall go through the `ConfirmPlugin` modal, which says a reboot is needed), and the protocol log card (latest line). Tapping the log card, or `Cross` on its `Open` button, opens the **log viewer** (`AppState::log_view`, `ui/screens/logs.rs`): a full-screen page (no sidebar) listing this session's lines (`AppState::logs`, last `LOG_BUFFER_LINES`), newest at the bottom; D-pad `Up`/`Down` scroll one line, `L`/`R` (or `Left`/`Right`, or touching the upper / lower half of the list) one page, `Triangle · Show All` loads the whole `syncthing.log` history (last `LOG_HISTORY_MAX_LINES` lines) and toggles back, `Circle · Back` returns to `Settings`.
- **Modal Dialogs** (`ModalDialog` in `domain/models.rs`, rendered by `ui/modals/*`):
  - `CreateShare`: Create a share from PS Vita folder presets (`ux0:user/00/savedata`, `ux0:picture`, `ux0:music`, RetroArch saves) or browse `ux0:`. The share is only kept once a device is linked in the `ManageDevices` modal that follows.
  - `ManageDevices`: Link/unlink discovered Syncthing peers to a share with circular checkboxes (`draw_row_tick`), add one by ID (`Square`) or by QR code (`R · Scan QR`); changes apply when the dialog closes (`Circle · Done`: `config_dirty` restarts the worker, which advertises the folder in its `ClusterConfig`); opening the dialog scans the LAN (`DevicesScan`): at once for a share with no linked device, after `DEVICES_DIALOG_SCAN_DELAY` (3 s, counted in VBlanks by `main.rs`, cancelled when the dialog closes) otherwise, also while another device is connected (`UserCommand::ScanLan`: `main.rs` asks the running worker for a discovery cycle now with `SyncControl::request_discovery`, or starts a sync when none runs); while no device is known, `Triangle` (hint `Scan LAN`) scans again; `SELECT` pauses / enables the share; `L` (header button showing the current mode) switches it between **Read & write** and **Read only** (Syncthing "Receive Only": the Vita receives its peers' changes but never sends its own). On a read-only share, `START · Revert` (header) opens `ConfirmRevertShare`. Read-only shares carry a lock badge on their `Overview` card.
  - `ConfirmRevertShare`: Syncthing's *Revert Local Changes* for a read-only share: files added on the PS Vita are deleted, edited or deleted files are downloaded again from the peers. `Cross` confirms, `Circle` cancels; both return to `ManageDevices`. Runs in the next connected session; a toast reports the result.
  - `AddDeviceById`: Enter a remote Short / Full Device ID with the PS Vita system keyboard (opened on entry or by tapping the text box). The fallback Base32 keypad is hidden by default; `Triangle · Virtual Keyboard` shows / hides it.
  - `ScanDeviceQr`: Rear-camera (`SceCamera`) viewfinder opened with `R · Scan QR` from `AddDeviceById`, from `ManageDevices` (next to `+ Add by ID`) or from `ShowDeviceId` (next to `Add Remote Device`). The camera runs only while the modal is open; a decoded full Device ID is added like a typed one, `Circle` goes back to the ID form. Vita3K's emulated camera only delivers blank frames, so decoding must be tested on hardware.
  - `ShowDeviceId`: Displays a scannable QR code (`qrcodegen`) alongside the 8-group Syncthing Device ID; `Square · Add Remote Device` opens `AddDeviceById`, `R · Scan QR` opens `ScanDeviceQr`.
  - `ConfirmDeleteShare`: Triggered by `Square` on the `Overview` share list; removes the share from `shares.cfg` (files on `ux0:` are kept) and registers the folder as declined for its linked devices (`ignored_folders.cfg`), so peers do not offer it back until `Settings` · `Clear Declined`.
  - `FolderOffer`: Opens when a peer's `ClusterConfig` lists a folder that is not shared with it yet (including a deleted share). `Cross · Accept` creates a paused share and opens `Files` to choose its target, `Square · Decline` ignores it (`ignored_folders.cfg`, cleared from `Settings` · `Clear Declined`), `Circle · Later` postpones it until the app restarts.
  - `ConfirmQuit`: Triggered by `START` (or tapping `Quit`), warns if a sync is in progress before exiting to LiveArea.
  - `ConfirmPlugin`: Install or uninstall the background sync plugin (`Settings`): what it does, that `config.txt` is backed up, and that a reboot of the PS Vita is needed to load / unload it. `Cross` confirms (`PluginService::execute`, result as a toast), `Circle` cancels.
- **Sync activity** (`AppState::sync_activity`: `Downloading` / `Uploading` / `Ready` / `Idle`): the single source for the sidebar transfer card and the OLED screensaver status line, so they never disagree. Both draw a progress bar only while `Downloading` / `Uploading`.
- **OLED Screensaver**: Activates automatically after 10s of inactivity during active sync (or 30s when idle; there is no manual activation), rendering a pure `#000000` black background with a bouncing status widget to protect PS Vita 1000 OLED screens. Its progress bar is only drawn while a transfer runs (`Downloading` / `Uploading`), never when `Ready` or `Idle`.

---

## 8. Background Sync Plugin (taiHEN user modules in `SceShell`)

Background sync runs **without the app**, inside `SceShell`, as two taiHEN user modules bundled in the VPK (`app0:plugins/`) and installed by the app's `Settings` into the active `tai` directory (§3.1, `plugin_service.rs`). **Vita3K cannot load taiHEN plugins into `SceShell`**: the modules are only built, statically audited (`scripts/audit_plugin.sh`) and their logic host-tested here. Every change to them must be described as *unverified on hardware* until someone ran it on a PS Vita.

```text
SceShell process
 ├── syncthing_launcher.suprx  (stage 1, resident: ~8 KiB, no libc, one 16 KiB-stack thread)
 │     polls every 5 s ── nothing runs + Wi-Fi + cycle due ──▶ sceKernelLoadStartModule(worker, &link)
 │     polls every 0.5 s while it runs ── game / app / SYNCTHING starts, plugin disabled ──▶ link.abort = 1
 │     link.state == DONE ──▶ sceKernelStopUnloadModule(worker)   (SceShell memory back to baseline)
 └── syncthing_worker.suprx    (stage 2, on demand: ~1 MiB SELF, 8 MiB heap, syncthing-core)
       module_start: newlib + pthread run-time, cycle thread ── run_cycle() ── link.result, STATE_DONE
       module_stop:  joins the cycle thread, frees the heap memory block and the run-time's kernel objects
```

### 8.1 Contract (`plugin/include/syncthing_plugin.h`, mirrored by `crates/syncthing-vita-worker/src/link.rs`)

- **Files**: `syncthing_launcher.suprx` and `syncthing_worker.suprx`, side by side in the active `tai` directory. Only the launcher is listed under `*main` in `config.txt`; it loads the worker from `ux0:tai/` when `ux0:tai/config.txt` exists (taiHEN's own choice), else `ur0:tai/`, falling back to the other directory.
- **`plugin.cfg`** (`ux0:data/syncthing/`, §5): re-read before every cycle (and every 30 s while idle), so `Settings` changes apply without a reboot. Same parsing in C (`policy.c`) and Rust (`plugin_cfg.rs`, checked against the app's `PluginConfig` by `tests/plugin_cfg.rs`): trimmed `key=value` lines, flags exactly `0` / `1`, invalid values keep the previous one, the last valid one wins. A missing *file* means idle; a missing *key* takes the app's default (`enabled=1`, `sync_on_game_exit=1`, `interval_secs=1800`). `interval_secs=0` disables periodic cycles; values below 120 s are raised to 120 s.
- **Named kernel mutexes** (`SCE_KERNEL_ATTR_OPENABLE`): `SyncthingGuiRunning` (held by the app for its whole life), `SyncthingLauncherAlive` (created by the launcher's `module_start`, deleted by its `module_stop`: the app's "installed, needs reboot" vs "active"), `SyncthingWorkerRunning` (created locked by the launcher before loading a worker, deleted once it is unloaded: the app should wait while it exists before its own first sync).
- **`SyncthingWorkerLink`**: passed by pointer as `module_start`'s argument (both modules live in the same process). `abort` (launcher -> worker), `state` (`LOADING` / `RUNNING` / `DONE` / `FATAL`) and `result` (`STP_RESULT_*`) (worker -> launcher). `tests/plugin_contract.rs` checks the names, codes and layout agree.

### 8.2 Launcher (`plugin/launcher/src/`, C, no libc)

- `main.c` (module entry points, thread, worker life cycle), `system.c` (system queries), `log.c` (lines in `syncthing.log`, same prefix `[VITA-SYNCTHING] Background sync: ` and 512 KiB rotation as the app), `policy.c` (pure decisions, host-tested by `crates/syncthing-vita-worker/tests/launcher_policy.rs`, which builds `plugin/launcher/tests/policy_test.c` with the host C compiler).
- **Blocking check** (`stp_observe_apps`): `sceAppMgrGetRunningAppIdListForShell` (SceAppMgrUser NID `0x613A70E2`, callable from `SceShell`) lists the running applications; each is named with `sceAppMgrGetProcessIdByAppIdForShell` (`0x63FAC2A9`) + `sceAppMgrGetNameById` (`0xF4D6AE3A`). **Any** running app blocks cycles (a game suspended with the PS button still runs and may write its save when resumed); `SYNCTHING` is told apart only to know it is not a "game exit". A failing list counts as "something runs". `sceKernelOpenMutex("SyncthingGuiRunning")` (SceThreadmgr `0x52E17182`, handle closed at once) also blocks, as defence in depth.
- **Triggers** (`stp_policy_tick`): a cycle starts only after 15 s of LiveArea idle (`STP_EXIT_SETTLE_SECS`), with Wi-Fi connected (`sceNetCtlInetGetState`): once after a game / app exits (`sync_on_game_exit`), and every `interval_secs` while nothing runs. Closing the app restarts the interval (it just synchronized) without an exit cycle. A cycle that could not start (no Wi-Fi, worker missing, load error) is retried after 300 s.
- **Abort**: while a worker runs, a game / app / `SYNCTHING` starting, the plugin being disabled or the launcher stopping sets `link.abort`; the launcher keeps the console out of auto-standby (`sceKernelPowerTick`) meanwhile. It **never unloads a worker whose threads may still run**: it waits for `STATE_DONE` (logging after 60 s), and after `STATE_FATAL` it keeps the module loaded and disables background sync until the next reboot.

### 8.3 Worker (`crates/syncthing-vita-worker`)

- Host-tested library (`rlib`) + `staticlib` linked by `scripts/build_plugin.sh`. `lib.rs` declares modules only. `cycle.rs` (`run_cycle(data_dir, abort, &CycleOptions)`), `config/` (read-only readers of `device_seed.bin`, `devices.cfg`, `shares.cfg`, `ignored_folders.cfg`: same formats, unescaping, crash recovery and protected-path rule as the app, checked against the app's `ConfigRepository` by `tests/config_formats.rs`), `plan.rs` (shares registered exactly like the app's `spawn_sync_worker`: path, paused, `ReceiveOnly` for read-only shares, `set_folder_devices` authorization; peers = devices of enabled shares, at most 4), `endpoints.rs`, `peer_session.rs`, `abort.rs` (watcher thread: abort signal / deadline -> `SyncControl::request_stop`), `log.rs`, `plugin_cfg.rs`, `link.rs`, `defaults.rs` (every timing and limit). `vita/` (`#[cfg(target_os = "vita")]` only): `module_start` / `module_stop`, newlib run-time, network, fatal-error path.
- **One bounded cycle**: one `SyncControl::single_cycle` session per peer, one after the other, a single TLS connection each (no `+2`), configured address first, then one LAN discovery round (UDP only: the 16-thread TCP scan is too heavy for `SceShell`), then Global Discovery / relays. Drain: 6 s idle, 120 s max; whole cycle capped at 15 min. The worker never creates an identity, never writes a `.cfg` file, never accepts a folder offer (it logs it: the app asks for consent).
- **Memory in `SceShell`**: `_newlib_heap_size_user = 8 MiB` (`WORKER_HEAP_BYTES`, read-only symbol checked by the audit), cycle thread stack 512 KiB, watcher 64 KiB, module image ~1.3 MiB text + 280 KiB BSS (newlib's per-thread reentrancy table). `tests/memory_budget.rs` measures a cycle's heap on its thread (≈ 0.9 MiB for 400 files + a 2 MiB file on the 64-bit host; ≈ 4.3 MiB for 3,000 files), `tests/stack_budget.rs` runs a cycle of the unoptimized host build in a 512 KiB thread.
- **Module run-time** (`vita/runtime.rs`): a module has no crt0, so `module_start` calls what crt0's `_init_vita_newlib` would (`_init_vita_heap` / `_reent` / `_malloc` / `_io`), checks the heap block was allocated (`sbrk(0) != -1`, else `STP_RESULT_NO_MEMORY`), then `__libc_init_array` (pthread-embedded's `pthread_init`). `module_stop` refuses unless `STATE_DONE`, joins the cycle thread, then runs `__libc_fini_array`, `pthread_terminate`, deletes pthread-embedded's six global kernel mutexes and frees newlib (heap memory block included). Linked with `crti.o` / `crtbegin.o` / `crtend.o` / `crtn.o` but **without `crt0.o`**.
- **Never take `SceShell` down**: the shim defines `abort` (Rust panics and allocation failures end there) and crt0's `_free_vita_newlib` (newlib's `_exit` calls it before `sceKernelExitProcess`): both mark the link `STATE_FATAL` and end only the calling thread (`sceKernelExitDeleteThread`). The audit fails if newlib's `abort` or crt0 get linked.
- **Network**: `SceShell` already runs `SceNet` / `SceNetCtl`; the worker only initializes (and later terminates) what is not (`sceNetShowNetstat() == SCE_NET_ERROR_ENOTINIT`, `sceNetCtlInit` != `NOT_TERMINATED`), with its own memory block. It never terminates the shell's network stack. newlib's own lazy `_vita_net_init` treats `SCE_NET_ERROR_EBUSY` as "already initialized" and never calls `sceNetTerm` then.

### 8.4 Build & verification

```bash
just plugin        # scripts/build_plugin.sh -> target/armv7-sony-vita-newlibeabihf/release/plugins/
just vpk           # also builds the plugin and bundles plugins/*.suprx in SYNCTHING.vpk (audited)
cargo test -p syncthing-vita-worker
```
- Both modules: `vita-elf-create -e plugin/<stage>/exports.yml`, then `vita-make-fself -c` (compressed, **unsafe**: VitaSDK adds `-s` only for "safe" homebrew). The app's eboot is the same unsafe SELF, which is what writing `ux0:tai/` / `ur0:tai/` requires; no `param.sfo` attribute is involved (`ATTRIBUTE2=12` is extended memory).
- `scripts/audit_plugin.sh <build dir> [<vpk>]` (run by `build_plugin.sh`, `build_vpk.sh`, `just verify` and the release workflow): SELF headers, `module_start` / `module_stop` exported, no `main`, launcher without heap and ≤ 32 KiB, worker heap symbol = 8 MiB, shim overrides linked, both modules bundled unchanged in the VPK.
- Tests (`crates/syncthing-vita-worker/tests/`): `cycle.rs` (against a `syncthing-core` peer on `127.0.0.1`: pulls and sends a file then ends with the folder database saved and no temp file left, paused share untouched, abort within 3 s, deadline, disabled / missing `plugin.cfg`, unreachable peer, nothing to sync, no identity), `config_formats.rs`, `sync_plan.rs`, `endpoints.rs`, `plugin_cfg.rs`, `plugin_contract.rs`, `launcher_policy.rs`, `abort_watcher.rs`, `worker_log.rs`, `memory_budget.rs`, `stack_budget.rs`.
- **Unverified on hardware** (keep this list current): loading a newlib + Rust std module into `SceShell` and unloading it repeatedly; the 8 MiB heap block being available in `SceShell`; cross-process visibility of the openable mutexes; what `sceAppMgrGetRunningAppIdListForShell` lists (a resident system app would block every cycle: the launcher logs the running title IDs at start); ForShell calls from a plugin; abort latency (between blocks, at most a block timeout of 20 s, or one folder scan); a few kernel objects of Rust's lazily created static mutexes leak per cycle.
