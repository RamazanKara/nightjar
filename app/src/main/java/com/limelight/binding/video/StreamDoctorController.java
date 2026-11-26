package com.limelight.binding.video;

import com.limelight.LimeLog;
import com.limelight.nativeapi.NativeApiException;
import com.limelight.nativeapi.NativeStreamDoctor;
import com.limelight.nvstream.jni.Json;
import com.limelight.utils.StreamHealth;

/**
 * Stream Doctor 2.0 on Android: once a second while its card shows, the shared verdict
 * ({@code nightjar_policy::doctor_v2} through {@code NightjarPolicyJni.doctorDiagnose}) from this device's stream report
 * and, on a host that lists {@code stream.doctor_v2}, the host's own stage report ({@code GET /stream/doctor}, read on a
 * worker, never on the UI thread). Nothing is read or sent while the card is closed or the host lacks the feature.
 *
 * <p>Every remedy is for this stream only, with an Undo, and nothing here writes a setting. The bitrate step-down goes
 * through the host's doctor where it offers one (undone by its trial id while the host says it can be), else through the
 * classic trial. The resolution step-down asks for the smaller mode in place (0x5532) at the same frame rate and keeps the
 * mode before it for Undo until something else changes the size or the stream reconnects. A refusal shows the host's
 * words, and a mode change the host did not make is said; nothing is left half applied. An upscaler the verdict names is
 * advice for the next stream.
 */
public final class StreamDoctorController {
    /** How often the open card looks again. */
    public static final long POLL_MS = 1000;
    /** A mode change the stream has not shown this long after asking was not made. */
    static final long RESIZE_CONFIRM_MS = 5000;
    public static final int RESIZE_OK = 0, RESIZE_NOT_SENT = 1, RESIZE_KEPT = 2;

    /** What the controller needs from the stream. Everything but {@link #fetch} and {@link #apply} runs on the UI thread. */
    public interface Host {
        /** The host lists {@code stream.doctor_v2} for this stream. */
        boolean offered();

        /** GET /stream/doctor; blocks. */
        NativeStreamDoctor fetch() throws NativeApiException;

        /** POST /stream/doctor/apply for the bitrate step-down, or its undo; blocks. */
        NativeStreamDoctor apply(boolean undo, String trialId) throws NativeApiException;

        /** Runs {@code task} off the UI thread; false once the stream is over. */
        boolean background(Runnable task);

        /** Runs {@code task} on the UI thread. */
        void ui(Runnable task);

        void later(Runnable task, long delayMs);

        void cancel(Runnable task);

        long nowMs();

        /** The stream's facts now; null once it is over. */
        Facts facts();

        /** {@code NightjarPolicyJni.doctorDiagnose}. */
        String diagnose(String request);

        /** The running stream; a new one (a reconnect) drops what this one tried. */
        Object stream();

        /** The classic bitrate trial, or its undo: it runs, and says what became of it, on its own. */
        void classicTrial(boolean undo);

        /** Asks for this mode in place at the stream's frame rate; false when nothing was sent. */
        boolean resize(int width, int height);

        /** The host's doctor set the bitrate to {@code kbps}. */
        void hostBitrateApplied(int kbps);

        /** A refusal in the host's words. */
        String describe(NativeApiException error);
    }

    /** Shows each look; the card stays as it was until the next one. */
    public interface View {
        void show(Card card);
    }

    /** The stream and this device as they are now. */
    public static final class Facts {
        public StreamHealth.Report report;
        public int width, height, fps, screenWidth, screenHeight;
        public boolean hdr, tenBit, pyrowave;
        /** The Upscaling setting: {@code off}, {@code sgsr1} or {@code fsr1}. */
        public String upscaler = "off";
        /** {@code MoonBridge.getVideoReceiveBufferInfo()}: asked in the high and granted in the low 32 bits; -1 unknown. */
        public long receiveBuffer = -1;
        /** Nothing automatic owns the bitrate: automatic bitrate on either side and automatic quality are off. */
        public boolean bitrateManual;
        /** The classic trial's own conditions hold: the host changes the bitrate live, above the floor, nothing in flight. */
        public boolean classicBitrate;
        public int currentKbps;
        /** The bitrate before the classic trial while it is in force, else 0. */
        public int classicUndoKbps;
        /** The stream can change its mode in place now. */
        public boolean resize;
        /** The classic report, for the technical measurements. */
        public String technical = "";
    }

