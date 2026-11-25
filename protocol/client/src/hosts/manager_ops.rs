//! Host addition, invite reconciliation, removal, address pinning, certificate clearing,
//! renaming, network-change reset, and unit tests for [`super::manager::HostManager`].

use std::io;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;

use nightjar_policy::hosts::{EndpointSlot, HostReachState, probe_plan_with_extras, punches_now};

use crate::http::{ServerInfo, certificate_sha256_der, pem_decode_certificate};
use crate::internet;

use super::manager::{HostManager, HostSnapshot, LinkAddOutcome, ManagedTuple};
use super::poll::{
    PollIpSuccess, compute_internet_path, is_site_local_host, parallel_poll_direct_cancellable,
    parse_internet_reply, poll_over_the_internet_cancellable,
};
use super::store::HostEntry;

/// Outcome of an ephemeral [`HostManager::poll_details`] check (Android `@VisibleForTesting pollComputer`).
#[derive(Clone, Debug)]
pub struct EphemeralPollOutcome {
    pub ok: bool,
    pub snapshot: HostSnapshot,
    pub should_clear_cert: bool,
}

pub(super) struct PollCommitOutcome {
    pub(super) winner: Option<PollIpSuccess>,
    pub(super) resolved_stun_remote: Option<EndpointSlot>,
    pub(super) needs_vpn_stun: bool,
    pub(super) internet_play_update: Option<Option<String>>,
    pub(super) another_certificate: bool,
    pub(super) unreachable_msg: Option<String>,
}

impl HostManager {
    fn get_tuple(&self, uuid: &str) -> Option<Arc<Mutex<ManagedTuple>>> {
        self.inner
            .tuples
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(uuid)
            .cloned()
    }

    /// Probes a user-entered/discovered candidate entry, verifies it, persists it, and begins polling.
    pub fn add_computer_blocking(&self, mut candidate: HostEntry) -> Option<HostSnapshot> {
        if !candidate.has_android_slots() {
            candidate.sync_slots_from_shared();
        }
        let slots = candidate.slots();
        let probes = probe_plan_with_extras(
            &slots,
            &candidate.shared.other_addresses,
            candidate.shared.http_port,
            true,
            None,
            0,
        );
        let direct = parallel_poll_direct_cancellable(
            &self.inner.pool,
            &probes,
            None,
            None,
            true,
            Some(&self.inner.shutdown_cancel),
        );
        let win = direct.winner?;
        let uuid = win.info.unique_id.clone();

        let existing_cert = self
            .get_snapshot(&uuid)
            .and_then(|s| s.entry.server_certificate_der());
        if existing_cert.is_some() {
            let mut updated_cand = win.candidate.clone();
            updated_cand.https_port = win.info.https_port;
            let pinned_direct = parallel_poll_direct_cancellable(
                &self.inner.pool,
                &[updated_cand],
                Some(&uuid),
                existing_cert.as_deref(),
                true,
                Some(&self.inner.shutdown_cancel),
            );
            let pwin = pinned_direct.winner?;
            return Some(self.finish_adding(candidate, existing_cert, pwin));
        }
        Some(self.finish_adding(candidate, existing_cert, win))
    }

