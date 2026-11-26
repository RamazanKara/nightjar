package com.limelight.preferences;

import android.content.SharedPreferences;
import org.junit.Test;
import java.lang.reflect.Proxy;
import java.util.HashMap;
import java.util.Map;
import static org.junit.Assert.*;

public class StreamPresetTest {
    private static final String PHONE = "2400x1080";

    @Test
    public void nativeUsesThePanelAndTurnsEveryLatencySwitchOn() {
        Map<String, Object> values = new HashMap<>();
        for (String key : StreamPreset.LATENCY_SWITCHES) values.put(key, false);
        values.put("checkbox_vrr", true);
        values.put("checkbox_real_vrr", false);
        values.put("upscaling_mode", "fsr1");
        values.put("frame_pacing", "balanced");
        int[] applies = {0};
        StreamPreset.NATIVE.apply(preferences(values, applies), PHONE, 120);
        // Variable refresh rate Off: Real VRR (Fastest's off) stays for when VRR comes back.
        assertEquals(false, values.get("checkbox_real_vrr"));
        assertEquals(1, applies[0]);
        assertEquals(PHONE, values.get("list_resolution"));
        assertEquals("120", values.get("list_fps"));
        assertEquals(PreferenceConfiguration.getDefaultBitrate(PHONE, "120"), values.get("seekbar_bitrate_kbps"));
        assertEquals(false, values.get("checkbox_vrr"));
        assertEquals("off", values.get("upscaling_mode"));
        assertEquals("latency", values.get("frame_pacing"));
        assertEquals(false, values.get("checkbox_reduce_refresh_rate"));
        assertEquals(true, values.get("checkbox_codec_performance"));
        for (String key : StreamPreset.LATENCY_SWITCHES) assertEquals(key, true, values.get(key));
    }

    @Test
    public void batterySaverKeepsNativeResolutionAt30Fps() {
        Map<String, Object> values = new HashMap<>();
        StreamPreset.BATTERY_SAVER.apply(preferences(values, new int[1]), PHONE, 144);
        assertEquals(PHONE, values.get("list_resolution"));
        assertEquals("30", values.get("list_fps"));
        assertEquals("cap-fps", values.get("frame_pacing"));
        assertEquals(true, values.get("checkbox_reduce_refresh_rate"));
        assertEquals(false, values.get("checkbox_codec_performance"));
        assertEquals(false, values.get(PreferenceConfiguration.GPU_MAX_CLOCKS_PREF_STRING));
    }

    @Test
    public void presetsLeaveCodecHdrAnd444Alone() {
        for (StreamPreset preset : StreamPreset.values()) {
            for (String codec : new String[] {"auto", "forceav1", "forceh265", "neverh265", "forcepyrowave"}) {
                Map<String, Object> values = new HashMap<>();
                values.put("video_format", codec);
                values.put("checkbox_enable_hdr", true);
                values.put("checkbox_yuv444", true);
                preset.apply(preferences(values, new int[1]), PHONE, 120);
                assertEquals(codec, values.get("video_format"));
                assertEquals(true, values.get("checkbox_enable_hdr"));
                assertEquals(true, values.get("checkbox_yuv444"));
            }
        }
    }

    @Test
    public void presetsFollowPanelRateWithinHostLimitExceptBatterySaver() {
        for (StreamPreset preset : StreamPreset.values()) {
            for (float rate : new float[] {59.94f, 90, 119.88f, 144, 165, 240, 1200}) {
                Map<String, Object> values = new HashMap<>();
                preset.apply(preferences(values, new int[1]), PHONE, rate);
                int screen = Math.min(1000, Math.round(rate));
                int expected = preset == StreamPreset.BATTERY_SAVER ? 30
                        : preset == StreamPreset.WIFI_AWAY ? Math.min(60, screen) : screen;
                assertEquals(Integer.toString(expected), values.get("list_fps"));
            }
        }
    }

    @Test
    public void matchingReflectsValuesRatherThanLastSelectedPreset() {
        Map<String, Object> values = new HashMap<>();
        SharedPreferences preferences = preferences(values, new int[1]);
        for (StreamPreset selected : StreamPreset.values()) {
            selected.apply(preferences, PHONE, 120);
            for (StreamPreset candidate : StreamPreset.values()) {
                assertEquals(selected == candidate, candidate.matches(preferences, PHONE, 120, true));
            }
            assertFalse(selected.matches(preferences, "1920x1080", 120, true));
            values.put("seekbar_bitrate_kbps", (Integer) values.get("seekbar_bitrate_kbps") + 500);
            assertFalse(selected.matches(preferences, PHONE, 120, true));
            selected.apply(preferences, PHONE, 120);
            values.put("upscaling_mode", "sgsr1");
            assertFalse(selected.matches(preferences, PHONE, 120, true));
        }
    }

