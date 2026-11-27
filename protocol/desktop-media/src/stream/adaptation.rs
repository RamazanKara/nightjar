//! Stream mode reconfiguration, adaptive bitrate, rate changes, and stream doctor controls.

use super::*;

impl Streaming {
    pub fn set_quality(&mut self, mode: QualityMode) {
        self.quality_mode = if self.facts.reconfigure { mode } else { QualityMode::Off };
        self.quality_policy = None;
        if self.quality_mode != QualityMode::Off {
            self.doctor.trial.clear();
            if !self.request_mode(self.quality_ceiling) { self.quality_mode = QualityMode::Off; return; }
            match quality::Quality::new(mode, self.quality_ceiling, self.settings.quality_min_short_edge,
                self.settings.quality_min_fps, self.started.elapsed().as_millis() as u64) {
                Ok(policy) => self.quality_policy = Some(policy),
                Err(error) => {
                    eprintln!("nightjar: automatic quality: {error}");
                    self.quality_mode = QualityMode::Off;
                    self.show_toast(tr(MSG_AUTO_QUALITY_NO_MODE));
                    return;
                }
            }
            if self.quality_takes_bitrate() { self.enable_adaptive_bitrate(true); }
        }
    }

    pub(super) fn quality_takes_bitrate(&self) -> bool {
        nightjar_policy::quality::quality_takes_bitrate(
            self.settings.lower_bitrate.keeps()
                || self.rate_change.as_ref().is_some_and(|rates| rates.kept()),
            self.facts.role == AppRole::Stream,
            self.adaptive_bitrate || self.bitrate_busy || self.host_abr_start.is_some(),
            self.client_hint_active(),
            self.plan.bitrate_before_manual.is_some(),
            self.facts.link_notice,
        )
    }

    /// Whether the PC's own `0x5538` link-rate / congestion controllers steer this stream's
    /// bitrate and still have bitrate headroom before automatic quality lowers resolution or FPS.
    pub(super) fn host_link_has_room(&self) -> bool {
        host_link_room_for_stream(
            self.facts.link_notice,
            self.settings.lower_bitrate,
            self.rate_change.as_ref().is_some_and(|rates| rates.kept()),
            self.plan.bitrate_before_manual.is_some(),
            self.adaptive_bitrate || self.host_abr_start.is_some(),
            self.client_hint_active(),
            self.link_notice.as_ref(),
        )
    }

    /// Asks the PC for `request`; false when it could not be sent (a toast says why).
    pub fn set_mode(&mut self, request: Reconfigure) -> bool {
        if !self.request_mode(request) {
            return false;
        }
        self.quality_ceiling = request;
        self.settings.width = u32::from(request.width);
        self.settings.height = u32::from(request.height);
        self.settings.fps = request.fps_millihz.div_ceil(1000);
        self.plan.settings = self.settings.clone();
        true
    }

    pub(super) fn request_mode(&mut self, request: Reconfigure) -> bool {
        if !self.facts.reconfigure || Reconfigure::decode(&request.encode()).is_none() {
            self.show_toast(tr(MSG_CANNOT_USE_MODE));
            return false;
        }
        if let Err(error) = self.session.send(ClientMessage::Reconfigure(request)) {
            self.show_toast(trf(MSG_COULD_NOT_CHANGE_STREAM, &[&error]));
            return false;
        }
        // Before any field takes the asked mode: a request the host does not apply goes back to
        // this.
        if self.mode_before.is_none() {
            self.mode_before = Some(ModeBefore::of((self.facts.width, self.facts.height, self.facts.fps), self.shared.fps_millihz.load(Ordering::Relaxed), &self.settings));
        }
        self.requested_mode = request;
        self.stream_sizes.ask((i32::from(request.width), i32::from(request.height)));
        self.pending_mode = Some((request, Instant::now()));
        self.late_mode = None;
        self.fps_evidence = 0;
        self.facts.fps = request.fps_millihz.div_ceil(1000) as i32;
        self.shared.fps_millihz.store(request.fps_millihz, Ordering::Relaxed);
        *lock(&self.shared.decode_window) = pipeline::DecodeWindow::default();
        self.decode_window_us = session::now_us();
        self.decode_generation += 1;
        if let Some(rate) = &mut self.bitrate_policy {
            rate.set_mode(u32::from(request.width), u32::from(request.height), request.fps_millihz.div_ceil(1000));
        }
        if let Some(rates) = &mut self.rate_change { rates.fps_set(request.fps_millihz); }
        true
    }

