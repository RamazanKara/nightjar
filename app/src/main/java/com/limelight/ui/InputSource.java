package com.limelight.ui;

import android.content.Context;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;

import com.limelight.R;
import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

/**
 * What the last input came from, so a button hint names what the device in the hand really has: taken from the event and
 * its {@link InputDevice}, never from the form factor. Touch and a mouse get no hints.
 *
 * <p>The key a hint names is nightjar-policy's {@code input_glyphs}, the desktop's too: a Nintendo controller (vendor
 * 0x057E) shows B/X, a PlayStation one ×/△, anything else A/Y, a keyboard Enter/F/Esc, a remote OK/Hold OK; with the face
 * button swap setting on, Go and Back trade places. The controller styles are ported from Moonlight V+
 * (streaming/input/gamepadglyphs.h and SdlGamepadKeyNavigation::detectUiStyle). The words for a remote's keys stay
 * Android strings, translated here.
 */
public enum InputSource {
    REMOTE, PAD, KEYBOARD, NONE;

    /** The face button swap setting (SettingsCatalogGen.FLIP_FACE_BUTTONS_PREF_STRING). */
    static final String FLIP_FACE_BUTTONS = "checkbox_flip_face_buttons";
    static final int SONY = 0x054C;
    static final int NINTENDO = 0x057E;

    /** The source of the last event any screen saw; null until there was one. */
    private static InputSource last;

    public static InputSource last() {
        return last;
    }

    /** Remembers {@code source} for the next screen; returns it. */
    public static InputSource remember(InputSource source) {
        last = source;
        return source;
    }

    /**
     * The source of a key, or null for a key that says nothing about the hand: volume, power, the system's keys, and one with no
     * source at all (a plain {@code adb shell input keyevent}), which must not take the hints away from the device in the hand.
     */
    public static InputSource of(KeyEvent event) {
        if (event.getSource() == InputDevice.SOURCE_UNKNOWN) return null;
        if (event.isSystem() && !isNavigation(event.getKeyCode())) return null;
        InputDevice device = event.getDevice();
        return of(event.getSource(), device == null ? InputDevice.KEYBOARD_TYPE_NONE : device.getKeyboardType());
    }

    /**
     * The key that ends touch mode never reaches the window, so all that says it came is touch mode ending: the likeliest device
     * until the next key names the real one.
     */
    public static InputSource leavingTouch(boolean gamepad, boolean keyboard) {
        return gamepad ? PAD : keyboard ? KEYBOARD : REMOTE;
    }