    /** One look, as the card shows it. */
    public static final class Card {
        /** Null when the verdict failed: the card shows the measurements only. */
        public final StreamDoctorVerdict verdict;
        /** The host's last snapshot; null without the feature or before it answered. */
        public final NativeStreamDoctor host;
        /** Another device owns the stream: the host's remedies are hidden. */
        public final boolean guest;
        /** A request runs: the buttons wait. */
        public final boolean busy;
        /** The host's measurements could not be read, in its words; null when they were. */
        public final String hostError;
        /** The last request's refusal in the host's words, or null. */
        public final String failure;
        /** The bitrate an Undo returns to, or 0. */
        public final int undoKbps;
        /** The host's trial can no longer be undone: something else changed the bitrate since. */
        public final boolean undoReplaced;
        /** The mode an Undo returns to, {width, height}, or null. */
        public final int[] undoSize;
        /** {@link #RESIZE_NOT_SENT} or {@link #RESIZE_KEPT} after a mode change that did not happen, with {@link #resized}. */
        public final int resizeProblem;
        public final int[] resized;
        /** Why the host offers no step-down now, in its words; null when it does or lists none. */
        public final String hostNote;
        public final String technical;

        Card(StreamDoctorVerdict verdict, NativeStreamDoctor host, boolean guest, boolean busy, String hostError,
             String failure, int undoKbps, boolean undoReplaced, int[] undoSize, int resizeProblem, int[] resized,
             String hostNote, String technical) {
            this.verdict = verdict;
            this.host = host;
            this.guest = guest;
            this.busy = busy;
            this.hostError = hostError;
            this.failure = failure;
            this.undoKbps = undoKbps;
            this.undoReplaced = undoReplaced;
            this.undoSize = undoSize;
            this.resizeProblem = resizeProblem;
            this.resized = resized;
            this.hostNote = hostNote;
            this.technical = technical;
        }
    }

    private final Host host;
    private final Runnable tick = this::tick;
    private View view;
    private Object stream;
    private NativeStreamDoctor snapshot;
    private String hostError, failure;
    private boolean fetching, busy;
    // The mode change in force: from and to {width, height}, when it was asked, and whether the stream showed it.
    private int[] resizeFrom, resizeTo;
    private long resizeAt;
    private boolean resizeShown;
    private int resizeProblem;
    private int[] resized;

    public StreamDoctorController(Host host) {
        this.host = host;
    }

    /** The card shows: look now and once a second until {@link #close}. */
    public void open(View view) {
        this.view = view;
        failure = null;
        resizeProblem = RESIZE_OK;
        host.cancel(tick);
        tick();
    }

    /** The card is gone: no more looks, and nothing more is read from the host. */
    public void close() {
        view = null;
        host.cancel(tick);
    }

    public boolean isOpen() {
        return view != null;
    }

    private void tick() {
        if (view == null) return;
        Object now = host.stream();
        if (now != stream) {
            // A new stream starts clean: the host dropped its trials, and the old mode is not this stream's.
            stream = now;
            snapshot = null;
            hostError = failure = null;
            resizeFrom = resizeTo = null;
        }
        if (host.offered() && !fetching) fetch();
        render();
        host.later(tick, POLL_MS);
    }

