//! Permanent Syncthing Relay Protocol v1 listener for incoming WAN reachability.
//!
//! Fetches the public Syncthing relay pool (`DEFAULT_RELAY_POOL`), connects to an available
//! relay (filtering out IPv6 relays on PS Vita where `SceNet` is IPv4-only), publishes the
//! active `relay://...` URI via [`SharedRelayUri`] for the Global Discovery v2 announcer,
//! and dispatches incoming [`SessionInvitation`]s:
//! - Unknown devices are triaged **before** taking a [`ConnectionSlot`] or spawning a thread,
//!   emitting [`WorkerMsg::DeviceOffered`] (gated by [`UnknownDeviceLog`]).
//! - Approved devices take a [`ConnectionSlot`], join the relay session in a `bep_relay_in`
//!   thread, branch on `invitation.server_socket`, claim [`SessionClaims`], and emit
//!   [`WorkerMsg::InboundSyncEvent`].
//! - All spawned `bep_relay_in` handles are reaped continuously and joined before returning.

use std::net::SocketAddr;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use syncthing_core::{
    accept_and_sync, accept_bep_tls_stream, connect_and_sync_stream, fetch_relay_pool,
    join_relay_session, resolve_endpoint_uri, BepHello, DeviceId, DeviceIdentity, FsSyncStorage,
    HttpsClient, HttpsServer, PeerEndpoint, RelayListener, SessionInvitation, SessionParams,
    SyncControl, SyncSessionStatus, DEFAULT_RELAY_POOL,
};

use super::peer_endpoints::sleep_interruptible;
use super::session_guard::{
    ConnectionSlot, ConnectionSlots, SessionClaims, UnknownDeviceLog, UnknownReport,
};
use super::sync_worker::{WorkerMsg, WORKER_STACK_SIZE};
use crate::domain::defaults::{
    CONNECT_TIMEOUT, GLOBAL_DISCOVERY_TIMEOUT, MAX_PENDING_RELAY_INVITATIONS, RELAY_POLL,
    RELAY_POOL_RETRY, RELAY_RETRY,
};
use crate::domain::KnownDevice;

/// Active `relay://<addr>/?id=<relay_id>` URI shared between the relay listener and global announcer.
pub type SharedRelayUri = Arc<Mutex<Option<String>>>;

/// Pure triage decision for an incoming relay [`SessionInvitation`] before spawning a thread.
#[derive(Debug)]
pub enum InvitationDecision {
    /// Approved device with no active session: spawn `bep_relay_in` holding this slot.
    Join(DeviceId, ConnectionSlot),
    /// `invitation.from` is not a valid 32-byte `DeviceId`.
    Malformed,
    /// `invitation.from` equals our own `DeviceId` (loopback or spoofed).
    SelfId,
    /// Not in `approved_devices`: report via [`UnknownDeviceLog`] + [`WorkerMsg::DeviceOffered`]
    /// without taking a slot or spawning a thread.
    Unknown(DeviceId),
    /// Device already has an active session in [`SessionClaims`].
    AlreadyConnected(DeviceId),
    /// All [`MAX_PENDING_RELAY_INVITATIONS`] slots are currently occupied.
    Busy(DeviceId),
}

/// Shared context passed to [`run_relay_listener`] by `spawn_sync_worker` (or tests).
#[derive(Clone)]
pub struct RelayListenerContext {
    pub identity: DeviceIdentity,
    pub hello: BepHello,
    pub storage: FsSyncStorage,
    pub approved_devices: Vec<KnownDevice>,
    pub claims: Arc<SessionClaims>,
    pub unknown_log: Arc<UnknownDeviceLog>,
    pub shared_relay_uri: SharedRelayUri,
    pub control: Arc<SyncControl>,
    pub tx: Sender<WorkerMsg>,
}

