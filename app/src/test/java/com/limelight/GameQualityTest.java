package com.limelight;

import android.app.Application;
import android.content.Context;
import android.content.Intent;
import android.hardware.display.DisplayManager;
import android.os.Handler;
import android.os.SystemClock;
import android.view.Display;
import com.limelight.binding.video.MediaCodecDecoderRenderer;
import com.limelight.binding.video.MediaCodecHelper;
import com.limelight.nvstream.AdaptiveQualityController;
import com.limelight.nvstream.DecodeTiming;
import com.limelight.nvstream.NvConnection;
import com.limelight.nvstream.StreamConfiguration;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.ui.StreamPresentation;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.Implementation;
import org.robolectric.annotation.Implements;
import org.robolectric.shadows.ShadowDisplayManager;
import org.robolectric.shadows.ShadowLooper;
import org.robolectric.shadows.ShadowSystemClock;
import org.robolectric.util.ReflectionHelpers;
import static org.junit.Assert.*;

@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class GameQualityTest {
    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void fractionalReconfigureChangesRendererOnlyAfterTheRequestIsSent() {
        Game game = reconfigurableGame();
        MediaCodecDecoderRenderer renderer = ReflectionHelpers.getField(game, "decoderRenderer");
        renderer.setStreamFrameRateMilliHz(119880);
        StreamReconfigureRequester requester = ReflectionHelpers.getField(game, "reconfigureRequester");
        requester.start(1920, 1080, 119880, 1920, 1080, 119880);
        for (boolean accepted : new boolean[] {false, true}) {
            QualityBridge.send = accepted;
            assertEquals(accepted, (boolean) ReflectionHelpers.callInstanceMethod(game.videoOutput, "requestStreamReconfigure",
                    ReflectionHelpers.ClassParameter.from(int[].class, new int[] {1920, 1080, 59940})));
            assertEquals(accepted ? 59940 : 119880, renderer.getStreamFrameRateMilliHz());
            assertEquals(accepted ? 59940 : 119880, requester.fpsMillihz());
        }
        clearQualityCallbacks(game);
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void fractionalQualityCeilingSurvivesReductionAndRecovery() {
        Game game = reconfigurableGame();
        ReflectionHelpers.setField(game, "qualityMaxFrameRateMilliHz", 119880);
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
        try {
            ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
            AdaptiveQualityController quality = ReflectionHelpers.getField(game, "adaptiveQuality");
            assertEquals(119880, QualityBridge.lastRate);
            long now = SystemClock.uptimeMillis() + 15000;
            AdaptiveQualityController.Target lower = pressuredTarget(quality, now);
            ReflectionHelpers.callInstanceMethod(game.adaptation, "applyAdaptiveQuality",
                    ReflectionHelpers.ClassParameter.from(AdaptiveQualityController.Target.class, lower),
                    ReflectionHelpers.ClassParameter.from(long.class, now + 3000));
            assertEquals(90000, QualityBridge.lastRate);
            ReflectionHelpers.callInstanceMethod(game.adaptation, "applyAdaptiveQuality",
                    ReflectionHelpers.ClassParameter.from(AdaptiveQualityController.Target.class, quality.ceiling()),
                    ReflectionHelpers.ClassParameter.from(long.class, now + 40000));
            assertEquals(119880, QualityBridge.lastRate);
            assertEquals(119880, (int) ReflectionHelpers.callInstanceMethod(game.adaptation, "qualityCeilingFrameRateMilliHz"));
        } finally { clearQualityCallbacks(game); }
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void fractionalOnlyPanelChangeReachesAutomaticQualityAndTracking() {
        Game game = reconfigurableGame();
        MediaCodecDecoderRenderer renderer = ReflectionHelpers.getField(game, "decoderRenderer");
        renderer.setStreamFrameRateMilliHz(120000);
        ReflectionHelpers.callInstanceMethod(game.videoOutput, "startReconfigureTracking");
        StreamReconfigureRequester requester = ReflectionHelpers.getField(game, "reconfigureRequester");
        assertEquals(120000, requester.fpsMillihz());
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
        ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
        try {
            // Finish the display-settling delay, then change between rates with the same rounded FPS.
            ShadowLooper.idleMainLooper(StreamReconfigureRequester.SETTLE_MS,
                    java.util.concurrent.TimeUnit.MILLISECONDS);
            clearPendingQualityTarget(game);
            int before = QualityBridge.requests;
            ReflectionHelpers.setField(game, "panelMaxRefreshRate", 119.88f);
            ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
            assertEquals(before + 1, QualityBridge.requests);
            assertEquals(119880, QualityBridge.lastRate);
            assertEquals(119880, renderer.getStreamFrameRateMilliHz());
            assertEquals(119880, (int) ReflectionHelpers.callInstanceMethod(game.adaptation, "qualityCeilingFrameRateMilliHz"));
        } finally { clearQualityCallbacks(game); }
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void qualityTicksUseDecoderTailAndResetEvidenceAcrossSuspensionAndPendingReconfigure() {
        for (int suspension = 0; suspension < 3; suspension++) {
            Game game = reconfigurableGame();
            PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
            TimingRenderer renderer = new TimingRenderer(game, config);
            ReflectionHelpers.setField(game, "decoderRenderer", renderer);
            AdaptiveQualityController quality = new AdaptiveQualityController(AdaptiveQualityController.Mode.DETAIL,
                    1920, 1080, 120, 1080, 30, 2, 2, SystemClock.uptimeMillis() - 20000);
            ReflectionHelpers.setField(game, "adaptiveQuality", quality);
            try {
                for (int i = 0; i < 3; i++) qualityTick(game);
                assertEquals(3, renderer.timingReads);
                assertEquals(120, renderer.sampledFps);
                assertEquals(0, QualityBridge.requests);

                if (suspension == 0) ReflectionHelpers.setField(game, "foreground", false);
                else if (suspension == 1) ReflectionHelpers.setField(game, "streamMenu", new android.app.Dialog(game));
                else ReflectionHelpers.setField(game, "pendingQualityTarget", quality.ceiling());
                qualityTick(game);
                assertEquals("Suspension must not consume a pressure observation", 3, renderer.timingReads);
                assertEquals(1, renderer.timingResets);
                if (suspension == 0) ReflectionHelpers.setField(game, "foreground", true);
                else if (suspension == 1) ReflectionHelpers.setField(game, "streamMenu", null);
                else assertNull(ReflectionHelpers.getField(game, "pendingQualityTarget"));

                // The mean is only 2 ms: a request proves that real updateQuality passed the tail
                // observation through, and four new ticks prove the old three votes were cleared.
                for (int i = 0; i < 3; i++) {
                    qualityTick(game);
                    assertEquals(0, QualityBridge.requests);
                }
                qualityTick(game);
                assertEquals(1, QualityBridge.requests);
                assertEquals(90000, QualityBridge.lastRate);
                assertEquals(90, (int) ReflectionHelpers.getField(game, "streamFps"));
                assertTrue("The accepted target starts a new timing window", renderer.timingResets > 1);
            } finally {
                clearQualityCallbacks(game);
            }
        }
    }

    private static void qualityTick(Game game) {
        clearQualityCallbacks(game);
        ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(1));
        Runnable update = ReflectionHelpers.getField(game, "updateQuality");
        update.run();
        clearQualityCallbacks(game);
    }

    private static final class TimingRenderer extends MediaCodecDecoderRenderer {
        int timingReads, timingResets, sampledFps;
        TimingRenderer(Game game, PreferenceConfiguration config) {
            super(game, config, null, 0, false, false, "", null);
        }
        @Override public int[] getReconfigureLimits() { return new int[] {4096, 4096, 2, 2}; }
        @Override public boolean prepareForReconfigure(int width, int height) { return true; }
        @Override public int[] getStreamSize() { return new int[] {1920, 1080}; }
        @Override public boolean hasRecentVideoFrames(long nowMs) { return true; }
        @Override public float getNetworkFrameLossPercent() { return 0; }
        @Override public float getDecodeTimeMs() { return 2; }
        @Override public float getQueueDelayMs() { return 0; }
        @Override public int getThermalLevel() { return 0; }
        @Override public void resetDecodeTiming() { timingResets++; }
        @Override public DecodeTiming takeDecodeTiming(int fps) {
            timingReads++;
            sampledFps = fps;
            long end = SystemClock.uptimeMillis() * 1_000_000L;
            return new DecodeTiming(timingResets, end - 1_000_000_000L, end,
                    end - 950_000_000L, end - 50_000_000L, fps, 120, 12);
        }
    }
    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void externalRefreshChangesUpdateTheStreamAndQualityCeilingIndependentlyOfThePhone() {
        for (boolean automatic : new boolean[] {false, true}) {
            Game game = reconfigurableGame();
            int displayId = ShadowDisplayManager.addDisplay("w1920dp-h1080dp");
            Display external = ((DisplayManager) game.getSystemService(Context.DISPLAY_SERVICE)).getDisplay(displayId);
            setDisplayMode(external, 1920, 1080, 120);
            StreamPresentation presentation = new StreamPresentation(game, external, false);
            ReflectionHelpers.setField(game, "externalPresentation", presentation);
            ReflectionHelpers.setField(game, "renderView", presentation.videoView());
            ReflectionHelpers.setField(game.videoOutput, "displaySignature",
                    ReflectionHelpers.callInstanceMethod(game.videoOutput, "displaySignature"));
            if (automatic) {
                PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
                config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
                ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
            }
            try {
                DisplayManager.DisplayListener listener = ReflectionHelpers.getField(game, "inputDisplayListener");
                for (int fps : new int[] {60, 120}) {
                    setDisplayMode(external, 1920, 1080, fps);
                    listener.onDisplayChanged(displayId);
                    assertEquals(fps, (float) ReflectionHelpers.getField(game, "panelMaxRefreshRate"), 0);
                    ShadowLooper.idleMainLooper(StreamReconfigureRequester.SETTLE_MS,
                            java.util.concurrent.TimeUnit.MILLISECONDS);
                    assertEquals(fps, (int) ReflectionHelpers.getField(game, "streamFps"));
                    assertEquals(fps, (int) ReflectionHelpers.getField(game, "qualityMaxFps"));
                    if (automatic) {
                        AdaptiveQualityController quality = ReflectionHelpers.getField(game, "adaptiveQuality");
                        assertEquals(fps, quality.ceiling().fps);
                        clearPendingQualityTarget(game);
                    }
                }
                assertEquals(2, QualityBridge.requests);
            } finally {
                clearQualityCallbacks(game);
                ShadowDisplayManager.removeDisplay(displayId);
            }
        }
    }

    @Test public void localDisplaySignatureStillTracksNativeResolutionChanges() {
        Game game = Robolectric.buildActivity(Game.class).get();
        Display local = game.getWindowManager().getDefaultDisplay();
        Display.Mode[] originalModes = local.getSupportedModes();
        try {
            setDisplayMode(local, 1080, 1920, 60);
            String before = ReflectionHelpers.callInstanceMethod(game.videoOutput, "displaySignature");
            setDisplayMode(local, 1440, 2560, 60);
            String after = ReflectionHelpers.callInstanceMethod(game.videoOutput, "displaySignature");
            assertNotEquals(before, after);
            assertTrue(after.contains("2560x1440"));
            // The problem report keeps the line whole: "2560x1440@60.0" read as an e-mail address
            // and became "[email]" (nightjar#119).
            com.limelight.utils.RedactedLog log = new com.limelight.utils.RedactedLog();
            String line = "Display modes changed: " + before + " -> " + after;
            log.add("INFO", line);
            assertEquals("INFO " + line, log.snapshot());
            assertTrue(after, after.endsWith(", up to 60.0 Hz"));
        } finally {
            ShadowDisplayManager.setSupportedModes(local.getDisplayId(), originalModes);
        }
    }

    private static void setDisplayMode(Display display, int width, int height, int fps) {
        Display.Mode mode = ReflectionHelpers.callConstructor(Display.Mode.class,
                ReflectionHelpers.ClassParameter.from(int.class, display.getMode().getModeId()),
                ReflectionHelpers.ClassParameter.from(int.class, width),
                ReflectionHelpers.ClassParameter.from(int.class, height),
                ReflectionHelpers.ClassParameter.from(float.class, (float) fps));
        ShadowDisplayManager.setSupportedModes(display.getDisplayId(), mode);
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void enablingQualityUsesOnlyTheAcceptedDisplayCeiling() {
        for (boolean accepted : new boolean[] {false, true}) {
            Game game = reconfigurableGame();
            QualityBridge.send = accepted;
            ReflectionHelpers.setField(game, "panelMaxRefreshRate", 60f);
            try {
                ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
                assertEquals(accepted ? 60 : 120, (int) ReflectionHelpers.getField(game, "qualityMaxFps"));
                assertEquals(accepted ? 60f : 120f, (float) ReflectionHelpers.getField(game, "streamPanelHz"), 0);
                assertEquals(1, QualityBridge.requests);
                PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
                config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
                ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
                AdaptiveQualityController quality = ReflectionHelpers.getField(game, "adaptiveQuality");
                assertEquals(accepted ? 60 : 120, quality.ceiling().fps);
                assertEquals("Enabling quality must not restore the old display rate", 1, QualityBridge.requests);
            } finally {
                clearQualityCallbacks(game);
            }
        }
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void adaptiveFpsStepDoesNotConsumeAPendingPanelChange() {
        Game game = reconfigurableGame();
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
        ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
        AdaptiveQualityController quality = ReflectionHelpers.getField(game, "adaptiveQuality");
        long now = SystemClock.uptimeMillis() + 15000;
        AdaptiveQualityController.Target target = pressuredTarget(quality, now);
        ReflectionHelpers.setField(game, "panelMaxRefreshRate", 60f);
        try {
            assertTrue(ReflectionHelpers.callInstanceMethod(game.adaptation, "applyAdaptiveQuality",
                    ReflectionHelpers.ClassParameter.from(AdaptiveQualityController.Target.class, target),
                    ReflectionHelpers.ClassParameter.from(long.class, now + 3000)));
            assertEquals(90, (int) ReflectionHelpers.getField(game, "streamFps"));
            assertEquals(120f, (float) ReflectionHelpers.getField(game, "streamPanelHz"), 0);
            clearPendingQualityTarget(game);
            ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
            assertEquals(60, quality.ceiling().fps);
            assertEquals(60, (int) ReflectionHelpers.getField(game, "qualityMaxFps"));
            assertEquals(60f, (float) ReflectionHelpers.getField(game, "streamPanelHz"), 0);
        } finally {
            clearQualityCallbacks(game);
        }
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void acceptedPanelChangeIsTrackedEvenWhenDegradedFpsIsUnchanged() {
        Game game = reconfigurableGame();
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
        config.qualityMinFps = 60;
        ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
        AdaptiveQualityController quality = ReflectionHelpers.getField(game, "adaptiveQuality");
        try {
            for (int i = 1; i <= 2; i++) {
                long now = SystemClock.uptimeMillis() + i * 20000L;
                AdaptiveQualityController.Target target = pressuredTarget(quality, now);
                assertTrue(ReflectionHelpers.callInstanceMethod(game.adaptation, "applyAdaptiveQuality",
                        ReflectionHelpers.ClassParameter.from(AdaptiveQualityController.Target.class, target),
                        ReflectionHelpers.ClassParameter.from(long.class, now + 3000)));
                clearPendingQualityTarget(game);
            }
            assertEquals(60, (int) ReflectionHelpers.getField(game, "streamFps"));
            ReflectionHelpers.setField(game, "panelMaxRefreshRate", 60f);
            ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
            assertEquals(60, (int) ReflectionHelpers.getField(game, "streamFps"));
            assertEquals(60f, (float) ReflectionHelpers.getField(game, "streamPanelHz"), 0);
            clearPendingQualityTarget(game);
            ReflectionHelpers.setField(game, "panelMaxRefreshRate", 120f);
            ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
            assertEquals(120, quality.ceiling().fps);
            assertEquals(120, (int) ReflectionHelpers.getField(game, "qualityMaxFps"));
        } finally {
            clearQualityCallbacks(game);
        }
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void rejectedPanelChangeKeepsTheLastAcceptedQualityCeiling() {
        Game game = reconfigurableGame();
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
        ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
        ReflectionHelpers.setField(game, "panelMaxRefreshRate", 60f);
        QualityBridge.send = false;
        try {
            ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
            assertNull(ReflectionHelpers.getField(game, "adaptiveQuality"));
            assertEquals(120, (int) ReflectionHelpers.getField(game, "qualityMaxFps"));
            assertEquals(120f, (float) ReflectionHelpers.getField(game, "streamPanelHz"), 0);
            assertEquals(120, (int) ReflectionHelpers.getField(game, "streamFps"));
        } finally {
            clearQualityCallbacks(game);
        }
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void automaticQualityUsesAutoBitrateForThisSessionWithoutSavingIt() {
        Game game = reconfigurableGame();
        NvConnection connection = ReflectionHelpers.getField(game, "conn");
        ComputerDetails host = new ComputerDetails();
        host.caps = com.limelight.nvstream.http.TestHosts.rubylight();
        host.encoderBitrateSupported = true;
        ReflectionHelpers.setField(connection, "hostDetails", host);
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
        try {
            // On a 0x5538 host whose link-rate controller has not yielded, Automatic quality leaves bitrate to the PC.
            QualityBridge.linkNotice = true;
            ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
            assertNull("0x5538 host keeps its own bitrate controller",
                    ReflectionHelpers.getField(game, "adaptiveBitrate"));

            // Without 0x5538 (e.g. host bitrate_adaptation = off or older host), Automatic quality runs session ABR.
            QualityBridge.linkNotice = false;
            ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
            assertNotNull("Automatic quality adapts the bitrate during this stream",
                    ReflectionHelpers.getField(game, "adaptiveBitrate"));
            assertFalse("The user's own auto-bitrate choice for this PC must not change",
                    game.getSharedPreferences("AdaptiveBitrate", 0).contains("display-quality-pc"));
        } finally {
            clearQualityCallbacks(game);
        }
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void turningAutomaticQualityOffRestoresTheAutoBitrateTheUserHad() {
        // {auto bitrate before quality started, the person toggles it from the menu meanwhile}
        boolean[][] cases = {{false, false}, {true, false}, {false, true}};
        for (boolean[] c : cases) {
            Game game = reconfigurableGame();
            NvConnection connection = ReflectionHelpers.getField(game, "conn");
            ComputerDetails host = new ComputerDetails();
            host.caps = com.limelight.nvstream.http.TestHosts.rubylight();
            host.encoderBitrateSupported = true;
            ReflectionHelpers.setField(connection, "hostDetails", host);
            PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
            config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
            try {
                // At stream start, auto bitrate is on only if the person saved it for this PC.
                if (c[0]) setAutoBitrate(game, true, false);
                ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
                assertNotNull(ReflectionHelpers.getField(game, "adaptiveBitrate"));
                if (c[1]) {
                    // Disable, then enable again from the stream's bitrate menu: now it is the person's choice.
                    setAutoBitrate(game, false, true);
                    setAutoBitrate(game, true, true);
                }
                // The person turns Automatic quality off for this PC from the stream menu.
                game.getSharedPreferences("AdaptiveQuality", 0).edit().putString("display-quality-pc", "off").commit();
                ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
                assertNull(ReflectionHelpers.getField(game, "adaptiveQuality"));
                assertEquals("before " + c[0] + ", menu " + c[1], c[0] || c[1],
                        ReflectionHelpers.getField(game, "adaptiveBitrate") != null);
            } finally {
                clearQualityCallbacks(game);
                game.getSharedPreferences("AdaptiveQuality", 0).edit().clear().commit();
                game.getSharedPreferences("AdaptiveBitrate", 0).edit().clear().commit();
            }
        }
    }

    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void automaticQualityStoppedByAnErrorAlsoReturnsItsAutoBitrate() {
        Game game = reconfigurableGame();
        NvConnection connection = ReflectionHelpers.getField(game, "conn");
        ComputerDetails host = new ComputerDetails();
        host.caps = com.limelight.nvstream.http.TestHosts.rubylight();
        host.encoderBitrateSupported = true;
        ReflectionHelpers.setField(connection, "hostDetails", host);
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
        try {
            ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
            assertNotNull(ReflectionHelpers.getField(game, "adaptiveBitrate"));
            // The host rejects a display change: Automatic quality stops for this stream.
            ReflectionHelpers.setField(game, "panelMaxRefreshRate", 60f);
            QualityBridge.send = false;
            ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
            assertNull(ReflectionHelpers.getField(game, "adaptiveQuality"));
            assertNull(ReflectionHelpers.getField(game, "adaptiveBitrate"));
        } finally {
            clearQualityCallbacks(game);
        }
    }

    private static void setAutoBitrate(Game game, boolean enabled, boolean remember) {
        ReflectionHelpers.callInstanceMethod(game.adaptation, "setAdaptiveBitrateEnabled",
                ReflectionHelpers.ClassParameter.from(boolean.class, enabled),
                ReflectionHelpers.ClassParameter.from(boolean.class, remember));
    }

    private static AdaptiveQualityController.Target pressuredTarget(AdaptiveQualityController quality, long now) {
        AdaptiveQualityController.Target target = null;
        for (int i = 0; i < 4; i++) target = quality.sample(now + i * 1000, true, false, 0, 2, 0, 0, 2, true);
        assertNotNull(target);
        return target;
    }

    private static void clearPendingQualityTarget(Game game) {
        Runnable update = ReflectionHelpers.getField(game, "updateQuality");
        update.run();
        assertNull(ReflectionHelpers.getField(game, "pendingQualityTarget"));
    }

    private static void clearQualityCallbacks(Game game) {
        Handler handler = ReflectionHelpers.getField(game, "bitrateHandler");
        handler.removeCallbacksAndMessages(null);
    }

    private Game reconfigurableGame() {
        Game game = Robolectric.buildActivity(Game.class).get();
        game.setIntent(new Intent().putExtra(Game.EXTRA_PC_UUID, "display-quality-pc"));
        PreferenceConfiguration config = new PreferenceConfiguration();
        config.videoFormat = PreferenceConfiguration.FormatOption.AUTO;
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.OFF;
        config.width = 1920; config.height = 1080; config.fps = 120;
        config.qualityMinShortEdge = 1080; config.qualityMinFps = 30;
        ReflectionHelpers.setStaticField(MediaCodecHelper.class, "initialized", true);
        MediaCodecDecoderRenderer renderer = new MediaCodecDecoderRenderer(game, config, null, 0,
                false, false, "", null) {
            @Override public int[] getReconfigureLimits() { return new int[] {4096, 4096, 2, 2}; }
            @Override public boolean prepareForReconfigure(int width, int height) { return true; }
            @Override public int[] getStreamSize() { return new int[] {1920, 1080}; }
        };
        ReflectionHelpers.setField(game, "prefConfig", config);
        ReflectionHelpers.setField(game, "decoderRenderer", renderer);
        ReflectionHelpers.setField(game, "connected", true);
        ReflectionHelpers.setField(game, "foreground", true);
        ReflectionHelpers.setField(game, "app", new NvApp("Game", 73, false));
        ReflectionHelpers.setField(game, "conn", new NvConnection(null,
                new ComputerDetails.AddressTuple("192.0.2.1", 47989), 47984, "test",
                new StreamConfiguration.Builder().build(), null, null));
        ReflectionHelpers.setField(game, "fpsFollowsDisplay", true);
        ReflectionHelpers.setField(game, "streamFps", 120);
        ReflectionHelpers.setField(game, "streamPanelHz", 120f);
        ReflectionHelpers.setField(game, "panelMaxRefreshRate", 120f);
        ReflectionHelpers.setField(game, "qualityMaxWidth", 1920);
        ReflectionHelpers.setField(game, "qualityMaxHeight", 1080);
        ReflectionHelpers.setField(game, "qualityMaxFps", 120);
        StreamReconfigureRequester requester = ReflectionHelpers.getField(game, "reconfigureRequester");
        requester.start(1920, 1080, 120000, 1920, 1080, 120000);
        QualityBridge.send = true;
        QualityBridge.linkNotice = false;
        QualityBridge.requests = 0;
        QualityBridge.lastRate = 0;
        return game;
    }

    @Implements(value = MoonBridge.class, isInAndroidSdk = false)
    public static class QualityBridge {
        static boolean send;
        static boolean linkNotice;
        static int requests;
        static int lastRate;
        static int lastWidth, lastHeight;
        @Implementation protected static void __staticInitializer__() {}
        @Implementation public static boolean hostSupportsControlMessage(int messageType) {
            return messageType != MoonBridge.CONTROL_MESSAGE_LINK_NOTICE || linkNotice;
        }
        @Implementation public static long getEstimatedRttInfo() { return 0; }
        @Implementation public static boolean requestReconfigure(int width, int height, int fpsMillihz) {
            requests++;
            lastRate = fpsMillihz;
            lastWidth = width;
            lastHeight = height;
            return send;
        }
    }

    public static class WindowedGame extends Game {
        boolean windowed;
        @Override public boolean isInMultiWindowMode() { return windowed; }
    }

    /**
     * The real screen launched from stored settings: {@code resolution} saved with "follows the screen" on or off, at
     * 60 fps, automatic quality on (motion) down to 360p, in settings already moved to the follow-the-screen switch
     * (no migration turns it on).
     */
    static <T extends Game> T launch(Class<T> type, String resolution, boolean followsScreen) {
        Context context = org.robolectric.RuntimeEnvironment.getApplication();
        android.preference.PreferenceManager.getDefaultSharedPreferences(context).edit()
                .putInt(preferenceKey("PREFS_VERSION_PREF_STRING"),
                        ReflectionHelpers.getStaticField(PreferenceConfiguration.class, "PROFILE_NATIVE_VERSION"))
                .putString(preferenceKey("RESOLUTION_PREF_STRING"), resolution)
                .putBoolean(preferenceKey("RESOLUTION_FOLLOWS_SCREEN_PREF_STRING"), followsScreen)
                .putString(preferenceKey("FPS_PREF_STRING"), "60")
                .putString("adaptive_quality_mode", "motion")
                .putString("adaptive_quality_min_resolution", "360")
                .commit();
        Intent intent = new Intent(context, type).putExtra(Game.EXTRA_HOST, "192.0.2.1")
                .putExtra(Game.EXTRA_UNIQUEID, "device").putExtra(Game.EXTRA_APP_ID, 11)
                .putExtra(Game.EXTRA_PC_UUID, "custom-size-pc");
        return Robolectric.buildActivity(type, intent).create().get();
    }

    private static String preferenceKey(String constant) {
        return ReflectionHelpers.getStaticField(PreferenceConfiguration.class, constant);
    }

    /**
     * {@link #launch} on this test's 1920x1080 60 Hz panel, then what connecting supplies, which this test has no host
     * for: a decoder set up at the launch size that can change size, a connected stream and its launch size.
     */
    private static <T extends Game> T launchedGame(Class<T> type, String resolution, boolean followsScreen) {
        T game = launch(type, resolution, followsScreen);
        assertEquals("the test's panel", "1920x1080", PreferenceConfiguration.nativeResolution(game));
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        assertEquals(AdaptiveQualityController.Mode.MOTION, config.adaptiveQualityMode);
        int width = config.width, height = config.height;
        ReflectionHelpers.setStaticField(MediaCodecHelper.class, "initialized", true);
        MediaCodecDecoderRenderer renderer = new MediaCodecDecoderRenderer(game, config, null, 0,
                false, false, "", null) {
            @Override public int[] getReconfigureLimits() { return new int[] {4096, 4096, 2, 2}; }
            @Override public boolean prepareForReconfigure(int w, int h) { return true; }
            @Override public int[] getStreamSize() { return new int[] {width, height}; }
        };
        ReflectionHelpers.setField(game, "decoderRenderer", renderer);
        ReflectionHelpers.setField(game, "connected", true);
        ReflectionHelpers.setField(game, "foreground", true);
        ReflectionHelpers.setField(game, "streamFps", 60);
        ReflectionHelpers.setField(game, "streamPanelHz", 60f);
        game.videoOutput.startInWindowShape(com.limelight.nvstream.http.HostCapabilities.UNKNOWN, true);
        QualityBridge.send = true;
        QualityBridge.requests = 0;
        QualityBridge.lastRate = QualityBridge.lastWidth = QualityBridge.lastHeight = 0;
        return game;
    }

    /**
     * The launch reads it from the stored settings: a custom size keeps its size, "follows the screen" streams at the
     * screen's own size and follows it.
     */
    @Test
    @Config(shadows = PcViewLifecycleTest.NoNativeMoonBridge.class, instrumentedPackages = "com.limelight.nvstream.jni",
            qualifiers = "w1920dp-h1080dp-land-mdpi")
    public void theLaunchLetsOnlyTheScreensOwnSizeFollowTheScreen() {
        for (boolean followsScreen : new boolean[] {false, true}) {
            Game game = launch(Game.class, "1600x900", followsScreen);
            PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
            assertEquals(followsScreen ? "1920x1080" : "1600x900", config.width + "x" + config.height);
            assertEquals(followsScreen, ReflectionHelpers.<Boolean>getField(game, "nativeResolutionStream"));
            game.finish();
        }
    }

    /**
     * Tracking starts as once connected, then automatic quality as the launch read it, and once the display-settling
     * delay is over the check its timer runs; run directly, as the launched screen's other queued work needs services
     * this test has not got.
     */
    private static AdaptiveQualityController startTrackingWithQuality(Game game) {
        ReflectionHelpers.callInstanceMethod(game.videoOutput, "startReconfigureTracking");
        ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
        settledCheck(game);
        AdaptiveQualityController quality = ReflectionHelpers.getField(game, "adaptiveQuality");
        assertNotNull(quality);
        return quality;
    }

    // The reconfigure check its timer runs once the display-settling delay is over.
    private static void settledCheck(Game game) {
        ShadowSystemClock.advanceBy(java.time.Duration.ofMillis(StreamReconfigureRequester.SETTLE_MS));
        ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
    }

    /**
     * Seen on a 1920x1080 handheld: a custom 1600x900 stream with automatic quality on was asked for 1920x1080 a
     * second after it started. The size chosen is automatic quality's ceiling, as a preset's is, on every reconfigure check.
     */
    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni",
            qualifiers = "w1920dp-h1080dp-land-mdpi")
    public void aCustomSizeIsAutomaticQualitysCeilingNotThePanel() {
        Game game = launchedGame(Game.class, "1600x900", false);
        try {
            AdaptiveQualityController quality = startTrackingWithQuality(game);
            assertEquals("nothing asked once the stream settled", 0, QualityBridge.requests);
            assertTrue(quality.ceiling().matches(1600, 900, 60));
            // A display change checks again (onDisplayModesChanged); the ceiling stays the size chosen.
            ReflectionHelpers.callInstanceMethod(game.videoOutput, "checkStreamReconfigure");
            assertEquals(0, QualityBridge.requests);
            assertTrue(quality.ceiling().matches(1600, 900, 60));
            assertEquals(1600, (int) ReflectionHelpers.getField(game, "qualityMaxWidth"));
            assertEquals(900, (int) ReflectionHelpers.getField(game, "qualityMaxHeight"));
        } finally {
            clearQualityCallbacks(game);
        }
    }

    /**
     * Split screen: a custom size, like a preset, stays the size asked for and is scaled into the pane (it was asked
     * for the pane's 948x970); a stream that follows the screen still takes the pane's size.
     */
    @Test
    @Config(shadows = QualityBridge.class, instrumentedPackages = "com.limelight.nvstream.jni",
            qualifiers = "w1920dp-h1080dp-land-mdpi")
    public void inSplitScreenOnlyAStreamFollowingTheScreenTakesThePanesSize() {
        for (int[] stream : new int[][] {{1600, 900, 0}, {1280, 720, 0}, {1920, 1080, 1}}) {
            boolean followsScreen = stream[2] == 1;
            WindowedGame game = launchedGame(WindowedGame.class, stream[0] + "x" + stream[1], followsScreen);
            try {
                AdaptiveQualityController quality = startTrackingWithQuality(game);
                assertEquals(0, QualityBridge.requests);
                game.windowed = true;
                // The pane laid out anew (onPaneLaidOut) asks for a check once the window settled.
                game.findViewById(android.R.id.content).layout(0, 0, 948, 970);
                settledCheck(game);
                if (followsScreen) {
                    assertEquals(1, QualityBridge.requests);
                    assertEquals(948, QualityBridge.lastWidth);
                    assertEquals(970, QualityBridge.lastHeight);
                } else {
                    assertEquals(stream[0] + "x" + stream[1] + " keeps its size", 0, QualityBridge.requests);
                    assertTrue(quality.ceiling().matches(stream[0], stream[1], 60));
                }
            } finally {
                clearQualityCallbacks(game);
            }
        }
    }

    @Test public void pyrowaveExplainsUnsupportedQualityWithoutStartingBitrateOrReconfiguring() {
        Game game = Robolectric.buildActivity(Game.class).get();
        game.setIntent(new Intent().putExtra(Game.EXTRA_PC_UUID, "quality-pc"));
        PreferenceConfiguration config = new PreferenceConfiguration();
        config.videoFormat = PreferenceConfiguration.FormatOption.FORCE_PYROWAVE;
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.MOTION;
        config.width = 1920; config.height = 1080; config.fps = 60;
        ReflectionHelpers.setStaticField(MediaCodecHelper.class, "initialized", true);
        MediaCodecDecoderRenderer renderer = new MediaCodecDecoderRenderer(game, config, null, 0,
                false, false, "", null);
        ReflectionHelpers.setField(renderer, "videoFormat", MoonBridge.VIDEO_FORMAT_PYROWAVE);
        ReflectionHelpers.setField(game, "prefConfig", config);
        ReflectionHelpers.setField(game, "decoderRenderer", renderer);
        ReflectionHelpers.setField(game, "connected", true);
        ReflectionHelpers.setField(game, "app", new NvApp("Game", 73, false));
        ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
        assertNull(ReflectionHelpers.getField(game, "adaptiveQuality"));
        assertNull(ReflectionHelpers.getField(game, "adaptiveBitrate"));
        assertNull(ReflectionHelpers.getField(game, "pyroWaveBitrate"));
        assertEquals(game.getString(R.string.quality_unavailable), ReflectionHelpers.getField(game, "qualityStatus"));
    }

    @Test public void offAndPcOverrideDoNotRequireAConnectedHost() {
        Game game = Robolectric.buildActivity(Game.class).get();
        game.setIntent(new Intent().putExtra(Game.EXTRA_PC_UUID, "quality-override"));
        PreferenceConfiguration config = new PreferenceConfiguration();
        config.adaptiveQualityMode = AdaptiveQualityController.Mode.DETAIL;
        ReflectionHelpers.setField(game, "prefConfig", config);
        game.getSharedPreferences("AdaptiveQuality", 0).edit().putString("quality-override", "off").commit();
        ReflectionHelpers.callInstanceMethod(game.adaptation, "startAdaptiveQuality");
        assertNull(ReflectionHelpers.getField(game, "adaptiveQuality"));
        assertNull(ReflectionHelpers.getField(game, "qualityStatus"));
        game.getSharedPreferences("AdaptiveQuality", 0).edit().remove("quality-override").commit();
        assertEquals(AdaptiveQualityController.Mode.DETAIL, ReflectionHelpers.callInstanceMethod(game.adaptation, "qualityMode"));
    }
}