    @Test
    public void turningOffALatencySwitchMakesNativeCustom() {
        for (String key : StreamPreset.LATENCY_SWITCHES) {
            Map<String, Object> values = new HashMap<>();
            SharedPreferences preferences = preferences(values, new int[1]);
            StreamPreset.NATIVE.apply(preferences, PHONE, 120);
            values.put(key, false);
            assertFalse(key, StreamPreset.NATIVE.matches(preferences, PHONE, 120, true));
        }
    }

    @Test
    public void nativeLeavesTheOptInGpuClocksAloneAndBatterySaverTurnsThemOff() {
        String gpu = PreferenceConfiguration.GPU_MAX_CLOCKS_PREF_STRING;
        for (Boolean stored : new Boolean[] {null, true, false}) {
            Map<String, Object> values = new HashMap<>();
            if (stored != null) values.put(gpu, stored);
            SharedPreferences preferences = preferences(values, new int[1]);
            StreamPreset.NATIVE.apply(preferences, PHONE, 120);
            assertEquals(stored, values.get(gpu));
            assertTrue(StreamPreset.NATIVE.matches(preferences, PHONE, 120, true));

            StreamPreset.BATTERY_SAVER.apply(preferences, PHONE, 120);
            assertEquals(false, values.get(gpu));
            assertTrue(StreamPreset.BATTERY_SAVER.matches(preferences, PHONE, 120, true));
            values.put(gpu, true);
            assertFalse(StreamPreset.BATTERY_SAVER.matches(preferences, PHONE, 120, true));
        }
        assertFalse(java.util.Arrays.asList(StreamPreset.LATENCY_SWITCHES).contains(gpu));
    }

    @Test
    public void freshDefaultsMatchNative() {
        // Values never written fall back to their defaults, which are Native's.
        Map<String, Object> values = new HashMap<>();
        values.put("list_resolution", PHONE);
        values.put("list_fps", "120");
        assertTrue(StreamPreset.NATIVE.matches(preferences(values, new int[1]), PHONE, 120, true));
    }

    @Test
    public void aVrrChoiceKeptWhereVrrIsntOfferedLeavesThePresetChosen() {
        Map<String, Object> values = new HashMap<>();
        SharedPreferences preferences = preferences(values, new int[1]);
        StreamPreset.NATIVE.apply(preferences, PHONE, 120);
        values.put("checkbox_vrr", true);
        assertTrue(StreamPreset.NATIVE.matches(preferences, PHONE, 120, false));
        assertFalse(StreamPreset.NATIVE.matches(preferences, PHONE, 120, true));
    }

    @Test
    public void migrationMovesOldDefaultsToNativeOnce() {
        Map<String, Object> values = new HashMap<>();
        // What PcView stored on first launch before this change.
        values.put("list_resolution", "1280x720");
        values.put("list_fps", "60");
        values.put("seekbar_bitrate_kbps", PreferenceConfiguration.getDefaultBitrate("1280x720", "60"));
        values.put("frame_pacing", "balanced");
        values.put("video_format", "forceav1");
        values.put("checkbox_enable_hdr", true);
        values.put("checkbox_vrr", true);
        for (String key : StreamPreset.LATENCY_SWITCHES) values.put(key, false);
        SharedPreferences preferences = preferences(values, new int[1]);

        assertTrue(PreferenceConfiguration.migrateToNativeDefaults(preferences, PHONE, 120));
        assertEquals(PHONE, values.get("list_resolution"));
        assertEquals("120", values.get("list_fps"));
        assertEquals(PreferenceConfiguration.getDefaultBitrate(PHONE, "120"), values.get("seekbar_bitrate_kbps"));
        // The migrated bitrate is a default, so it keeps following resolution and frame rate.
        assertEquals(false, values.get(PreferenceConfiguration.BITRATE_USER_SET_PREF_STRING));
        assertEquals("latency", values.get("frame_pacing"));
        for (String key : StreamPreset.LATENCY_SWITCHES) assertEquals(key, true, values.get(key));
        assertEquals("forceav1", values.get("video_format"));
        assertEquals(true, values.get("checkbox_enable_hdr"));
        assertEquals(true, values.get("checkbox_vrr"));
        assertEquals(PreferenceConfiguration.FOLLOW_SCREEN_VERSION, values.get("prefs_version"));
        assertEquals(true, values.get(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING));
        // Max GPU clocks is opt-in: the migration does not turn it on.
        assertFalse(values.containsKey(PreferenceConfiguration.GPU_MAX_CLOCKS_PREF_STRING));

        // Later changes stick.
        values.put("list_resolution", "1920x1080");
        values.put("checkbox_unbatched_input", false);
        assertFalse(PreferenceConfiguration.migrateToNativeDefaults(preferences, PHONE, 120));
        assertEquals("1920x1080", values.get("list_resolution"));
        assertEquals(false, values.get("checkbox_unbatched_input"));
    }