    /// A new rate from the host: whether the stream has refreshes to spare, and what the hold learned.
    pub(super) fn stream_rate_changed(&mut self, fps: u32) {
        self.pacer.set_fps(fps);
        self.skipped.restart();
    }

    /// Applies a mode the PC has shown it took: a confirmation, or the size a timed-out request
    /// asked for arriving after the timeout. Everything the timeout put back is set again, together
    /// with the pacer and the menus.
    pub(super) fn apply_mode(&mut self, mode: Reconfigure) {
        self.facts.fps = mode.fps_millihz.div_ceil(1000) as i32;
        self.facts.width = i32::from(mode.width);
        self.facts.height = i32::from(mode.height);
        self.shared.fps_millihz.store(mode.fps_millihz, Ordering::Relaxed);
        self.settings.width = u32::from(mode.width);
        self.settings.height = u32::from(mode.height);
        self.settings.fps = mode.fps_millihz.div_ceil(1000);
        self.plan.settings = self.settings.clone();
        self.requested_mode = mode;
        self.decode_generation += 1;
        self.decode_window_us = session::now_us();
        *lock(&self.shared.decode_window) = pipeline::DecodeWindow::default();
        if let Some(rate) = &mut self.bitrate_policy {
            rate.set_mode(u32::from(mode.width), u32::from(mode.height), mode.fps_millihz.div_ceil(1000));
        }
        if let Some(rates) = &mut self.rate_change { rates.fps_set(mode.fps_millihz); }
        self.stream_rate_changed(mode.fps_millihz.div_ceil(1000).max(1));
    }

    /// One picture's RTP interval toward confirming an fps-only reconfigure: the host's own frame
    /// timestamps show it applied the rate. A request that changes the size is confirmed by the
    /// pictures' size instead (`StreamSizes::arrive`); a request that only changes the rate has no
    /// other evidence, and without this the pacing stays at the old frame time for the stream.
    pub(super) fn fps_evidence_step(&mut self, ticks: i32) {
        let Some((mode, _)) = self.pending_mode else { return };
        if (i32::from(mode.width), i32::from(mode.height)) != (self.facts.width, self.facts.height) {
            return;
        }
        if ticks <= 0 || !shows_rate(ticks as u32, mode.fps_millihz) {
            self.fps_evidence = 0;
            return;
        }
        self.fps_evidence += 1;
        if self.fps_evidence < FPS_EVIDENCE_FRAMES {
            return;
        }
        self.fps_evidence = 0;
        self.mode_before = None;
        self.late_mode = None;
        if let Some((mode, _)) = self.pending_mode.take() {
            self.stream_rate_changed(mode.fps_millihz.div_ceil(1000));
        }
    }

    pub fn follow_display(&mut self, display: Display) {
        self.pacer.set_min_interval_us(display.refresh_interval_us().unwrap_or(0));
        if display.refresh_hz != self.plan.display.refresh_hz { self.skipped.restart(); }
        self.plan.display = display;
        if !self.facts.reconfigure || !self.display_change.is_some_and(|at| at.elapsed() >= Duration::from_millis(300)) { return; }
        self.display_change = None;
        let (width, height) = if self.settings.width == 0 || self.settings.height == 0 {
            let (w, h, _, _) = crate::stream::negotiate::stream_mode(&self.settings, display);
            ((w.clamp(256, 8192) + 1) & !1, (h.clamp(256, 8192) + 1) & !1)
        } else { (u32::from(self.quality_ceiling.width), u32::from(self.quality_ceiling.height)) };
        let fps = if self.settings.fps == 0 { (display.refresh_hz * 1000.0).round() as u32 } else { self.quality_ceiling.fps_millihz };
        let Some(request) = quality::target(width, height, fps) else { return; };
        if request == self.quality_ceiling { return; }
        if self.quality_mode == QualityMode::Off {
            if self.request_mode(request) {
                self.quality_ceiling = request;
            }
        } else if let Some(policy) = &mut self.quality_policy {
            let started = match policy.rebase(request, self.started.elapsed().as_millis() as u64) {
                Ok(Some(request)) => self.request_mode(request),
                other => {
                    eprintln!("nightjar: automatic quality: no request for {request:?}: {other:?}");
                    self.show_toast(tr(MSG_CANNOT_USE_MODE));
                    false
                }
            };
            if started {
                self.quality_ceiling = request;
            } else {
                self.quality_policy = None;
                self.quality_mode = QualityMode::Off;
            }
        }
    }

