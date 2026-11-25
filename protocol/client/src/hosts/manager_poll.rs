//! Per-host polling execution (`HostManager::poll_host_inner`), committing poll state and
//! dropping mutexes before emitting snapshots or running post-commit STUN / internet refresh.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

use nightjar_policy::hosts::{
    HostReachState, is_internet_play_read_due, probe_plan_with_extras, punches_now,
};

use crate::http::{Error, HostApiError};

use super::manager::{HostManager, Inner, ManagedTuple};
use super::manager_ops::PollCommitOutcome;
use super::poll::{
    PollCancel, compute_internet_path, is_site_local_host, parallel_poll_direct_cancellable,
    parse_internet_reply, poll_over_the_internet_cancellable, refresh_internet_play_cancellable,
};

struct ActivePollGuard<'a>(&'a Inner);

impl Drop for ActivePollGuard<'_> {
    fn drop(&mut self) {
        let mut count = self
            .0
            .active_polls
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.0.active_polls_cv.notify_all();
        }
    }
}

struct ActiveCancelClearGuard {
    arc: Arc<Mutex<ManagedTuple>>,
    cancel: PollCancel,
}

impl Drop for ActiveCancelClearGuard {
    fn drop(&mut self) {
        let mut t = self.arc.lock().unwrap_or_else(|e| e.into_inner());
        if t.active_cancel
            .as_ref()
            .is_some_and(|c| c.ptr_eq(&self.cancel))
        {
            t.active_cancel = None;
        }
    }
}

impl HostManager {
    #[cfg(test)]
    pub(super) fn set_allow_loopback_stun_for_test(&self, allow: bool) {
        self.inner
            .allow_loopback_stun
            .store(allow, Ordering::Release);
    }

    fn stun_eligible_host(&self, host: &str) -> bool {
        #[cfg(test)]
        if self.inner.allow_loopback_stun.load(Ordering::Acquire) {
            return true;
        }
        is_site_local_host(host)
    }

