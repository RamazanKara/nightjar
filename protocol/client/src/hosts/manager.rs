//! Unified host registry, polling lifecycle, address pinning, add/invite/remove flows,
//! and change-detecting persistence shared across Android `ComputerManagerService` and Desktop.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nightjar_policy::hosts::{EndpointSlot, HostReachState, PollTupleState, SERVERINFO_POLL_PERIOD_MS};
use serde::{Deserialize, Serialize};

use crate::http::{ClientIdentity, ServerInfo};
use crate::internet;
use crate::stun::{MOONLIGHT_STUN_PORT, MOONLIGHT_STUN_SERVER, STUN_TIMEOUT, find_external_ipv4};

use super::client_pool::HostClientPool;
use super::identity::IdentityStore;
#[cfg(test)]
pub(super) use super::manager_ops::apply_server_info;
pub(super) use super::manager_ops::merge_entry_in_memory;
use super::meta::HostMetaStore;
use super::poll::{PollCancel, compute_internet_path, parse_internet_reply};
use super::store::{HostEntry, HostStore};

/// Snapshot of a managed host's persisted entry and live reachability/session fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostSnapshot {
    pub entry: HostEntry,
    pub state: HostReachState,
    pub active_endpoint: Option<EndpointSlot>,
    pub paired: bool,
    pub another_certificate: bool,
    pub running_game_id: i32,
    pub running_game_uuid: Option<String>,
    pub external_port: u16,
    pub unreachable: Option<String>,
    pub capabilities_json: Option<String>,
    pub alias: Option<String>,
    pub pinned_endpoint: Option<EndpointSlot>,
    pub internet_path: Option<String>,
    pub needs_vpn_stun: bool,
    #[serde(skip)]
    pub server_info: Option<ServerInfo>,
}

/// Outcome of [`HostManager::add_computer_for_link`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkAddOutcome {
    Added,
    Unreachable,
    CertificateMismatch,
}

/// Hook for resolving external IPv4 via STUN (used by Android when VPN requires `Network.bindSocket`).
pub type ExternalIpv4Resolver = Arc<dyn Fn(u16) -> Option<EndpointSlot> + Send + Sync>;

/// Callback invoked whenever a host snapshot changes meaningfully.
pub type HostUpdateListener = Arc<dyn Fn(&HostSnapshot) + Send + Sync>;

pub(super) struct ManagedTuple {
    pub(super) snapshot: HostSnapshot,
    pub(super) poll_state: PollTupleState,
    pub(super) last_internet_read_ms: Option<u64>,
    pub(super) next_stun_retry_ms: u64,
    pub(super) stun_backoff_ms: u64,
    pub(super) network_lock: Arc<Mutex<()>>,
    pub(super) stop_signal: Arc<(Mutex<bool>, Condvar)>,
    pub(super) poll_thread: Option<JoinHandle<()>>,
    pub(super) active_cancel: Option<PollCancel>,
    pub(super) removal_tombstone: bool,
}

pub(super) struct Inner {
    epoch: Instant,
    pub(super) store: HostStore,
    pub(super) meta: HostMetaStore,
    #[allow(dead_code)]
    identity_store: IdentityStore,
    pub(super) pool: Arc<HostClientPool>,
    pub(super) tuples: Mutex<HashMap<String, Arc<Mutex<ManagedTuple>>>>,
    pub(super) removal_lock: Mutex<()>,
    pub(super) polling_active: AtomicBool,
    pub(super) vpn_active: AtomicBool,
    pub(super) active_polls: Mutex<usize>,
    pub(super) active_polls_cv: Condvar,
    pub(super) stopping_threads: Mutex<Vec<JoinHandle<()>>>,
    wait_lock: Mutex<()>,
    pub(super) shutdown_cancel: PollCancel,
    listener: Mutex<Option<HostUpdateListener>>,
    pub(super) stun_resolver: Mutex<ExternalIpv4Resolver>,
    #[cfg(test)]
    pub(super) allow_loopback_stun: AtomicBool,
}

/// Default non-VPN direct UDP STUN resolver.
pub fn refresh_external_ipv4_direct(external_port: u16) -> Option<EndpointSlot> {
    let port = if external_port == 0 {
        nightjar_policy::hosts::DEFAULT_HTTP_PORT
    } else {
        external_port
    };
    let ip = find_external_ipv4(MOONLIGHT_STUN_SERVER, MOONLIGHT_STUN_PORT, STUN_TIMEOUT)
        .ok()
        .flatten()?;
    Some(EndpointSlot {
        address: ip.to_string(),
        port,
    })
}