    pub fn set_bitrate(&mut self) {
        if !self.facts.runtime_bitrate || self.bitrate_busy { return; }
        let client = lock(&self.shared.host_client).clone().expect("connected HTTP client");
        let shared = self.shared.clone();
        self.bitrate_kbps = capped_bitrate(self.bitrate_kbps, self.plan.bitrate_limit_kbps);
        let bitrate = self.bitrate_kbps;
        self.bitrate_busy = true;
        std::thread::spawn(move || {
            let result = client.server_info_https().map_err(|e| e.to_string()).and_then(|info| {
                if !info.capabilities.has(Feature::RuntimeBitrate) {
                    return Err("The PC does not allow bitrate changes.".into());
                }
                client.set_bitrate(bitrate).map_err(|e| e.to_string())
            });
            shared.notify(Notice::Bitrate { requested: bitrate, result });
        });
    }

    /// Sends the stream menu's rate change; the host's answer comes back as a toast.
    pub fn change_rates(&mut self) {
        let Some(rates) = &mut self.rate_change else { return };
        match rates.menu_change(self.started.elapsed().as_millis() as u64) {
            Ok(request) => self.send_rate_change(request),
            Err(toast) => self.show_toast(toast),
        }
    }

    pub(super) fn send_rate_change(&mut self, request: RateChangeRequest) {
        if let Err(error) = self.session.send(ClientMessage::RateChange(request)) {
            let toast = self.rate_change.as_mut().expect("rate changes offered").send_failed(&request, &error);
            self.show_toast(toast);
        }
    }

    /// The query for the stream's bitrate as the PC counts it; one that could not go out goes
    /// once more a second later (`RateChange::query_again`), quietly.
    pub(super) fn send_rate_query(&mut self, request: RateChangeRequest) {
        if let Err(error) = self.session.send(ClientMessage::RateChange(request)) {
            let now = self.started.elapsed().as_millis() as u64;
            if let Some(rates) = &mut self.rate_change { rates.query_unsent(&request, &error, now); }
        }
    }

    pub(super) fn rate_change_result(&mut self, result: &RateChangeResult) {
        let Some(rates) = &mut self.rate_change else { return };
        let answer = rates.result(result, self.started.elapsed().as_millis() as u64);
        if let Some(kbps) = answer.bitrate_kbps {
            self.facts.bitrate_kbps = i32::try_from(kbps).unwrap_or(i32::MAX);
            self.bitrate_kbps = kbps;
            let now = self.started.elapsed().as_millis() as u64;
            if let Some(policy) = &mut self.bitrate_policy { policy.applied(kbps, kbps, now); }
            // The player's own rate from the menu, which a reconnect keeps.
            if answer.by_hand { self.keep_manual_bitrate(kbps); }
        }
        if let Some(fps_millihz) = answer.fps_millihz {
            self.facts.fps = fps_millihz.div_ceil(1000) as i32;
            self.stream_rate_changed(self.facts.fps as u32);
            self.shared.fps_millihz.store(fps_millihz, Ordering::Relaxed);
            *lock(&self.shared.decode_window) = pipeline::DecodeWindow::default();
            self.decode_window_us = session::now_us();
            self.decode_generation += 1;
            self.requested_mode.fps_millihz = fps_millihz;
            if let Some(rate) = &mut self.bitrate_policy {
                let mode = self.requested_mode;
                rate.set_mode(u32::from(mode.width), u32::from(mode.height), fps_millihz.div_ceil(1000));
            }
        }
        if let Some(toast) = answer.toast { self.show_toast(toast); }
    }

    /// Which bitrate and rate controls the stream menu shows.
    pub fn menu_controls(&self) -> rate_change::MenuControls {
        rate_change::menu_controls(self.facts.runtime_bitrate, self.facts.reconfigure, self.rate_change.is_some())
    }

    /// The client-hint controller steers this stream's bitrate in place of the PC's adaptive
    /// bitrate.
    pub fn client_hint_active(&self) -> bool {
        self.rate_change.as_ref().is_some_and(|rates| rates.hint_active())
    }