    /// Adds a PC for a pairing link, refusing if the responder claims a known UUID pinned to
    /// another certificate, while preserving an existing matching certificate and returning the snapshot.
    pub fn add_computer_for_link(
        &self,
        mut candidate: HostEntry,
        link_cert_sha256: [u8; 32],
    ) -> (LinkAddOutcome, Option<HostSnapshot>) {
        if !candidate.has_android_slots() {
            candidate.sync_slots_from_shared();
        }
        let slots = candidate.slots();
        let probes = probe_plan_with_extras(
            &slots,
            &candidate.shared.other_addresses,
            candidate.shared.http_port,
            true,
            None,
            0,
        );
        let direct = parallel_poll_direct_cancellable(
            &self.inner.pool,
            &probes,
            None,
            None,
            true,
            Some(&self.inner.shutdown_cancel),
        );
        let Some(win) = direct.winner else {
            return (LinkAddOutcome::Unreachable, None);
        };
        let uuid = win.info.unique_id.clone();
        let existing_cert = self
            .get_snapshot(&uuid)
            .and_then(|s| s.entry.server_certificate_der());
        if let Some(ref pinned_der) = existing_cert
            && certificate_sha256_der(pinned_der) != link_cert_sha256
        {
            return (LinkAddOutcome::CertificateMismatch, None);
        }
        let final_win = if existing_cert.is_some() {
            let mut updated_cand = win.candidate.clone();
            updated_cand.https_port = win.info.https_port;
            let pinned_direct = parallel_poll_direct_cancellable(
                &self.inner.pool,
                &[updated_cand],
                Some(&uuid),
                existing_cert.as_deref(),
                true,
                Some(&self.inner.shutdown_cancel),
            );
            let Some(pwin) = pinned_direct.winner else {
                let outcome = if pinned_direct.another_certificate {
                    LinkAddOutcome::CertificateMismatch
                } else {
                    LinkAddOutcome::Unreachable
                };
                return (outcome, None);
            };
            pwin
        } else {
            win
        };
        let snap = self.finish_adding(candidate, existing_cert, final_win);
        (LinkAddOutcome::Added, Some(snap))
    }

    fn finish_adding(
        &self,
        mut candidate: HostEntry,
        existing_cert: Option<Vec<u8>>,
        win: PollIpSuccess,
    ) -> HostSnapshot {
        candidate.shared.unique_id = win.info.unique_id.clone();
        candidate.set_server_certificate_der(existing_cert.as_deref());
        if win.should_clear_cert {
            candidate.shared.server_certificate = None;
            candidate.shared.internet_play = None;
        }
        let mut needs_vpn_stun = false;
        if candidate.remote_endpoint.is_none()
            && win.info.external_ip.is_none()
            && is_site_local_host(&win.candidate.address)
        {
            if self.inner.vpn_active.load(Ordering::Acquire) {
                needs_vpn_stun = true;
            } else {
                let resolver = self
                    .inner
                    .stun_resolver
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                if let Some(remote) = resolver(win.info.external_port) {
                    candidate.remote_endpoint = Some(remote);
                }
            }
        }
        candidate.sync_shared_from_slots();
        let _rem_guard = self.inner.removal_lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut snap = self.upsert_tuple(candidate);
        let uuid = win.info.unique_id.clone();
        if let Some(arc) = self.get_tuple(&uuid) {
            let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
            if t.removal_tombstone {
                return snap;
            }
            t.poll_state.record_poll_success(self.now_ms());
            apply_server_info(&mut t.snapshot, &win.info, win.candidate.port, &win.candidate);
            t.snapshot.state = HostReachState::Online;
            t.snapshot.another_certificate = false;
            t.snapshot.unreachable = None;
            t.snapshot.needs_vpn_stun =
                needs_vpn_stun && t.snapshot.entry.remote_endpoint.is_none();
            if win.should_clear_cert {
                t.snapshot.entry.shared.server_certificate = None;
                t.snapshot.entry.shared.internet_play = None;
                t.snapshot.paired = false;
                internet::stop(&uuid);
            } else {
                t.snapshot.paired = win.info.paired;
            }
            if let Some(caps_json) = win.info.capabilities.to_versioned_json() {
                let _ = self.inner.meta.save_capabilities(&uuid, &caps_json);
                t.snapshot.capabilities_json = Some(caps_json);
            }
            let reply = parse_internet_reply(t.snapshot.entry.shared.internet_play.as_deref());
            t.snapshot.internet_path = compute_internet_path(
                t.snapshot.active_endpoint.as_ref(),
                t.snapshot.entry.local_endpoint.as_ref(),
                t.snapshot.entry.manual_endpoint.as_ref(),
                reply.as_ref(),
            );
            t.snapshot.entry.sync_shared_from_slots();
            let _ = self.inner.store.replace_entry(t.snapshot.entry.clone());
            snap = t.snapshot.clone();
        }
        drop(_rem_guard);
        self.emit(&snap);
        snap
    }

