//! PS Vita executable (`vita-syncthing-app`)
//! Complete interactive UI (modeled after **VitaSend**):
//! - PS Vita Gamepad controls (`SceCtrl`) + Front Touchscreen (`SceTouch`)
//! - Syncthing share creation (`+ New Share`), target folder selection on `ux0:`, and device assignment (`Add / Toggle Devices`)
//! - Asynchronous BEP/1.0 worker thread + OLED screensaver (pure `#000000` + live progress)

mod display;
mod gfx;
mod input;

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use display::{
    ActiveTab, KnownDevice, ModalDialog, PickerMode, ShareConfigEntry, VitaDashboard,
    NEW_SHARE_TEMPLATES,
};
use input::{
    InputFrame, VitaInput, BTN_CIRCLE, BTN_CROSS, BTN_DOWN, BTN_L, BTN_R, BTN_SELECT, BTN_SQUARE,
    BTN_START, BTN_TRIANGLE, BTN_UP,
};
use syncthing_core::{
    connect_and_sync, resolve_peer_tcp_addrs, run_discovery_cycle, Announce, BepFolder, BepHello,
    DeviceId, FsSyncStorage, SyncEvent, SyncSessionStatus, VitaIdentity, BEP_DEFAULT_PORT,
    DISCOVERY_BROADCAST_ADDR,
};

#[cfg(target_os = "vita")]
const NET_INIT_SIZE: usize = 1024 * 1024;

/// Explicit 64 MB Newlib heap size on PS Vita hardware.
/// Without `_newlib_heap_size_user`, VitaSDK's `_init_vita_heap` attempts a single 128 MB
/// `sceKernelAllocMemBlock` which fails on real PS Vita hardware (`_newlib_heap_base = NULL`),
/// causing the very first Rust `malloc` to abort with `C2-12828-1`.
#[cfg(target_os = "vita")]
#[used]
#[no_mangle]
pub static _newlib_heap_size_user: u32 = 64 * 1024 * 1024;

#[cfg(target_os = "vita")]
#[used]
#[no_mangle]
pub static sceUserMainThreadStackSize: u32 = 2 * 1024 * 1024;

#[cfg(target_os = "vita")]
#[used]
#[no_mangle]
pub static sceUserMainThreadName: [u8; 16] = *b"vita_syncthing\0\0";

#[cfg(target_os = "vita")]
const BASE_DATA_DIR: &str = "ux0:data/syncthing";
#[cfg(not(target_os = "vita"))]
const BASE_DATA_DIR: &str = "/tmp/syncthing-vita";

#[cfg(target_os = "vita")]
#[allow(dead_code)]
mod vita_ffi {
    use libc::{c_char, c_int, c_ushort, c_void};

    pub const SCE_SYSMODULE_NET: c_ushort = 0x0001;
    pub const SCE_KERNEL_MEMBLOCK_TYPE_USER_RW: u32 = 0x0C20_D060;

    #[repr(C)]
    pub struct SceNetInitParam {
        pub memory: *mut c_void,
        pub size: c_int,
        pub flags: c_int,
    }

    #[link(name = "SceSysmodule_stub")]
    extern "C" {
        pub fn sceSysmoduleLoadModule(id: c_ushort) -> c_int;
    }

    #[link(name = "SceLibKernel_stub")]
    extern "C" {
        pub fn sceKernelGetRandomNumber(output: *mut c_void, size: u32) -> c_int;
    }

    #[link(name = "SceSysmem_stub")]
    extern "C" {
        pub fn sceKernelAllocMemBlock(
            name: *const c_char,
            memblock_type: u32,
            size: c_int,
            opt: *const c_void,
        ) -> c_int;
        pub fn sceKernelGetMemBlockBase(uid: c_int, basep: *mut *mut c_void) -> c_int;
    }

    #[link(name = "SceNet_stub")]
    extern "C" {
        pub fn sceNetInit(param: *mut SceNetInitParam) -> c_int;
        pub fn sceNetTerm() -> c_int;
    }

    #[link(name = "SceNetCtl_stub")]
    extern "C" {
        pub fn sceNetCtlInit() -> c_int;
        pub fn sceNetCtlTerm();
    }
}

/// Messages sent by the BEP network thread to the 60 FPS UI thread.
enum WorkerMsg {
    Log(String),
    Progress {
        pct: u32,
        status: String,
        file: String,
    },
    DeviceDiscovered {
        device_id: DeviceId,
        name: String,
        address: String,
    },
    SyncEvent(SyncEvent),
    Finished(String),
}

fn write_log_file(msg: &str) {
    println!("[VITA-SYNCTHING] {}", msg);
    let _ = fs::create_dir_all(BASE_DATA_DIR);
    let log_path = format!("{}/syncthing.log", BASE_DATA_DIR);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = writeln!(file, "[VITA-SYNCTHING] {}", msg);
    }
}

fn log_and_ui(ui: &mut VitaDashboard, msg: &str) {
    write_log_file(msg);
    ui.push_log(msg);
}

fn generate_random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];

    #[cfg(target_os = "vita")]
    unsafe {
        let ret = vita_ffi::sceKernelGetRandomNumber(
            seed.as_mut_ptr() as *mut libc::c_void,
            seed.len() as u32,
        );
        if ret == 0 && seed.iter().any(|&b| b != 0) {
            return seed;
        }
    }

    #[cfg(not(target_os = "vita"))]
    if let Ok(mut urandom) = fs::File::open("/dev/urandom") {
        use std::io::Read;
        if urandom.read_exact(&mut seed).is_ok() && seed.iter().any(|&b| b != 0) {
            return seed;
        }
    }

    // Fallback entropy mixing using high-resolution timestamps and stack addresses
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0x5649_5441_2026_1106);
    let mut state = (nanos as u64) ^ ((&seed as *const _ as usize) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    for byte in &mut seed {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        *byte = (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 56) as u8;
    }
    seed
}