    pub(super) fn poll_host_inner(
        &self,
        uuid: &str,
        force: bool,
        bg_stop: Option<&Arc<(Mutex<bool>, Condvar)>>,
    ) -> bool {
        let Some(arc) = self
            .inner
            .tuples
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(uuid)
            .cloned()
        else {
            return false;
        };
        let (network_lock, initial_state, snap_before, last_internet_read_ms) = {
            let t = arc.lock().unwrap_or_else(|e| e.into_inner());
            if t.removal_tombstone {
                return false;
            }
            if !force && !t.poll_state.is_poll_due(self.now_ms(), false) {
                return false;
            }
            (
                Arc::clone(&t.network_lock),
                t.snapshot.state,
                t.snapshot.clone(),
                t.last_internet_read_ms,
            )
        };

        let mut net_guard = Some(network_lock.lock().unwrap_or_else(|e| e.into_inner()));
        {
            let mut count = self
                .inner
                .active_polls
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *count += 1;
        }
        let _active_guard = ActivePollGuard(&self.inner);

        let pass_cancel = PollCancel::new();
        {
            let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
            let bg_stopped = bg_stop.is_some_and(|sig| {
                *sig.0.lock().unwrap_or_else(|e| e.into_inner())
                    || !self.inner.polling_active.load(Ordering::Acquire)
            });
            if t.removal_tombstone || bg_stopped || self.inner.shutdown_cancel.is_cancelled() {
                return false;
            }
            t.active_cancel = Some(pass_cancel.clone());
        }
        let _cancel_guard = ActiveCancelClearGuard {
            arc: Arc::clone(&arc),
            cancel: pass_cancel.clone(),
        };
        let is_cancelled =
            || pass_cancel.is_cancelled() || self.inner.shutdown_cancel.is_cancelled();

        let cert_der = snap_before.entry.server_certificate_der();
        let slots = snap_before.entry.slots();
        let active_port = snap_before.active_endpoint.as_ref().map(|e| e.port);
        let candidates = probe_plan_with_extras(
            &slots,
            &snap_before.entry.shared.other_addresses,
            snap_before.entry.shared.http_port,
            initial_state == HostReachState::Online,
            active_port,
            snap_before.entry.shared.https_port,
        );
        let likely_online = initial_state == HostReachState::Online;
        let direct = parallel_poll_direct_cancellable(
            &self.inner.pool,
            &candidates,
            Some(uuid),
            cert_der.as_deref(),
            likely_online,
            Some(&pass_cancel),
        );
        if is_cancelled() {
            return false;
        }

        let now_ms = self.now_ms();
        let mut winner = direct.winner;
        let mut unreachable_msg: Option<String> = None;
        let mut internet_another_cert = false;

        if winner.is_none()
            && slots.pinned.is_none()
            && let Some(reply_json) = snap_before.entry.shared.internet_play.as_deref()
        {
            let active_is_tunnel = snap_before
                .active_endpoint
                .as_ref()
                .is_some_and(|e| nightjar_policy::hosts::is_tunnel_address(&e.address));
            let allow_punch = punches_now(
                slots.local.is_some(),
                initial_state == HostReachState::Offline,
                active_is_tunnel,
            );
            match poll_over_the_internet_cancellable(
                &self.inner.pool,
                uuid,
                reply_json,
                Some(uuid),
                cert_der.as_deref(),
                &candidates,
                direct.tried,
                likely_online,
                allow_punch,
                Some(&pass_cancel),
            ) {
                Ok((_, win)) => winner = Some(win),
                Err(fail) => {
                    internet_another_cert = fail.another_certificate;
                    unreachable_msg = Some(fail.reason);
                }
            }
            if is_cancelled() {
                return false;
            }
        }

        let needs_vpn_stun = winner.as_ref().is_some_and(|w| {
            snap_before.entry.remote_endpoint.is_none()
                && w.info.external_ip.is_none()
                && self.stun_eligible_host(&w.candidate.address)
                && self.inner.vpn_active.load(Ordering::Acquire)
        });
        let committed_win = winner.clone();
        let mut changed_any = {
            let _rem_guard = self
                .inner
                .removal_lock
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
            if t.removal_tombstone || t.snapshot.entry.server_certificate_der() != cert_der {
                return false;
            }
            let changed = self.commit_poll_outcome(
                &mut t,
                uuid,
                now_ms,
                initial_state,
                PollCommitOutcome {
                    winner: committed_win,
                    resolved_stun_remote: None,
                    needs_vpn_stun,
                    internet_play_update: None,
                    another_certificate: direct.another_certificate || internet_another_cert,
                    unreachable_msg,
                },
            );
            let emitted = changed.then(|| t.snapshot.clone());
            drop(t);
            drop(_rem_guard);
            net_guard.take();
            if let Some(snap) = emitted {
                self.emit(&snap);
            }
            changed
        };

        let Some(ref win) = winner else {
            return changed_any;
        };
        let need_stun = snap_before.entry.remote_endpoint.is_none()
            && win.info.external_ip.is_none()
            && self.stun_eligible_host(&win.candidate.address)
            && !self.inner.vpn_active.load(Ordering::Acquire)
            && now_ms
                >= arc
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .next_stun_retry_ms;
        if need_stun && !is_cancelled() {
            let stun_net = Some(network_lock.lock().unwrap_or_else(|e| e.into_inner()));
            let resolver = self
                .inner
                .stun_resolver
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            let resolved = resolver(win.info.external_port);
            if !is_cancelled() {
                let _rem_guard = self
                    .inner
                    .removal_lock
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
                if !t.removal_tombstone && t.snapshot.entry.server_certificate_der() == cert_der {
                    if let Some(remote) = resolved {
                        t.stun_backoff_ms = 0;
                        t.next_stun_retry_ms = 0;
                        t.snapshot.entry.remote_endpoint = Some(remote);
                        t.snapshot.needs_vpn_stun = false;
                        t.snapshot.entry.sync_shared_from_slots();
                        let _ = self.inner.store.replace_entry(t.snapshot.entry.clone());
                        let snap = t.snapshot.clone();
                        drop(t);
                        drop(_rem_guard);
                        drop(stun_net);
                        self.emit(&snap);
                        changed_any = true;
                    } else {
                        let backoff = if t.stun_backoff_ms == 0 {
                            15_000
                        } else {
                            (t.stun_backoff_ms * 2).min(300_000)
                        };
                        t.stun_backoff_ms = backoff;
                        t.next_stun_retry_ms = now_ms.saturating_add(backoff);
                    }
                }
            }
        }
        if win.info.paired
            && !win.should_clear_cert
            && !is_cancelled()
            && is_internet_play_read_due(last_internet_read_ms, now_ms)
            && is_site_local_host(&win.candidate.address)
            && let Some(ref cert) = cert_der
        {
            let ip_net = Some(network_lock.lock().unwrap_or_else(|e| e.into_inner()));
            let ip_res = refresh_internet_play_cancellable(
                &self.inner.pool,
                &win.candidate.address,
                win.candidate.port,
                win.info.https_port,
                cert,
                &win.info.capabilities,
                Some(&pass_cancel),
            );
            let ip_update = match ip_res {
                Ok(raw) => Some(Some(raw)),
                Err(HostApiError::Host(Error::NotFound(_))) => Some(None),
                Err(_) => None,
            };
            if let Some(update) = ip_update
                && !is_cancelled()
            {
                let _rem_guard = self
                    .inner
                    .removal_lock
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let mut t = arc.lock().unwrap_or_else(|e| e.into_inner());
                if !t.removal_tombstone && t.snapshot.entry.server_certificate_der() == cert_der {
                    t.snapshot.entry.shared.internet_play = update;
                    t.last_internet_read_ms = Some(now_ms);
                    let reply =
                        parse_internet_reply(t.snapshot.entry.shared.internet_play.as_deref());
                    t.snapshot.internet_path = compute_internet_path(
                        t.snapshot.active_endpoint.as_ref(),
                        t.snapshot.entry.local_endpoint.as_ref(),
                        t.snapshot.entry.manual_endpoint.as_ref(),
                        reply.as_ref(),
                    );
                    t.snapshot.entry.sync_shared_from_slots();
                    let _ = self.inner.store.replace_entry(t.snapshot.entry.clone());
                    let snap = t.snapshot.clone();
                    drop(t);
                    drop(_rem_guard);
                    drop(ip_net);
                    self.emit(&snap);
                    changed_any = true;
                }
            }
        }
        changed_any
    }
}