    /// Saves a host redeemed via an invite link, refusing if another certificate is already pinned.
    pub fn add_invited_computer(
        &self,
        mut entry: HostEntry,
        internet_play_reply: &str,
    ) -> io::Result<bool> {
        let uuid = entry.shared.unique_id.clone();
        let incoming_der = entry.server_certificate_der();
        if let Some(existing_snap) = self.get_snapshot(&uuid)
            && let Some(existing_der) = existing_snap.entry.server_certificate_der()
            && incoming_der.as_deref() != Some(existing_der.as_slice())
        {
            return Ok(false);
        }
        let _rem_guard = self.inner.removal_lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(saved) = self.inner.store.get_by_uuid(&uuid)
            && let Some(saved_der) = saved.server_certificate_der()
            && incoming_der.as_deref() != Some(saved_der.as_slice())
        {
            return Ok(false);
        }
        entry.shared.internet_play = Some(internet_play_reply.to_owned());
        entry.sync_shared_from_slots();
        self.inner.store.upsert(entry.clone())?;
        let snap = self.upsert_tuple(entry);
        self.emit(&snap);
        Ok(true)
    }

    /// Checks whether `link_fp_sha256` conflicts with an already-pinned host UUID.
    pub fn invite_claims_known_pc(&self, uuid: &str, link_fp_sha256: [u8; 32]) -> bool {
        let cert_der = self
            .get_snapshot(uuid)
            .and_then(|s| s.entry.server_certificate_der())
            .or_else(|| self.inner.store.get_by_uuid(uuid).and_then(|e| e.server_certificate_der()));
        let existing_sha = cert_der.map(|der| certificate_sha256_der(&der));
        nightjar_policy::hosts::invite_claims_known_pc(existing_sha, link_fp_sha256)
    }

    /// Signals stop, cancels active poll sockets, marks tombstone, and deletes host from stores.
    pub fn remove_computer(&self, uuid: &str) -> io::Result<()> {
        let removed_tuple = {
            let mut tuples = self.inner.tuples.lock().unwrap_or_else(|e| e.into_inner());
            tuples.remove(uuid)
        };
        if let Some(arc) = removed_tuple {
            let (handle, cancel) = {
                let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
                t.removal_tombstone = true;
                t.poll_state.removed = true;
                let (lock, cvar) = &*t.stop_signal;
                *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
                cvar.notify_all();
                (t.poll_thread.take(), t.active_cancel.take())
            };
            if let Some(c) = cancel {
                c.cancel();
            }
            self.inner.pool.clear();
            if let Some(h) = handle {
                let mut stopping = self.inner.stopping_threads.lock().unwrap_or_else(|e| e.into_inner());
                stopping.retain(|existing| !existing.is_finished());
                stopping.push(h);
            }
        }
        let _rem_guard = self.inner.removal_lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.inner.store.delete_by_uuid(uuid)?;
        let _ = self.inner.meta.forget(uuid)?;
        internet::stop(uuid);
        Ok(())
    }

    fn mutate_live_tuple(
        &self,
        uuid: &str,
        f: impl FnOnce(&mut ManagedTuple) -> io::Result<()>,
    ) -> io::Result<()> {
        let snap = {
            let _rem_guard = self.inner.removal_lock.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(arc) = self.get_tuple(uuid) {
                let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
                if t.removal_tombstone {
                    return Ok(());
                }
                f(&mut t)?;
                Some(t.snapshot.clone())
            } else {
                None
            }
        };
        if let Some(snap) = snap {
            self.emit(&snap);
        }
        Ok(())
    }

    pub fn pin_address(&self, uuid: &str, endpoint: Option<EndpointSlot>) -> io::Result<()> {
        self.mutate_live_tuple(uuid, |t| {
            let _ = self.inner.meta.set_pinned(uuid, endpoint.clone())?;
            t.snapshot.pinned_endpoint = endpoint.clone();
            t.snapshot.entry.pinned_endpoint = endpoint.clone();
            t.snapshot.entry.shared.pinned_address = endpoint.as_ref().map(|e| e.address.clone());
            if let Some(ref ep) = endpoint {
                t.snapshot.active_endpoint = Some(ep.clone());
            }
            let _ = self.inner.store.replace_entry(t.snapshot.entry.clone());
            Ok(())
        })
    }