fn load_or_create_identity() -> io::Result<VitaIdentity> {
    fs::create_dir_all(BASE_DATA_DIR)?;
    let seed_path = format!("{}/device_seed.bin", BASE_DATA_DIR);
    let cert_path = format!("{}/cert.der", BASE_DATA_DIR);

    let seed: [u8; 32] = if let Ok(bytes) = fs::read(&seed_path) {
        if bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        } else {
            let fresh_seed = generate_random_seed();
            fs::write(&seed_path, &fresh_seed)?;
            fresh_seed
        }
    } else {
        let fresh_seed = generate_random_seed();
        fs::write(&seed_path, &fresh_seed)?;
        fresh_seed
    };

    let identity = VitaIdentity::from_seed(&seed)?;
    let _ = fs::write(&cert_path, &identity.cert_der);
    Ok(identity)
}

#[cfg(target_os = "vita")]
fn init_vita_network() -> Result<Vec<u8>, String> {
    use std::ptr;
    use vita_ffi::{
        sceKernelAllocMemBlock, sceKernelGetMemBlockBase, sceNetCtlInit, sceNetInit,
        sceSysmoduleLoadModule, SceNetInitParam, SCE_KERNEL_MEMBLOCK_TYPE_USER_RW,
        SCE_SYSMODULE_NET,
    };

    const SCE_NET_ERROR_EEXIST: u32 = 0x80410118;
    const SCE_NET_CTL_ERROR_NOT_TERMINATED: u32 = 0x80412102;

    unsafe {
        // Must load SCE_SYSMODULE_NET (0x0001) on real PS Vita hardware before calling sceNetInit
        let _ = sceSysmoduleLoadModule(SCE_SYSMODULE_NET);

        let mut net_memory = Vec::new();
        let mut mem_ptr: *mut libc::c_void = ptr::null_mut();

        let uid = sceKernelAllocMemBlock(
            b"SceNetMemory\0".as_ptr() as *const libc::c_char,
            SCE_KERNEL_MEMBLOCK_TYPE_USER_RW,
            NET_INIT_SIZE as i32,
            ptr::null(),
        );
        if uid >= 0 {
            let _ = sceKernelGetMemBlockBase(uid, &mut mem_ptr);
        }
        if mem_ptr.is_null() {
            net_memory = vec![0u8; NET_INIT_SIZE];
            mem_ptr = net_memory.as_mut_ptr() as *mut libc::c_void;
        }

        let mut init_param = SceNetInitParam {
            memory: mem_ptr,
            size: NET_INIT_SIZE as i32,
            flags: 0,
        };

        let ret = sceNetInit(&mut init_param);
        if ret < 0 && (ret as u32) != SCE_NET_ERROR_EEXIST {
            return Err(format!("sceNetInit failed: 0x{:08X}", ret as u32));
        }

        let ctl_ret = sceNetCtlInit();
        if ctl_ret < 0 && (ctl_ret as u32) != SCE_NET_CTL_ERROR_NOT_TERMINATED {
            // Non-blocking in Vita3K / if already initialized
        }

        Ok(net_memory)
    }
}

#[cfg(not(target_os = "vita"))]
fn init_vita_network() -> Result<Vec<u8>, String> {
    Ok(Vec::new())
}