    /** A D-pad's or stick's keys often arrive as joystick keys without the gamepad bit, a pad's face buttons never lack it. */
    static InputSource of(int source, int keyboardType) {
        if ((source & InputDevice.SOURCE_GAMEPAD) == InputDevice.SOURCE_GAMEPAD
                || (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK) {
            return PAD;
        }
        if ((source & InputDevice.SOURCE_KEYBOARD) == InputDevice.SOURCE_KEYBOARD
                && keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC) {
            return KEYBOARD;
        }
        return (source & (InputDevice.SOURCE_DPAD | InputDevice.SOURCE_KEYBOARD)) != 0 ? REMOTE : NONE;
    }

    /** The source of a touch or a pointer's move (no hints), a stick's move (a pad); null for anything else. */
    public static InputSource of(MotionEvent event) {
        int source = event.getSource();
        if ((source & InputDevice.SOURCE_CLASS_POINTER) != 0) return NONE;
        return (source & InputDevice.SOURCE_JOYSTICK) == InputDevice.SOURCE_JOYSTICK ? PAD : null;
    }

    private static boolean isNavigation(int keyCode) {
        return keyCode == KeyEvent.KEYCODE_DPAD_UP || keyCode == KeyEvent.KEYCODE_DPAD_DOWN
                || keyCode == KeyEvent.KEYCODE_DPAD_LEFT || keyCode == KeyEvent.KEYCODE_DPAD_RIGHT
                || keyCode == KeyEvent.KEYCODE_DPAD_CENTER;
    }

    /** The key for Go on this source; null with no hints. A pad's own labels when the pad is known to differ. */
    public String goKey(Context context, int padVendor) {
        return key(context, "go", padVendor);
    }

    /** The key for More on this source; null with no hints. Whatever is on a pad's Y also opens from Hold OK. */
    public String moreKey(Context context, int padVendor) {
        return key(context, "more", padVendor);
    }

    /** The key for Back on a pad or a keyboard; null elsewhere (a remote's Back key carries no word to show). */
    public String backKey(Context context, int padVendor) {
        return this == REMOTE ? null : key(context, "back", padVendor);
    }

    private String key(Context context, String hint, int padVendor) {
        boolean swap = android.preference.PreferenceManager.getDefaultSharedPreferences(context)
                .getBoolean(FLIP_FACE_BUTTONS, false);
        String glyph = glyph(policyKey(), padVendor, hint, swap);
        if (glyph == null) return null;
        int colon = glyph.indexOf(':');
        String kind = glyph.substring(0, colon);
        String text = glyph.substring(colon + 1);
        switch (kind) {
            case "remote_ok": return context.getString(R.string.console_hint_key_ok);
            case "remote_hold_ok": return context.getString(R.string.console_hint_key_hold_ok);
            case "enter": return context.getString(R.string.console_hint_key_enter);
            default: return text;
        }
    }

    /** The policy's name for this source. */
    String policyKey() {
        switch (this) {
            case REMOTE: return "remote";
            case PAD: return "pad";
            case KEYBOARD: return "keyboard";
            default: return "touch";
        }
    }

    /** {@code "<glyph key>:<English text>"} for a hint, or null for none: the policy's answer, or its mirror. */
    static String glyph(String source, int vendor, String hint, boolean swap) {
        if (NightjarJni.isAvailable()) {
            try {
                return NightjarUiPolicyJni.inputGlyph(source, vendor, hint, swap);
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return glyphMirror(source, vendor, hint, swap);
    }

    /** input_glyphs::glyph in Java, pinned to the policy by InputSourceGlyphTest. */
    static String glyphMirror(String source, int vendor, String hint, boolean swap) {
        boolean go = "go".equals(hint), more = "more".equals(hint), back = "back".equals(hint);
        if (!go && !more && !back) return null;
        switch (source) {
            case "keyboard": return go ? "enter:Enter" : more ? "key:F" : "escape:Esc";
            case "remote": return go ? "remote_ok:OK" : more ? "remote_hold_ok:Hold OK" : "remote_back:Back";
            case "pad": {
                // South, east, north as each style labels them.
                String[] faces = vendor == SONY ? new String[] {"×", "○", "△"}
                        : vendor == NINTENDO ? new String[] {"B", "A", "X"} : new String[] {"A", "B", "Y"};
                if (more) return "button:" + faces[2];
                return "button:" + faces[go != swap ? 0 : 1];
            }
            default: return null;
        }
    }

    /** The vendor of the device behind a key, or 0: a pad's face buttons have that vendor's labels. */
    public static int vendorOf(KeyEvent event) {
        InputDevice device = event.getDevice();
        return device == null ? 0 : device.getVendorId();
    }

    /** True for the keys that do More on a card: a pad's Y, a keyboard's F. (X and Menu do too; a held OK is a long press.) */
    public static boolean isMoreKey(KeyEvent event) {
        int keyCode = event.getKeyCode();
        return keyCode == KeyEvent.KEYCODE_BUTTON_Y || (keyCode == KeyEvent.KEYCODE_F && event.hasNoModifiers());
    }

    /** A key that moves focus. */
    public static boolean isDpad(int keyCode) {
        return keyCode == android.view.KeyEvent.KEYCODE_DPAD_UP || keyCode == android.view.KeyEvent.KEYCODE_DPAD_DOWN
                || keyCode == android.view.KeyEvent.KEYCODE_DPAD_LEFT || keyCode == android.view.KeyEvent.KEYCODE_DPAD_RIGHT;
    }
}