    @Test
    public void resolutionSavedOnAnotherPanelFollowsTheScreenAfterUpdating() {
        // A 0.4.0 test.6 to test.8 install: native defaults already applied, at a size from another panel.
        Map<String, Object> values = new HashMap<>();
        values.put("prefs_version", PreferenceConfiguration.NATIVE_DEFAULTS_VERSION);
        values.put("list_resolution", "1920x1080");
        values.put("checkbox_unbatched_input", false);
        SharedPreferences preferences = preferences(values, new int[1]);
        assertTrue(PreferenceConfiguration.migrateToNativeDefaults(preferences, PHONE, 120));
        assertEquals(PreferenceConfiguration.FOLLOW_SCREEN_VERSION, values.get("prefs_version"));
        // Only the resolution changes meaning; other choices made since stay.
        assertEquals(false, values.get("checkbox_unbatched_input"));
        assertEquals("1920x1080", values.get("list_resolution"));
        assertEquals(PHONE, PreferenceConfiguration.effectiveResolution(preferences, "1920x1080", PHONE));
        // The inner screen of a foldable gets its own size at the next stream.
        assertEquals("2184x1968", PreferenceConfiguration.effectiveResolution(preferences, "1920x1080", "2184x1968"));
        assertFalse(PreferenceConfiguration.migrateToNativeDefaults(preferences, PHONE, 120));
    }

    @Test
    public void aFixedResolutionStaysFixed() {
        Map<String, Object> values = new HashMap<>();
        values.put(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING, false);
        SharedPreferences preferences = preferences(values, new int[1]);
        assertEquals("1920x1080", PreferenceConfiguration.effectiveResolution(preferences, "1920x1080", PHONE));
        // An unreadable screen keeps the saved size.
        values.put(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING, true);
        assertEquals("1920x1080", PreferenceConfiguration.effectiveResolution(preferences, "1920x1080", null));
    }

    @Test
    public void presetsFollowTheScreenButWifiAwaysFixedTwoThirds() {
        for (StreamPreset preset : StreamPreset.values()) {
            Map<String, Object> values = new HashMap<>();
            values.put(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING, preset != StreamPreset.WIFI_AWAY);
            preset.apply(preferences(values, new int[1]), PHONE, 120);
            assertEquals(preset != StreamPreset.WIFI_AWAY, values.get(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING));
        }
    }

    @Test
    public void theFourPresetsInTheChipsOrder() {
        assertArrayEquals(new StreamPreset[] {StreamPreset.NATIVE, StreamPreset.SMOOTH_TV, StreamPreset.WIFI_AWAY, StreamPreset.BATTERY_SAVER},
                StreamPreset.values());
    }

