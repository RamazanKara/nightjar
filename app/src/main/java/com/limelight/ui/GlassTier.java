package com.limelight.ui;

import android.app.ActivityManager;
import android.content.Context;
import android.content.SharedPreferences;
import android.os.Build;
import android.os.PowerManager;

import com.limelight.BuildConfig;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

/**
 * How real a glass surface can afford to be on this device, chosen by capability and by a frame probe, never by the
 * device's name. The rule is nightjar-policy's {@code glass_probe}, the desktop's too, reached through
 * {@link NightjarUiPolicyJni}; {@link #decide} mirrors it where the library is not loaded. What the probe found, and the
 * crash guard around it, are kept per app version and OS build in their own small file.
 */
public enum GlassTier {
    /** Live blur plus the refracted edge band (API 33+). */
    LENS,
    /** Live blur (API 31+). */
    LIVE,
    /** The static ambient, sampled in screen space under the fill: a complete design at no GPU cost. */
    BAKED,
    /** High contrast: an opaque fill and a light rim. */
    SOLID;

    private static final String FILE = "glass_tier";
    private static final String OVERRIDE = "debug_override";
    /** The crash guard (glass_probe::Guard): set while a probe runs, with the build it runs on. */
    static final String GUARD_IN_FLIGHT = "probe_in_flight";
    static final String GUARD_FINGERPRINT = "probe_fingerprint";
    /** The log line for a tripped guard; the policy's {@code GUARD_WARNING}. Never UI text. */
    static final String GUARD_WARNING = "Previous live-surface probe did not finish; using baked surfaces";
    /** The demo's sources, also in a not-debuggable build made for frame-time A/Bs. */
    private static final boolean DEMO = "debug".equals(BuildConfig.BUILD_TYPE);
    /** This process armed the guard: a probe of its own is running, not one that died before it. */
    private static volatile boolean armedHere;