    pub fn forget_certificate(&self, uuid: &str) -> io::Result<()> {
        let snap = {
            let _rem_guard = self.inner.removal_lock.lock().unwrap_or_else(|e| e.into_inner());
            let snap = if let Some(arc) = self.get_tuple(uuid) {
                let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
                if t.removal_tombstone {
                    return Ok(());
                }
                t.snapshot.entry.shared.server_certificate = None;
                t.snapshot.entry.shared.internet_play = None;
                t.snapshot.paired = false;
                Some(t.snapshot.clone())
            } else {
                None
            };
            if let Some(mut saved) = self.inner.store.get_by_uuid(uuid)
                && (saved.shared.server_certificate.take().is_some()
                    | saved.shared.internet_play.take().is_some())
            {
                let _ = self.inner.store.replace_entry(saved)?;
            }
            snap
        };
        if let Some(snap) = snap {
            self.emit(&snap);
        }
        internet::stop(uuid);
        self.inner.pool.clear();
        Ok(())
    }

    /// Updates a host's STUN-resolved remote endpoint and clears `needs_vpn_stun`.
    pub fn set_remote_endpoint(&self, uuid: &str, remote: Option<EndpointSlot>) -> io::Result<()> {
        self.mutate_live_tuple(uuid, |t| {
            t.snapshot.entry.remote_endpoint = remote;
            t.snapshot.needs_vpn_stun = false;
            t.snapshot.entry.sync_shared_from_slots();
            let _ = self.inner.store.replace_entry(t.snapshot.entry.clone())?;
            Ok(())
        })
    }

    /// Resets a single host's reachability state to [`HostReachState::Unknown`].
    pub fn invalidate_state(&self, uuid: &str) {
        let Some(arc) = self.get_tuple(uuid) else {
            return;
        };
        let (net_lock, cancel) = {
            let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
            (Arc::clone(&t.network_lock), t.active_cancel.take())
        };
        if let Some(c) = cancel {
            c.cancel();
        }
        let _net_guard = net_lock.lock().unwrap_or_else(|e| e.into_inner());
        let snap = {
            let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
            if t.removal_tombstone {
                return;
            }
            t.poll_state.has_polled_successfully = false;
            t.snapshot.state = HostReachState::Unknown;
            t.snapshot.server_info = None;
            t.snapshot.clone()
        };
        self.emit(&snap);
    }

    /// Imports or clears stored `internet_play` JSON for `uuid` (e.g. legacy Android migration).
    pub fn import_internet_play(&self, uuid: &str, reply_json: Option<&str>) -> io::Result<()> {
        let _rem_guard = self.inner.removal_lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(arc) = self.get_tuple(uuid) {
            let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
            if t.removal_tombstone {
                return Ok(());
            }
            if t.snapshot.entry.shared.internet_play.as_deref() != reply_json {
                t.snapshot.entry.shared.internet_play = reply_json.map(ToOwned::to_owned);
                let _ = self.inner.store.replace_entry(t.snapshot.entry.clone())?;
            }
        }
        if let Some(mut saved) = self.inner.store.get_by_uuid(uuid)
            && saved.shared.internet_play.as_deref() != reply_json
        {
            saved.shared.internet_play = reply_json.map(ToOwned::to_owned);
            self.inner.store.replace_entry(saved)?;
        }
        Ok(())
    }

    /// Updates a host's pinned PEM certificate in both memory and `hosts.json`.
    pub fn set_server_certificate_pem(&self, uuid: &str, pem: Option<&str>) -> io::Result<()> {
        let der = pem
            .map(pem_decode_certificate)
            .transpose()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
        self.mutate_live_tuple(uuid, |t| {
            t.poll_state.has_polled_successfully = false;
            t.snapshot.entry.set_server_certificate_der(der.as_deref());
            t.snapshot.paired = der.is_some();
            let _ = self.inner.store.replace_entry(t.snapshot.entry.clone());
            Ok(())
        })?;
        self.inner.pool.clear();
        Ok(())
    }

    pub fn rename_computer(&self, uuid: &str, alias: Option<&str>) -> io::Result<()> {
        let normalized = nightjar_policy::hosts::HostNames::normalize_alias(alias, None);
        self.mutate_live_tuple(uuid, |t| {
            let _ = self.inner.meta.set_alias(uuid, alias)?;
            t.snapshot.alias = normalized.clone();
            t.snapshot.entry.shared.alias = normalized;
            let _ = self.inner.store.replace_entry(t.snapshot.entry.clone());
            Ok(())
        })
    }

