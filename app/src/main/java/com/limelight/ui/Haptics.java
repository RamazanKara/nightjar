package com.limelight.ui;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.Context;
import android.content.ContextWrapper;
import android.os.Build;
import android.os.SystemClock;
import android.view.HapticFeedbackConstants;
import android.view.View;

import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

/**
 * Touch feedback for the app's own controls: the PC keyboard's keys, the trackpads, the WASD stick, slider steps,
 * settings switches and the shell bar. View.performHapticFeedback follows the system's touch feedback setting; the
 * stream's on-screen controls also follow Vibrate on press ({@code checkbox_vibrate_osc}).
 * Each kind takes the newest constant the device knows and the closest older one below it.
 *
 * <p>The 7.0 interface's own events ({@link #feedback}: focus moving, a press, Back, a switch, a snap in the layout
 * editor, a refused press, a game starting) follow nightjar-policy's {@code ui_feedback_contract}, the desktop's too:
 * each maps to one of the policy's haptics, a held D-pad ticks no faster than a motor can, and while a stream is on
 * screen the interface is silent. The stream's own controls above ({@link #perform}) keep Vibrate on press.
 */
public final class Haptics {
    public enum Kind {
        /**
         * A key of an on-screen keyboard going down. Not KEYBOARD_TAP: that constant is for the input method, and phones
         * can tie it to their own keyboard vibration setting or play nothing for it (nightjar#61: the trackpad's click
         * was felt, the keys were not), while Vibrate on press is the setting that decides here.
         */
        KEY,
        /** A trackpad tap or an on-screen mouse button going down. */
        CLICK,
        /** A WASD stick entering one direction: W, A, S or D. */
        DIRECTION,
        /** A WASD stick entering a diagonal, two keys at once; it must feel unlike {@link #DIRECTION}. */
        DIAGONAL,
        /** A button that moves to another screen, such as PCs or Settings in the shell bar. */
        NAVIGATION,
        /** A slider's value moving one step. */
        STEP,
        /** A switch turned on. */
        TOGGLE_ON,
        /** A switch turned off. */
        TOGGLE_OFF
    }

    private Haptics() {
    }

    /** The interface events of ui_feedback_contract, by their policy keys. */
    public enum Event {
        FOCUS_TICK, CONFIRM, BACK, TOGGLE, SNAP_GRID, REJECT, LAUNCH_START;

        String key() {
            return name().toLowerCase(java.util.Locale.ROOT);
        }

        /** Event::min_interval_ms: the shortest time between two of this event that are both felt. */
        long minIntervalMs() {
            switch (this) {
                case FOCUS_TICK: return 45;
                case SNAP_GRID: return 60;
                case REJECT: return 180;
                default: return 0;
            }
        }
    }

    /** The policy's rate limiter, made on first use and kept for the process. */
    private static long limiter;
    /** The mirror's clock per event where the library is not loaded. */
    private static final long[] MIRROR_LAST = new long[Event.values().length];
    private static final boolean[] MIRROR_FELT = new boolean[Event.values().length];

    /**
     * Answers an interface event with its touch, unless a stream is on the screen {@code view} belongs to. The system's
     * touch feedback setting still decides whether anything is felt.
     */
    public static void feedback(View view, Event event) {
        String haptic = haptic(event, streamOnScreen(view.getContext()), SystemClock.uptimeMillis());
        if (haptic != null) view.performHapticFeedback(constant(haptic, Build.VERSION.SDK_INT));
    }