    /// What the stream menu's Keep my bitrate shows; `None` hides it, on a PC that did not take
    /// this stream's rate changes. On while the PC keeps the bitrate (after the takeover, the
    /// switch or a bitrate the player chose in the menu) and no controller of this device steers it.
    pub fn bitrate_kept(&self) -> Option<bool> {
        rate_change::keep_switch(self.rate_change.as_ref(), self.adaptive_bitrate)
    }

    /// Keep my bitrate turned on or off in the stream menu. On sends the bitrate the player last
    /// chose in this stream, else the stream's own, as one bitrate change (nothing when the PC
    /// already keeps it) and stops this device's own lowering. Off sends nothing: the PC has no
    /// message that takes the bitrate back, so the stream keeps it and the notice says the choice
    /// applies from the next stream. Either is kept as Lower bitrate's choice in the main
    /// settings, as the overlay's switch is (`take_keep_change`).
    pub fn set_bitrate_kept(&mut self, keep: bool) {
        if self.rate_change.is_none() { return; }
        let now = self.started.elapsed().as_millis() as u64;
        if keep {
            let request = self.rate_change.as_mut().and_then(|rates| rates.keep_now(now));
            if self.adaptive_bitrate || self.host_abr_start.is_some() { self.enable_adaptive_bitrate(false); }
            if let Some(request) = request { self.send_rate_change(request); }
        } else {
            if let Some(rates) = &mut self.rate_change { rates.release(); }
            eprintln!("nightjar: keep my bitrate turned off in the stream menu: The PC decides from the next stream; this stream keeps its bitrate");
            self.show_toast(tr(MSG_PC_DECIDES_NEXT_STREAM));
        }
        rate_change::store_keep(&mut self.settings, keep);
        // A reconnect starts from the plan: it sends the takeover again on the PC's new session.
        rate_change::store_keep(&mut self.plan.settings, keep);
        self.keep_change = Some(keep);
    }

    /// Keep my bitrate as the stream menu last turned it, once: the main settings keep it as
    /// Lower bitrate's choice. Only that value, since the stream's may be a PC's, game's or
    /// profile's own (as `take_overlay_change`).
    pub fn take_keep_change(&mut self) -> Option<bool> {
        self.keep_change.take()
    }

    /// Automatic bitrate, automatic quality or the client-hint controller owns the bitrate.
    pub(super) fn automatic_bitrate(&self) -> bool {
        bitrate_automatic(self.adaptive_bitrate, self.quality_mode, self.rate_change.as_ref())
    }

    pub fn enable_adaptive_bitrate(&mut self, enabled: bool) {
        if enabled && self.bitrate_busy { return; }
        if enabled && self.client_hint_active() {
            eprintln!("nightjar: client-hint bitrate adapts this stream; the PC's adaptive bitrate stays off");
            return;
        }
        if disable_abr_state(
            &mut self.bitrate_policy,
            &mut self.adaptive_bitrate,
            &mut self.host_abr,
            &mut self.host_abr_start,
        ) {
            self.spawn_host_abr_stop();
        }
        if !enabled {
            return;
        }
        self.forget_manual_bitrate();
        self.adaptive_bitrate = self.facts.runtime_bitrate && self.facts.role == AppRole::Stream;
        if !self.adaptive_bitrate {
            // A PC without Rubylight's runtime bitrate may run its own controller (Foundation).
            if self.facts.role == AppRole::Stream { self.start_host_abr(); }
            return;
        }
        self.doctor.trial.clear();
        let client = lock(&self.shared.host_client).clone().expect("connected HTTP client");
        let shared = self.shared.clone();
        self.bitrate_busy = true;
        let limit = self.plan.bitrate_limit_kbps;
        // After an automatic reconnect the first start is from the rate it had cut to, once; the
        // stream's start bitrate stays the ceiling.
        let resume = self.plan.resume_bitrate_kbps.take();
        std::thread::spawn(move || {
            let result = client.server_info_https().map_err(|e| e.to_string()).and_then(|info| {
                if !info.capabilities.has(Feature::RuntimeBitrate) || info.encoder_bitrate_kbps == 0 || info.initial_encoder_bitrate_kbps == 0 {
                    return Err("The PC did not provide an encoder budget for automatic bitrate.".into());
                }
                let start = automatic_bitrate_start(info.encoder_bitrate_kbps, resume);
                let applied = client.set_bitrate(capped_bitrate(start, limit)).map_err(|e| e.to_string())?;
                Ok((capped_bitrate(info.initial_encoder_bitrate_kbps.max(applied), limit), applied))
            });
            shared.notify(Notice::AdaptiveBitrate(result));
        });
    }