    private void fetch() {
        fetching = true;
        Object at = stream;
        boolean queued = host.background(() -> {
            NativeStreamDoctor read = null;
            String error = null;
            try {
                read = host.fetch();
            } catch (NativeApiException e) {
                error = host.describe(e);
            } catch (RuntimeException e) {
                error = String.valueOf(e.getMessage());
            }
            NativeStreamDoctor answer = read;
            String failed = error;
            host.ui(() -> {
                fetching = false;
                if (at != stream) return;
                if (answer != null) snapshot = answer;
                else LimeLog.warning("Stream doctor: the PC's measurements were not read: " + failed);
                hostError = answer != null ? null : failed;
                if (view != null) render();
            });
        });
        if (!queued) fetching = false;
    }

    private void render() {
        Facts f = view == null ? null : host.facts();
        if (f == null) return;
        followResize(f);
        boolean guest = snapshot != null && snapshot.sessionActive && !snapshot.callerIsOwner;
        NativeStreamDoctor.Trial trial = snapshot == null ? null : snapshot.bitrateTrial;
        boolean lists = snapshot != null && snapshot.bitrateRemedy != null;
        // A trial something else replaced blocks nothing: the host takes a new one.
        boolean bitrate = !guest && f.bitrateManual && (trial == null || !trial.canUndo) && f.classicUndoKbps == 0
                && (lists ? snapshot.offersBitrateStepdown() : f.classicBitrate);
        boolean resize = !guest && f.resize && resizeTo == null;
        int liveKbps = snapshot != null && snapshot.liveBitrateKbps > 0 ? snapshot.liveBitrateKbps : f.currentKbps;
        String answer = host.diagnose(request(f, snapshot, bitrate, liveKbps, resize));
        StreamDoctorVerdict verdict = StreamDoctorVerdict.parse(answer);
        if (verdict == null) LimeLog.warning("Stream doctor: no verdict: " + answer);
        int undoKbps = guest ? 0 : trial != null ? (trial.canUndo ? trial.previousKbps : 0) : f.classicUndoKbps;
        String note = lists && !guest && trial == null && !snapshot.bitrateRemedy.configurable
                && !"trial_active".equals(snapshot.bitrateRemedy.reason) ? snapshot.bitrateRemedy.message : null;
        view.show(new Card(verdict, snapshot, guest, busy, hostError, failure, undoKbps, trial != null && !trial.canUndo,
                resizeTo == null || guest ? null : resizeFrom.clone(), resizeProblem, resized, note, f.technical));
    }

    // A mode change stays undoable while the stream is at the mode it asked for (or still on its way there); another
    // size means something else changed it, and Undo would replace that change.
    private void followResize(Facts f) {
        if (resizeTo == null) return;
        boolean there = f.width == resizeTo[0] && f.height == resizeTo[1];
        boolean before = f.width == resizeFrom[0] && f.height == resizeFrom[1];
        if (there) {
            resizeShown = true;
        } else if (before && !resizeShown && host.nowMs() - resizeAt > RESIZE_CONFIRM_MS) {
            resizeProblem = RESIZE_KEPT;
            resized = resizeTo;
            resizeFrom = resizeTo = null;
        } else if (!before || resizeShown) {
            resizeFrom = resizeTo = null;
        }
    }

    /** A remedy's button. */
    public void apply(StreamDoctorVerdict.Remedy remedy) {
        if (view == null || busy || remedy == null) return;
        failure = null;
        resizeProblem = RESIZE_OK;
        if (StreamDoctorVerdict.BITRATE_STEPDOWN.equals(remedy.kind)) {
            if (remedy.onHost) {
                hostBitrate(false, null);
                return;
            }
            host.classicTrial(false);
        } else if (StreamDoctorVerdict.RESOLUTION_STEPDOWN.equals(remedy.kind)) {
            Facts f = host.facts();
            if (f == null || resizeTo != null) return;
            int[] to = {remedy.width, remedy.height};
            if (host.resize(to[0], to[1])) {
                resizeFrom = new int[] {f.width, f.height};
                resizeTo = to;
                resizeAt = host.nowMs();
                resizeShown = false;
            } else {
                resizeProblem = RESIZE_NOT_SENT;
                resized = to;
            }
        }
        render();
    }