/// Decides how to handle a relay invitation without performing any network I/O or thread spawn.
pub fn triage_invitation(
    from: &[u8],
    self_id: DeviceId,
    is_approved: impl FnOnce(&DeviceId) -> bool,
    is_claimed: impl FnOnce(&DeviceId) -> bool,
    slots: &Arc<ConnectionSlots>,
) -> InvitationDecision {
    let Ok(raw_id) = <[u8; 32]>::try_from(from) else {
        return InvitationDecision::Malformed;
    };
    let id = DeviceId(raw_id);
    if id == self_id {
        return InvitationDecision::SelfId;
    }
    if !is_approved(&id) {
        return InvitationDecision::Unknown(id);
    }
    if is_claimed(&id) {
        return InvitationDecision::AlreadyConnected(id);
    }
    match slots.try_take() {
        Some(slot) => InvitationDecision::Join(id, slot),
        None => InvitationDecision::Busy(id),
    }
}

/// Returns `true` if `addr` is usable for a relay connection under the `ipv4_only` constraint
/// (`true` on PS Vita where `SceNet` only supports IPv4, `false` on host).
pub fn is_supported_relay_addr(addr: SocketAddr, ipv4_only: bool) -> bool {
    !ipv4_only || addr.is_ipv4()
}

/// Updates the shared active relay URI (poison-safe).
pub fn set_shared_relay_uri(shared: &SharedRelayUri, uri: Option<String>) {
    let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
    *guard = uri;
}

/// Reads the current shared active relay URI (poison-safe).
pub fn get_shared_relay_uri(shared: &SharedRelayUri) -> Option<String> {
    let guard = shared.lock().unwrap_or_else(|p| p.into_inner());
    guard.clone()
}

/// RAII guard that publishes `uri` into `shared` on creation and clears it to `None` on drop.
struct RelayUriGuard<'a> {
    shared: &'a SharedRelayUri,
}

impl<'a> RelayUriGuard<'a> {
    fn new(shared: &'a SharedRelayUri, uri: String) -> Self {
        set_shared_relay_uri(shared, Some(uri));
        Self { shared }
    }
}

impl Drop for RelayUriGuard<'_> {
    fn drop(&mut self) {
        set_shared_relay_uri(self.shared, None);
    }
}

/// Runs the permanent Relay Protocol v1 listener until `ctx.control.is_stop_requested()`.
/// Joins all spawned `bep_relay_in` handler threads before returning.
pub fn run_relay_listener(ctx: RelayListenerContext) {
    let slots = Arc::new(ConnectionSlots::new(MAX_PENDING_RELAY_INVITATIONS));
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    let client = HttpsClient::default();
    let mut pool_err_logged = false;

    while !ctx.control.is_stop_requested() {
        reap_finished_handles(&mut handles);

        let candidates = fetch_candidate_relays(&client, &ctx, &mut pool_err_logged);
        if candidates.is_empty() {
            if !sleep_interruptible(&ctx.control, RELAY_POOL_RETRY) {
                break;
            }
            continue;
        }
        pool_err_logged = false;

        let mut joined_any = false;
        for uri in candidates {
            if ctx.control.is_stop_requested() {
                break;
            }
            reap_finished_handles(&mut handles);

            let Some(PeerEndpoint::Relay { addr, device_id }) = resolve_endpoint_uri(&uri) else {
                continue;
            };
            if !is_supported_relay_addr(addr, cfg!(target_os = "vita")) {
                continue;
            }

            if let Ok(listener) =
                RelayListener::join(&ctx.identity, addr, device_id, CONNECT_TIMEOUT)
            {
                joined_any = true;
                serve_relay_listener(&ctx, listener, &slots, &mut handles);
                if !sleep_interruptible(&ctx.control, RELAY_RETRY) {
                    break;
                }
            }
        }

        if !joined_any && !sleep_interruptible(&ctx.control, RELAY_RETRY) {
            break;
        }
    }

    set_shared_relay_uri(&ctx.shared_relay_uri, None);
    for handle in handles {
        let _ = handle.join();
    }
}

