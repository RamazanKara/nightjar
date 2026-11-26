package com.limelight.binding.video;

import com.limelight.nvstream.jni.Json;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Stream Doctor 2.0's verdict as {@code nightjar_policy::doctor_v2::diagnose} gives it (through
 * {@code NightjarPolicyJni.doctorDiagnose}), read from its stable snake_case JSON: the cause and stage, the number it
 * stands on, the one-tap remedies best first, the advice, and whether the upscaler in use earns its cost. A value a
 * newer policy adds reads as unknown and shows nothing.
 */
public final class StreamDoctorVerdict {
    public static final String GATHERING = "gathering", HEALTHY = "healthy", RECEIVE_BUFFER_SHORT = "receive_buffer_short",
            HOST_OVERLOAD = "host_overload", DECODE_PRESSURE = "decode_pressure", NETWORK_JITTER = "network_jitter",
            DISPLAY_PACING = "display_pacing";
    public static final String BITRATE_STEPDOWN = "bitrate_stepdown", RESOLUTION_STEPDOWN = "resolution_stepdown";

    public final String cause, stage;
    public final Key key;
    public final List<Remedy> remedies;
    public final List<Advice> advice;
    /** {@code fine}, {@code not_needed}, {@code too_costly} (with {@link #lighter}) or {@code blocked} (with {@link #blocker}). */
    public final String upscaler, lighter, blocker;
    public final boolean hostMeasured;

    /** The number a finding stands on; a field the kind lacks is NaN or 0. */
    public static final class Key {
        /** {@code none}, {@code loss_percent}, {@code late_seconds}, {@code host_lost}, {@code host_p95}, {@code decode_ms}, {@code display_p95} or {@code receive_buffer}. */
        public final String kind;
        public final double percent, lostMs, ms, budgetMs;
        public final long late, of, pictures, grantedKib, askedKib;
        /** The host stage of {@code host_lost}: capture, convert, encode, packetize or send. */
        public final String hostStage;

        Key(Json.Obj k) {
            kind = k.optString("kind", "none");
            percent = number(k, "percent");
            lostMs = number(k, "lost_ms");
            ms = number(k, "ms");
            budgetMs = number(k, "budget_ms");
            late = k.optLong("late");
            of = k.optLong("of");
            pictures = k.optLong("pictures");
            grantedKib = k.optLong("granted_kib");
            askedKib = k.optLong("asked_kib");
            hostStage = k.optString("stage", "");
        }
    }

    /** A one-tap remedy for this stream. */
    public static final class Remedy {
        /** {@link #BITRATE_STEPDOWN} or {@link #RESOLUTION_STEPDOWN}. */
        public final String kind;
        public final int targetKbps, width, height;
        /** The bitrate step-down goes through the host's doctor; otherwise the classic trial. */
        public final boolean onHost;
        /** What to turn on for the next stream with a lower resolution: {@code none}, {@code sgsr1} or {@code fsr1}. */
        public final String upscaler;

        Remedy(Json.Obj r) {
            kind = r.optString("remedy", "");
            targetKbps = r.optInt("target_kbps", 0);
            onHost = r.optBoolean("on_host");
            width = r.optInt("width", 0);
            height = r.optInt("height", 0);
            upscaler = r.optString("upscaler", "none");
        }
    }

    /** Advice the player follows on their own. */
    public static final class Advice {
        /** The policy's name: {@code wired_or_quieter_wifi}, {@code periodic_scans}, {@code use_upscaler}, ... */
        public final String kind;
        public final long periodMs, stalls, askedBytes;
        public final String upscaler;

        Advice(Json.Obj a) {
            kind = a.optString("advice", "");
            periodMs = a.optLong("period_ms");
            stalls = a.optLong("stalls");
            askedBytes = a.optLong("asked_bytes");
            upscaler = a.optString("upscaler", "none");
        }
    }

    private StreamDoctorVerdict(Json.Obj d) {
        cause = d.optString("cause", "");
        stage = d.optString("stage", "none");
        Json.Obj k = d.optObject("key");
        key = new Key(k == null ? Json.obj() : k);
        List<Remedy> r = new ArrayList<>();
        Json.Arr list = d.optArray("remedies");
        for (int i = 0; list != null && i < list.length(); i++) if (list.optObject(i) != null) r.add(new Remedy(list.optObject(i)));
        remedies = Collections.unmodifiableList(r);
        List<Advice> a = new ArrayList<>();
        list = d.optArray("advice");
        for (int i = 0; list != null && i < list.length(); i++) if (list.optObject(i) != null) a.add(new Advice(list.optObject(i)));
        advice = Collections.unmodifiableList(a);
        Json.Obj u = d.optObject("upscaler");
        upscaler = u == null ? "fine" : u.optString("verdict", "fine");
        lighter = u == null ? "none" : u.optString("lighter", "none");
        blocker = u == null ? "" : u.optString("blocker", "");
        hostMeasured = d.optBoolean("host_measured");
    }

    private static double number(Json.Obj o, String key) {
        return o.isNull(key) ? Double.NaN : o.optDouble(key);
    }

    /** The verdict in {@code answer} ({@code {"ok": ...}}); null for an error, which is logged by the caller. */
    public static StreamDoctorVerdict parse(String answer) {
        try {
            Json.Obj ok = Json.parseObject(answer).optObject("ok");
            return ok == null ? null : new StreamDoctorVerdict(ok);
        } catch (Json.SyntaxException e) {
            return null;
        }
    }

    /** The first remedy of {@code kind}, or null. */
    public Remedy remedy(String kind) {
        for (Remedy remedy : remedies) if (remedy.kind.equals(kind)) return remedy;
        return null;
    }
}