    @Test
    public void smoothTvKeepsTheScreenAndPacesForSmoothness() {
        Map<String, Object> values = new HashMap<>();
        for (String key : StreamPreset.LATENCY_SWITCHES) values.put(key, false);
        values.put("frame_pacing", "latency");
        values.put(PreferenceConfiguration.HOST_FRAME_TIMES_PREF_STRING, true);
        values.put("checkbox_reduce_refresh_rate", true);
        values.put("checkbox_codec_performance", false);
        SharedPreferences preferences = preferences(values, new int[1]);
        StreamPreset.SMOOTH_TV.apply(preferences, PHONE, 120);
        assertEquals(PHONE, values.get("list_resolution"));
        assertEquals(true, values.get(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING));
        assertEquals("120", values.get("list_fps"));
        assertEquals(PreferenceConfiguration.getDefaultBitrate(PHONE, "120"), values.get("seekbar_bitrate_kbps"));
        assertEquals("smoothness", values.get("frame_pacing"));
        assertEquals(false, values.get(PreferenceConfiguration.HOST_FRAME_TIMES_PREF_STRING));
        assertEquals(false, values.get("checkbox_vrr"));
        assertEquals("off", values.get("upscaling_mode"));
        // Power like Native; the latency switches and GPU clocks stay as they were.
        assertEquals(false, values.get("checkbox_reduce_refresh_rate"));
        assertEquals(true, values.get("checkbox_codec_performance"));
        assertFalse(values.containsKey(PreferenceConfiguration.GPU_MAX_CLOCKS_PREF_STRING));
        for (String key : StreamPreset.LATENCY_SWITCHES) assertEquals(key, false, values.get(key));
        assertTrue(StreamPreset.SMOOTH_TV.matches(preferences, PHONE, 120, true));
    }

    @Test
    public void wifiAwayStreamsTwoThirdsAt60FpsBalanced() {
        Map<String, Object> values = new HashMap<>();
        for (String key : StreamPreset.LATENCY_SWITCHES) values.put(key, false);
        SharedPreferences preferences = preferences(values, new int[1]);
        StreamPreset.WIFI_AWAY.apply(preferences, PHONE, 120);
        assertEquals("1600x720", values.get("list_resolution"));
        assertEquals(false, values.get(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING));
        assertEquals("60", values.get("list_fps"));
        assertEquals(PreferenceConfiguration.getDefaultBitrate("1600x720", "60"), values.get("seekbar_bitrate_kbps"));
        assertEquals("balanced", values.get("frame_pacing"));
        assertEquals(false, values.get(PreferenceConfiguration.HOST_FRAME_TIMES_PREF_STRING));
        assertEquals(false, values.get("checkbox_reduce_refresh_rate"));
        assertEquals(true, values.get("checkbox_codec_performance"));
        for (String key : StreamPreset.LATENCY_SWITCHES) assertEquals(key, false, values.get(key));
        // Balanced keeping every frame (the newest-frame switch off) still reads as Wi-Fi / Away.
        assertTrue(StreamPreset.WIFI_AWAY.matches(preferences, PHONE, 120, true));
        values.put(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING, true);
        assertFalse("the screen's own size is not two thirds", StreamPreset.WIFI_AWAY.matches(preferences, PHONE, 120, true));

        // Never above the panel's top rate.
        StreamPreset.WIFI_AWAY.apply(preferences, PHONE, 50);
        assertEquals("50", values.get("list_fps"));
        assertTrue(StreamPreset.WIFI_AWAY.matches(preferences, PHONE, 50, true));
        assertFalse(StreamPreset.WIFI_AWAY.matches(preferences, PHONE, 120, true));
    }

    @Test
    public void twoThirdsIsTheResolutionPolicysStep() {
        // nightjar_policy::resolution_scale::scaled(w, h, 67).
        assertEquals("1280x720", StreamPreset.twoThirds("1920x1080"));
        assertEquals("2560x1440", StreamPreset.twoThirds("3840x2160"));
        assertEquals("1600x720", StreamPreset.twoThirds("2400x1080"));
        assertEquals("1456x1312", StreamPreset.twoThirds("2184x1968"));
        assertEquals("320x240", StreamPreset.twoThirds("400x300"));
        assertEquals("garbage", StreamPreset.twoThirds("garbage"));
    }

