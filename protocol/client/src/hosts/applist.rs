//! Background app-list polling handle (`ApplistPollerHandle`) for an active paired host.
//!
//! Replaces the network/polling loop inside Android `ComputerManagerService.ApplistPoller`:
//! uses [`HostClientPool`] + [`accept_applist_response`] (suppressing transient empty lists up to
//! [`EMPTY_APPLIST_THRESHOLD`]), deduplicates identical XML payloads before notifying listeners,
//! and cleanly joins its worker thread on `stop` / `Drop`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use nightjar_policy::hosts::{accept_applist_response, next_applist_interval_ms};

use crate::http::{App, Evidence, HostCapabilities, HostClientCanceller, parse_app_list};

use super::client_pool::HostClientPool;

const STOP_JOIN_TIMEOUT: Duration = Duration::from_millis(150);

/// Parsed app list update emitted when a non-duplicate accepted XML payload arrives.
#[derive(Clone, Debug)]
pub struct ApplistUpdate {
    pub apps: Vec<App>,
    pub raw_xml: String,
}

/// Outcome reported for a poll pass (especially explicit requested refreshes).
#[derive(Clone, Debug)]
pub enum ApplistPollOutcome {
    Updated(ApplistUpdate),
    Unchanged,
    Failed(String),
}

struct DoneSignal(Arc<(Mutex<bool>, Condvar)>);

impl Drop for DoneSignal {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.0;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cvar.notify_all();
    }
}

/// Owned background app-list poller; stops cleanly on `stop` / `Drop` without stalling callers.
pub struct ApplistPollerHandle {
    state: Arc<(Mutex<PollerSignal>, Condvar)>,
    stopped: Arc<AtomicBool>,
    in_flight_cancel: Arc<Mutex<Option<HostClientCanceller>>>,
    done: Arc<(Mutex<bool>, Condvar)>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Default)]
struct PollerSignal {
    stop: bool,
    poll_now: bool,
}

impl ApplistPollerHandle {
    /// Starts polling `(address, http_port, https_port, cert_der)` immediately on a background
    /// thread and invokes `on_update` whenever a new accepted app list arrives.
    pub fn start(
        pool: Arc<HostClientPool>,
        address: String,
        http_port: u16,
        https_port: u16,
        cert_der: Vec<u8>,
        on_update: Box<dyn Fn(ApplistUpdate) + Send + Sync>,
    ) -> Self {
        Self::start_with_outcomes(
            pool,
            address,
            http_port,
            https_port,
            cert_der,
            Box::new(move |outcome| {
                if let ApplistPollOutcome::Updated(update) = outcome {
                    on_update(update);
                }
            }),
        )
    }

