/**
 * Pure-JDK JNI declarations for the 7.0 interface's decisions in {@code nightjar-policy}
 * ({@code protocol/android/src/policy_ui_70.rs}): the surface tier, its frame probe and crash guard
 * ({@code glass_probe}), the interface's size ({@code viewport_scale}), the Resolution picker's
 * steps ({@code resolution_scale}), hint glyphs ({@code input_glyphs}), touch feedback
 * ({@code ui_feedback_contract}) and the Control Center ({@code control_center}, JSON in and out).
 * Imports only {@code java.*} so Robolectric loads it without Android shadow acquisition; loads
 * {@code nightjar_core} via {@link NightjarJni#ensureLibraryLoaded()}.
 *
 * <p>Tiers, form factors, sources, hints and events cross as their stable keys: tiers
 * {@code lens|live|baked|solid} (an upper-case stored name reads too), forms
 * {@code phone|handheld|tv|desktop}, sources {@code touch|mouse|keyboard|remote|pad}, hints
 * {@code go|more|back}, events {@code focus_tick|confirm|back|toggle|snap_grid|reject|launch_start}.
 */
package com.limelight.nvstream.jni;

public final class NightjarUiPolicyJni {
    static {
        NightjarJni.ensureLibraryLoaded();
    }

    private NightjarUiPolicyJni() {
    }

    // glass_probe: tier
    /** Where a device lands; {@code stored} is the probe's kept verdict or null. Returns a tier key. */
    public static native String tierLand(boolean lens, boolean live, boolean lowRam, boolean powerSave,
                                         boolean thermalSevere, boolean highContrast,
                                         boolean crashGuardTripped, String stored);
    /** One tier down, as a failed probe steps; an unknown key reads as baked. */
    public static native String tierStepDown(String tier);

    // glass_probe: probe
    /** The frame budget at {@code refreshHz}, floored at a 120 Hz frame; an unknown rate is 60 Hz. */
    public static native long probeBudgetNs(float refreshHz);
    /** Whether a frame was over its share of work and also late; {@code totalNs < 0}: landing unknown. */
    public static native boolean probeSlow(float refreshHz, long workNs, long totalNs);
    public static native int probeSlowFramesToFail(int frames);
    public static native boolean probeSettled(long elapsedMs);
    /** {frames, settle ms, the 120 Hz floor's budget in ns}. */
    public static native long[] probeConstants();
    /** {p90 budget fraction, late fraction, minimum budget rate in Hz}. */
    public static native float[] probeFractions();

    // glass_probe: crash guard
    /** The flag to store: true when arming before the first live frame, false when clearing. */
    public static native boolean guardInFlight(boolean arm, String fingerprint);
    /** Whether the stored flag and fingerprint say the last probe on this build never finished. */
    public static native boolean guardTripped(boolean inFlight, String storedFingerprint, String fingerprint);
    /** The log line for a tripped guard (never UI text). */
    public static native String guardWarning();

    // viewport_scale
    /**
     * {ui, density, typeScale, marginDp, heroHeight, columns, touchMinDp, stageMaxWidthDp,
     * textMaxWidthDp}; sizes are design dp (multiply by ui for system dp).
     */
    public static native float[] viewportScale(float widthDp, float heightDp, float dpi, String form,
                                               int userScalePct, boolean tvCompact);

    // resolution_scale
    /** This screen's steps, flattened {percent, width, height, ...}, largest first. */
    public static native int[] resolutionChoices(int nativeWidth, int nativeHeight);
    /** {width, height} of {@code percent} of the screen: even, and never under 320x240. */
    public static native int[] resolutionScaled(int nativeWidth, int nativeHeight, int percent);
    /** The step a saved size is on this screen, or 0. */
    public static native int resolutionStepOf(int nativeWidth, int nativeHeight, int width, int height);

    // input_glyphs
    /**
     * {@code "<glyph key>:<English text>"} for a hint (split at the first colon), or null for no hint
     * (touch, mouse). Keys: button, key, enter, escape, remote_ok, remote_hold_ok, remote_back.
     */
    public static native String inputGlyph(String source, int vendorId, String hint, boolean swapFaceButtons);
    /** {@code xbox}, {@code playstation} or {@code nintendo}. */
    public static native String inputStyleOfVendor(int vendorId);
    public static native boolean figtreeCovers(String locale);

    // ui_feedback_contract
    /**
     * {@code "<haptic>|<sound>"}, an empty side for none (haptics tick|click|press|double|rise); both
     * empty while a stream is active.
     */
    public static native String feedback(String event, boolean streamActive, boolean haptics, boolean sounds);
    public static native long feedbackLimiterCreate();
    public static native void feedbackLimiterDestroy(long handle);
    /** {@link #feedback} through the limiter at {@code handle}. */
    public static native String feedbackLimited(long handle, String event, boolean streamActive,
                                                  boolean haptics, boolean sounds, long nowMs);

    // control_center
    /**
     * One JSON request, one JSON answer ({@code {"ok": ...}} or {@code {"error", "detail"}}). Ops:
     * {@code dock} {offers, shortcuts?, buttons?, gates?}, {@code step} (dock fields + from, forward),
     * {@code hotkey} (dock fields + key), {@code placement} {form, width_dp, height_dp},
     * {@code surface} {open, tier, blur_available, power_save, thermal_severe, high_contrast}.
     */
    public static native String controlCenterCall(String request);
}