    @Test
    public void nativeAndBatterySaverWriteExactlyWhatTheyDid() {
        Map<String, Object> nativeValues = new HashMap<>();
        StreamPreset.NATIVE.apply(preferences(nativeValues, new int[1]), PHONE, 120);
        Map<String, Object> expectedNative = new HashMap<>();
        expectedNative.put("list_resolution", PHONE);
        expectedNative.put(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING, true);
        expectedNative.put("list_fps", "120");
        expectedNative.put("seekbar_bitrate_kbps", PreferenceConfiguration.getDefaultBitrate(PHONE, "120"));
        expectedNative.put(PreferenceConfiguration.BITRATE_USER_SET_PREF_STRING, false);
        expectedNative.put("checkbox_vrr", false);
        expectedNative.put("upscaling_mode", "off");
        expectedNative.put("frame_pacing", "latency");
        expectedNative.put(PreferenceConfiguration.HOST_FRAME_TIMES_PREF_STRING, false);
        expectedNative.put("checkbox_reduce_refresh_rate", false);
        expectedNative.put("checkbox_codec_performance", true);
        for (String key : StreamPreset.LATENCY_SWITCHES) {
            if (StreamPreset.setsLatencySwitch(key)) expectedNative.put(key, true);
        }
        assertEquals(expectedNative, nativeValues);

        Map<String, Object> saverValues = new HashMap<>();
        StreamPreset.BATTERY_SAVER.apply(preferences(saverValues, new int[1]), PHONE, 120);
        Map<String, Object> expectedSaver = new HashMap<>();
        expectedSaver.put("list_resolution", PHONE);
        expectedSaver.put(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING, true);
        expectedSaver.put("list_fps", "30");
        expectedSaver.put("seekbar_bitrate_kbps", PreferenceConfiguration.getDefaultBitrate(PHONE, "30"));
        expectedSaver.put(PreferenceConfiguration.BITRATE_USER_SET_PREF_STRING, false);
        expectedSaver.put("checkbox_vrr", false);
        expectedSaver.put("upscaling_mode", "off");
        expectedSaver.put("frame_pacing", "cap-fps");
        expectedSaver.put(PreferenceConfiguration.HOST_FRAME_TIMES_PREF_STRING, false);
        expectedSaver.put("checkbox_reduce_refresh_rate", true);
        expectedSaver.put("checkbox_codec_performance", false);
        expectedSaver.put(PreferenceConfiguration.GPU_MAX_CLOCKS_PREF_STRING, false);
        assertEquals(expectedSaver, saverValues);
    }

    @Test
    public void eachPresetMatchesOnlyItselfWhateverTheLatencySwitches() {
        for (boolean switches : new boolean[] {false, true}) {
            for (float rate : new float[] {50, 60, 120}) {
                for (StreamPreset selected : StreamPreset.values()) {
                    Map<String, Object> values = new HashMap<>();
                    for (String key : StreamPreset.LATENCY_SWITCHES) values.put(key, switches);
                    SharedPreferences preferences = preferences(values, new int[1]);
                    selected.apply(preferences, PHONE, rate);
                    for (StreamPreset candidate : StreamPreset.values()) {
                        assertEquals(selected + " as " + candidate + " at " + rate, selected == candidate,
                                candidate.matches(preferences, PHONE, rate, true));
                    }
                }
            }
        }
    }

    @Test
    public void matchesFollowsScreenTreatsStaleStoredResolutionAsNative() {
        Map<String, Object> values = new HashMap<>();
        SharedPreferences preferences = preferences(values, new int[1]);
        StreamPreset.NATIVE.apply(preferences, PHONE, 120);
        values.put("list_resolution", "1280x720");
        assertTrue(StreamPreset.NATIVE.matches(preferences, PHONE, 120, true));
    }

    @Test
    public void migrationWaitsWhenTheScreenIsUnknown() {
        Map<String, Object> values = new HashMap<>();
        values.put("list_resolution", "1280x720");
        SharedPreferences preferences = preferences(values, new int[1]);
        assertFalse(PreferenceConfiguration.migrateToNativeDefaults(preferences, null, 60));
        assertEquals("1280x720", values.get("list_resolution"));
        assertFalse(values.containsKey("prefs_version"));
    }

    private SharedPreferences preferences(Map<String, Object> values, int[] applies) {
        SharedPreferences.Editor editor = (SharedPreferences.Editor) Proxy.newProxyInstance(
                getClass().getClassLoader(), new Class<?>[] {SharedPreferences.Editor.class}, (proxy, method, args) -> {
                    if (method.getName().startsWith("put")) {
                        values.put((String) args[0], args[1]);
                        return proxy;
                    }
                    if (method.getName().equals("remove")) {
                        values.remove((String) args[0]);
                        return proxy;
                    }
                    if (method.getName().equals("apply")) {
                        applies[0]++;
                    }
                    return null;
                });
        return (SharedPreferences) Proxy.newProxyInstance(
                getClass().getClassLoader(), new Class<?>[] {SharedPreferences.class}, (proxy, method, args) -> {
                    if (method.getName().startsWith("get")) {
                        return values.getOrDefault(args[0], args[1]);
                    }
                    if (method.getName().equals("contains")) {
                        return values.containsKey(args[0]);
                    }
                    return editor;
                });
    }
}