    /** The policy's haptic key for {@code event} at {@code nowMs}, or null for nothing. */
    static synchronized String haptic(Event event, boolean streamActive, long nowMs) {
        if (NightjarJni.isAvailable()) {
            try {
                if (limiter == 0) limiter = NightjarUiPolicyJni.feedbackLimiterCreate();
                String pair = NightjarUiPolicyJni.feedbackLimited(limiter, event.key(), streamActive, true, false, nowMs);
                if (pair != null) {
                    String haptic = pair.substring(0, pair.indexOf('|'));
                    return haptic.isEmpty() ? null : haptic;
                }
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return hapticMirror(event, streamActive, nowMs);
    }

    /** ui_feedback_contract's feedback and Limiter in Java, pinned to the policy by HapticsFeedbackTest. */
    static synchronized String hapticMirror(Event event, boolean streamActive, long nowMs) {
        if (streamActive) return null;
        int slot = event.ordinal();
        if (MIRROR_FELT[slot] && Math.max(0, nowMs - MIRROR_LAST[slot]) < event.minIntervalMs()) return null;
        MIRROR_FELT[slot] = true;
        MIRROR_LAST[slot] = nowMs;
        return hapticOf(event);
    }

    /** The policy's haptic for an event that is felt. */
    static String hapticOf(Event event) {
        switch (event) {
            case FOCUS_TICK: case SNAP_GRID: return "tick";
            case BACK: case TOGGLE: return "click";
            case CONFIRM: return "press";
            case REJECT: return "double";
            default: return "rise";
        }
    }

    /** Forgets every clock, for tests. */
    static synchronized void resetForTests() {
        if (limiter != 0) {
            try {
                NightjarUiPolicyJni.feedbackLimiterDestroy(limiter);
            } catch (LinkageError ignored) {
                // Nothing was made.
            }
            limiter = 0;
        }
        java.util.Arrays.fill(MIRROR_FELT, false);
    }

    /** Whether the screen a view belongs to shows a stream: the interface over a stream is silent and still. */
    static boolean streamOnScreen(Context context) {
        while (context instanceof ContextWrapper) {
            if (context instanceof StreamWindow) return true;
            if (context instanceof Activity) return false;
            context = ((ContextWrapper) context).getBaseContext();
        }
        return false;
    }

    /** The feedback constant for one of the policy's haptics on API level {@code sdk}: the closest the device has. */
    @SuppressLint("InlinedApi") // Each newer constant is used only from its API level on.
    static int constant(String haptic, int sdk) {
        switch (haptic) {
            case "tick":
                return sdk >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE ? HapticFeedbackConstants.SEGMENT_FREQUENT_TICK
                        : HapticFeedbackConstants.CLOCK_TICK;
            case "click":
                return HapticFeedbackConstants.CONTEXT_CLICK;
            case "double":
                return sdk >= Build.VERSION_CODES.R ? HapticFeedbackConstants.REJECT : HapticFeedbackConstants.LONG_PRESS;
            case "rise":
                return sdk >= Build.VERSION_CODES.R ? HapticFeedbackConstants.GESTURE_START : HapticFeedbackConstants.VIRTUAL_KEY;
            default:
                return sdk >= Build.VERSION_CODES.R ? HapticFeedbackConstants.CONFIRM : HapticFeedbackConstants.VIRTUAL_KEY;
        }
    }

    /** The feedback constant for {@code kind} on a device running API level {@code sdk}. */
    @SuppressLint("InlinedApi") // Each newer constant is used only from its API level on.
    static int constant(Kind kind, int sdk) {
        switch (kind) {
            case DIRECTION:
                // One light tick for one key.
                return HapticFeedbackConstants.CONTEXT_CLICK;
            case DIAGONAL:
                // Two keys: a full click, firmer than the direction's tick. Android 11's confirm plays the same click.
                return HapticFeedbackConstants.VIRTUAL_KEY;
            case STEP:
                // A slider steps many times in one drag: the light tick Android 14 made for that, a clock tick before.
                return sdk >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE ? HapticFeedbackConstants.SEGMENT_FREQUENT_TICK
                        : HapticFeedbackConstants.CLOCK_TICK;
            case TOGGLE_ON:
                return sdk >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE ? HapticFeedbackConstants.TOGGLE_ON
                        : HapticFeedbackConstants.VIRTUAL_KEY;
            case TOGGLE_OFF:
                return sdk >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE ? HapticFeedbackConstants.TOGGLE_OFF
                        : HapticFeedbackConstants.VIRTUAL_KEY;
            default:
                return HapticFeedbackConstants.VIRTUAL_KEY;
        }
    }

    public static void perform(View view, Kind kind) {
        view.performHapticFeedback(constant(kind, Build.VERSION.SDK_INT));
    }

    public static void toggle(View view, boolean on) {
        perform(view, on ? Kind.TOGGLE_ON : Kind.TOGGLE_OFF);
    }
}