    /// Clears pooled clients and internet tunnels after an OS network transition, skipping any
    /// host that already succeeded polling at or after `at_ms`.
    pub fn reset_network_state(&self, available: bool, at_ms: u64) {
        self.inner.pool.clear();
        let arcs: Vec<_> = self
            .inner
            .tuples
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect();
        // Cancel all in-flight polls upfront so per-host `network_lock` waits do not stall.
        for arc in &arcs {
            if let Some(c) = arc.lock().unwrap_or_else(|e| e.into_inner()).active_cancel.take() {
                c.cancel();
            }
        }
        for arc in arcs {
            let net_lock = Arc::clone(&arc.lock().unwrap_or_else(|e| e.into_inner()).network_lock);
            let _net_guard = net_lock.lock().unwrap_or_else(|e| e.into_inner());
            let (uuid, snap) = {
                let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
                if t.removal_tombstone {
                    continue;
                }
                let uuid = t.snapshot.entry.shared.unique_id.clone();
                if t.poll_state.has_polled_successfully
                    && t.poll_state.last_successful_poll_ms >= at_ms
                {
                    (uuid, None)
                } else {
                    t.poll_state.has_polled_successfully = false;
                    t.snapshot.state = if available {
                        HostReachState::Unknown
                    } else {
                        HostReachState::Offline
                    };
                    t.snapshot.server_info = None;
                    (uuid, Some(t.snapshot.clone()))
                }
            };
            if let Some(snap) = snap {
                internet::stop(&uuid);
                self.emit(&snap);
            }
        }
    }

    /// Polls an ad-hoc `candidate` through the manager's pool and internet-play fallback without
    /// mutating tracked tuples (backing Android `@VisibleForTesting pollComputer`).
    pub fn poll_details(
        &self,
        mut candidate: HostEntry,
        initial_state: HostReachState,
        active_endpoint: Option<EndpointSlot>,
    ) -> EphemeralPollOutcome {
        if !candidate.has_android_slots() {
            candidate.sync_slots_from_shared();
        }
        let uuid = candidate.shared.unique_id.clone();
        if candidate.shared.internet_play.is_none() && !uuid.is_empty() {
            candidate.shared.internet_play = self
                .get_snapshot(&uuid)
                .and_then(|s| s.entry.shared.internet_play);
        }
        let cert_der = candidate.server_certificate_der();
        let slots = candidate.slots();
        let active_port = active_endpoint.as_ref().map(|e| e.port);
        let probes = probe_plan_with_extras(
            &slots,
            &candidate.shared.other_addresses,
            candidate.shared.http_port,
            initial_state == HostReachState::Online,
            active_port,
            candidate.shared.https_port,
        );
        let expected_uuid = (!uuid.is_empty()).then_some(uuid.as_str());
        let likely_online = initial_state == HostReachState::Online;
        let direct = parallel_poll_direct_cancellable(
            &self.inner.pool,
            &probes,
            expected_uuid,
            cert_der.as_deref(),
            likely_online,
            Some(&self.inner.shutdown_cancel),
        );
        let mut winner = direct.winner;
        let mut unreachable_msg = None;
        let mut internet_another_cert = false;

        if winner.is_none()
            && slots.pinned.is_none()
            && let Some(reply_json) = candidate.shared.internet_play.as_deref()
        {
            let active_is_tunnel = active_endpoint
                .as_ref()
                .is_some_and(|e| nightjar_policy::hosts::is_tunnel_address(&e.address));
            let allow_punch = punches_now(
                slots.local.is_some(),
                initial_state == HostReachState::Offline,
                active_is_tunnel,
            );
            match poll_over_the_internet_cancellable(
                &self.inner.pool,
                &uuid,
                reply_json,
                expected_uuid,
                cert_der.as_deref(),
                &probes,
                direct.tried,
                likely_online,
                allow_punch,
                Some(&self.inner.shutdown_cancel),
            ) {
                Ok((_, win)) => winner = Some(win),
                Err(fail) => {
                    internet_another_cert = fail.another_certificate;
                    unreachable_msg = Some(fail.reason);
                }
            }
        }

        let reply = parse_internet_reply(candidate.shared.internet_play.as_deref());
        let pinned_endpoint = candidate.pinned_endpoint.clone();
        let alias = candidate.shared.alias.clone();
        let mut snap = HostSnapshot {
            paired: candidate.shared.server_certificate.is_some(),
            entry: candidate,
            state: initial_state,
            active_endpoint,
            another_certificate: false,
            running_game_id: 0,
            running_game_uuid: None,
            external_port: 0,
            unreachable: None,
            capabilities_json: None,
            alias,
            pinned_endpoint,
            internet_path: None,
            needs_vpn_stun: false,
            server_info: None,
        };
        if let Some(win) = winner {
            apply_server_info(&mut snap, &win.info, win.candidate.port, &win.candidate);
            snap.state = HostReachState::Online;
            snap.paired = win.info.paired && !win.should_clear_cert;
            if win.should_clear_cert {
                snap.entry.shared.server_certificate = None;
                snap.entry.shared.internet_play = None;
            }
            snap.capabilities_json = win.info.capabilities.to_versioned_json();
            snap.internet_path = compute_internet_path(
                snap.active_endpoint.as_ref(),
                snap.entry.local_endpoint.as_ref(),
                snap.entry.manual_endpoint.as_ref(),
                reply.as_ref(),
            );
            EphemeralPollOutcome {
                ok: true,
                snapshot: snap,
                should_clear_cert: win.should_clear_cert,
            }
        } else {
            snap.another_certificate = direct.another_certificate || internet_another_cert;
            snap.unreachable = unreachable_msg;
            EphemeralPollOutcome {
                ok: false,
                snapshot: snap,
                should_clear_cert: false,
            }
        }
    }
}