    /** One tier down, as a failed frame probe steps. Baked and solid cost nothing and stay. */
    public GlassTier stepDown() {
        if (Policy.available()) {
            try {
                return parse(NightjarUiPolicyJni.tierStepDown(key()));
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return stepDownMirror();
    }

    /** {@link #stepDown} in Java, pinned to the policy by GlassTierPolicyParityTest. */
    GlassTier stepDownMirror() {
        switch (this) {
            case LENS: return LIVE;
            case SOLID: return SOLID;
            default: return BAKED;
        }
    }

    /** The policy's key for this tier. */
    String key() {
        return name().toLowerCase(java.util.Locale.ROOT);
    }

    /**
     * The tier from what is known now.
     *
     * @param stored what the probe left for this app version, or null before it has run
     */
    static GlassTier decide(int api, boolean lowRam, boolean powerSave, boolean hot, boolean highContrast, GlassTier stored) {
        return decide(api, lowRam, powerSave, hot, highContrast, false, stored);
    }

    /** {@link #decide}, with the crash guard: a probe that never finished on this build lands on baked. */
    static GlassTier decide(int api, boolean lowRam, boolean powerSave, boolean hot, boolean highContrast,
                            boolean guardTripped, GlassTier stored) {
        boolean lens = api >= Build.VERSION_CODES.TIRAMISU;
        boolean live = api >= Build.VERSION_CODES.S;
        if (Policy.available()) {
            try {
                GlassTier tier = parse(NightjarUiPolicyJni.tierLand(lens, live, lowRam, powerSave, hot, highContrast,
                        guardTripped, stored == null ? null : stored.key()));
                if (tier != null) return tier;
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return decideMirror(lens, live, lowRam, powerSave, hot, highContrast, guardTripped, stored);
    }

    /** glass_probe::land in Java, pinned to the policy by GlassTierPolicyParityTest. */
    static GlassTier decideMirror(boolean lens, boolean live, boolean lowRam, boolean powerSave, boolean hot,
                                  boolean highContrast, boolean guardTripped, GlassTier stored) {
        if (highContrast) return SOLID;
        if (!live || lowRam || powerSave || hot || guardTripped) return BAKED;
        GlassTier top = lens ? LENS : LIVE;
        // A stored solid is a high-contrast leftover: the probe never writes it.
        if (stored == null || stored == SOLID) return top;
        return stored.ordinal() > top.ordinal() ? stored : top;
    }

    /** Where this device lands right now. Battery saver and heat are read each time and never stored. */
    public static GlassTier land(Context context) {
        Context app = context.getApplicationContext();
        SharedPreferences prefs = app.getSharedPreferences(FILE, Context.MODE_PRIVATE);
        if (DEMO) {
            GlassTier forced = parse(prefs.getString(OVERRIDE, null));
            if (forced != null) return forced;
        }
        if (guardTripped(prefs)) {
            com.limelight.LimeLog.warning(GUARD_WARNING);
            // Kept like a probe's verdict, so this build is never probed again; the guard is done.
            remember(app, BAKED);
            clearGuard(app);
        }
        ActivityManager activities = (ActivityManager) app.getSystemService(Context.ACTIVITY_SERVICE);
        PowerManager power = (PowerManager) app.getSystemService(Context.POWER_SERVICE);
        boolean hot = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && power != null
                && power.getCurrentThermalStatus() >= PowerManager.THERMAL_STATUS_SEVERE;
        return decide(Build.VERSION.SDK_INT, activities != null && activities.isLowRamDevice(),
                power != null && power.isPowerSaveMode(), hot, AppTheme.isHighContrast(context),
                parse(prefs.getString(versionKey(), null)));
    }

    /** Whether a probe's verdict for this app version is already kept. A forced tier is never probed. */
    public static boolean probed(Context context) {
        SharedPreferences prefs = context.getApplicationContext().getSharedPreferences(FILE, Context.MODE_PRIVATE);
        return prefs.contains(versionKey()) || (DEMO && prefs.contains(OVERRIDE));
    }

    /** Keeps the probe's verdict for this app version. */
    public static void remember(Context context, GlassTier tier) {
        SharedPreferences prefs = context.getApplicationContext().getSharedPreferences(FILE, Context.MODE_PRIVATE);
        SharedPreferences.Editor editor = prefs.edit();
        // Only the verdict for what runs now is ever read again; the guard has its own lifetime.
        for (String key : prefs.getAll().keySet()) {
            if (!OVERRIDE.equals(key) && !GUARD_IN_FLIGHT.equals(key) && !GUARD_FINGERPRINT.equals(key)) editor.remove(key);
        }
        editor.putString(versionKey(), tier.name()).apply();
    }

    /**
     * Arms the crash guard before a probe watches its first live frame (glass_probe::Guard::arm). Committed at once: a
     * driver that dies on the first live target must find it on the next launch.
     */
    static void armGuard(Context context) {
        String fingerprint = versionKey();
        boolean inFlight = true;
        if (Policy.available()) {
            try {
                inFlight = NightjarUiPolicyJni.guardInFlight(true, fingerprint);
            } catch (LinkageError ignored) {
                // Arming is storing the flag.
            }
        }
        armedHere = true;
        context.getApplicationContext().getSharedPreferences(FILE, Context.MODE_PRIVATE).edit()
                .putBoolean(GUARD_IN_FLIGHT, inFlight).putString(GUARD_FINGERPRINT, fingerprint).commit();
    }

    /** Clears the guard once the probe has ended, with a verdict or stopped (glass_probe::Guard::clear). */
    static void clearGuard(Context context) {
        armedHere = false;
        // A cleared guard trips for no build, so it is stored as no guard at all.
        context.getApplicationContext().getSharedPreferences(FILE, Context.MODE_PRIVATE).edit()
                .remove(GUARD_IN_FLIGHT).remove(GUARD_FINGERPRINT).apply();
    }

    /** Whether the guard says a probe of an earlier launch never finished on this very build. */
    private static boolean guardTripped(SharedPreferences prefs) {
        if (armedHere) return false;
        boolean inFlight = prefs.getBoolean(GUARD_IN_FLIGHT, false);
        String stored = prefs.getString(GUARD_FINGERPRINT, "");
        return guardTripped(inFlight, stored == null ? "" : stored, versionKey());
    }

    static boolean guardTripped(boolean inFlight, String stored, String fingerprint) {
        if (Policy.available()) {
            try {
                return NightjarUiPolicyJni.guardTripped(inFlight, stored, fingerprint);
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return inFlight && stored.equals(fingerprint);
    }

    /** Debug builds only: pins the tier, for the demo. Null clears it. */
    public static void force(Context context, GlassTier tier) {
        SharedPreferences.Editor editor = context.getApplicationContext()
                .getSharedPreferences(FILE, Context.MODE_PRIVATE).edit();
        if (tier == null) editor.remove(OVERRIDE);
        else editor.putString(OVERRIDE, tier.name());
        editor.apply();
    }

    /** The tier of a stored name or a policy key; null for anything else. */
    static GlassTier parse(String name) {
        if (name == null) return null;
        for (GlassTier tier : values()) if (tier.name().equalsIgnoreCase(name)) return tier;
        return null;
    }

    /** An OS or driver update changes what the same device can afford, so it probes again as a new app version does. */
    static String versionKey() {
        return "v" + BuildConfig.VERSION_CODE + "-" + Build.FINGERPRINT;
    }

    /** Whether nightjar-policy can be asked: the library loads (it does on devices and in the unit tests). */
    static final class Policy {
        private Policy() { }

        static boolean available() {
            return com.limelight.nvstream.jni.NightjarJni.isAvailable();
        }
    }
}