/// Unified host manager coordinating [`HostStore`], [`HostMetaStore`], [`HostClientPool`],
/// per-host polling threads, and host lifecycle mutations.
#[derive(Clone)]
pub struct HostManager {
    pub(super) inner: Arc<Inner>,
}

impl HostManager {
    /// Opens stores in `config_dir`, initializes [`HostClientPool`] with `identity` (or loads from
    /// [`IdentityStore`]), and seeds tuples from `hosts.json`.
    pub fn open(config_dir: &Path, identity: Option<Arc<ClientIdentity>>) -> io::Result<Self> {
        let store = HostStore::open(config_dir);
        let meta = HostMetaStore::open(config_dir);
        let identity_store = IdentityStore::open(config_dir);
        let resolved_identity = match identity {
            Some(id) => id,
            None => {
                let loaded = identity_store
                    .load_or_create(None, None, None)
                    .map_err(io::Error::other)?;
                Arc::new(
                    ClientIdentity::from_pem(
                        &loaded.unique_id,
                        &loaded.certificate_pem,
                        &loaded.private_key_pem,
                    )
                    .map_err(io::Error::other)?,
                )
            }
        };
        let pool = Arc::new(HostClientPool::new(resolved_identity));
        let mgr = Self {
            inner: Arc::new(Inner {
                epoch: Instant::now(),
                store,
                meta,
                identity_store,
                pool,
                tuples: Mutex::new(HashMap::new()),
                removal_lock: Mutex::new(()),
                polling_active: AtomicBool::new(false),
                vpn_active: AtomicBool::new(false),
                active_polls: Mutex::new(0),
                active_polls_cv: Condvar::new(),
                stopping_threads: Mutex::new(Vec::new()),
                wait_lock: Mutex::new(()),
                shutdown_cancel: PollCancel::new(),
                listener: Mutex::new(None),
                stun_resolver: Mutex::new(Arc::new(refresh_external_ipv4_direct)),
                #[cfg(test)]
                allow_loopback_stun: AtomicBool::new(false),
            }),
        };
        mgr.reload_from_store();
        Ok(mgr)
    }

    pub fn store(&self) -> &HostStore {
        &self.inner.store
    }

    pub fn meta(&self) -> &HostMetaStore {
        &self.inner.meta
    }

    pub fn pool(&self) -> &Arc<HostClientPool> {
        &self.inner.pool
    }

    pub fn set_listener(&self, listener: Option<HostUpdateListener>) {
        *self.inner.listener.lock().unwrap_or_else(|e| e.into_inner()) = listener;
    }

    pub fn set_stun_resolver(&self, resolver: ExternalIpv4Resolver) {
        *self
            .inner
            .stun_resolver
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = resolver;
    }

    /// Sets whether an OS VPN interface is active (suppressing direct UDP STUN in favor of Android's
    /// bound-socket fallback flagged via `needs_vpn_stun`).
    pub fn set_vpn_active(&self, active: bool) {
        self.inner.vpn_active.store(active, Ordering::Release);
    }

    /// Updates the global internet-play enabled switch (stopping active tunnels when turned off).
    pub fn set_internet_play_enabled(&self, enabled: bool) {
        internet::set_enabled(enabled);
    }

