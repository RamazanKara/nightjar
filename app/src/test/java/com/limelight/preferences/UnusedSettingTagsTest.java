package com.limelight.preferences;

import android.app.Application;
import android.content.SharedPreferences;
import android.preference.Preference;
import android.preference.PreferenceManager;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;

import static org.junit.Assert.*;

/** A setting that works only in some setups says why where it does nothing (client/AGENTS.md). */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class UnusedSettingTagsTest {
    private SharedPreferences prefs;

    @Before
    public void clear() {
        prefs = PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication());
        prefs.edit().clear().commit();
    }

    private String reason(String key) {
        return SettingRules.unusedReason(RuntimeEnvironment.getApplication(), key, prefs);
    }

    @Test
    public void compatibilityVideoViewNamesWhatKeepsTheStreamsOwnSurface() {
        assertNull(reason("checkbox_texture_view"));
        prefs.edit().putBoolean("checkbox_enable_hdr", true).commit();
        assertEquals("Not used with HDR", reason("checkbox_texture_view"));
        assertFalse(PreferenceConfiguration.readPreferences(RuntimeEnvironment.getApplication()).useTextureView);
        prefs.edit().putBoolean("checkbox_enable_hdr", false).putBoolean("checkbox_enable_sdr_10bit", true).commit();
        assertTrue(reason("checkbox_texture_view").startsWith("Not used with "));
        prefs.edit().putBoolean("checkbox_enable_sdr_10bit", false).putString("video_format", "forcepyrowave").commit();
        assertEquals("Not used with PyroWave", reason("checkbox_texture_view"));
    }

    @Test
    public void thermalProtectionNamesTheThreeItActsThrough() {
        assertEquals("Not used without Lower bitrate, Automatic quality or Boost GPU performance for PyroWave",
                reason(PreferenceConfiguration.THERMAL_PROTECTION_PREF_STRING));
        // Lower it sooner: its controller follows no heat on a PC that takes rate changes.
        prefs.edit().putString(AutoBitrateChoice.SETTINGS_KEY, AutoBitrateChoice.SOONER).commit();
        assertEquals("Not used with Lower it sooner on a PC that takes bitrate changes",
                reason(PreferenceConfiguration.THERMAL_PROTECTION_PREF_STRING));
        prefs.edit().clear().putString("adaptive_quality_mode", "motion").commit();
        assertNull(reason(PreferenceConfiguration.THERMAL_PROTECTION_PREF_STRING));
        // The boost acts only on PyroWave.
        prefs.edit().clear().putBoolean(PreferenceConfiguration.GPU_MAX_CLOCKS_PREF_STRING, true).commit();
        assertNotNull(reason(PreferenceConfiguration.THERMAL_PROTECTION_PREF_STRING));
        prefs.edit().putString("video_format", "forcepyrowave").commit();
        assertNull(reason(PreferenceConfiguration.THERMAL_PROTECTION_PREF_STRING));
    }

    @Test
    public void rowsThatAnotherSwitchOverrulesNameIt() {
        prefs.edit().putString("adaptive_quality_mode", "motion").commit();
        assertNull(reason("adaptive_quality_mode"));
        prefs.edit().putString("video_format", "forcepyrowave").commit();
        assertEquals("Not used with PyroWave", reason("adaptive_quality_mode"));
        prefs.edit().putString("adaptive_quality_mode", "off").commit();
        assertNull("Off needs no reason", reason("adaptive_quality_mode"));

        assertNull(reason("checkbox_onscreen_own_player"));
        prefs.edit().putBoolean("checkbox_only_show_L3R3", true).commit();
        assertTrue(reason("checkbox_onscreen_own_player").startsWith("Not used with "));

        assertNull(reason("checkbox_audio_passthrough"));
        prefs.edit().putBoolean("checkbox_mute_audio", true).commit();
        assertTrue(reason("checkbox_audio_passthrough").startsWith("Not used with "));
    }

    /**
     * The default screen's mode now at {@code hz}, and its other modes at {@code others}, all of its size:
     * fractional V-Sync's row names the refresh of the mode the stream switches to (DisplayModeChoice).
     */
    private static void screenAt(float hz, float... others) {
        android.view.Display display = PreferenceConfiguration.defaultDisplay(RuntimeEnvironment.getApplication());
        org.robolectric.Shadows.shadowOf(display).setRefreshRate(hz);
        android.view.Display.Mode now = display.getMode();
        android.view.Display.Mode[] modes = new android.view.Display.Mode[others.length + 1];
        modes[0] = mode(now.getModeId(), now.getPhysicalWidth(), now.getPhysicalHeight(), hz);
        for (int i = 0; i < others.length; i++) {
            modes[i + 1] = mode(now.getModeId() + 100 + i, now.getPhysicalWidth(), now.getPhysicalHeight(), others[i]);
        }
        org.robolectric.shadows.ShadowDisplayManager.setSupportedModes(android.view.Display.DEFAULT_DISPLAY, modes);
    }

    /** Display.Mode's constructor is hidden from apps. */
    private static android.view.Display.Mode mode(int id, int width, int height, float hz) {
        try {
            java.lang.reflect.Constructor<android.view.Display.Mode> make = android.view.Display.Mode.class
                    .getDeclaredConstructor(int.class, int.class, int.class, float.class);
            make.setAccessible(true);
            return make.newInstance(id, width, height, hz);
        } catch (ReflectiveOperationException e) {
            throw new AssertionError(e);
        }
    }

    @Test
    public void fractionalVsyncNamesWhatShowsFramesWithoutIt() {
        String key = SettingRules.FRACTIONAL_VSYNC;
        screenAt(120f);
        assertNull("Lowest latency, the default", reason(key));
        // Balanced shows a stream at a half to a quarter of the refresh from the vsync callback, at that
        // cadence already, and sends no phase report there.
        prefs.edit().putString("frame_pacing", "balanced").commit();
        assertTrue(reason(key).startsWith("Not used with Balanced"));
        prefs.edit().putBoolean("checkbox_drop_late_frames", false).commit();
        assertTrue("Balanced, keep every frame", reason(key).startsWith("Not used with Balanced"));
        prefs.edit().putString("frame_pacing", "smoothness").commit();
        assertEquals("Not used with Smoothest video", reason(key));
        prefs.edit().putString("frame_pacing", "cap-fps").commit();
        assertEquals("Not used with Balanced with FPS limit", reason(key));
        prefs.edit().putString("frame_pacing", "latency").putBoolean("checkbox_host_frame_times", true).commit();
        assertEquals("Not used with PC’s frame times", reason(key));
        prefs.edit().clear().putBoolean(PreferenceConfiguration.VRR_PREF_STRING, true).commit();
        if (SettingRules.vrrOn(RuntimeEnvironment.getApplication(), prefs)) {
            assertEquals("Not used with Variable refresh rate", reason(key));
        } else {
            assertNull("VRR not offered here: the pacing decides", reason(key));
        }
    }

    @Test
    public void fractionalVsyncNamesTheScreensRefreshWhenItIsNoMultiple() {
        String key = SettingRules.FRACTIONAL_VSYNC;
        prefs.edit().putString(PreferenceConfiguration.FPS_PREF_STRING, "60").commit();
        screenAt(60f);
        assertEquals("the stream's own rate", "Not used at this screen’s 60 Hz: that isn’t 2 to 4 times the frame rate", reason(key));
        screenAt(144f);
        assertEquals("Not used at this screen’s 144 Hz: that isn’t 2 to 4 times the frame rate", reason(key));
        screenAt(119.88f);
        assertNull("within the rate tolerance of twice 60", reason(key));
        prefs.edit().putString(PreferenceConfiguration.FPS_PREF_STRING, "30").commit();
        assertNull("four times", reason(key));
        prefs.edit().putString(PreferenceConfiguration.FPS_PREF_STRING, "24").commit();
        assertNotNull("five times is past the hold's 4", reason(key));
    }

    /** The stream takes its own mode: the highest refresh that is a whole multiple of the frame rate. */
    @Test
    public void fractionalVsyncNamesTheRefreshTheStreamSwitchesTo() {
        String key = SettingRules.FRACTIONAL_VSYNC;
        prefs.edit().putString(PreferenceConfiguration.FPS_PREF_STRING, "60").commit();
        screenAt(144f, 120f, 60f);
        assertNull("144 Hz now, but the stream holds at 120 Hz", reason(key));
        screenAt(90f, 60f);
        assertEquals("60 Hz, the only multiple, even below the 90 Hz now",
                "Not used at the 60 Hz streams switch this screen to: that isn’t 2 to 4 times the frame rate", reason(key));
        screenAt(144f, 100f);
        assertEquals("no multiple: the stream keeps the highest", "Not used at this screen’s 144 Hz: that isn’t 2 to 4 times the frame rate", reason(key));
        screenAt(60f, 120f);
        assertNull("60 Hz now, but the stream raises it to 120 Hz", reason(key));
        // With "Reduce refresh rate" the stream asks Android for the frame rate, and Android picks the refresh.
        prefs.edit().putBoolean(PreferenceConfiguration.REDUCE_REFRESH_RATE_PREF_STRING, true).commit();
        assertNull(reason(key));
        prefs.edit().putBoolean(PreferenceConfiguration.REDUCE_REFRESH_RATE_PREF_STRING, false)
                .putString(PreferenceConfiguration.FPS_PREF_STRING, "120").commit();
        screenAt(60f);
        assertNull("no mode reaches 120 fps: the stream asks Android for it", reason(key));
    }

    @Test
    @Config(sdk = 29)
    public void fractionalVsyncNamesTheRendererThatKeepsNoTurn() {
        String key = SettingRules.FRACTIONAL_VSYNC;
        screenAt(120f);
        assertNull(reason(key));
        prefs.edit().putBoolean(PreferenceConfiguration.FRAME_WARP_PREF_STRING, true).commit();
        assertEquals("Not used with Frame Warp", reason(key));
        prefs.edit().putBoolean(PreferenceConfiguration.VULKAN_RENDER_PREF_STRING, true).commit();
        assertTrue(reason(key).startsWith("Not used with "));
        assertNotEquals("Not used with Frame Warp", reason(key));
    }

    @Test
    @Config(sdk = 29)
    public void upscalingIsUnusedWithFrameWarpAsWellAsVulkan() {
        assertTrue(SettingRules.upscalingApplies(prefs));
        prefs.edit().putBoolean(PreferenceConfiguration.FRAME_WARP_PREF_STRING, true).commit();
        assertFalse(SettingRules.upscalingApplies(prefs));
    }

    @Test
    public void fractionalVsyncRowStaysAndSaysWhy() {
        // The PC's frame times: a first start's defaults rewrite the pacing itself (NATIVE_DEFAULTS_VERSION).
        prefs.edit().putBoolean("checkbox_host_frame_times", true).commit();
        try (ActivityController<StreamSettings> controller = Robolectric.buildActivity(StreamSettings.class).setup()) {
            StreamSettings screen = controller.get();
            Preference row = SettingsStructureTest.open(screen, "display").findPreference(SettingRules.FRACTIONAL_VSYNC);
            assertNotNull("shown, not hidden (a hidden row is detached)", row);
            assertEquals("Not used with PC’s frame times", String.valueOf(row.getSummary()));
        }
    }

    @Test
    public void theRowShowsItsReason() {
        prefs.edit().putBoolean("checkbox_enable_hdr", true).commit();
        try (ActivityController<StreamSettings> controller = Robolectric.buildActivity(StreamSettings.class).setup()) {
            StreamSettings screen = controller.get();
            screen.moreOpen.add("advanced");
            Preference view = SettingsStructureTest.open(screen, "advanced").findPreference("checkbox_texture_view");
            assertEquals("Not used with HDR", String.valueOf(view.getSummary()));
        }
    }
}