pub(super) fn apply_server_info(
    snap: &mut HostSnapshot,
    info: &ServerInfo,
    answered_port: u16,
    candidate: &nightjar_policy::hosts::ProbeCandidate,
) {
    if !info.hostname.is_empty() && info.hostname != "UNKNOWN" {
        snap.entry.shared.name = info.hostname.clone();
    }
    if let Some(ref mac) = info.mac_address {
        snap.entry.shared.mac = Some(mac.clone());
    }
    if info.https_port != 0 {
        snap.entry.shared.https_port = info.https_port;
    }
    snap.external_port = info.external_port;
    snap.running_game_id = info.current_game;
    snap.running_game_uuid = info.current_game_uuid.clone();
    snap.active_endpoint = Some(EndpointSlot {
        address: candidate.address.clone(),
        port: answered_port,
    });
    if let Some(ref local_ip) = info.local_ip
        && !local_ip.is_empty()
        && !local_ip.starts_with("127.")
    {
        snap.entry.local_endpoint = EndpointSlot::new(local_ip, answered_port);
    }
    if let Some(ref external_ip) = info.external_ip
        && !external_ip.is_empty()
    {
        let ext_port = if info.external_port != 0 {
            info.external_port
        } else {
            answered_port
        };
        snap.entry.remote_endpoint = EndpointSlot::new(external_ip, ext_port);
    }
    snap.server_info = Some(info.clone());
}

fn replace_slot_keeping_other(
    dst: &mut Option<EndpointSlot>,
    src: Option<EndpointSlot>,
    others: &mut Vec<String>,
) {
    if let Some(next) = src
        && let Some(prev) = dst.replace(next)
        && dst.as_ref().is_some_and(|n| n.address != prev.address)
        && !others.contains(&prev.address)
    {
        others.push(prev.address);
    }
}