    /// Starts polling and invokes `on_outcome` with [`ApplistPollOutcome::Updated`],
    /// [`ApplistPollOutcome::Unchanged`], or [`ApplistPollOutcome::Failed`] on each poll pass.
    pub fn start_with_outcomes(
        pool: Arc<HostClientPool>,
        address: String,
        http_port: u16,
        https_port: u16,
        cert_der: Vec<u8>,
        on_outcome: Box<dyn Fn(ApplistPollOutcome) + Send + Sync>,
    ) -> Self {
        let state = Arc::new((Mutex::new(PollerSignal::default()), Condvar::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let in_flight_cancel = Arc::new(Mutex::new(None));
        let done = Arc::new((Mutex::new(false), Condvar::new()));

        let state_bg = Arc::clone(&state);
        let stopped_bg = Arc::clone(&stopped);
        let cancel_bg = Arc::clone(&in_flight_cancel);
        let done_bg = Arc::clone(&done);

        let handle = std::thread::Builder::new()
            .name("nj-applist-poll".into())
            .spawn(move || {
                let _done = DoneSignal(done_bg);
                let mut empty_count: u32 = 0;
                let mut received_app_list = false;
                let mut last_raw: Option<String> = None;

                loop {
                    if stopped_bg.load(Ordering::Acquire) {
                        break;
                    }
                    match pool.borrow(&address, http_port, https_port, Some(&cert_der)) {
                        Ok(client) => {
                            let canceller = client.canceller();
                            *cancel_bg.lock().unwrap_or_else(|e| e.into_inner()) =
                                Some(canceller.clone());
                            // Re-check after storing canceller so `stop()` cannot miss an in-flight call.
                            if stopped_bg.load(Ordering::Acquire) {
                                canceller.cancel();
                                *cancel_bg.lock().unwrap_or_else(|e| e.into_inner()) = None;
                                break;
                            }
                            let raw_res = client.app_list_raw();
                            *cancel_bg.lock().unwrap_or_else(|e| e.into_inner()) = None;

                            if stopped_bg.load(Ordering::Acquire) {
                                break;
                            }
                            match raw_res {
                                Ok(raw_xml) => {
                                    if Some(&raw_xml) == last_raw.as_ref() {
                                        on_outcome(ApplistPollOutcome::Unchanged);
                                    } else {
                                        match parse_app_list(
                                            &raw_xml,
                                            &HostCapabilities::from_evidence(Evidence::default()),
                                        ) {
                                            Ok(apps) => {
                                                let non_empty_xml = !raw_xml.is_empty();
                                                let accepted = accept_applist_response(
                                                    non_empty_xml,
                                                    apps.is_empty(),
                                                    &mut empty_count,
                                                );
                                                if accepted {
                                                    received_app_list = true;
                                                    last_raw = Some(raw_xml.clone());
                                                    on_outcome(ApplistPollOutcome::Updated(
                                                        ApplistUpdate { apps, raw_xml },
                                                    ));
                                                } else {
                                                    on_outcome(ApplistPollOutcome::Unchanged);
                                                }
                                            }
                                            Err(err) => {
                                                on_outcome(ApplistPollOutcome::Failed(err.to_string()));
                                            }
                                        }
                                    }
                                }
                                Err(err) => {
                                    on_outcome(ApplistPollOutcome::Failed(err.to_string()));
                                }
                            }
                        }
                        Err(err) => {
                            if !stopped_bg.load(Ordering::Acquire) {
                                on_outcome(ApplistPollOutcome::Failed(err.to_string()));
                            }
                        }
                    }

                    let delay = Duration::from_millis(next_applist_interval_ms(received_app_list));
                    let (lock, cvar) = &*state_bg;
                    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    if guard.stop || stopped_bg.load(Ordering::Acquire) {
                        break;
                    }
                    if guard.poll_now {
                        let mut guard = guard;
                        guard.poll_now = false;
                        continue;
                    }
                    let (mut guard, _) = cvar
                        .wait_timeout_while(guard, delay, |s| !s.stop && !s.poll_now)
                        .unwrap_or_else(|e| e.into_inner());
                    if guard.stop || stopped_bg.load(Ordering::Acquire) {
                        break;
                    }
                    guard.poll_now = false;
                }
            })
            .ok();

        if handle.is_none() {
            *done.0.lock().unwrap_or_else(|e| e.into_inner()) = true;
        }

        Self {
            state,
            stopped,
            in_flight_cancel,
            done,
            thread: Mutex::new(handle),
        }
    }

    /// Wakes the poller to run immediately.
    pub fn poll_now(&self) {
        let (lock, cvar) = &*self.state;
        let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        guard.poll_now = true;
        cvar.notify_all();
    }

    /// Cancels any active request, signals stop, and waits briefly for the worker to wind down
    /// without blocking callers across a full HTTP timeout.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        {
            let (lock, cvar) = &*self.state;
            let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            guard.stop = true;
            cvar.notify_all();
        }
        let canceller = self
            .in_flight_cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(cancel) = canceller {
            cancel.cancel();
        }
        let Some(handle) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() else {
            return;
        };
        let (lock, cvar) = &*self.done;
        let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = cvar
            .wait_timeout_while(guard, STOP_JOIN_TIMEOUT, |finished| !*finished)
            .unwrap_or_else(|e| e.into_inner());
        if *guard {
            let _ = handle.join();
        }
    }
}

impl Drop for ApplistPollerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}