/// Spawns a complete UDP discovery + TLS 1.3 / BEP synchronization cycle in a dedicated thread
/// so that the interface (gamepad, touch, OLED saver) remains smooth at 60 FPS.
fn spawn_sync_worker(
    identity: VitaIdentity,
    shares_snapshot: Vec<ShareConfigEntry>,
    known_devices: Vec<KnownDevice>,
) -> Receiver<WorkerMsg> {
    let (tx, rx): (Sender<WorkerMsg>, Receiver<WorkerMsg>) = mpsc::channel();

    let builder = std::thread::Builder::new()
        .name("bep_worker".to_owned())
        .stack_size(2 * 1024 * 1024);

    let _ = builder.spawn(move || {
        let _ = tx.send(WorkerMsg::Progress {
            pct: 10,
            status: "LAN discovery (UDP 21027 + TCP 22000)...".to_owned(),
            file: "-".to_owned(),
        });

        let sync_root = format!("{}/sync", BASE_DATA_DIR);
        let mut storage = FsSyncStorage::new(&sync_root);
        for s in &shares_snapshot {
            storage.set_folder_path(&s.folder_id, &s.target_path);
            storage.set_folder_paused(&s.folder_id, !s.enabled);
            if s.enabled {
                let device_ids: Vec<DeviceId> = s
                    .shared_devices
                    .iter()
                    .filter_map(|hex| DeviceId::from_hex(hex))
                    .collect();
                storage.add_local_folder(BepFolder {
                    id: s.folder_id.clone(),
                    label: s.label.clone(),
                    device_ids,
                });
            }
        }

        let dyn_instance_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| (d.as_nanos() as i64) ^ 0x5649_5441_0000_0006)
            .unwrap_or(0x5649_5441_0000_0006);

        let announce = Announce {
            id: identity.device_id,
            addresses: vec![format!("tcp://0.0.0.0:{}", BEP_DEFAULT_PORT)],
            instance_id: dyn_instance_id,
        };

        let our_hello = BepHello::new_vita_default();

        // Fast-path: if we already have known devices linked to one of our shares, connect directly first
        // so the BEP/1.0 connection stays persistent without a 2-second discovery gap between cycles.
        for kd in &known_devices {
            let kd_hex = kd.device_id.to_hex();
            let is_linked = shares_snapshot
                .iter()
                .any(|s| s.enabled && s.shared_devices.contains(&kd_hex));
            if !is_linked {
                continue;
            }
            if let Ok(tcp_addr) = kd.address.parse::<std::net::SocketAddr>() {
                let _ = tx.send(WorkerMsg::Progress {
                    pct: 25,
                    status: format!("Connecting TLS 1.3 to {}...", tcp_addr),
                    file: "-".to_owned(),
                });
                let tx_ev = tx.clone();
                if let Ok(hs) = connect_and_sync(
                    &identity,
                    tcp_addr,
                    Some(kd.device_id),
                    &our_hello,
                    &storage,
                    Duration::from_secs(4),
                    move |event| {
                        let _ = tx_ev.send(WorkerMsg::SyncEvent(event));
                    },
                ) {
                    if let SyncSessionStatus::Active {
                        files_up_to_date,
                        files_downloaded,
                        bytes_downloaded,
                        blocks_uploaded,
                        bytes_uploaded,
                        ..
                    } = hs.session_status
                    {
                        let summary = format!(
                            "Sync OK: {} up-to-date | {} DL ({} KB) | {} UL ({} B)",
                            files_up_to_date,
                            files_downloaded,
                            bytes_downloaded / 1024,
                            blocks_uploaded,
                            bytes_uploaded
                        );
                        let _ = tx.send(WorkerMsg::Finished(summary));
                        return;
                    }
                }
            }
        }

        match run_discovery_cycle(&announce, DISCOVERY_BROADCAST_ADDR, Duration::from_secs(2)) {
            Ok(mut res) => {
                // Include any previously saved devices from `devices.cfg` if not in discovered_peers
                for kd in &known_devices {
                    if !res.discovered_peers.iter().any(|(_, a)| a.id == kd.device_id) {
                        if let Ok(sock_addr) = kd.address.parse::<std::net::SocketAddr>() {
                            res.discovered_peers.push((
                                sock_addr,
                                Announce {
                                    id: kd.device_id,
                                    addresses: vec![format!(
                                        "tcp://{}:{}",
                                        sock_addr.ip(),
                                        BEP_DEFAULT_PORT
                                    )],
                                    instance_id: 0,
                                },
                            ));
                        }
                    }
                }

                if res.discovered_peers.is_empty() {
                    let _ = tx.send(WorkerMsg::Finished(
                        "No peers detected on LAN (press Cross to retry).".to_owned(),
                    ));
                    return;
                }

                // Prioritize peers that are explicitly linked to one of our shares
                res.discovered_peers.sort_by_key(|(_, peer)| {
                    let p_hex = peer.id.to_hex();
                    let is_linked = shares_snapshot
                        .iter()
                        .any(|s| s.shared_devices.contains(&p_hex));
                    !is_linked
                });

                let mut seen_ids = Vec::new();
                let mut last_summary: Option<String> = None;

                for (udp_src, peer) in &res.discovered_peers {
                    if seen_ids.contains(&peer.id) {
                        continue;
                    }
                    seen_ids.push(peer.id);

                    let _ = tx.send(WorkerMsg::DeviceDiscovered {
                        device_id: peer.id,
                        name: "LAN Peer".to_owned(),
                        address: udp_src.to_string(),
                    });

                    let _ = tx.send(WorkerMsg::Log(format!(
                        "Discovered peer: {} (UDP {})",
                        peer.id.short(),
                        udp_src
                    )));

                    let tcp_targets = resolve_peer_tcp_addrs(*udp_src, &peer.addresses);
                    for tcp_addr in tcp_targets {
                        let _ = tx.send(WorkerMsg::Progress {
                            pct: 25,
                            status: format!("Connecting TLS 1.3 to {}...", tcp_addr),
                            file: "-".to_owned(),
                        });

                        let tx_ev = tx.clone();
                        let sync_res = connect_and_sync(
                            &identity,
                            tcp_addr,
                            Some(peer.id),
                            &our_hello,
                            &storage,
                            Duration::from_secs(4),
                            move |event| {
                                let _ = tx_ev.send(WorkerMsg::SyncEvent(event));
                            },
                        );

                        match sync_res {
                            Ok(hs) => {
                                let is_active =
                                    matches!(hs.session_status, SyncSessionStatus::Active { .. });
                                let summary = match hs.session_status {
                                    SyncSessionStatus::AwaitingPairing => {
                                        "Awaiting device pairing in Syncthing Web UI."
                                            .to_owned()
                                    }
                                    SyncSessionStatus::ClosedByPeer(reason) => {
                                        format!("Session closed by peer: {}", reason)
                                    }
                                    SyncSessionStatus::Active {
                                        files_up_to_date,
                                        files_downloaded,
                                        bytes_downloaded,
                                        blocks_uploaded,
                                        bytes_uploaded,
                                        ..
                                    } => {
                                        format!(
                                            "Sync OK: {} up-to-date | {} DL ({} KB) | {} UL ({} B)",
                                            files_up_to_date,
                                            files_downloaded,
                                            bytes_downloaded / 1024,
                                            blocks_uploaded,
                                            bytes_uploaded
                                        )
                                    }
                                };
                                if is_active {
                                    let _ = tx.send(WorkerMsg::Finished(summary));
                                    return;
                                }
                                last_summary = Some(summary);
                            }
                            Err(e) => {
                                let _ = tx.send(WorkerMsg::Log(format!(
                                    "Error connecting to {}: {}",
                                    tcp_addr, e
                                )));
                            }
                        }
                    }
                }
                let final_msg =
                    last_summary.unwrap_or_else(|| "Connection attempts finished.".to_owned());
                let _ = tx.send(WorkerMsg::Finished(final_msg));
            }
            Err(e) => {
                let _ = tx.send(WorkerMsg::Finished(format!(
                    "UDP discovery error: {}",
                    e
                )));
            }
        }
    });

    rx
}