    /// Joins stopped worker threads and blocks until every in-flight `poll_host` call completes.
    pub fn wait_for_polling_stopped(&self) {
        let _wait = self.inner.wait_lock.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            let handles = {
                let mut stopping = self
                    .inner
                    .stopping_threads
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                std::mem::take(&mut *stopping)
            };
            for h in handles {
                let _ = h.join();
            }
            let mut count = self
                .inner
                .active_polls
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            while *count != 0 {
                count = self
                    .inner
                    .active_polls_cv
                    .wait(count)
                    .unwrap_or_else(|e| e.into_inner());
            }
            let empty = self
                .inner
                .stopping_threads
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty();
            if empty {
                break;
            }
        }
    }

    /// Whether any poll pass is active or any stopped worker thread has not yet exited.
    pub fn has_active_workers(&self) -> bool {
        let active = *self
            .inner
            .active_polls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            != 0;
        let mut stopping = self
            .inner
            .stopping_threads
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        stopping.retain(|h| !h.is_finished());
        active || !stopping.is_empty()
    }

    /// Cancels all polling and ad-hoc manager requests non-blockingly for teardown.
    pub fn shutdown(&self) {
        self.inner.shutdown_cancel.cancel();
        self.stop_polling();
    }

    /// Monotonic milliseconds elapsed since this manager was opened.
    pub fn now_ms(&self) -> u64 {
        self.inner.epoch.elapsed().as_millis() as u64
    }

    pub(super) fn emit(&self, snapshot: &HostSnapshot) {
        let cb = self
            .inner
            .listener
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(cb) = cb {
            cb(snapshot);
        }
    }

    /// Loads saved entries from `hosts.json` into in-memory tuples.
    pub fn reload_from_store(&self) {
        let saved = self.inner.store.all();
        for mut entry in saved {
            if entry.shared.unique_id.is_empty() {
                continue;
            }
            if !entry.has_android_slots() {
                entry.sync_slots_from_shared();
            }
            self.upsert_tuple(entry);
        }
    }

    /// Inserts or refreshes a tuple in memory and spawns its poll thread if polling is active.
    pub fn upsert_tuple(&self, mut entry: HostEntry) -> HostSnapshot {
        let uuid = entry.shared.unique_id.clone();
        if entry.has_android_slots() {
            entry.sync_shared_from_slots();
        }
        let alias = self.inner.meta.get_alias(&uuid).or(entry.shared.alias.clone());
        let pinned = self
            .inner
            .meta
            .get_pinned(&uuid)
            .or(entry.pinned_endpoint.clone());
        if pinned.is_some() {
            entry.pinned_endpoint = pinned.clone();
            entry.sync_shared_from_slots();
        }
        let capabilities_json = self.inner.meta.load_capabilities(&uuid);

        let tuple_arc = {
            let mut tuples = self.inner.tuples.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = tuples.get(&uuid) {
                let existing = Arc::clone(existing);
                drop(tuples);
                let mut t = existing.lock().unwrap_or_else(|e| e.into_inner());
                if !t.removal_tombstone {
                    merge_entry_in_memory(&mut t.snapshot.entry, entry);
                    t.snapshot.alias = alias;
                    t.snapshot.pinned_endpoint = pinned.clone();
                    if pinned.is_some() {
                        t.snapshot.active_endpoint = pinned;
                    }
                    if t.snapshot.entry.shared.server_certificate.is_some() {
                        t.snapshot.paired = true;
                    }
                    let reply =
                        parse_internet_reply(t.snapshot.entry.shared.internet_play.as_deref());
                    t.snapshot.internet_path = compute_internet_path(
                        t.snapshot.active_endpoint.as_ref(),
                        t.snapshot.entry.local_endpoint.as_ref(),
                        t.snapshot.entry.manual_endpoint.as_ref(),
                        reply.as_ref(),
                    );
                    if let Some(caps) = capabilities_json {
                        t.snapshot.capabilities_json = Some(caps);
                    }
                }
                let snap = t.snapshot.clone();
                drop(t);
                self.maybe_start_tuple_thread(&existing);
                return snap;
            }
            let active_endpoint = pinned.clone().or_else(|| entry.local_endpoint.clone());
            let reply = parse_internet_reply(entry.shared.internet_play.as_deref());
            let internet_path = compute_internet_path(
                active_endpoint.as_ref(),
                entry.local_endpoint.as_ref(),
                entry.manual_endpoint.as_ref(),
                reply.as_ref(),
            );
            let snap = HostSnapshot {
                paired: entry.shared.server_certificate.is_some(),
                entry,
                state: HostReachState::Unknown,
                active_endpoint,
                another_certificate: false,
                running_game_id: 0,
                running_game_uuid: None,
                external_port: 0,
                unreachable: None,
                capabilities_json,
                alias,
                pinned_endpoint: pinned,
                internet_path,
                needs_vpn_stun: false,
                server_info: None,
            };
            let arc = Arc::new(Mutex::new(ManagedTuple {
                snapshot: snap.clone(),
                poll_state: PollTupleState::default(),
                last_internet_read_ms: None,
                next_stun_retry_ms: 0,
                stun_backoff_ms: 0,
                network_lock: Arc::new(Mutex::new(())),
                stop_signal: Arc::new((Mutex::new(false), Condvar::new())),
                poll_thread: None,
                active_cancel: None,
                removal_tombstone: false,
            }));
            tuples.insert(uuid, Arc::clone(&arc));
            arc
        };
        self.maybe_start_tuple_thread(&tuple_arc);
        tuple_arc
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .clone()
    }

    pub fn snapshots(&self) -> Vec<HostSnapshot> {
        let tuples = self.inner.tuples.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<HostSnapshot> = tuples
            .values()
            .filter_map(|arc| {
                let t = arc.lock().unwrap_or_else(|e| e.into_inner());
                (!t.removal_tombstone).then(|| t.snapshot.clone())
            })
            .collect();
        out.sort_by(|a, b| a.entry.shared.unique_id.cmp(&b.entry.shared.unique_id));
        out
    }

    pub fn get_snapshot(&self, uuid: &str) -> Option<HostSnapshot> {
        let arc = self
            .inner
            .tuples
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(uuid)
            .cloned()?;
        let t = arc.lock().unwrap_or_else(|e| e.into_inner());
        (!t.removal_tombstone).then(|| t.snapshot.clone())
    }

    /// Starts periodic background polling for all current and future tuples, emitting initial
    /// snapshots after [`PollTupleState::enforce_ttl`].
    pub fn start_polling(&self) {
        self.inner.polling_active.store(true, Ordering::Release);
        let arcs: Vec<_> = self
            .inner
            .tuples
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        let now_ms = self.now_ms();
        for arc in arcs {
            let initial_snap = {
                let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
                t.poll_state.enforce_ttl(now_ms);
                if t.poll_state.state == HostReachState::Unknown {
                    t.snapshot.state = HostReachState::Unknown;
                }
                (!t.removal_tombstone).then(|| t.snapshot.clone())
            };
            if let Some(snap) = initial_snap {
                self.emit(&snap);
            }
            self.maybe_start_tuple_thread(&arc);
        }
    }

    /// Signals polling to stop and cancels active poll sockets without blocking on thread joins.
    ///
    /// Call [`Self::wait_for_polling_stopped`] off the UI thread when synchronous join is needed.
    pub fn stop_polling(&self) {
        self.inner.polling_active.store(false, Ordering::Release);
        let arcs: Vec<_> = self
            .inner
            .tuples
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        let mut handles = Vec::new();
        let mut cancels = Vec::new();
        for arc in arcs {
            let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
            let (lock, cvar) = &*t.stop_signal;
            *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
            cvar.notify_all();
            if let Some(h) = t.poll_thread.take() {
                handles.push(h);
            }
            if let Some(c) = t.active_cancel.take() {
                cancels.push(c);
            }
        }
        if !handles.is_empty() {
            let mut stopping = self
                .inner
                .stopping_threads
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            stopping.retain(|h| !h.is_finished());
            stopping.extend(handles);
        }
        self.inner.pool.clear();
        for c in cancels {
            c.cancel();
        }
    }

    fn maybe_start_tuple_thread(&self, tuple_arc: &Arc<Mutex<ManagedTuple>>) {
        if !self.inner.polling_active.load(Ordering::Acquire)
            || self.inner.shutdown_cancel.is_cancelled()
        {
            return;
        }
        let mut t = tuple_arc.lock().unwrap_or_else(|e| e.into_inner());
        if t.removal_tombstone || t.poll_thread.is_some() {
            return;
        }
        let now_ms = self.now_ms();
        t.poll_state.enforce_ttl(now_ms);
        let stop_signal = Arc::new((Mutex::new(false), Condvar::new()));
        t.stop_signal = Arc::clone(&stop_signal);

        let uuid = t.snapshot.entry.shared.unique_id.clone();
        let weak = Arc::downgrade(&self.inner);
        let handle = std::thread::Builder::new()
            .name(format!("nj-host-poll-{uuid}"))
            .spawn(move || {
                loop {
                    let Some(inner) = weak.upgrade() else {
                        break;
                    };
                    let mgr = HostManager { inner };
                    let _ = mgr.poll_host_inner(&uuid, false, Some(&stop_signal));
                    let active = mgr.inner.polling_active.load(Ordering::Acquire);
                    drop(mgr);
                    let (lock, cvar) = &*stop_signal;
                    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    if *guard || !active {
                        break;
                    }
                    let (guard, _) = cvar
                        .wait_timeout(guard, Duration::from_millis(SERVERINFO_POLL_PERIOD_MS))
                        .unwrap_or_else(|e| e.into_inner());
                    let still_active = weak
                        .upgrade()
                        .is_some_and(|inner| inner.polling_active.load(Ordering::Acquire));
                    if *guard || !still_active {
                        break;
                    }
                }
            })
            .ok();
        t.poll_thread = handle;
    }

    /// Runs a single poll pass for an existing tracked host UUID.
    ///
    /// Returns `true` if visible state or persisted details changed.
    pub fn poll_host(&self, uuid: &str, force: bool) -> bool {
        self.poll_host_inner(uuid, force, None)
    }
}

impl Drop for HostManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            self.shutdown();
        }
    }
}
