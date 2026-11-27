package com.limelight.preferences;

import android.app.Application;
import android.content.res.XmlResourceParser;
import android.preference.PreferenceManager;

import com.limelight.R;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.xmlpull.v1.XmlPullParser;

import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import static org.junit.Assert.*;

/**
 * The Controller section, the controller editor, gathers today's controller rows in one place without touching a
 * key or a default: a new install stores what 6.4 stored, an existing one keeps its values, and the PC receives the
 * same input either way.
 */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class ControllerEditorDefaultsTest {
    private static final String ANDROID = "http://schemas.android.com/apk/res/android";
    /** Every stored row the editor shows, with the default 6.4.4's preferences.xml gave it. */
    private static final String[][] DEFAULTS = {
            {"seekbar_deadzone", "7"}, {"seekbar_trigger_deadzone", "2"}, {"checkbox_gamepad_motion_sensors", "true"},
            {"list_rumble_feel", "standard"},
            {"checkbox_flip_face_buttons", "false"}, {"list_gamepad_quit_combo", "off"},
            {"checkbox_gamepad_motion_fallback", "false"}, {"checkbox_vibrate_fallback", "false"},
            {"seekbar_vibrate_fallback_strength", "100"}, {"checkbox_mouse_emulation", "true"}, {"analog_scrolling", "right"},
            {"checkbox_mouse_emulation_fixed_step", "false"}, {"seekbar_mouse_emulation_sensitivity", "100"},
            {"checkbox_gamepad_touchpad_as_mouse", "false"},
            {"list_guide_menu_hold", "1000"}, {"checkbox_floating_menu_button", "false"},
            {"list_stream_menu_button_y", "default"}, {"list_stream_menu_button_x", "default"},
            {"list_stream_menu_button_lb", "default"}, {"list_stream_menu_button_rb", "default"},
            {"list_osc_stick_mode", "floating"}, {"seekbar_osc_stick_deadzone", "8"}, {"checkbox_vibrate_osc", "true"},
            {"checkbox_osc_auto_hide", "true"}, {"checkbox_only_show_L3R3", "false"}, {"checkbox_onscreen_own_player", "false"},
            {"checkbox_show_guide_button", "true"}, {"checkbox_show_touchpad_button", "false"},
            {"checkbox_osc_steam_controls", "false"}, {"seekbar_osc_opacity", "90"},
            {"checkbox_usb_driver", "true"}, {"checkbox_usb_bind_all", "false"}, {"checkbox_usb_dualsense", "false"},
            {"checkbox_usb_dualsense_haptics", "false"}, {"checkbox_usb_switch_pro", "false"},
            {"checkbox_usb_forwarding", "false"},
    };

    private static Application app() {
        return RuntimeEnvironment.getApplication();
    }

    /** The stored rows of the section's categories in preferences.xml and their defaults, in XML order. */
    private static Map<String, String> sectionDefaults(String section) throws Exception {
        List<String> categories = Arrays.asList(StreamSettings.SECTIONS.get(section));
        Map<String, String> found = new LinkedHashMap<>();
        String category = null;
        try (XmlResourceParser parser = app().getResources().getXml(R.xml.preferences)) {
            for (int event = parser.getEventType(); event != XmlPullParser.END_DOCUMENT; event = parser.next()) {
                if (event != XmlPullParser.START_TAG) continue;
                String key = parser.getAttributeValue(ANDROID, "key");
                if (parser.getName().equals("PreferenceCategory")) {
                    category = key;
                    continue;
                }
                String value = parser.getAttributeValue(ANDROID, "defaultValue");
                if (key != null && value != null && categories.contains(category)) found.put(key, value);
            }
        }
        return found;
    }

    @Test
    public void theEditorsRowsKeepTheirKeysAndDefaults() throws Exception {
        Map<String, String> expected = new LinkedHashMap<>();
        for (String[] row : DEFAULTS) expected.put(row[0], row[1]);
        assertEquals("every stored controller row, with the default it had", expected,
                sectionDefaults(StreamSettings.CONTROLLER_SECTION));
    }

    @Test
    public void aNewInstallReadsTheSameControllerSettings() {
        PreferenceManager.setDefaultValues(app(), R.xml.preferences, true);
        PreferenceConfiguration config = PreferenceConfiguration.readPreferences(app());
        assertEquals(7, config.deadzonePercentage);
        assertEquals(2, config.triggerDeadzonePercentage);
        assertTrue(config.gamepadMotionSensors);
        assertEquals("standard", config.rumbleFeel);
        // What the stream reads: controllers on, one player each, Xbox face buttons.
        assertTrue(config.sendControllers);
        assertTrue(config.multiController);
        assertFalse(config.flipFaceButtons);
        assertTrue(config.oscAutoHideOnController);
    }
}
