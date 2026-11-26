package com.limelight;

import android.app.Application;
import android.content.Intent;
import android.os.Looper;
import android.os.SystemClock;

import com.limelight.GameRecoveryTestSupport.RecoveryBridge;
import com.limelight.GameRecoveryTestSupport.TerminationProbe;
import com.limelight.nvstream.NvConnection;
import com.limelight.nvstream.StreamConfiguration;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.preferences.PreferenceConfiguration;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.annotation.Config;
import org.robolectric.util.ReflectionHelpers;

import static com.limelight.GameRecoveryTestSupport.*;
import static org.junit.Assert.*;

/**
 * Tests {@link GameRecovery} packed 4:4:4 chroma failure fallback, full-chroma RTSP refusal
 * restarts, and bounded stop-wait hand-offs.
 */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class GameRecoveryChromaTest {
    @Test public void packedChromaFailureReconnectsOnceWithoutPackedEvenWithAutoReconnectOff() throws Exception {
        Game game = recoveryGame();
        PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
        config.autoReconnect = false;
        config.experimentalPacked444 = true;
        try {
            assertTrue(ReflectionHelpers.callInstanceMethod(game.streamRecovery, "reconnectWithoutPackedChroma",
                    ReflectionHelpers.ClassParameter.from(String.class, "the GPU reconstruction could not keep up")));
            Intent retry = awaitRecoveryLaunch(game, 1);
            assertTrue(retry.getBooleanExtra("PackedChromaOff", false));
            assertEquals(1, retry.getIntExtra("RecoveryAttempts", 0));
            assertEquals("the setting itself is untouched", true, config.experimentalPacked444);
            // The replacement session turns packed off for itself only.
            assertFalse(com.limelight.binding.video.PackedFallback.effective(config.experimentalPacked444, retry.getBooleanExtra("PackedChromaOff", false)));
            // A session that already runs without packed chroma does not degrade again.
            Game replacement = recoveryGame();
            replacement.setIntent(retry);
            assertFalse(ReflectionHelpers.callInstanceMethod(replacement.streamRecovery, "reconnectWithoutPackedChroma",
                    ReflectionHelpers.ClassParameter.from(String.class, "again")));
            assertNull(ReflectionHelpers.getField(replacement, "pendingRecovery"));
        } finally {
            cleanupRecovery(game);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void packedChromaFailureThatCannotReconnectEndsTheStreamOffTheUiThread() throws Exception {
        Game game = recoveryGame();
        // This session already runs without packed chroma, so a failure has nothing left to turn off.
        game.setIntent(new Intent().putExtra("PackedChromaOff", true));
        java.util.concurrent.CountDownLatch ended = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.atomic.AtomicReference<Thread> endedOn = new java.util.concurrent.atomic.AtomicReference<>();
        ReflectionHelpers.setField(game, "conn", new MuteConnection() {
            @Override public boolean hasEndedSession() {
                endedOn.set(Thread.currentThread());
                ended.countDown();
                return false;
            }
        });
        // An unrelated newer stream registered on MoonBridge must never be terminated by this Game.
        TerminationProbe otherStream = new TerminationProbe();
        ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", otherStream.listener);
        try {
            ReflectionHelpers.callInstanceMethod(game.streamRecovery, "onPackedChromaFailure",
                    ReflectionHelpers.ClassParameter.from(String.class, "the GPU path failed"));
            assertTrue(ended.await(HANG_GUARD_MS, java.util.concurrent.TimeUnit.MILLISECONDS));
            // connectionTerminated asks the PC and tests the network: blocking calls, never on the UI thread.
            assertNotSame(Looper.getMainLooper().getThread(), endedOn.get());
            awaitOnUi(() -> game.isFinishing());
            assertEquals("unrelated MoonBridge listener must not be killed", 0, otherStream.ends);
        } finally {
            ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", null);
            cleanupRecovery(game);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void packedChromaFailureDuringARecoveryMarksTheReplacementInsteadOfEndingTheStream() throws Exception {
        Game game = recoveryGame();
        TerminationProbe probe = new TerminationProbe();
        ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", probe.listener);
        try {
            assertTrue(ReflectionHelpers.callInstanceMethod(game.streamRecovery, "tryAutomaticRecovery"));
            ReflectionHelpers.callInstanceMethod(game.streamRecovery, "onPackedChromaFailure",
                    ReflectionHelpers.ClassParameter.from(String.class, "the GPU path failed"));
            assertFalse("no second reconnect and no dialog over the recovery", probe.ended.await(300, java.util.concurrent.TimeUnit.MILLISECONDS));
            Intent retry = awaitRecoveryLaunch(game, 1);
            assertTrue(retry.getBooleanExtra("PackedChromaOff", false));
            assertEquals(0, probe.ends);
        } finally {
            ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", null);
            cleanupRecovery(game);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aManualReconnectAfterAPackedFailureDuringARecoveryStaysWithoutPacked() {
        Game game = recoveryGame();
        ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", new TerminationProbe().listener);
        try {
            assertTrue(ReflectionHelpers.callInstanceMethod(game.streamRecovery, "tryAutomaticRecovery"));
            ReflectionHelpers.callInstanceMethod(game.streamRecovery, "onPackedChromaFailure",
                    ReflectionHelpers.ClassParameter.from(String.class, "the GPU path failed"));
            // The recovery goes stale (cancelled, timed out) and the user taps Reconnect.
            ReflectionHelpers.callInstanceMethod(game.streamRecovery, "cancelAutomaticRecovery");
            Intent reconnect = ReflectionHelpers.callInstanceMethod(game.streamRecovery, "createReconnectIntent");
            assertTrue(reconnect.getBooleanExtra("PackedChromaOff", false));
            Intent resume = ReflectionHelpers.callInstanceMethod(game.streamRecovery, "createStreamHandoffIntent",
                    ReflectionHelpers.ClassParameter.from(String.class, com.limelight.utils.LaunchSource.RETURN));
            assertTrue(resume.getBooleanExtra("PackedChromaOff", false));
        } finally {
            ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", null);
            cleanupRecovery(game);
        }
    }

    @Test
    public void aStreamStartedAgainDoesNotNameTheGameItsFirstStartReplaced() {
        Game game = recoveryGame();
        try {
            game.setIntent(new Intent().putExtra(Game.EXTRA_REPLACED_APP_NAME, "Balatro"));
            // By the reconnect another device's game may be running: its confirmation must not name Balatro.
            Intent reconnect = ReflectionHelpers.callInstanceMethod(game.streamRecovery, "createReconnectIntent");
            assertFalse(reconnect.hasExtra(Game.EXTRA_REPLACED_APP_NAME));
            Intent resume = ReflectionHelpers.callInstanceMethod(game.streamRecovery, "createStreamHandoffIntent",
                    ReflectionHelpers.ClassParameter.from(String.class, com.limelight.utils.LaunchSource.RETURN));
            assertFalse(resume.hasExtra(Game.EXTRA_REPLACED_APP_NAME));
        } finally {
            cleanupRecovery(game);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void packedChromaFailureWhileTheStreamWaitsToResumeMarksTheResume() throws Exception {
        Game game = recoveryGame();
        ReflectionHelpers.setField(game, "foreground", false);
        Intent resume = new Intent();
        ReflectionHelpers.setField(game, "stoppedStreamResumeIntent", resume);
        TerminationProbe probe = new TerminationProbe();
        ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", probe.listener);
        try {
            ReflectionHelpers.callInstanceMethod(game.streamRecovery, "onPackedChromaFailure",
                    ReflectionHelpers.ClassParameter.from(String.class, "the GPU path failed"));
            assertTrue(resume.getBooleanExtra("PackedChromaOff", false));
            assertFalse(probe.ended.await(300, java.util.concurrent.TimeUnit.MILLISECONDS));
        } finally {
            ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", null);
        }
    }

    @Test public void packedChromaFailureInTheBackgroundDoesNotReconnect() {
        Game game = recoveryGame();
        ReflectionHelpers.setField(game, "foreground", false);
        assertFalse(ReflectionHelpers.callInstanceMethod(game.streamRecovery, "reconnectWithoutPackedChroma",
                ReflectionHelpers.ClassParameter.from(String.class, "gpu")));
        assertNull(ReflectionHelpers.getField(game, "pendingRecovery"));
    }

    // Nightjar #88: a refusal in the handshake after the launch went through the delayed, resume-only recovery.
    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aFullChromaRefusalAfterTheLaunchStartsTheSameStreamAgainWithoutIt() throws Exception {
        // Also on a startup retry, where a transient-looking failure would otherwise retry as it was.
        for (int attempts : new int[] {0, 1}) {
            Game game = recoveryGame();
            long started = SystemClock.elapsedRealtime();
            game.setIntent(new Intent().putExtra("RecoveryAttempts", attempts).putExtra("RecoveryStarted", started)
                    .putExtra(Game.EXTRA_APP_ID, 73).putExtra(Game.EXTRA_PC_UUID, "pc-88"));
            setBudget(game, attempts, started);
            PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
            config.autoReconnect = false;
            RecoveryBridge.connectivityTests = 0;
            org.robolectric.shadows.ShadowToast.reset();
            try {
                refuseFullChromaAtStart(game);
                assertNull("no error dialog", ReflectionHelpers.getField(game, "streamMenu"));
                assertNull("no delayed recovery", ReflectionHelpers.getField(game, "pendingRecovery"));
                assertEquals("not a network failure", 0, RecoveryBridge.connectivityTests);
                // The same start again: it finds the game it launched running and resumes it.
                Intent again = awaitStartedActivity(game);
                assertTrue(again.getBooleanExtra("PackedChromaOff", false));
                assertEquals(73, again.getIntExtra(Game.EXTRA_APP_ID, 0));
                assertEquals("not counted as a recovery", attempts, again.getIntExtra("RecoveryAttempts", 0));
                // Said every time: this restart is slower than one decided before the launch.
                assertEquals(1, org.robolectric.shadows.ShadowToast.shownToastCount());
            } finally {
                cleanupRecovery(game);
            }
        }
    }

    @Test
    public void onlyAStartThatLaunchedBesideTheGameReconnectsAfterAFullChromaRefusal() {
        assertTrue(com.limelight.binding.video.PackedFallback.restartsAsTheSameStart(false));
        assertFalse(com.limelight.binding.video.PackedFallback.restartsAsTheSameStart(true));
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aFullChromaRefusalOnAResumeOrRunningGameTileAlsoReconnectsWithoutIt() throws Exception {
        // Resume (2147483501) and Running Game (2147483507) tiles resolve to a game only once the
        // stream has started; a refusal comes before that.
        for (int tile : new int[] {2147483501, 2147483507}) {
            for (boolean running : new boolean[] {true, false}) {
                Game game = recoveryGame();
                NvApp app = new NvApp("Tile", tile, false);
                assertNotEquals(NvApp.Control.NONE, app.getControl());
                ReflectionHelpers.setField(game, "app", app);
                if (running) {
                    NvConnection connection = ReflectionHelpers.getField(game, "conn");
                    ComputerDetails host = new ComputerDetails();
                    host.runningGameId = 73;
                    host.runningGameUuid = "game-uuid";
                    ReflectionHelpers.setField(connection, "hostDetails", host);
                }
                game.setIntent(new Intent().putExtra(Game.EXTRA_APP_ID, tile));
                PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
                config.autoReconnect = false;
                try {
                    refuseFullChromaAtStart(game);
                    assertNull("no error dialog", ReflectionHelpers.getField(game, "streamMenu"));
                    Intent retry = awaitRecoveryLaunch(game, 1);
                    assertTrue(retry.getBooleanExtra("PackedChromaOff", false));
                    // The game the PC reported running, as a started stream resolves it; else the tile again.
                    assertEquals(running ? 73 : tile, retry.getIntExtra(Game.EXTRA_APP_ID, 0));
                    if (running) assertEquals("game-uuid", retry.getStringExtra(Game.EXTRA_APP_UUID));
                } finally {
                    cleanupRecovery(game);
                }
            }
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aSecondFullChromaRefusalShowsTheConnectionErrorWithoutAnotherRetry() {
        Game game = recoveryGame();
        // The session the first refusal started: already without full chroma.
        game.setIntent(new Intent().putExtra("PackedChromaOff", true).putExtra("RecoveryAttempts", 1));
        try {
            refuseFullChromaAtStart(game);
            assertNull(ReflectionHelpers.getField(game, "pendingRecovery"));
            assertTrue(ReflectionHelpers.getField(game, "displayedFailureDialog"));
            assertNotNull("the usual connection error", ReflectionHelpers.getField(game, "streamMenu"));
        } finally {
            cleanupRecovery(game);
        }
        assertTrue(Game.terminationDescription(MoonBridge.CLIENT_ERROR_PACKED_444_REFUSED).contains("full chroma"));
    }

    // Nightjar #88: a restart waited for the old connection's stop without a limit, behind a spinner.
    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aRestartWhoseOldConnectionNeverEndsSaysSoInsteadOfWaitingOn() throws Exception {
        Game game = recoveryGame();
        java.util.concurrent.CountDownLatch release = new java.util.concurrent.CountDownLatch(1);
        ReflectionHelpers.setField(game, "conn", new NvConnection(null,
                new ComputerDetails.AddressTuple("192.0.2.1", 47989), 47984, "test",
                new StreamConfiguration.Builder().build(), null, null) {
            @Override public void stop() {
                try {
                    release.await();
                } catch (InterruptedException ignored) {
                }
            }
        });
        game.setIntent(new Intent().putExtra(Game.EXTRA_APP_ID, 73).putExtra(Game.EXTRA_PC_UUID, "pc-88"));
        game.streamRecovery.stopWaitMs = 50;
        try {
            refuseFullChromaAtStart(game);
            for (Thread thread : Thread.getAllStackTraces().keySet()) {
                if ("Stream reconnect".equals(thread.getName())) join(thread);
            }
            org.robolectric.shadows.ShadowLooper.idleMainLooper();
            assertNull("nothing starts over a connection that has not ended",
                    org.robolectric.Shadows.shadowOf(game).getNextStartedActivity());
            assertFalse(ReflectionHelpers.getField(game, "recoveryHandoffInProgress"));
            assertNull("no spinner left up", ReflectionHelpers.getField(game, "spinner"));
            assertNotNull("a visible failure with Try again", ReflectionHelpers.getField(game, "streamMenu"));
        } finally {
            release.countDown();
            join(ReflectionHelpers.getField(game, "connectionStopThread"));
            cleanupRecovery(game);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void packedChromaFailureReportsStartFailureOnlyForActiveStreamListener() {
        Game game = recoveryGame();
        ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", game);
        ReflectionHelpers.setStaticField(MoonBridge.class, "startFailureReported", false);
        try {
            com.limelight.nvstream.NvConnectionListener otherStream =
                    (com.limelight.nvstream.NvConnectionListener) java.lang.reflect.Proxy.newProxyInstance(
                            MoonBridge.class.getClassLoader(),
                            new Class<?>[] { com.limelight.nvstream.NvConnectionListener.class },
                            (proxy, method, args) -> null);
            MoonBridge.markStartFailureReported(otherStream);
            assertFalse(MoonBridge.startFailureReported());

            MoonBridge.markStartFailureReported(game);
            assertTrue(MoonBridge.startFailureReported());
        } finally {
            ReflectionHelpers.setStaticField(MoonBridge.class, "connectionListener", null);
            ReflectionHelpers.setStaticField(MoonBridge.class, "startFailureReported", false);
            cleanupRecovery(game);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void lateUsbServiceConnectedIsIgnoredAfterConnectionRelease() {
        Game game = recoveryGame();
        try {
            game.usbDriverServiceBound = true;
            game.streamConnection.releaseUsbDriverBinder();
            assertTrue(game.usbDriverReleased);
            // Passing null iBinder proves the guard returns early before casting to UsbDriverBinder.
            game.usbDriverServiceConnection.onServiceConnected(null, null);
        } finally {
            cleanupRecovery(game);
        }
    }
}
