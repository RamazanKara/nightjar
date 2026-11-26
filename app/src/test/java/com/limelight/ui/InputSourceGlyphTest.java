package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNull;

import android.app.Application;
import android.content.Context;
import android.preference.PreferenceManager;

import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

import org.junit.After;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

/** A hint names what the device in the hand has, by nightjar-policy's input_glyphs. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 33, application = Application.class, qualifiers = "en")
public class InputSourceGlyphTest {
    private static final int XBOX = 0x045E;
    private final Context context = RuntimeEnvironment.getApplication();

    @After
    public void clearSwap() {
        PreferenceManager.getDefaultSharedPreferences(context).edit().clear().commit();
    }

    private void swap(boolean on) {
        PreferenceManager.getDefaultSharedPreferences(context).edit().putBoolean(InputSource.FLIP_FACE_BUTTONS, on).commit();
    }

    @Test
    public void eachDeviceNamesItsOwnKeys() {
        assertEquals("B", InputSource.PAD.goKey(context, InputSource.NINTENDO));
        assertEquals("X", InputSource.PAD.moreKey(context, InputSource.NINTENDO));
        assertEquals("×", InputSource.PAD.goKey(context, InputSource.SONY));
        assertEquals("△", InputSource.PAD.moreKey(context, InputSource.SONY));
        assertEquals("A", InputSource.PAD.goKey(context, XBOX));
        assertEquals("Y", InputSource.PAD.moreKey(context, 0));
        assertEquals("Enter", InputSource.KEYBOARD.goKey(context, 0));
        assertEquals("F", InputSource.KEYBOARD.moreKey(context, 0));
        assertEquals("Esc", InputSource.KEYBOARD.backKey(context, 0));
        assertEquals(context.getString(com.limelight.R.string.console_hint_key_ok), InputSource.REMOTE.goKey(context, 0));
        assertEquals(context.getString(com.limelight.R.string.console_hint_key_hold_ok), InputSource.REMOTE.moreKey(context, 0));
        assertNull(InputSource.NONE.goKey(context, 0));
        assertNull(InputSource.NONE.moreKey(context, 0));
    }

    @Test
    public void theSwapSettingTradesGoAndBack() {
        swap(true);
        assertEquals("A", InputSource.PAD.goKey(context, InputSource.NINTENDO));
        assertEquals("B", InputSource.PAD.backKey(context, InputSource.NINTENDO));
        assertEquals("X", InputSource.PAD.moreKey(context, InputSource.NINTENDO));
        assertEquals("B", InputSource.PAD.goKey(context, XBOX));
        assertEquals("○", InputSource.PAD.goKey(context, InputSource.SONY));
        assertEquals("Enter", InputSource.KEYBOARD.goKey(context, 0));
    }

    @Test
    public void theMirrorMatchesThePolicy() {
        for (String source : new String[] {"touch", "mouse", "keyboard", "remote", "pad", "glove"}) {
            for (int vendor : new int[] {0, XBOX, InputSource.SONY, InputSource.NINTENDO, 0x28DE}) {
                for (String hint : new String[] {"go", "more", "back", "jump"}) {
                    for (boolean swap : new boolean[] {false, true}) {
                        assertEquals(source + vendor + hint + swap, NightjarUiPolicyJni.inputGlyph(source, vendor, hint, swap),
                                InputSource.glyphMirror(source, vendor, hint, swap));
                    }
                }
            }
        }
    }

    @Test
    public void theMirrorAnswersWithoutTheLibrary() {
        NightjarJni.forceUnavailableForTest(new UnsatisfiedLinkError("test"));
        try {
            assertEquals("B", InputSource.PAD.goKey(context, InputSource.NINTENDO));
            assertEquals("F", InputSource.KEYBOARD.moreKey(context, 0));
        } finally {
            NightjarJni.forceUnavailableForTest(null);
        }
    }
}