    /// Asks a Foundation PC to run its own bitrate controller up to the stream's bitrate, as
    /// Moonlight V+ PC's `startSunshineAbr` does. Other PCs answer 404: nothing changes there.
    /// The start may outlive the stream (only `finish` ends it while in flight: `bitrate_busy` holds
    /// the setting): its own client is not cancelled by `finish`, and `AbrStart` decides who turns
    /// the PC's controller off.
    fn start_host_abr(&mut self) {
        let Some(client) = lock(&self.shared.host_client).as_ref().map(|c| c.detached()) else { return };
        let shared = self.shared.clone();
        let max_kbps = self.facts.bitrate_kbps.max(0) as u32;
        let start = host_abr::AbrStart::default();
        self.host_abr_start = Some(start.clone());
        self.bitrate_busy = true;
        let stopping = self.host_abr_stop.take();
        std::thread::spawn(move || {
            // An off sent just before must not land after this on.
            if let Some(stop) = stopping { stop.wait(); }
            let result = match client.abr_capabilities() {
                Ok(caps) if caps.supported => client
                    .configure_abr(&AbrRequest::balanced_up_to(max_kbps))
                    .map(Some)
                    .map_err(|e| trf(MSG_HOST_ABR_START_FAILED, &[&e])),
                Ok(_) => Ok(None),
                Err(e) => Err(trf(MSG_HOST_ABR_START_FAILED, &[&e])),
            };
            if matches!(result, Ok(Some(_))) && !start.turned_on() {
                // The stream ended or ABR was turned off while the PC turned it on: nobody else will turn it off.
                if let Err(e) = client.configure_abr(&AbrRequest::off()) {
                    eprintln!("nightjar: the PC's automatic bitrate did not stop: {e}");
                }
                // Still clear `bitrate_busy` on the stream if it is running.
                shared.notify(Notice::HostAbr(Ok(None)));
                return;
            }
            shared.notify(Notice::HostAbr(result));
        });
    }

    /// The request of the stream before that turns the PC's controller off, while it may still
    /// be in flight: a start here waits for it, and `finish` hands it on when none started.
    pub fn follow_abr_stop(&mut self, stop: Option<host_abr::AbrStop>) {
        if self.host_abr_stop.is_none() { self.host_abr_stop = stop; }
    }

    /// Turns a Foundation PC's controller off again (V+ PC's `stopSunshineAbr`), on its own
    /// thread and client: `finish` cancels the stream's requests right after, and a dead network
    /// would hold the window for the request's timeouts.
    pub(super) fn stop_host_abr(&mut self) {
        if self.host_abr.take().is_some() { self.adaptive_bitrate = false; }
        // On, though its answer may not have come yet; a start still in flight turns it off itself.
        if !self.host_abr_start.take().is_some_and(|start| start.end()) { return; }
        self.spawn_host_abr_stop();
    }

    fn spawn_host_abr_stop(&mut self) {
        let Some(client) = lock(&self.shared.host_client).as_ref().map(|c| c.detached()) else { return };
        self.host_abr_stop = Some(host_abr::AbrStop::spawn(move || {
            if let Err(e) = client.configure_abr(&AbrRequest::off()) {
                eprintln!("nightjar: the PC's automatic bitrate did not stop: {e}");
            }
        }));
    }

    /// The trial the doctor offers with this report: none while automatic bitrate or quality owns
    /// the bitrate, the host cannot change it mid-stream or another bitrate request runs.
    pub(super) fn trial_offer(&self, report: &nightjar_policy::health::Report) -> TrialOffer {
        let automatic = self.automatic_bitrate();
        self.doctor.trial.offer(report.can_suggest_bitrate(), self.facts.bitrate_kbps.max(0) as u32, self.facts.runtime_bitrate, automatic, self.bitrate_busy)
    }

    /// The stream doctor's report for the stream menu, and the trial it offers.
    pub fn health(&mut self) -> (String, TrialOffer) {
        let report = self.doctor.report(self.started.elapsed().as_millis() as u64);
        let offer = self.trial_offer(&report);
        let mut text = doctor::report_text(&report);
        let packed = doctor::packed_hint(&report, *lock(&self.shared.packed_from_memory));
        for hint in [packed, self.doctor.receive_buffer_hint()].into_iter().flatten() {
            text.push_str("\n\n");
            text.push_str(&hint);
        }
        if offer != TrialOffer::None {
            text.push_str("\n\n");
            text.push_str(doctor::trial_help());
        }
        (text, offer)
    }