/// Serves invitations on an already-joined [`RelayListener`] until it disconnects or stop is
/// requested. Publishes [`SharedRelayUri`] for the lifetime of the connection and clears it on exit.
pub fn serve_relay_listener(
    ctx: &RelayListenerContext,
    mut listener: RelayListener,
    slots: &Arc<ConnectionSlots>,
    handles: &mut Vec<JoinHandle<()>>,
) {
    let relay_addr = listener.relay_addr();
    let _uri_guard = RelayUriGuard::new(&ctx.shared_relay_uri, listener.relay_uri());
    let _ = ctx.tx.send(WorkerMsg::Log(format!(
        "Reachable through relay {relay_addr}"
    )));

    let mut busy_logged = false;

    while !ctx.control.is_stop_requested() {
        reap_finished_handles(handles);

        match listener.poll_invitation(RELAY_POLL) {
            Ok(None) => {}
            Ok(Some(invitation)) => {
                dispatch_invitation(
                    ctx,
                    relay_addr,
                    invitation,
                    slots,
                    handles,
                    &mut busy_logged,
                );
            }
            Err(e) => {
                if !ctx.control.is_stop_requested() {
                    let _ = ctx
                        .tx
                        .send(WorkerMsg::Log(format!("Lost relay {relay_addr}: {e}")));
                }
                break;
            }
        }
    }
}

fn fetch_candidate_relays(
    client: &HttpsClient,
    ctx: &RelayListenerContext,
    pool_err_logged: &mut bool,
) -> Vec<String> {
    let Some(pool) = HttpsServer::parse(DEFAULT_RELAY_POOL) else {
        return Vec::new();
    };
    let mut relays = match fetch_relay_pool(&pool, client, GLOBAL_DISCOVERY_TIMEOUT) {
        Ok(list) => list,
        Err(e) => {
            if !*pool_err_logged && !ctx.control.is_stop_requested() {
                *pool_err_logged = true;
                let _ = ctx
                    .tx
                    .send(WorkerMsg::Log(format!("Relay pool lookup failed: {e}")));
            }
            return Vec::new();
        }
    };
    if !relays.is_empty() {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as usize)
            .unwrap_or(0);
        let offset = seed % relays.len();
        relays.rotate_left(offset);
    }
    relays
}

fn dispatch_invitation(
    ctx: &RelayListenerContext,
    relay_addr: SocketAddr,
    invitation: SessionInvitation,
    slots: &Arc<ConnectionSlots>,
    handles: &mut Vec<JoinHandle<()>>,
    busy_logged: &mut bool,
) {
    let decision = triage_invitation(
        &invitation.from,
        ctx.identity.device_id,
        |id| {
            ctx.approved_devices
                .iter()
                .any(|kd| kd.device_id.matches_peer(id))
        },
        |id| ctx.claims.is_claimed(id),
        slots,
    );

    match decision {
        InvitationDecision::Malformed
        | InvitationDecision::SelfId
        | InvitationDecision::AlreadyConnected(_) => {}
        InvitationDecision::Unknown(from) => match ctx.unknown_log.record(from) {
            UnknownReport::First => {
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Relay: unknown device {} wants to connect.",
                    from.short()
                )));
                let _ = ctx.tx.send(WorkerMsg::DeviceOffered {
                    device_id: from,
                    address: String::new(),
                    via_relay: true,
                });
            }
            UnknownReport::LimitReached => {
                let _ = ctx.tx.send(WorkerMsg::Log(
                    "Relay: unknown device report limit reached; suppressing further reports."
                        .to_owned(),
                ));
            }
            UnknownReport::Quiet => {}
        },
        InvitationDecision::Busy(from) => {
            if !*busy_logged {
                *busy_logged = true;
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Too many relay invitations in progress; dropping invitation from {}.",
                    from.short()
                )));
            }
        }
        InvitationDecision::Join(from, slot) => {
            *busy_logged = false;
            let conn_ctx = ctx.clone();
            match thread::Builder::new()
                .name("bep_relay_in".to_owned())
                .stack_size(WORKER_STACK_SIZE)
                .spawn(move || handle_invitation(conn_ctx, relay_addr, invitation, from, slot))
            {
                Ok(handle) => handles.push(handle),
                Err(e) => {
                    let _ = ctx.tx.send(WorkerMsg::Log(format!(
                        "Cannot spawn relay session handler for {}: {e}",
                        from.short()
                    )));
                }
            }
        }
    }
}

