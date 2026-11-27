/**
 * The stream's launch curtain in the Rust engine (protocol/android, launch_curtain.rs), which
 * runs the rules both Nightjar apps share ({@code nightjar_policy::launch_curtain}). A stream's
 * start calls {@link #begin}; each change calls {@code MoonBridge.bridgeClLaunchCurtain}, and the
 * app reads {@link #state}. The state is JSON, or null while no curtain is down or up for this
 * stream: {@code down}; {@code stage} (a {@code launch_curtain_<key>} string's key or null);
 * {@code attention} ({@code key}, {@code launcher} to fill in or null, {@code title}) or null;
 * {@code dismissible}; {@code host_dismissible}; {@code due_ms}, when {@link #tick} lifts it unless
 * the host speaks first, or null; and once it lifted {@code lift} ({@code how}, the
 * {@code notice} key or null, the host's {@code reason} for a failed start or null).
 *
 * <p>Its natives live in {@link NightjarSessionJni}, the session's JNI holder; this only checks
 * that the engine is loaded.
 */
package com.limelight.nvstream.jni;

public final class CurtainBridge {
    private CurtainBridge() {}

    /** A stream starts; {@code asked} when its launch asked the host for the curtain. */
    public static void begin(boolean asked) {
        if (NightjarJni.isAvailable()) NightjarSessionJni.launchCurtainBegin(asked);
    }

    /** The stream ended: its curtain goes with it. */
    public static void end() {
        if (NightjarJni.isAvailable()) NightjarSessionJni.launchCurtainEnd();
    }

    public static String state() {
        return NightjarJni.isAvailable() ? NightjarSessionJni.launchCurtainState() : null;
    }

    /** Lifts the curtain when its watchdog is due, and returns the state. */
    public static String tick() {
        return NightjarJni.isAvailable() ? NightjarSessionJni.launchCurtainTick() : null;
    }

    /** Show PC screen: lifts the curtain here at once, and returns the state. */
    public static String dismiss() {
        return NightjarJni.isAvailable() ? NightjarSessionJni.launchCurtainDismiss() : null;
    }
}