    /** Undo for the bitrate: the host's trial while it can be undone, else the classic one. */
    public void undoBitrate() {
        if (view == null || busy) return;
        failure = null;
        NativeStreamDoctor.Trial trial = snapshot == null ? null : snapshot.bitrateTrial;
        if (trial != null) {
            if (trial.canUndo) hostBitrate(true, trial.trialId);
            return;
        }
        host.classicTrial(true);
        render();
    }

    /** Undo for the mode: back to the one before the step-down. */
    public void undoResize() {
        if (view == null || busy || resizeFrom == null) return;
        int[] back = resizeFrom;
        resizeProblem = RESIZE_OK;
        if (host.resize(back[0], back[1])) {
            resizeFrom = resizeTo = null;
        } else {
            resizeProblem = RESIZE_NOT_SENT;
            resized = back;
        }
        render();
    }

    private void hostBitrate(boolean undo, String trialId) {
        busy = true;
        Object at = stream;
        boolean queued = host.background(() -> {
            NativeStreamDoctor answer = null;
            String refused = null;
            try {
                answer = host.apply(undo, trialId);
            } catch (NativeApiException e) {
                refused = host.describe(e);
            } catch (RuntimeException e) {
                refused = String.valueOf(e.getMessage());
            }
            NativeStreamDoctor done = answer;
            String why = refused;
            host.ui(() -> {
                busy = false;
                if (at != stream) return;
                if (done != null) {
                    snapshot = done;
                    // After an undo, the bitrate the PC answers is in force: the trial's old value only if
                    // nothing (such as its limit) held it lower.
                    int kbps = undo || done.bitrateTrial == null ? done.liveBitrateKbps : done.bitrateTrial.appliedKbps;
                    if (kbps > 0) host.hostBitrateApplied(kbps);
                } else {
                    LimeLog.warning("Stream doctor: the PC refused the bitrate " + (undo ? "undo" : "step-down") + ": " + why);
                    failure = why;
                }
                if (view != null) render();
            });
        });
        if (!queued) busy = false;
        render();
    }

    /** The request {@code NightjarPolicyJni.doctorDiagnose} reads, for these facts. */
    static String request(Facts f, NativeStreamDoctor snapshot, boolean bitrate, int liveKbps, boolean resize) {
        StreamHealth.Report r = f.report;
        Json.Arr findings = Json.arr();
        for (StreamHealth.Finding finding : r.findings) findings.add(finding.name());
        Json.Obj report = Json.obj().put("findings", findings).put("samples", r.samples).put("recent", r.recent)
                .put("worst_loss_percent", finite(r.worstLossPercent)).put("worst_decode_ms", finite(r.worstDecodeMs))
                .put("worst_host_p95_ms", finite(r.worstHostP95Ms)).put("worst_display_p95_ms", finite(r.worstDisplayP95Ms))
                .put("stalls", r.stalls).put("stall_period_ms", r.stallPeriodMs)
                .put("host_includes_send", r.hostIncludesSend).put("late_samples", r.lateSamples);
        Json.Obj stream = Json.obj().put("width", f.width).put("height", f.height).put("fps", f.fps).put("hdr", f.hdr)
                .put("ten_bit", f.tenBit).put("pyrowave", f.pyrowave)
                .put("screen_width", f.screenWidth).put("screen_height", f.screenHeight);
        Json.Arr buffer = f.receiveBuffer < 0 ? null : Json.arr().add(f.receiveBuffer & 0xffffffffL).add(f.receiveBuffer >>> 32);
        Json.Obj device = Json.obj().put("constrained", true).put("upscaler", f.upscaler).put("receive_buffer", buffer);
        Json.Obj abilities = Json.obj().put("bitrate", bitrate).put("live_kbps", Math.max(0, liveKbps)).put("resize", resize);
        return Json.obj().put("report", report).put("host", snapshot == null ? null : Json.parseObject(snapshot.json))
                .put("stream", stream).put("device", device).put("abilities", abilities).toString();
    }

    private static double finite(float value) {
        return Float.isFinite(value) ? value : -1;
    }
}
