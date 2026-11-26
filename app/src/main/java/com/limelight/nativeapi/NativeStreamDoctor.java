package com.limelight.nativeapi;

import com.limelight.nvstream.jni.Json;

/**
 * GET /stream/doctor and POST /stream/doctor/apply (Stream Doctor 2.0, {@code stream.doctor_v2}): which stage of the
 * host lost this device's stream time over the last 30 seconds, its encoder as it runs, and the session-only bitrate
 * step-down the host applies and undoes. {@link #json} goes as it came to {@code NightjarPolicyJni.doctorDiagnose}, which
 * fuses it with this device's own report; the fields below are what the card shows or sends back.
 */
public final class NativeStreamDoctor {
    public static final String BITRATE_STEPDOWN = "bitrate_stepdown";

    /** The host's answer as JSON, for the shared verdict. */
    public final String json;
    /** This device has a stream on the host, as owner or guest. */
    public final boolean sessionActive;
    /** This device owns the stream: only the owner applies the host's remedies. */
    public final boolean callerIsOwner;
    /** The encoder's live bitrate in kbps, 0 when unknown or without a stream. */
    public final int liveBitrateKbps;
    /** The encoder and codec by the host's names, empty without a stream. */
    public final String backend, codec;
    /** The host's stage report: new pictures and the late ones over the window, 0 before the first. */
    public final long pictures, latePictures;
    /** The stage that lost the most time, null when too little was lost or none was measured. */
    public final String dominantStage;
    /** Time the late pictures lost in capture, convert, encode, packetize and send, in microseconds. */
    public final long[] lostUs;
    /** The bitrate step-down in force for this stream, or null. */
    public final Trial bitrateTrial;
    /** The step-down's eligibility, or null when the host lists none. */
    public final Remedy bitrateRemedy;

    /** A remedy in force. */
    public static final class Trial {
        public final String trialId;
        public final int appliedKbps, previousKbps;
        /** False once something else changed the bitrate: undo would replace that change, and the host refuses it. */
        public final boolean canUndo;

        Trial(String trialId, int appliedKbps, int previousKbps, boolean canUndo) {
            this.trialId = trialId;
            this.appliedKbps = appliedKbps;
            this.previousKbps = previousKbps;
            this.canUndo = canUndo;
        }
    }

    /** Whether the host can apply a remedy now, or why not in its stable name and plain words. */
    public static final class Remedy {
        public final boolean configurable;
        public final int targetKbps;
        public final String reason, message;

        Remedy(boolean configurable, int targetKbps, String reason, String message) {
            this.configurable = configurable;
            this.targetKbps = targetKbps;
            this.reason = reason;
            this.message = message;
        }
    }

    private NativeStreamDoctor(Json.Obj json) {
        this.json = json.toString();
        sessionActive = json.optBoolean("session_active");
        callerIsOwner = json.optBoolean("caller_is_owner");
        Json.Obj encoder = json.optObject("encoder");
        liveBitrateKbps = encoder == null ? 0 : encoder.optInt("live_bitrate_kbps", 0);
        backend = encoder == null ? "" : encoder.optString("backend", "");
        codec = encoder == null ? "" : encoder.optString("codec", "");
        Json.Obj stages = json.optObject("host_stages");
        pictures = stages == null ? 0 : stages.optLong("pictures");
        latePictures = stages == null ? 0 : stages.optLong("late_pictures");
        dominantStage = stages == null || stages.isNull("dominant_stage") ? null : stages.optString("dominant_stage", null);
        Json.Obj lost = stages == null ? null : stages.optObject("lost_us");
        String[] names = {"capture", "convert", "encode", "packetize", "send"};
        lostUs = new long[names.length];
        for (int i = 0; lost != null && i < names.length; i++) lostUs[i] = lost.optLong(names[i]);
        bitrateTrial = trial(json.optArray("active_trials"));
        bitrateRemedy = remedy(json.optArray("eligible_remedies"));
    }

    private static Trial trial(Json.Arr trials) {
        for (int i = 0; trials != null && i < trials.length(); i++) {
            Json.Obj t = trials.optObject(i);
            if (t != null && BITRATE_STEPDOWN.equals(t.optString("remedy", ""))) {
                return new Trial(t.optString("trial_id", ""), t.optInt("applied_value", 0), t.optInt("previous_value", 0),
                        t.optBoolean("can_undo"));
            }
        }
        return null;
    }

    private static Remedy remedy(Json.Arr remedies) {
        for (int i = 0; remedies != null && i < remedies.length(); i++) {
            Json.Obj r = remedies.optObject(i);
            if (r != null && BITRATE_STEPDOWN.equals(r.optString("id", ""))) {
                return new Remedy(r.optBoolean("configurable"), r.isNull("target_kbps") ? 0 : r.optInt("target_kbps", 0),
                        r.isNull("reason") ? null : r.optString("reason", null),
                        r.isNull("message") ? null : r.optString("message", null));
            }
        }
        return null;
    }

    /** The host offers its step-down now, to this device. */
    public boolean offersBitrateStepdown() {
        return callerIsOwner && bitrateRemedy != null && bitrateRemedy.configurable;
    }

    public static NativeStreamDoctor parse(Json.Obj json) {
        return new NativeStreamDoctor(json);
    }

    /** The body of POST /stream/doctor/apply for the bitrate step-down: apply, or undo the trial {@code trialId}. */
    static String applyJson(boolean undo, String trialId) {
        return Json.obj().put("action", undo ? "undo" : "apply").put("remedy", BITRATE_STEPDOWN)
                .put("trial_id", undo ? trialId : null).toString();
    }
}