pub(super) fn merge_entry_in_memory(dst: &mut HostEntry, src: HostEntry) {
    if !src.shared.name.is_empty() {
        dst.shared.name = src.shared.name;
    }
    if src.shared.mac.is_some() {
        dst.shared.mac = src.shared.mac;
    }
    replace_slot_keeping_other(&mut dst.local_endpoint, src.local_endpoint, &mut dst.shared.other_addresses);
    replace_slot_keeping_other(&mut dst.manual_endpoint, src.manual_endpoint, &mut dst.shared.other_addresses);
    if src.ipv6_endpoint.is_some() {
        dst.ipv6_endpoint = src.ipv6_endpoint;
    }
    replace_slot_keeping_other(&mut dst.remote_endpoint, src.remote_endpoint, &mut dst.shared.other_addresses);
    for addr in src.shared.other_addresses {
        if !addr.is_empty() && !dst.shared.other_addresses.contains(&addr) {
            dst.shared.other_addresses.push(addr);
        }
    }
    if src.pinned_endpoint.is_some() {
        dst.pinned_endpoint = src.pinned_endpoint;
    }
    if src.shared.server_certificate.is_some() {
        dst.shared.server_certificate = src.shared.server_certificate;
    }
    if src.shared.internet_play.is_some() {
        dst.shared.internet_play = src.shared.internet_play;
    }
    dst.sync_shared_from_slots();
}

impl HostManager {
    pub(super) fn commit_poll_outcome(
        &self,
        t: &mut ManagedTuple,
        uuid: &str,
        now_ms: u64,
        initial_state: HostReachState,
        outcome: PollCommitOutcome,
    ) -> bool {
        match outcome.winner {
            Some(win) => {
                t.poll_state.record_poll_success(now_ms);
                let prev = t.snapshot.clone();
                apply_server_info(&mut t.snapshot, &win.info, win.candidate.port, &win.candidate);
                t.snapshot.state = HostReachState::Online;
                t.snapshot.another_certificate = false;
                t.snapshot.unreachable = None;
                t.snapshot.needs_vpn_stun = outcome.needs_vpn_stun;
                if win.should_clear_cert {
                    t.snapshot.entry.shared.server_certificate = None;
                    t.snapshot.entry.shared.internet_play = None;
                    t.snapshot.paired = false;
                    internet::stop(uuid);
                    self.inner.pool.clear();
                } else {
                    t.snapshot.paired = win.info.paired;
                }
                if let Some(remote) = outcome.resolved_stun_remote {
                    t.snapshot.entry.remote_endpoint = Some(remote);
                    t.snapshot.needs_vpn_stun = false;
                }
                if !win.should_clear_cert
                    && let Some(ref raw_update) = outcome.internet_play_update
                {
                    t.snapshot.entry.shared.internet_play = raw_update.clone();
                    t.last_internet_read_ms = Some(now_ms);
                }
                if let Some(caps_json) = win.info.capabilities.to_versioned_json() {
                    let _ = self.inner.meta.save_capabilities(uuid, &caps_json);
                    t.snapshot.capabilities_json = Some(caps_json);
                }
                let reply = parse_internet_reply(t.snapshot.entry.shared.internet_play.as_deref());
                t.snapshot.internet_path = compute_internet_path(
                    t.snapshot.active_endpoint.as_ref(),
                    t.snapshot.entry.local_endpoint.as_ref(),
                    t.snapshot.entry.manual_endpoint.as_ref(),
                    reply.as_ref(),
                );
                t.snapshot.entry.sync_shared_from_slots();
                let _ = self.inner.store.replace_entry(t.snapshot.entry.clone());
                t.snapshot != prev
            }
            None => {
                let offline_now =
                    t.poll_state
                        .record_poll_failure(initial_state == HostReachState::Unknown);
                let prev_state = t.snapshot.state;
                let prev_cert_flag = t.snapshot.another_certificate;
                let prev_unreach = t.snapshot.unreachable.clone();
                let prev_path = t.snapshot.internet_path.clone();
                let prev_vpn_stun = t.snapshot.needs_vpn_stun;
                if offline_now {
                    t.snapshot.state = HostReachState::Offline;
                }
                t.snapshot.server_info = None;
                t.snapshot.another_certificate = outcome.another_certificate;
                t.snapshot.internet_path = None;
                t.snapshot.needs_vpn_stun = false;
                if let Some(msg) = outcome.unreachable_msg {
                    t.snapshot.unreachable = Some(msg);
                }
                t.snapshot.state != prev_state
                    || t.snapshot.another_certificate != prev_cert_flag
                    || t.snapshot.unreachable != prev_unreach
                    || t.snapshot.internet_path != prev_path
                    || t.snapshot.needs_vpn_stun != prev_vpn_stun
            }
        }
    }
}