/// Processes messages from the network synchronization thread.
fn process_worker_messages(
    ui: &mut VitaDashboard,
    worker_rx: &mut Option<Receiver<WorkerMsg>>,
) {
    let mut disconnected = false;
    if let Some(rx) = worker_rx.as_ref() {
        while let Ok(msg) = rx.try_recv() {
            match msg {
                WorkerMsg::Log(line) => log_and_ui(ui, &line),
                WorkerMsg::Progress { pct, status, file } => {
                    ui.progress_pct = pct;
                    ui.status_line = status;
                    ui.current_file = file;
                }
                WorkerMsg::DeviceDiscovered {
                    device_id,
                    name,
                    address,
                } => {
                    ui.upsert_device(BASE_DATA_DIR, device_id, &name, &address);
                }
                WorkerMsg::SyncEvent(event) => match event {
                    SyncEvent::PeerConnected {
                        peer_addr,
                        peer_device_id,
                        peer_name,
                        client_version,
                    } => {
                        let name = if peer_name.is_empty() {
                            "Syncthing Peer".to_owned()
                        } else {
                            peer_name
                        };
                        ui.upsert_device(
                            BASE_DATA_DIR,
                            peer_device_id,
                            &name,
                            &peer_addr.to_string(),
                        );
                        ui.peer_label = format!("{} @ {} ({})", name, peer_addr, client_version);
                        ui.status_line = "Exchanging ClusterConfig & BEP Index...".to_owned();
                        ui.progress_pct = 40;
                        log_and_ui(
                            ui,
                            &format!("Connected via TLS 1.3 to {} ({})", name, peer_addr),
                        );
                    }
                    SyncEvent::ClusterConfigReceived { folders } => {
                        let mut labels = Vec::new();
                        for f in &folders {
                            ui.upsert_discovered_folder(BASE_DATA_DIR, &f.id, &f.label);
                            labels.push(format!("{} ({})", f.id, f.label));
                        }
                        ui.refresh_picker_items(BASE_DATA_DIR);
                        let joined = labels.join(", ");
                        ui.folder_label = joined.clone();
                        ui.progress_pct = 55;
                        log_and_ui(ui, &format!("Received shares from peer: {}", joined));
                    }
                    SyncEvent::FileUpToDate { name, size, .. } => {
                        ui.files_up_to_date += 1;
                        ui.current_file = name.clone();
                        ui.status_line = format!("Up-to-date (Delta 0 B): {}", name);
                        log_and_ui(
                            ui,
                            &format!("Already up-to-date (SHA-256 OK): {} ({} B)", name, size),
                        );
                    }
                    SyncEvent::BlockDownloaded {
                        name,
                        offset,
                        size,
                        total_size,
                        ..
                    } => {
                        ui.bytes_transferred += size;
                        ui.current_file = name.clone();
                        let pct = if total_size > 0 {
                            (((offset + size as u64) * 100) / total_size) as u32
                        } else {
                            100
                        };
                        ui.progress_pct = pct.clamp(55, 95);
                        ui.status_line = format!("Downloading {} ({}%)", name, pct);
                    }
                    SyncEvent::FileSynced {
                        name,
                        blocks_downloaded,
                        bytes_downloaded,
                        ..
                    } => {
                        ui.files_downloaded += 1;
                        ui.current_file = name.clone();
                        log_and_ui(
                            ui,
                            &format!(
                                "Synced: {} ({} block(s), {} bytes)",
                                name, blocks_downloaded, bytes_downloaded
                            ),
                        );
                    }
                    SyncEvent::LocalIndexPublished { folder, file_count } => {
                        ui.progress_pct = 90;
                        ui.status_line =
                            format!("Local Vita index published ({} files)", file_count);
                        log_and_ui(
                            ui,
                            &format!(
                                "Local index sent for {} ({} files)",
                                folder, file_count
                            ),
                        );
                    }
                    SyncEvent::BlockUploaded {
                        name, offset, size, ..
                    } => {
                        ui.blocks_uploaded += 1;
                        ui.bytes_transferred += size;
                        ui.current_file = name.clone();
                        ui.status_line = format!("Uploading to peer: {}", name);
                        log_and_ui(
                            ui,
                            &format!(
                                "Block uploaded (Vita -> PC/Mac): {} (offset={}, {} B)",
                                name, offset, size
                            ),
                        );
                    }
                },
                WorkerMsg::Finished(summary) => {
                    ui.sync_in_progress = false;
                    ui.progress_pct = 100;
                    ui.current_file = "Finished".to_owned();
                    ui.status_line = summary.clone();
                    log_and_ui(ui, &summary);
                    disconnected = true;
                }
            }
        }
    }
    if disconnected {
        *worker_rx = None;
    }
}

