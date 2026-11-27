/**
 * Thin JNI delegate for {@code nightjar_policy::client_hint::Bitrate}.
 */
package com.limelight.nvstream;

import com.limelight.nvstream.jni.HandleReaper;
import com.limelight.nvstream.jni.NightjarPolicyJni;

public final class ClientHintBitrateController implements AutoCloseable {
    public static final long COOLDOWN_MS = 2000;
    public static final long START_DELAY_MS = 3000;
    public static final int STABLE_SAMPLES = 5;
    public static final float QUEUE_HOLD_MS = 20;

    private final long handle;
    private final HandleReaper.Token reaper;

    public ClientHintBitrateController(int initialKbps, int limitKbps, long nowMs) {
        this.handle = NightjarPolicyJni.clientHintCreate(initialKbps, limitKbps, nowMs);
        this.reaper = HandleReaper.register(this, handle, NightjarPolicyJni::clientHintDestroy);
    }

    public static int[] range(int initialKbps) {
        return NightjarPolicyJni.clientHintRange(initialKbps);
    }

    public void resumeFrom(int kbps) { NightjarPolicyJni.clientHintResumeFrom(handle, kbps); }
    public void setThermalLevel(int level) { NightjarPolicyJni.clientHintSetThermalLevel(handle, level); }

    public int sample(long nowMs, boolean recentVideo, float lossPercent, float queueMs) {
        return NightjarPolicyJni.clientHintSample(handle, nowMs, recentVideo, lossPercent, queueMs);
    }

    public void applied(int kbps, long nowMs) { NightjarPolicyJni.clientHintApplied(handle, kbps, nowMs); }
    public void refused() { NightjarPolicyJni.clientHintRefused(handle); }
    public void overridden(int kbps, long nowMs) { NightjarPolicyJni.clientHintOverridden(handle, kbps, nowMs); }

    public int currentKbps() { return NightjarPolicyJni.clientHintState(handle)[0]; }
    public int minKbps() { return NightjarPolicyJni.clientHintState(handle)[1]; }
    public int maxKbps() { return NightjarPolicyJni.clientHintState(handle)[2]; }
    public boolean pending() { return NightjarPolicyJni.clientHintState(handle)[3] != 0; }
    public boolean stopped() { return NightjarPolicyJni.clientHintState(handle)[4] != 0; }
    public boolean hasRoomBeforeQualityReduction() { return NightjarPolicyJni.clientHintState(handle)[5] != 0; }

    @Override public void close() { reaper.close(); }
    public void destroy() { close(); }
}