fn handle_invitation(
    ctx: RelayListenerContext,
    relay_addr: SocketAddr,
    invitation: SessionInvitation,
    from: DeviceId,
    slot: ConnectionSlot,
) {
    let Some(kd) = ctx
        .approved_devices
        .iter()
        .find(|kd| kd.device_id.matches_peer(&from))
    else {
        return;
    };

    let tcp = match join_relay_session(relay_addr, &invitation, CONNECT_TIMEOUT) {
        Ok(stream) => stream,
        Err(e) => {
            if !ctx.control.is_stop_requested() {
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Relay session join for {} failed: {e}",
                    kd.name
                )));
            }
            return;
        }
    };

    let params = SessionParams {
        identity: &ctx.identity,
        hello: &ctx.hello,
        storage: &ctx.storage,
        expected_device_id: Some(from),
        timeout: CONNECT_TIMEOUT,
        control: Some(Arc::clone(&ctx.control)),
    };

    let result = if invitation.server_socket {
        let incoming = match accept_bep_tls_stream(&ctx.identity, tcp, CONNECT_TIMEOUT) {
            Ok(s) => s,
            Err(e) => {
                if !ctx.control.is_stop_requested() {
                    let _ = ctx.tx.send(WorkerMsg::Log(format!(
                        "Relayed TLS handshake from {} failed: {e}",
                        kd.name
                    )));
                }
                return;
            }
        };
        if incoming.peer_device_id() != from {
            let _ = ctx.tx.send(WorkerMsg::Log(format!(
                "Relay TLS identity mismatch: expected {}, got {}.",
                from.short(),
                incoming.peer_device_id().short()
            )));
            return;
        }
        drop(slot);
        let Some(_claim) = ctx.claims.try_claim(from) else {
            return;
        };
        accept_and_sync(&params, incoming, relay_addr, |ev| {
            let _ = ctx.tx.send(WorkerMsg::InboundSyncEvent(ev));
        })
    } else {
        drop(slot);
        let Some(_claim) = ctx.claims.try_claim(from) else {
            return;
        };
        connect_and_sync_stream(&params, tcp, relay_addr, |ev| {
            let _ = ctx.tx.send(WorkerMsg::InboundSyncEvent(ev));
        })
    };

    if ctx.control.is_stop_requested() {
        return;
    }

    match result {
        Ok(hs) => match hs.session_status {
            SyncSessionStatus::Active { .. } => {}
            SyncSessionStatus::AwaitingPairing => {
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Relayed peer {} awaiting device acceptance.",
                    kd.name
                )));
            }
            SyncSessionStatus::ClosedByPeer(reason) => {
                let _ = ctx.tx.send(WorkerMsg::Log(format!(
                    "Relayed session with {} closed by peer: {}",
                    kd.name, reason
                )));
            }
        },
        Err(e) => {
            let _ = ctx.tx.send(WorkerMsg::Log(format!(
                "Relayed session with {} failed: {e}",
                kd.name
            )));
        }
    }
}

fn reap_finished_handles(handles: &mut Vec<JoinHandle<()>>) {
    let mut i = 0;
    while i < handles.len() {
        if handles[i].is_finished() {
            let _ = handles.swap_remove(i).join();
        } else {
            i += 1;
        }
    }
}