    /// Runs the step `offer` names if the doctor still offers it: lowers the host's live encoder
    /// bitrate by a fifth, or goes back to the bitrate before the trial.
    pub fn health_trial(&mut self, offer: TrialOffer) {
        let report = self.doctor.report(self.started.elapsed().as_millis() as u64);
        if offer == TrialOffer::None || self.trial_offer(&report) != offer { return; }
        let client = lock(&self.shared.host_client).clone().expect("connected HTTP client");
        let shared = self.shared.clone();
        let trial = self.doctor.trial;
        self.bitrate_busy = true;
        std::thread::spawn(move || {
            let live = || {
                let info = client.server_info_https().map_err(|e| e.to_string())?;
                if !info.capabilities.has(Feature::RuntimeBitrate) {
                    return Err("The PC does not allow bitrate changes.".to_owned());
                }
                Ok(info.encoder_bitrate_kbps)
            };
            let outcome = doctor::run_trial(trial, offer == TrialOffer::Undo, live, |kbps| client.set_bitrate(kbps).map_err(|e| e.to_string()));
            shared.notify(Notice::HealthTrial(outcome));
        });
    }

    /// Copies the doctor's report to this device's clipboard.
    pub fn copy_health(&mut self) {
        let text = self.health().0;
        self.copy_report(&text);
    }

    /// Copies a report (the Stream Doctor card's, `doctor_card::report`) to this device's clipboard.
    pub fn copy_report(&mut self, text: &str) {
        let text = std::ffi::CString::new(text);
        if text.as_ref().is_ok_and(|text| unsafe { SDL_SetClipboardText(text.as_ptr()) }) {
            self.show_toast(tr(MSG_REPORT_COPIED));
        } else {
            self.show_toast(tr(MSG_REPORT_COPY_FAILED));
        }
    }

    /// The report for the screen after the stream when it found something; the log gets it either way.
    pub fn health_summary(&mut self) -> Option<String> {
        if self.facts.role == AppRole::InputOnly { return None; }
        let report = self.doctor.report(self.started.elapsed().as_millis() as u64);
        eprintln!("nightjar: stream health: {}", doctor::log_line(&report));
        let packed = doctor::packed_hint(&report, *lock(&self.shared.packed_from_memory));
        if let Some(hint) = &packed {
            eprintln!("nightjar: {hint}");
        }
        // "Hide connection warnings" leaves the connection out, the receive buffer's note on lost video with it.
        let hide_connection = self.settings.hide_connection_warnings;
        let buffer = if hide_connection { None } else { self.doctor.receive_buffer_hint() };
        doctor::summary(&report, hide_connection).map(|summary| {
            [packed, buffer].into_iter().flatten().fold(summary, |text, hint| format!("{text}\n\n{hint}"))
        })
    }
}

/// Clears client and host adaptive-bitrate state; returns `true` if a Foundation host ABR
/// controller had already turned on and needs an asynchronous `AbrRequest::off()` stop.
fn disable_abr_state(
    bitrate_policy: &mut Option<bitrate::Bitrate>,
    adaptive_bitrate: &mut bool,
    host_abr: &mut Option<host_abr::HostAbr>,
    host_abr_start: &mut Option<host_abr::AbrStart>,
) -> bool {
    *bitrate_policy = None;
    *host_abr = None;
    *adaptive_bitrate = false;
    host_abr_start.take().is_some_and(|start| start.end())
}