/// Handles gamepad (`SceCtrl`) and touchscreen (`SceTouch`) inputs.
/// Returns `true` if the user requested to exit the application (`START`).
fn handle_user_input(
    ui: &mut VitaDashboard,
    input: InputFrame,
    identity: &VitaIdentity,
    worker_rx: &mut Option<Receiver<WorkerMsg>>,
) -> bool {
    // 0. If OLED screensaver is active, any button or touch contact wakes the display
    if ui.oled_saver_active {
        if input.any_activity {
            ui.oled_saver_active = false;
            log_and_ui(ui, "Exited OLED screensaver mode.");
        }
        return false;
    }

    // 1. If a Modal Dialog is open, route all gamepad & touch input to the modal
    if ui.modal != ModalDialog::None {
        return handle_modal_input(ui, input, identity, worker_rx);
    }

    let mut trigger_sync = false;
    let mut toggle_share = false;
    let mut open_picker = false;

    // 2. Front touchscreen tap detection (960x544, VitaSend layout)
    if let Some((tx, ty)) = input.touch_tap {
        // Left Sidebar (x <= 212, y < 500)
        if tx <= 212 && ty < 500 {
            if (88..=134).contains(&ty) {
                ui.active_tab = ActiveTab::Dashboard;
            } else if (138..=184).contains(&ty) {
                ui.active_tab = ActiveTab::Shares;
            } else if (188..=234).contains(&ty) {
                ui.picker_mode = PickerMode::ChangeShareTarget;
                ui.refresh_picker_items(BASE_DATA_DIR);
                ui.active_tab = ActiveTab::FolderPicker;
            } else if (238..=284).contains(&ty) {
                ui.active_tab = ActiveTab::Settings;
            } else if (345..=440).contains(&ty) {
                trigger_sync = true;
            }
        }
        // Bottom Hint Bar (y >= 500)
        else if ty >= 500 {
            if ui.active_tab == ActiveTab::Shares {
                if tx < 195 {
                    ui.modal = ModalDialog::CreateShare { selected_idx: 0 };
                } else if tx < 420 {
                    if !ui.shares.is_empty() {
                        ui.modal = ModalDialog::ManageDevices {
                            share_idx: ui.selected_share_idx,
                            selected_dev_idx: 0,
                        };
                    }
                } else if tx < 610 {
                    toggle_share = true;
                } else if tx < 790 {
                    open_picker = true;
                }
            } else if ui.active_tab == ActiveTab::Settings {
                if tx < 250 {
                    ui.toggle_theme();
                } else if tx < 440 {
                    ui.modal = ModalDialog::ShowDeviceId;
                } else if tx < 640 {
                    ui.oled_saver_active = true;
                    return false;
                } else if tx >= 760 {
                    ui.modal = ModalDialog::ConfirmQuit;
                    return false;
                }
            } else if tx < 160 {
                trigger_sync = true;
            } else if tx < 330 {
                ui.modal = ModalDialog::ShowDeviceId;
            } else if tx < 500 {
                toggle_share = true;
            } else if tx < 650 {
                open_picker = true;
            } else if tx < 840 {
                ui.oled_saver_active = true;
                return false;
            } else {
                ui.modal = ModalDialog::ConfirmQuit;
                return false;
            }
        }
        // Main Content Area (x >= 236, y < 500)
        else if tx >= 236 {
            match ui.active_tab {
                ActiveTab::Dashboard => {
                    if (86..=238).contains(&ty) {
                        if ty >= 110 && tx < 785 {
                            ui.modal = ModalDialog::ShowDeviceId;
                        } else {
                            trigger_sync = true;
                        }
                    }
                }
                ActiveTab::Shares => {
                    // Top-right "+ New Share" button (y in 24..68, x >= 760)
                    if (24..=68).contains(&ty) && tx >= 760 {
                        ui.modal = ModalDialog::CreateShare { selected_idx: 0 };
                    } else {
                        for i in 0..ui.shares.len().min(4) {
                            let ry = 86 + i * 98;
                            if ty >= ry && ty <= ry + 90 {
                                ui.selected_share_idx = i;
                                if ty >= ry + 42 && ty <= ry + 78 {
                                    if (580..=695).contains(&tx) {
                                        ui.modal = ModalDialog::ManageDevices {
                                            share_idx: i,
                                            selected_dev_idx: 0,
                                        };
                                    } else if (696..=810).contains(&tx) {
                                        toggle_share = true;
                                    } else if (811..=936).contains(&tx) {
                                        open_picker = true;
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
                ActiveTab::FolderPicker => {
                    // Breadcrumb "Up" button at top right (y in 80..122, x >= 820)
                    if (80..=122).contains(&ty) && tx >= 820 {
                        navigate_picker_up(ui);
                    } else {
                        for vis_i in 0..display::BROWSER_ROWS {
                            let idx = ui.picker_top_idx + vis_i;
                            if idx >= ui.picker_items.len() {
                                break;
                            }
                            let ry = (display::BROWSER_TOP as usize)
                                + vis_i * (display::BROWSER_STEP as usize);
                            if ty >= ry && ty <= ry + 38 {
                                ui.selected_picker_idx = idx;
                                apply_picker_selection(ui, false);
                                break;
                            }
                        }
                    }
                }
                ActiveTab::Settings => {
                    if (86..=204).contains(&ty) {
                        if (630..=760).contains(&tx) {
                            ui.modal = ModalDialog::ShowDeviceId;
                        } else {
                            ui.toggle_theme();
                        }
                    }
                }
            }
        }
    }

    // 3. Global PS Vita gamepad buttons
    if (input.pressed & BTN_START) != 0 {
        ui.modal = ModalDialog::ConfirmQuit;
        return false;
    }
    if (input.pressed & BTN_SELECT) != 0 {
        ui.oled_saver_active = !ui.oled_saver_active;
        return false;
    }
    if (input.pressed & BTN_L) != 0 {
        ui.active_tab = match ui.active_tab {
            ActiveTab::Dashboard => ActiveTab::Settings,
            ActiveTab::Shares => ActiveTab::Dashboard,
            ActiveTab::FolderPicker => ActiveTab::Shares,
            ActiveTab::Settings => ActiveTab::FolderPicker,
        };
        if ui.active_tab == ActiveTab::FolderPicker {
            ui.picker_mode = PickerMode::ChangeShareTarget;
            ui.refresh_picker_items(BASE_DATA_DIR);
        }
    }
    if (input.pressed & BTN_R) != 0 {
        ui.active_tab = match ui.active_tab {
            ActiveTab::Dashboard => ActiveTab::Shares,
            ActiveTab::Shares => ActiveTab::FolderPicker,
            ActiveTab::FolderPicker => ActiveTab::Settings,
            ActiveTab::Settings => ActiveTab::Dashboard,
        };
        if ui.active_tab == ActiveTab::FolderPicker {
            ui.picker_mode = PickerMode::ChangeShareTarget;
            ui.refresh_picker_items(BASE_DATA_DIR);
        }
    }

    // 4. Tab-specific actions
    match ui.active_tab {
        ActiveTab::Dashboard => {
            if (input.pressed & BTN_CROSS) != 0 {
                trigger_sync = true;
            }
            if (input.pressed & BTN_CIRCLE) != 0 {
                ui.modal = ModalDialog::ShowDeviceId;
            }
            if (input.pressed & BTN_SQUARE) != 0 {
                toggle_share = true;
            }
            if (input.pressed & BTN_TRIANGLE) != 0 {
                open_picker = true;
            }
        }
        ActiveTab::Shares => {
            if (input.pressed & BTN_UP) != 0 && ui.selected_share_idx > 0 {
                ui.selected_share_idx -= 1;
            }
            if (input.pressed & BTN_DOWN) != 0 && ui.selected_share_idx + 1 < ui.shares.len() {
                ui.selected_share_idx += 1;
            }
            // Circle opens "+ New Share" modal dialog
            if (input.pressed & BTN_CIRCLE) != 0 {
                ui.modal = ModalDialog::CreateShare { selected_idx: 0 };
            }
            // Cross opens "Add / Edit Devices" modal dialog for the selected share
            if (input.pressed & BTN_CROSS) != 0 {
                if ui.shares.is_empty() {
                    ui.modal = ModalDialog::CreateShare { selected_idx: 0 };
                } else {
                    ui.modal = ModalDialog::ManageDevices {
                        share_idx: ui.selected_share_idx,
                        selected_dev_idx: 0,
                    };
                }
            }
            if (input.pressed & BTN_SQUARE) != 0 {
                toggle_share = true;
            }
            if (input.pressed & BTN_TRIANGLE) != 0 {
                open_picker = true;
            }
        }
        ActiveTab::FolderPicker => {
            if (input.pressed & BTN_UP) != 0 && ui.selected_picker_idx > 0 {
                ui.selected_picker_idx -= 1;
                ui.clamp_picker_scroll();
            }
            if (input.pressed & BTN_DOWN) != 0
                && ui.selected_picker_idx + 1 < ui.picker_items.len()
            {
                ui.selected_picker_idx += 1;
                ui.clamp_picker_scroll();
            }
            if (input.pressed & BTN_CROSS) != 0 {
                apply_picker_selection(ui, false);
            }
            if (input.pressed & BTN_TRIANGLE) != 0 {
                apply_picker_selection(ui, true);
            }
            if (input.pressed & BTN_CIRCLE) != 0 {
                navigate_picker_up(ui);
            }
        }
        ActiveTab::Settings => {
            if (input.pressed & BTN_CROSS) != 0 || (input.pressed & BTN_SQUARE) != 0 {
                ui.toggle_theme();
            }
            if (input.pressed & BTN_CIRCLE) != 0 {
                ui.modal = ModalDialog::ShowDeviceId;
            }
        }
    }

    if toggle_share {
        if let Some(share) = ui.shares.get_mut(ui.selected_share_idx) {
            share.enabled = !share.enabled;
            let fid = share.folder_id.clone();
            let st = if share.enabled { "Active" } else { "Paused" };
            ui.save_shares_config(BASE_DATA_DIR);
            let msg = format!("Share {} is now {}", fid, st);
            ui.show_toast(&msg);
            log_and_ui(ui, &msg);
        }
    }

    if open_picker {
        ui.picker_mode = PickerMode::ChangeShareTarget;
        ui.refresh_picker_items(BASE_DATA_DIR);
        ui.active_tab = ActiveTab::FolderPicker;
    }

    if trigger_sync && !ui.sync_in_progress {
        start_sync_job(ui, identity, worker_rx);
    }

    false
}

fn start_sync_job(
    ui: &mut VitaDashboard,
    identity: &VitaIdentity,
    worker_rx: &mut Option<Receiver<WorkerMsg>>,
) {
    ui.sync_in_progress = true;
    ui.files_up_to_date = 0;
    ui.files_downloaded = 0;
    ui.blocks_uploaded = 0;
    ui.bytes_transferred = 0;
    ui.progress_pct = 5;
    ui.status_line = "Starting synchronization...".to_owned();
    ui.show_toast("Synchronizing with LAN peers...");
    log_and_ui(ui, "Starting BEP/1.0 synchronization...");
    *worker_rx = Some(spawn_sync_worker(
        identity.clone(),
        ui.shares.clone(),
        ui.devices.clone(),
    ));
}

/// Handles Gamepad & Touch input inside `ModalDialog::CreateShare` and `ModalDialog::ManageDevices`.
fn handle_modal_input(
    ui: &mut VitaDashboard,
    input: InputFrame,
    identity: &VitaIdentity,
    worker_rx: &mut Option<Receiver<WorkerMsg>>,
) -> bool {
    match ui.modal {
        ModalDialog::None => false,
        ModalDialog::CreateShare { mut selected_idx } => {
            let count = NEW_SHARE_TEMPLATES.len();
            if (input.pressed & BTN_UP) != 0 && selected_idx > 0 {
                selected_idx -= 1;
                ui.modal = ModalDialog::CreateShare { selected_idx };
            }
            if (input.pressed & BTN_DOWN) != 0 && selected_idx + 1 < count {
                selected_idx += 1;
                ui.modal = ModalDialog::CreateShare { selected_idx };
            }
            if (input.pressed & BTN_CIRCLE) != 0 {
                ui.modal = ModalDialog::None;
                return false;
            }

            let mut confirm = (input.pressed & BTN_CROSS) != 0;
            if let Some((tx, ty)) = input.touch_tap {
                let w = 680usize;
                let h = 408usize;
                let dx = (960 - w) / 2;
                let dy = (500 - h) / 2;
                if tx >= dx && tx <= dx + w && ty >= dy && ty <= dy + h {
                    for i in 0..count {
                        let ry = dy + 90 + i * 50;
                        if ty >= ry && ty <= ry + 44 {
                            selected_idx = i;
                            ui.modal = ModalDialog::CreateShare { selected_idx };
                            confirm = true;
                            break;
                        }
                    }
                } else {
                    ui.modal = ModalDialog::None;
                    return false;
                }
            }

            if confirm {
                let (fid, label, path) = NEW_SHARE_TEMPLATES[selected_idx];
                if fid == "__browse__" {
                    ui.modal = ModalDialog::None;
                    ui.picker_mode = PickerMode::CreateNewShare;
                    ui.refresh_picker_items(BASE_DATA_DIR);
                    ui.active_tab = ActiveTab::FolderPicker;
                } else {
                    let share_idx = ui.create_share(BASE_DATA_DIR, fid, label, path);
                    log_and_ui(
                        ui,
                        &format!("Created share '{}' ({}) -> {}", label, fid, path),
                    );
                    ui.show_toast(&format!("Created share '{}'", label));
                    ui.modal = ModalDialog::ManageDevices {
                        share_idx,
                        selected_dev_idx: 0,
                    };
                }
            }
            false
        }
        ModalDialog::ManageDevices {
            share_idx,
            mut selected_dev_idx,
        } => {
            let dev_count = ui.devices.len().min(5);
            if (input.pressed & BTN_UP) != 0 && selected_dev_idx > 0 {
                selected_dev_idx -= 1;
                ui.modal = ModalDialog::ManageDevices {
                    share_idx,
                    selected_dev_idx,
                };
            }
            if (input.pressed & BTN_DOWN) != 0 && selected_dev_idx + 1 < dev_count {
                selected_dev_idx += 1;
                ui.modal = ModalDialog::ManageDevices {
                    share_idx,
                    selected_dev_idx,
                };
            }
            if (input.pressed & BTN_CIRCLE) != 0 {
                ui.modal = ModalDialog::None;
                return false;
            }
            if (input.pressed & BTN_TRIANGLE) != 0 {
                if !ui.devices.is_empty() {
                    ui.modal = ModalDialog::None;
                }
                if !ui.sync_in_progress {
                    start_sync_job(ui, identity, worker_rx);
                }
                return false;
            }

            let mut toggle_dev = (input.pressed & BTN_CROSS) != 0;
            if let Some((tx, ty)) = input.touch_tap {
                let w = 680usize;
                let h = 390usize;
                let dx = (960 - w) / 2;
                let dy = (500 - h) / 2;
                if tx >= dx && tx <= dx + w && ty >= dy && ty <= dy + h {
                    // Bottom buttons inside modal (Apply & Sync / Done)
                    if ty >= dy + h - 56 {
                        if tx >= dx + w - 140 {
                            ui.modal = ModalDialog::None;
                        } else if tx >= dx + w - 340 {
                            if !ui.devices.is_empty() {
                                ui.modal = ModalDialog::None;
                            }
                            if !ui.sync_in_progress {
                                start_sync_job(ui, identity, worker_rx);
                            }
                        }
                        return false;
                    }
                    for i in 0..dev_count {
                        let ry = dy + 92 + i * 52;
                        if ty >= ry && ty <= ry + 46 {
                            selected_dev_idx = i;
                            ui.modal = ModalDialog::ManageDevices {
                                share_idx,
                                selected_dev_idx,
                            };
                            toggle_dev = true;
                            break;
                        }
                    }
                } else {
                    ui.modal = ModalDialog::None;
                    return false;
                }
            }

            if toggle_dev && selected_dev_idx < ui.devices.len() {
                let dev_id = ui.devices[selected_dev_idx].device_id;
                if let Some((folder_lbl, dev_name, added)) =
                    ui.toggle_device_for_share(BASE_DATA_DIR, share_idx, dev_id)
                {
                    let action_str = if added { "Added" } else { "Removed" };
                    let prep = if added { "to" } else { "from" };
                    let msg = format!("{} {} {} '{}'", action_str, dev_name, prep, folder_lbl);
                    ui.show_toast(&msg);
                    log_and_ui(ui, &msg);
                }
            }
            false
        }
        ModalDialog::ShowDeviceId => {
            if (input.pressed & (BTN_CIRCLE | BTN_CROSS | BTN_SQUARE | BTN_TRIANGLE)) != 0
                || input.touch_tap.is_some()
            {
                ui.modal = ModalDialog::None;
            }
            false
        }
        ModalDialog::ConfirmQuit => {
            if (input.pressed & BTN_CROSS) != 0 {
                return true;
            }
            if (input.pressed & (BTN_CIRCLE | BTN_START | BTN_SQUARE | BTN_TRIANGLE)) != 0 {
                ui.modal = ModalDialog::None;
                return false;
            }
            if let Some((tx, ty)) = input.touch_tap {
                let w = 520usize;
                let h = 236usize;
                let dx = (960 - w) / 2;
                let dy = (500 - h) / 2;
                // Check if user tapped "Quit App" button at bottom-right of modal
                if tx >= dx + w - 160
                    && tx <= dx + w - 16
                    && ty >= dy + h - 56
                    && ty <= dy + h - 12
                {
                    return true;
                }
                // Check if user tapped "Confirm Quit" in bottom hint bar
                if ty >= 500 && tx < 200 {
                    return true;
                }
                ui.modal = ModalDialog::None;
            }
            false
        }
    }
}

fn navigate_picker_up(ui: &mut VitaDashboard) {
    if let Some(parent) = Path::new(&ui.picker_current_dir).parent() {
        let p_str = parent.to_string_lossy().into_owned();
        if !p_str.is_empty() {
            ui.picker_current_dir = p_str;
            ui.refresh_picker_items(BASE_DATA_DIR);
            return;
        }
    }
    ui.active_tab = ActiveTab::Shares;
}

/// Applies selection in `ux0:` directory browser:
/// - If `enter_subdir == true` and item is a directory, descends into it.
/// - If `ui.picker_mode == PickerMode::CreateNewShare`, creates a new Syncthing share for that folder and opens device assignment.
/// - Otherwise, assigns this target path to the selected Syncthing share and saves `shares.cfg`.
fn apply_picker_selection(ui: &mut VitaDashboard, enter_subdir: bool) {
    let Some(item) = ui.picker_items.get(ui.selected_picker_idx).cloned() else {
        return;
    };

    if enter_subdir && !item.is_preset {
        ui.picker_current_dir = item.path;
        ui.refresh_picker_items(BASE_DATA_DIR);
        return;
    }

    if ui.picker_mode == PickerMode::CreateNewShare {
        let chosen_path = item.path.trim_end_matches('/').to_owned();
        let raw_name = chosen_path
            .rsplit(['/', ':'])
            .next()
            .filter(|s| !s.is_empty() && *s != "ux0")
            .unwrap_or("vita-shared");
        let folder_id: String = raw_name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let label = format!("Vita {}", raw_name);
        let share_idx = ui.create_share(BASE_DATA_DIR, &folder_id, &label, &chosen_path);
        log_and_ui(
            ui,
            &format!(
                "Created custom share '{}' ({}) -> {}",
                label, folder_id, chosen_path
            ),
        );
        ui.show_toast(&format!("Created share '{}'", label));
        ui.picker_mode = PickerMode::ChangeShareTarget;
        ui.active_tab = ActiveTab::Shares;
        ui.modal = ModalDialog::ManageDevices {
            share_idx,
            selected_dev_idx: 0,
        };
        return;
    }

    let chosen_path = item.path.trim_end_matches('/').to_owned();

    let mut updated_fid = None;
    if let Some(share) = ui.shares.get_mut(ui.selected_share_idx) {
        share.target_path = chosen_path.clone();
        updated_fid = Some(share.folder_id.clone());
    }
    if let Some(fid) = updated_fid {
        ui.save_shares_config(BASE_DATA_DIR);
        let _ = fs::create_dir_all(&chosen_path);
        let msg = format!("Saved in {}", chosen_path);
        ui.show_toast(&msg);
        log_and_ui(
            ui,
            &format!("Target directory updated for {} -> {}", fid, chosen_path),
        );
        ui.active_tab = ActiveTab::Shares;
    }
}

fn main() {
    // Initialize or migrate device identity first so any legacy test config is wiped before UI loads
    let identity_res = load_or_create_identity();

    let mut ui = Box::new(VitaDashboard::new(BASE_DATA_DIR));
    let mut input_ctl = VitaInput::new();

    log_and_ui(
        &mut ui,
        "=== VitaSyncthing // Gamepad + Touch UI + Share Manager + OLED Saver ===",
    );

    let _net_pool = match init_vita_network() {
        Ok(pool) => {
            log_and_ui(&mut ui, "sceNet network subsystem ready.");
            pool
        }
        Err(e) => {
            log_and_ui(&mut ui, &format!("Network init error: {}", e));
            return;
        }
    };

    let identity = match identity_res {
        Ok(id) => id,
        Err(e) => {
            log_and_ui(&mut ui, &format!("X.509 identity error: {}", e));
            return;
        }
    };

    ui.vita_device_id = identity.device_id.to_syncthing_string();
    log_and_ui(&mut ui, &format!("PS Vita Device ID: {}", identity.device_id));

    // Automatically trigger initial synchronization on startup
    ui.sync_in_progress = true;
    let mut worker_rx: Option<Receiver<WorkerMsg>> = Some(spawn_sync_worker(
        identity.clone(),
        ui.shares.clone(),
        ui.devices.clone(),
    ));

    let mut last_activity = Instant::now();
    let mut last_sync_finished = Instant::now();

    // Main interactive 60 FPS loop (Gamepad SceCtrl + Touch SceTouch + OLED Saver)
    loop {
        let frame_input = input_ctl.poll();
        if frame_input.any_activity {
            last_activity = Instant::now();
        }

        if handle_user_input(&mut ui, frame_input, &identity, &mut worker_rx) {
            log_and_ui(&mut ui, "Exit requested by user [START].");
            break;
        }

        let was_running = worker_rx.is_some();
        process_worker_messages(&mut ui, &mut worker_rx);
        if was_running && worker_rx.is_none() {
            last_sync_finished = Instant::now();
        }

        // Automatically maintain BEP/1.0 connection in the background every 1s when idle
        if worker_rx.is_none() && last_sync_finished.elapsed() >= Duration::from_secs(1) {
            ui.sync_in_progress = true;
            worker_rx = Some(spawn_sync_worker(
                identity.clone(),
                ui.shares.clone(),
                ui.devices.clone(),
            ));
        }

        // Automatic activation of OLED screensaver (pure black #000000 background):
        // - After 12 seconds of inactivity while synchronization is in progress
        // - After 45 seconds of inactivity when idle (or press SELECT anytime)
        let oled_timeout = if ui.sync_in_progress {
            Duration::from_secs(12)
        } else {
            Duration::from_secs(45)
        };
        if !ui.oled_saver_active && last_activity.elapsed() >= oled_timeout {
            ui.oled_saver_active = true;
            write_log_file("Automatic OLED screensaver activation (#000000).");
        }

        ui.render();
        std::thread::sleep(Duration::from_millis(16));
    }
}
