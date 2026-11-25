package com.limelight.preferences;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;

import android.content.Context;
import android.content.SharedPreferences;
import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import android.media.MediaFormat;
import android.preference.PreferenceManager;

import com.limelight.binding.video.DecoderBenchmark;
import com.limelight.console.HomeSettings;
import com.limelight.console.OpenPlay;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.MediaCodecInfoBuilder;
import org.robolectric.shadows.ShadowMediaCodecList;

import java.lang.reflect.Method;

/**
 * The simple layer on Android: a choice writes the existing settings, a changed advanced setting reads as
 * Custom, Back to recommended restores it, and console mode reuses Home. Uses real {@code nightjar_core} via JNI.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = android.app.Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class SimpleProfileTest {
    private Context context;
    private SharedPreferences prefs;

    @Before
    public void setUp() {
        context = RuntimeEnvironment.getApplication();
        prefs = PreferenceManager.getDefaultSharedPreferences(context);
        prefs.edit().clear().commit();
        SimpleProfile.forgetCapabilities();
        SimpleProfile.forgetCodecs();
    }

    @After
    public void tearDown() {
        if (settingsController != null) {
            settingsController.pause().stop().destroy();
            settingsController = null;
        }
        prefs.edit().clear().commit();
        context.getSharedPreferences("decoder_benchmark", Context.MODE_PRIVATE).edit().clear().commit();
        ShadowMediaCodecList.reset();
        SimpleProfile.forgetCapabilities();
        SimpleProfile.forgetCodecs();
    }

    @Test
    public void aChoiceWritesTheExistingSettingsAndKeepsTheOthers() {
        assertTrue(SimpleProfile.apply(context, SimpleProfile.SMOOTHNESS, "smoothest"));
        assertEquals("smoothness", prefs.getString(PreferenceConfiguration.FRAME_PACING_PREF_STRING, null));
        assertFalse(prefs.getBoolean(PreferenceConfiguration.VRR_PREF_STRING, true));
        assertEquals("smoothest", prefs.getString(SimpleProfile.SMOOTHNESS, null));

        // Picture quality changes the bitrate and codec and leaves the pacing as the first choice made it.
        assertTrue(SimpleProfile.apply(context, SimpleProfile.PICTURE, "save-data"));
        assertEquals("auto", prefs.getString(PreferenceConfiguration.VIDEO_FORMAT_PREF_STRING, null));
        assertEquals(AutoBitrateChoice.SOONER, prefs.getString(AutoBitrateChoice.SETTINGS_KEY, null));
        assertEquals("battery", prefs.getString("adaptive_quality_mode", null));
        assertEquals("save-data", prefs.getString(SimpleProfile.PICTURE, null));
        assertEquals("smoothest", prefs.getString(SimpleProfile.SMOOTHNESS, null));
        assertTrue("save data picks a lower bitrate than the default",
                prefs.getBoolean(PreferenceConfiguration.BITRATE_USER_SET_PREF_STRING, false));
    }

    @Test
    public void neverSurvivesEveryChoiceButSaveData() {
        prefs.edit().putString(AutoBitrateChoice.SETTINGS_KEY, AutoBitrateChoice.KEEP).commit();
        assertTrue(SimpleProfile.apply(context, SimpleProfile.SMOOTHNESS, "smoothest"));
        assertTrue(SimpleProfile.apply(context, SimpleProfile.HDR, "off"));
        assertTrue(SimpleProfile.apply(context, SimpleProfile.PICTURE, "auto"));
        assertTrue(SimpleProfile.apply(context, SimpleProfile.PICTURE, "best"));
        assertEquals("the page does not show that row", AutoBitrateChoice.KEEP, prefs.getString(AutoBitrateChoice.SETTINGS_KEY, null));
        assertTrue(SimpleProfile.apply(context, SimpleProfile.PICTURE, "save-data"));
        assertEquals("Save data turns Nightjar's own lowering on", AutoBitrateChoice.SOONER,
                prefs.getString(AutoBitrateChoice.SETTINGS_KEY, null));
        // Leaving Save data takes back the lowering it turned on.
        assertTrue(SimpleProfile.apply(context, SimpleProfile.PICTURE, "auto"));
        assertEquals(AutoBitrateChoice.PC, prefs.getString(AutoBitrateChoice.SETTINGS_KEY, null));
        // A Lower it sooner the player chose survives a choice other than Save data.
        prefs.edit().putString(AutoBitrateChoice.SETTINGS_KEY, AutoBitrateChoice.SOONER).commit();
        assertTrue(SimpleProfile.apply(context, SimpleProfile.PICTURE, "best"));
        assertEquals(AutoBitrateChoice.SOONER, prefs.getString(AutoBitrateChoice.SETTINGS_KEY, null));
    }

    @Test
    public void aChangedAdvancedSettingReadsAsCustomAndBackRestoresTheProfile() {
        assertTrue(SimpleProfile.applyRecommended(context));
        assertEquals("auto", prefs.getString(SimpleProfile.SMOOTHNESS, null));
        assertEquals("auto", prefs.getString(SimpleProfile.PICTURE, null));
        assertEquals("auto", prefs.getString(SimpleProfile.HDR, null));
        assertFalse(SimpleProfile.custom(context, SimpleProfile.SMOOTHNESS));

        prefs.edit().putString(PreferenceConfiguration.FRAME_PACING_PREF_STRING, "cap-fps").commit();
        assertTrue(SimpleProfile.custom(context, SimpleProfile.SMOOTHNESS));
        assertTrue(SimpleProfile.custom(context, SimpleProfile.PICTURE));

        assertTrue(SimpleProfile.applyRecommended(context));
        assertFalse(SimpleProfile.custom(context, SimpleProfile.SMOOTHNESS));
        assertEquals("latency", prefs.getString(PreferenceConfiguration.FRAME_PACING_PREF_STRING, null));
    }

    @Test
    public void neverOpeningTheSimpleLayerLeavesEveryDefaultAlone() {
        assertNull(prefs.getString(SimpleProfile.SMOOTHNESS, null));
        assertFalse(prefs.contains(PreferenceConfiguration.RESOLUTION_PREF_STRING));
        assertFalse(prefs.contains(PreferenceConfiguration.FRAME_PACING_PREF_STRING));
        assertFalse(prefs.contains(PreferenceConfiguration.VIDEO_FORMAT_PREF_STRING));
    }

    @Test
    public void aRowFollowsSettingsThatChangedUnderIt() {
        hdrDisplay();
        assertTrue(SimpleProfile.apply(context, SimpleProfile.SMOOTHNESS, "smoothest"));
        SimpleProfilePreference row = openSmoothnessRow();
        assertEquals("Smoothest", row.getEntry().toString());
        // The fresh install's HDR switch is off, and the row follows that too instead of calling
        // the profile Custom: only the advanced settings decide.
        assertEquals("off", SimpleProfile.chosen(context)[2]);

        // Back to recommended, an advanced setting or a preset moves the settings under the row.
        assertTrue(SimpleProfile.applyRecommended(context));
        assertEquals("Auto (recommended)", row.getEntry().toString());
    }

    @Test
    public void aPickAppliesEvenWhenTheRowRemembersTheSameChoice() {
        assertTrue(SimpleProfile.applyRecommended(context));
        SimpleProfilePreference row = openSmoothnessRow();
        // The row remembers Auto; the settings changed to nothing the row names.
        prefs.edit().putString(PreferenceConfiguration.FRAME_PACING_PREF_STRING, "cap-fps").commit();
        assertTrue(SimpleProfile.custom(context, SimpleProfile.SMOOTHNESS));

        assertTrue(row.callChangeListener("auto"));
        row.setValue("auto");

        // The framework skips persisting a value equal to the remembered one; the pick must apply anyway.
        assertEquals("latency", prefs.getString(PreferenceConfiguration.FRAME_PACING_PREF_STRING, null));
        assertFalse(SimpleProfile.custom(context, SimpleProfile.SMOOTHNESS));
    }

    /** The smoothness row of the open Recommended section, attached like any row of the screen. */
    private SimpleProfilePreference openSmoothnessRow() {
        // Opening Settings must not run the first-launch migration, which would reset the settings read.
        prefs.edit().putInt(PreferenceConfiguration.PREFS_VERSION_PREF_STRING,
                PreferenceConfiguration.FOLLOW_SCREEN_VERSION).commit();
        settingsController = org.robolectric.Robolectric.buildActivity(StreamSettings.class).setup();
        StreamSettings.SettingsFragment simple = SettingsStructureTest.open(settingsController.get(), "simple");
        return (SimpleProfilePreference) simple.findPreference(SimpleProfile.SMOOTHNESS);
    }

    private org.robolectric.android.controller.ActivityController<StreamSettings> settingsController;

    @Test
    public void theCurrentSettingsReadAsTheChoicesTheyAre() {
        assertTrue(SimpleProfile.applyRecommended(context));
        SimpleProfileModel.Current current = SimpleProfile.current(context);
        assertTrue(current.nativeResolution);
        assertNull("the screen's rate is the unstored default", current.fps);
        assertEquals("latency", current.pacing);
        assertFalse(current.vrr);
        assertNull(current.bitrateMilli);
        assertEquals("auto", current.codec);
        assertFalse(current.adaptiveBitrate);
        assertEquals("off", current.adaptiveQuality);
    }

    /** An HDR10 screen and 10-bit HEVC decoder, as an HDR TV or phone reports. */
    private void hdrDisplay() {
        android.view.Display display = PreferenceConfiguration.defaultDisplay(context);
        assertNotNull(display);
        org.robolectric.Shadows.shadowOf(display).setDisplayHdrCapabilities(display.getDisplayId(),
                1000f, 500f, 0.0001f, android.view.Display.HdrCapabilities.HDR_TYPE_HDR10);
        MediaFormat format = MediaFormat.createVideoFormat("video/hevc", 1920, 1080);
        MediaCodecInfo.CodecProfileLevel main10 = new MediaCodecInfo.CodecProfileLevel();
        main10.profile = MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10;
        main10.level = MediaCodecInfo.CodecProfileLevel.HEVCMainTierLevel51;
        ShadowMediaCodecList.addCodec(MediaCodecInfoBuilder.newBuilder()
                .setName("OMX.rubylight.hevc.hdr.decoder")
                .setIsEncoder(false)
                .setIsHardwareAccelerated(true)
                .setCapabilities(MediaCodecInfoBuilder.CodecCapabilitiesBuilder.newBuilder()
                        .setMediaFormat(format)
                        .setColorFormats(new int[] {MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface})
                        .setProfileLevels(new MediaCodecInfo.CodecProfileLevel[] {main10})
                        .build())
                .build());
        SimpleProfile.forgetCodecs();
        SimpleProfile.forgetCapabilities();
    }

    @Test
    public void aFreshHdrInstallShowsTheHdrRowOffBecauseTheStreamIsSdr() {
        hdrDisplay();
        PreferenceConfiguration.migrateToNativeDefaults(context);
        assertTrue("the screen offers HDR10", SimpleProfile.hdrOffered(context));
        assertFalse("a fresh install leaves the HDR switch off", SimpleProfile.current(context).hdr);
        // The HDR row renders SimpleProfile.chosen()'s HDR entry: it must be what the stream does.
        assertFalse(SimpleProfile.custom(context, SimpleProfile.HDR));
        String[] shown = SimpleProfile.chosen(context);
        assertNotNull(shown);
        assertEquals("off", shown[2]);
        // A pick in another row keeps the HDR switch and the HDR choice Off.
        assertTrue(SimpleProfile.apply(context, SimpleProfile.SMOOTHNESS, "balanced"));
        assertFalse("a pick in another row must not turn HDR on",
                prefs.getBoolean(PreferenceConfiguration.ENABLE_HDR_PREF_STRING, true));
        assertEquals("off", prefs.getString(SimpleProfile.HDR, null));
        assertNotNull(SimpleProfile.chosen(context));
        assertEquals("off", SimpleProfile.chosen(context)[2]);
        // The HDR row's own Auto choice turns it on, and a later pick elsewhere leaves it on.
        assertTrue(SimpleProfile.apply(context, SimpleProfile.HDR, "auto"));
        assertTrue(prefs.getBoolean(PreferenceConfiguration.ENABLE_HDR_PREF_STRING, false));
        assertTrue(SimpleProfile.apply(context, SimpleProfile.PICTURE, "best"));
        assertTrue("a pick in another row keeps HDR on",
                prefs.getBoolean(PreferenceConfiguration.ENABLE_HDR_PREF_STRING, false));
        assertNotNull(SimpleProfile.chosen(context));
        assertEquals("auto", SimpleProfile.chosen(context)[2]);
    }

    @Test
    @Config(sdk = 28, application = android.app.Application.class,
            shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
            instrumentedPackages = "com.limelight.nvstream.jni")
    public void openingTheRecommendedSectionChangesNoAdvancedKey() {
        prefs.edit()
                .putString(SimpleProfile.SMOOTHNESS, "balanced")
                .putString(SimpleProfile.PICTURE, "best")
                .putString(SimpleProfile.HDR, "off")
                .putString(PreferenceConfiguration.FRAME_PACING_PREF_STRING, "latency")
                .putString(PreferenceConfiguration.VIDEO_FORMAT_PREF_STRING, "forceh265")
                .putInt(PreferenceConfiguration.BITRATE_PREF_STRING, 12_345)
                .putBoolean(PreferenceConfiguration.BITRATE_USER_SET_PREF_STRING, true)
                .putBoolean(PreferenceConfiguration.ENABLE_HDR_PREF_STRING, true)
                .putString(OpenPlay.SETTING, "last")
                .putBoolean(HomeSettings.PREF_KEY, true)
                .commit();
        prefs.edit().putInt(PreferenceConfiguration.PREFS_VERSION_PREF_STRING,
                PreferenceConfiguration.FOLLOW_SCREEN_VERSION).commit();
        java.util.Map<String, ?> before = new java.util.HashMap<>(prefs.getAll());
        org.robolectric.android.controller.ActivityController<StreamSettings> controller =
                org.robolectric.Robolectric.buildActivity(StreamSettings.class).setup();
        try {
            SettingsStructureTest.open(controller.get(), "simple");
            for (String key : new String[] {PreferenceConfiguration.FRAME_PACING_PREF_STRING,
                    PreferenceConfiguration.VIDEO_FORMAT_PREF_STRING, PreferenceConfiguration.BITRATE_PREF_STRING,
                    PreferenceConfiguration.ENABLE_HDR_PREF_STRING, OpenPlay.SETTING, HomeSettings.PREF_KEY}) {
                assertEquals(key, before.get(key), prefs.getAll().get(key));
            }
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void theConsoleModeSwitchFollowsTheStartScreenChosenOnTheSameScreen() {
        org.robolectric.android.controller.ActivityController<StreamSettings> controller =
                org.robolectric.Robolectric.buildActivity(StreamSettings.class).setup();
        try {
            StreamSettings.SettingsFragment simple = SettingsStructureTest.open(controller.get(), "simple");
            android.preference.SwitchPreference console =
                    (android.preference.SwitchPreference) simple.findPreference("checkbox_console_home");
            assertNotNull(console);
            assertTrue(console.isChecked());

            // "When Nightjar opens" on the same screen picks Show my PCs.
            SharedPreferences.Editor editor = prefs.edit();
            StartScreenChoice.save(prefs, editor, StartScreenChoice.PCS);
            assertTrue(editor.commit());

            assertFalse("the switch follows the start screen", console.isChecked());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void bindingSimpleRowsNeverWritesAProfileOrTheStartScreen() {
        prefs.edit()
                .putString(SimpleProfile.SMOOTHNESS, "balanced")
                .putString(SimpleProfile.PICTURE, "best")
                .putString(SimpleProfile.HDR, "off")
                .putString(PreferenceConfiguration.FRAME_PACING_PREF_STRING, "latency")
                .putString(PreferenceConfiguration.VIDEO_FORMAT_PREF_STRING, "forceh265")
                .putInt(PreferenceConfiguration.BITRATE_PREF_STRING, 12_345)
                .putBoolean(PreferenceConfiguration.BITRATE_USER_SET_PREF_STRING, true)
                .putBoolean(PreferenceConfiguration.ENABLE_HDR_PREF_STRING, true)
                .putString(OpenPlay.SETTING, "last")
                .putBoolean(HomeSettings.PREF_KEY, true)
                .commit();
        prefs.edit().putInt(PreferenceConfiguration.PREFS_VERSION_PREF_STRING,
                PreferenceConfiguration.FOLLOW_SCREEN_VERSION).commit();
        java.util.Map<String, ?> before = new java.util.HashMap<>(prefs.getAll());
        for (String key : new String[] {SimpleProfile.SMOOTHNESS, SimpleProfile.PICTURE, SimpleProfile.HDR}) {
            SimpleProfilePreference row = new SimpleProfilePreference(context, null);
            row.setKey(key);
            row.onSetInitialValue(true, null);
        }
        ConsoleModePreference console = new ConsoleModePreference(context, null);
        console.setKey(HomeSettings.PREF_KEY);
        console.onSetInitialValue(true, true);
        for (String key : new String[] {PreferenceConfiguration.FRAME_PACING_PREF_STRING,
                PreferenceConfiguration.VIDEO_FORMAT_PREF_STRING, PreferenceConfiguration.BITRATE_PREF_STRING,
                PreferenceConfiguration.ENABLE_HDR_PREF_STRING, OpenPlay.SETTING, HomeSettings.PREF_KEY}) {
            assertEquals(key, before.get(key), prefs.getAll().get(key));
        }
    }

    @Test
    public void aFailedDecoderListReadIsNotKeptForTheRun() {
        SimpleProfile.forgetCodecs();
        SimpleProfile.rememberCodecs(null);
        assertNull("a failed read is tried again on the next visit",
                org.robolectric.util.ReflectionHelpers.getStaticField(SimpleProfile.class, "cachedCodecs"));

        SimpleProfile.Codecs read = new SimpleProfile.Codecs();
        assertSame(read, SimpleProfile.rememberCodecs(read));
        assertSame(read, org.robolectric.util.ReflectionHelpers.getStaticField(SimpleProfile.class, "cachedCodecs"));
    }

    @Test
    public void theDecoderListIsReadOnceForTheAppRunNotEverySettingsVisit() {        MediaFormat format = MediaFormat.createVideoFormat("video/hevc", 1920, 1080);
        MediaCodecInfo codec = MediaCodecInfoBuilder.newBuilder()
                .setName("OMX.rubylight.hevc.decoder")
                .setIsEncoder(false)
                .setIsHardwareAccelerated(true)
                .setCapabilities(MediaCodecInfoBuilder.CodecCapabilitiesBuilder.newBuilder()
                        .setMediaFormat(format)
                        .setColorFormats(new int[] {MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface})
                        .build())
                .build();
        ShadowMediaCodecList.addCodec(codec);
        assertTrue(SimpleProfile.capabilities(context).hevc);

        // Settings resumes and drops what it measures, but Android's decoder list does not change
        // while the app runs: the next visit answers without scanning again.
        ShadowMediaCodecList.reset();
        SimpleProfile.forgetCapabilities();
        assertTrue("the decoder list is kept for the run", SimpleProfile.capabilities(context).hevc);
    }

    @Test
    public void theDecodeBenchmarkNeverCapsTheStreamRate() throws Exception {
        // A measured benchmark far below the screen's rate (200 ms a frame, five fps) is still a
        // decode time, not how many frames a second the decoder sustains, so it must not reach the
        // mapping: the stream keeps the screen's own rate.
        seedSlowDecodeBenchmark();
        SimpleProfile.forgetCapabilities();
        assertTrue(SimpleProfile.applyRecommended(context));
        SimpleProfileModel.Capabilities caps = SimpleProfile.capabilities(context);
        assertEquals(Integer.toString(caps.refreshHz),
                prefs.getString(PreferenceConfiguration.FPS_PREF_STRING, null));
        assertFalse("the benchmark is not part of the simple layer's capabilities",
                caps.json().getJSONObject("decoder").has("max_fps_at_native"));
    }

    /** A measured decode time of 200 ms a frame for every codec this device lists, as a run leaves it. */
    private void seedSlowDecodeBenchmark() throws Exception {
        MediaFormat format = MediaFormat.createVideoFormat("video/hevc", 1920, 1080);
        MediaCodecInfo codec = MediaCodecInfoBuilder.newBuilder()
                .setName("OMX.rubylight.hevc.decoder")
                .setIsEncoder(false)
                .setIsHardwareAccelerated(true)
                .setCapabilities(MediaCodecInfoBuilder.CodecCapabilitiesBuilder.newBuilder()
                        .setMediaFormat(format)
                        .setColorFormats(new int[] {MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface})
                        .build())
                .build();
        ShadowMediaCodecList.addCodec(codec);
        Method key = DecoderBenchmark.class.getDeclaredMethod("key", int.class);
        key.setAccessible(true);
        String benchmarkKey = (String) key.invoke(null,
                PreferenceConfiguration.readHevcLowLatencyMode(prefs));
        context.getSharedPreferences("decoder_benchmark", Context.MODE_PRIVATE).edit().clear()
                .putString("key", benchmarkKey)
                .putLong(codec.getName() + "|video/hevc", 200_000)
                .commit();
        assertEquals("the seeded benchmark is what a measured run reads back",
                200_000, DecoderBenchmark.result(context, codec, "video/hevc"));
        // The decoder list the simple layer scans reports it too, so the old capping read it.
        boolean listed = false;
        for (MediaCodecInfo info : new MediaCodecList(MediaCodecList.REGULAR_CODECS).getCodecInfos()) {
            listed |= info.getName().equals(codec.getName());
        }
        assertTrue("the decoder list reports the seeded codec", listed);
    }

    @Test
    public void balancedRestoresTheVrrSmoothestTurnedOff() {
        prefs.edit().putBoolean(PreferenceConfiguration.VRR_PREF_STRING, true).commit();
        assertTrue(SimpleProfile.apply(context, SimpleProfile.SMOOTHNESS, "smoothest"));
        assertFalse(prefs.getBoolean(PreferenceConfiguration.VRR_PREF_STRING, true));
        assertTrue("the player's opt-in is kept", prefs.getBoolean(SimpleProfile.VRR_OPT_IN, false));
        assertTrue(SimpleProfile.apply(context, SimpleProfile.SMOOTHNESS, "balanced"));
        assertTrue("Balanced restores it", prefs.getBoolean(PreferenceConfiguration.VRR_PREF_STRING, false));
    }

    @Test
    public void aPcOverrideDoesNotChangeTheGlobalRows() {
        assertTrue(SimpleProfile.applyRecommended(context));
        HostStreamProfile profile = new HostStreamProfile();
        profile.hdr = true;
        context.getSharedPreferences("HostStreamProfiles", Context.MODE_PRIVATE).edit()
                .putString("pc", profile.serialize()).commit();
        assertFalse("the rows describe the global settings, not one PC's overrides",
                SimpleProfile.custom(context, SimpleProfile.SMOOTHNESS));
        context.getSharedPreferences("HostStreamProfiles", Context.MODE_PRIVATE).edit().clear().commit();
    }

    @Test
    public void consoleModeReusesHomeAndThePcList() {
        ConsoleModePreference console = new ConsoleModePreference(context, null);
        console.setKey(HomeSettings.PREF_KEY);
        prefs.edit().putString(OpenPlay.SETTING, "last").commit();

        assertTrue(console.persistBoolean(true));
        assertTrue(HomeSettings.enabled(context));
        assertEquals("pcs", prefs.getString(OpenPlay.SETTING, null));

        assertTrue(console.persistBoolean(false));
        assertFalse(HomeSettings.enabled(context));
        assertEquals("pcs", prefs.getString(OpenPlay.SETTING, null));
    }

    @Test
    public void bindingConsoleModeKeepsLastAndChosen() {
        for (String screen : new String[] {"last", "chosen"}) {
            prefs.edit().putString(OpenPlay.SETTING, screen).putBoolean(HomeSettings.PREF_KEY, true).commit();
            ConsoleModePreference console = new ConsoleModePreference(context, null);
            console.setKey(HomeSettings.PREF_KEY);
            console.onSetInitialValue(true, true);
            assertEquals(screen, prefs.getString(OpenPlay.SETTING, null));
            assertTrue(prefs.getBoolean(HomeSettings.PREF_KEY, false));
        }
    }

    @Test
    public void bindingConsoleModeInTheHomeStateWritesNothing() {
        // Home with an unknown start value reads as Home, so a bind would persist the pair unless the
        // guard stops it; "old" would become "pcs".
        prefs.edit().putString(OpenPlay.SETTING, "old").putBoolean(HomeSettings.PREF_KEY, true).commit();
        org.robolectric.android.controller.ActivityController<StreamSettings> controller =
                org.robolectric.Robolectric.buildActivity(StreamSettings.class).setup().visible();
        try {
            settleConsole(controller.get());
            android.app.Fragment root = controller.get().getFragmentManager().findFragmentById(com.limelight.R.id.settings_sections);
            if (root == null) root = controller.get().getFragmentManager().findFragmentById(com.limelight.R.id.stream_settings);
            assertTrue("the settings fragment owns the preference manager", root != null);
            android.preference.PreferenceScreen screen =
                    ((android.preference.PreferenceFragment) root).getPreferenceManager()
                            .createPreferenceScreen(context);
            ConsoleModePreference console = new ConsoleModePreference(context, null);
            console.setKey(HomeSettings.PREF_KEY);
            screen.addPreference(console);
            assertEquals("old", prefs.getString(OpenPlay.SETTING, null));
            assertTrue(prefs.getBoolean(HomeSettings.PREF_KEY, false));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void clickingRecommendedScreenRowOpensVideoSectionNotDisplay() {
        prefs.edit().putInt(PreferenceConfiguration.PREFS_VERSION_PREF_STRING,
                PreferenceConfiguration.FOLLOW_SCREEN_VERSION).commit();
        settingsController = org.robolectric.Robolectric.buildActivity(StreamSettings.class).setup();
        StreamSettings screen = settingsController.get();
        StreamSettings.SettingsFragment simple = SettingsStructureTest.open(screen, "simple");
        android.preference.Preference screenRow = simple.findPreference("simple_screen");
        assertNotNull(screenRow);
        assertTrue(screenRow.getOnPreferenceClickListener().onPreferenceClick(screenRow));
        settleConsole(screen);

        StreamSettings.SettingsFragment active = (StreamSettings.SettingsFragment)
                screen.getFragmentManager().findFragmentById(com.limelight.R.id.stream_settings);
        assertNotNull("NJ#147: active section fragment switched after clicking simple_screen", active);
        assertEquals("video", active.getArguments().getString(StreamSettingsFragment.ARG_SECTION));
        android.preference.PreferenceScreen shown = active.getPreferenceScreen();
        assertNotNull("NJ#147: video section contains list_resolution", shown.findPreference("list_resolution"));
        assertNotNull("NJ#147: video section contains list_fps", shown.findPreference("list_fps"));
        assertNull("NJ#147: must not land in display section", shown.findPreference("checkbox_vrr"));
    }

    private static void settleConsole(StreamSettings screen) {
        org.robolectric.shadows.ShadowLooper.idleMainLooper();
        screen.getFragmentManager().executePendingTransactions();
        org.robolectric.shadows.ShadowLooper.idleMainLooper();
    }
}