fn host_link_room_for_stream(
    host_link_notice: bool,
    lower_bitrate: nightjar_policy::choices::LowerBitrate,
    kept: bool,
    manual_bitrate: bool,
    busy_on: bool,
    hint_on: bool,
    notice: Option<&LinkNotice>,
) -> bool {
    nightjar_policy::quality::host_steers_link_bitrate(
        host_link_notice,
        lower_bitrate == nightjar_policy::choices::LowerBitrate::Off,
        kept,
        manual_bitrate,
        busy_on,
        hint_on,
    ) && match notice {
        Some(n) => nightjar_policy::quality::host_link_has_room(n.state, n.current_kbps, n.ceiling_kbps),
        None => nightjar_policy::quality::host_link_has_room(LinkState::Full, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nightjar_policy::choices::LowerBitrate;
    use rubylight_protocol::control::LinkReason;

    /// Disabling adaptive bitrate (including from `set_bitrate_kept(true)`) must succeed even
    /// while a start request is still in flight (`bitrate_busy == true`).
    #[test]
    fn disabling_abr_for_keep_bitrate_proceeds_while_bitrate_busy() {
        let mut rates = rate_change::RateChange::new(20_000, 60_000, false, 0, 0);
        let mut adaptive_bitrate = true;
        let mut bitrate_policy = Some(bitrate::Bitrate::new(VIDEO_FORMAT_H265, 20_000, 20_000, 60, (1920, 1080), 0));
        let mut host_abr = None;
        let mut host_abr_start: Option<host_abr::AbrStart> = None;
        let bitrate_busy = true;

        // `set_bitrate_kept(true)` path while `bitrate_busy == true`.
        let request = rates.keep_now(1_000);
        assert!(request.is_some(), "keep_now sends takeover request");
        assert!(bitrate_busy, "in-flight ABR start must not block disabling");
        if adaptive_bitrate || host_abr_start.is_some() {
            let needs_stop = disable_abr_state(&mut bitrate_policy, &mut adaptive_bitrate, &mut host_abr, &mut host_abr_start);
            assert!(!needs_stop);
        }
        assert!(!adaptive_bitrate);
        assert!(bitrate_policy.is_none());
        assert_eq!(rate_change::keep_switch(Some(&rates), adaptive_bitrate), Some(true));
    }

    #[test]
    fn disabling_in_flight_host_abr_cancels_pending_start_or_requests_stop() {
        // Pending start (not yet turned on): `disable_abr_state` cancels it so the worker turns it off itself.
        let pending = host_abr::AbrStart::default();
        let mut adaptive = false;
        let mut policy = None;
        let mut host_abr = None;
        let mut host_start = Some(pending.clone());
        assert!(!disable_abr_state(&mut policy, &mut adaptive, &mut host_abr, &mut host_start));
        assert!(host_start.is_none());
        assert!(!pending.turned_on(), "worker sees cancelled start and sends AbrRequest::off()");

        // Start already turned on on the host (notice queued, not yet drained): `disable_abr_state` requests stop.
        let active = host_abr::AbrStart::default();
        assert!(active.turned_on());
        let mut host_start = Some(active);
        assert!(disable_abr_state(&mut policy, &mut adaptive, &mut host_abr, &mut host_start));
        assert!(host_start.is_none());
    }

    #[test]
    fn host_link_notice_steers_room_only_under_pc_decides_without_client_takeover() {
        let limited_above = LinkNotice {
            state: LinkState::Limited,
            reason: LinkReason::Network,
            ceiling_kbps: 50_000,
            current_kbps: 40_000,
        };
        let limited_cut = LinkNotice {
            current_kbps: 32_500,
            ..limited_above
        };
        let floor = LinkNotice {
            state: LinkState::Floor,
            current_kbps: 5_000,
            ..limited_above
        };

        assert!(host_link_room_for_stream(true, LowerBitrate::Off, false, false, false, false, None));
        assert!(host_link_room_for_stream(true, LowerBitrate::Off, false, false, false, false, Some(&limited_above)));
        assert!(!host_link_room_for_stream(true, LowerBitrate::Off, false, false, false, false, Some(&limited_cut)));
        assert!(!host_link_room_for_stream(true, LowerBitrate::Off, false, false, false, false, Some(&floor)));

        // Non-0x5538 hosts, "sooner", "keep", Keep my bitrate, manual bitrate, or active client ABR never claim host link room.
        assert!(!host_link_room_for_stream(false, LowerBitrate::Off, false, false, false, false, None));
        assert!(!host_link_room_for_stream(true, LowerBitrate::WhenFramesLost, false, false, false, false, None));
        assert!(!host_link_room_for_stream(true, LowerBitrate::Keep, false, false, false, false, None));
        assert!(!host_link_room_for_stream(true, LowerBitrate::Off, true, false, false, false, None));
        assert!(!host_link_room_for_stream(true, LowerBitrate::Off, false, true, false, false, None));
        assert!(!host_link_room_for_stream(true, LowerBitrate::Off, false, false, true, false, None));
        assert!(!host_link_room_for_stream(true, LowerBitrate::Off, false, false, false, true, None));
    }
}
