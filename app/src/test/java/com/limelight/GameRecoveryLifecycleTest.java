package com.limelight;

import android.app.Application;
import android.content.Intent;
import android.os.SystemClock;

import com.limelight.GameRecoveryTestSupport.MuteConnection;
import com.limelight.GameRecoveryTestSupport.RecoveryBridge;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarPolicyJni;
import com.limelight.preferences.PreferenceConfiguration;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.annotation.Config;
import org.robolectric.util.ReflectionHelpers;

import static com.limelight.GameRecoveryTestSupport.*;
import static org.junit.Assert.*;

/**
 * Tests {@link GameRecovery} lock/resume retry budgets, user-trip accounting, connecting window
 * cleanup, surface destruction, and activity-stop eligibility rules.
 */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class GameRecoveryLifecycleTest {
    @Test public void lockResumePreservesCapturedIdentityOutputAndExistingRetryBudget() throws Exception {
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(10));
        Game game = establishedGame();
        long started = SystemClock.elapsedRealtime() - 5000;
        game.setIntent(new Intent().putExtra(Game.EXTRA_APP_ID, 2147483501)
                .putExtra("RecoveryAttempts", 2).putExtra("RecoveryStarted", started)
                .putExtra("rubylight.outputDisplay", 42).putExtra("rubylight.outputResumeOnly", true));
        setBudget(game, 2, started);
        try {
            game.onPause();
            long leftAt = SystemClock.elapsedRealtime();
            game.onStop();
            assertFalse(game.isFinishing());
            assertNotNull(ReflectionHelpers.getField(game, "stoppedStreamResumeIntent"));
            org.robolectric.shadows.ShadowLooper.idleMainLooper(10, java.util.concurrent.TimeUnit.SECONDS);
            assertNull(org.robolectric.Shadows.shadowOf(game).getNextStartedActivity());
            // A shortcut or tile must not replace the identity captured when the session began.
            game.getIntent().putExtra(Game.EXTRA_APP_ID, 99);
            long returnedAt = SystemClock.elapsedRealtime();
            game.onResume();
            assertNull(ReflectionHelpers.getField(game, "stoppedStreamResumeIntent"));
            assertFalse(ReflectionHelpers.callInstanceMethod(game.streamRecovery, "resumeStoppedStreamIfNeeded"));
            Intent retry = awaitRecoveryLaunch(game, 1);
            assertEquals(73, retry.getIntExtra(Game.EXTRA_APP_ID, 0));
            assertEquals("captured-game", retry.getStringExtra(Game.EXTRA_APP_UUID));
            assertEquals(42, retry.getIntExtra("rubylight.outputDisplay", 0));
            assertTrue(retry.getBooleanExtra("rubylight.outputResumeOnly", false));
            // The resume is not an attempt: it carries the budget as the trip began and its own deadline.
            assertArrayEquals(new long[] {2, started, leftAt, 0}, retry.getLongArrayExtra("rubylight.tripBudget"));
            assertEquals(returnedAt, retry.getLongExtra("rubylight.tripResumeStarted", 0));
            assertEquals(2, game.recoveryAttempts);
            assertEquals(started, game.recoveryStartedAt);
            Game replacement = Robolectric.buildActivity(Game.class).get();
            assertNull(org.robolectric.Shadows.shadowOf(game).getNextStartedActivity());
            // The failure being recovered keeps its two attempts; its window starts later by the time away.
            long createdAt = SystemClock.elapsedRealtime();
            startStream(replacement, retry);
            assertTrue(ReflectionHelpers.callInstanceMethod(replacement.streamRecovery, "isResumeOnlyStream"));
            assertEquals(2, replacement.recoveryAttempts);
            assertEquals(started + (createdAt - leftAt), replacement.recoveryStartedAt);
            assertEquals(2, replacement.getIntent().getIntExtra("RecoveryAttempts", 0));
            assertFalse(replacement.getIntent().hasExtra("rubylight.tripBudget"));
        } finally {
            cleanupRecovery(game);
        }
    }

    @Test public void userTripsWithinAMinuteEachResumeWithoutSpendingTheFailureBudget() throws Exception {
        // R5 on the Retroid: Files, Back, under 30 s of stream, Files again; two and three trips.
        for (long[] trips : new long[][] {{15000, 45000}, {15000, 15000, 15000}}) {
            Game stream = establishedGame();
            try {
                for (long away : trips) {
                    stream = leaveAndReturn(stream, away);
                    org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(20));
                }
            } finally {
                cleanupRecovery(stream);
            }
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void tripsAfterARecoveredBlipEachResumeAndCountOnlyConnectedTime() throws Exception {
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(100));
        // A Wi-Fi blip at t=0 recovered by t=3 (attempt 1), then Files every 25 s.
        long blip = SystemClock.elapsedRealtime();
        Game stream = establishedGame();
        stream.setIntent(new Intent().putExtra("RecoveryAttempts", 1).putExtra("RecoveryStarted", blip));
        setBudget(stream, 1, blip);
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(3));
        stream.streamRecovery.onStreamConnected(SystemClock.elapsedRealtime());
        try {
            org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(20));
            stream = leaveAndReturn(stream, 5000);
            // The blip's budget is back, its window run only by the 3 s startup and the 20 s connected.
            assertEquals(1, stream.recoveryAttempts);
            assertEquals(23000, SystemClock.elapsedRealtime() - stream.recoveryStartedAt);
            for (int trip = 0; trip < 3; trip++) {
                org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(20));
                stream = leaveAndReturn(stream, 5000);
            }
            // 40 s connected in all by the second trip: as without the trips, the blip's budget is
            // done and a failure now has a full one.
            assertTrue(SystemClock.elapsedRealtime() - blip > NightjarPolicyJni.recoveryWindowMs());
            assertEquals(0, stream.recoveryAttempts);
            org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(5));
            long failedAt = SystemClock.elapsedRealtime();
            ReflectionHelpers.setField(stream, "conn", new MuteConnection() {
                @Override public boolean hasEndedSession() { return false; }
            });
            stream.connectionTerminated(-1);
            Intent retry = awaitRecoveryLaunch(stream, 1);
            assertEquals(1, retry.getIntExtra("RecoveryAttempts", 0));
            assertEquals(failedAt, retry.getLongExtra("RecoveryStarted", 0));
        } finally {
            cleanupRecovery(stream);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void neitherTimeAwayNorTheResumeCountsAsConnectedTimeAndTheCarryCountsOnce() throws Exception {
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(100));
        // A blip at t=0 recovered by t=3 (attempt 1); 10 s connected, a minute away, 10 s connected.
        long blip = SystemClock.elapsedRealtime();
        Game stream = establishedGame();
        stream.setIntent(new Intent().putExtra("RecoveryAttempts", 1).putExtra("RecoveryStarted", blip));
        setBudget(stream, 1, blip);
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(3));
        stream.streamRecovery.onStreamConnected(SystemClock.elapsedRealtime());
        try {
            org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(10));
            long leftAt = SystemClock.elapsedRealtime();
            stream = leaveAndReturn(stream, 60000);
            long resumedAt = SystemClock.elapsedRealtime();
            org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(10));
            ReflectionHelpers.setField(stream, "conn", new MuteConnection() {
                @Override public boolean hasEndedSession() { return false; }
            });
            stream.connectionTerminated(-1);
            // 20 s connected in all, short of the 30 s that would earn a fresh budget: had the
            // minute away, the resume's startup or the carried 10 s twice counted, it would be one.
            Intent retry = awaitRecoveryLaunch(stream, 2);
            assertEquals(2, retry.getIntExtra("RecoveryAttempts", 0));
            // The blip's window, its start later by the time from leaving until the resume started.
            assertEquals(blip + (resumedAt - leftAt), retry.getLongExtra("RecoveryStarted", 0));
        } finally {
            cleanupRecovery(stream);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aStartupFailureOfAResumeAfterATripWithNoFailureRetriesOnAFreshBudget() throws Exception {
        Intent resume = leaveAndComeBack(establishedGame(), 10000);
        // The replacement activity before connectionStarted, its budget read as onCreate does.
        Game resumed = recoveryGame();
        startStream(resumed, resume);
        assertEquals(0, resumed.recoveryAttempts);
        ReflectionHelpers.setField(resumed, "connected", false);
        ReflectionHelpers.setField(resumed, "connecting", true);
        RecoveryBridge.connectivityTests = 0;
        try {
            long failedAt = SystemClock.elapsedRealtime();
            resumed.stageFailed("Control stream establishment", MoonBridge.ML_PORT_FLAG_UDP_47999, 110);
            // Still a recovery attempt with no attempt spent: it retries instead of an error.
            assertNotNull(ReflectionHelpers.getField(resumed, "pendingRecovery"));
            assertNull(ReflectionHelpers.getField(resumed, "streamMenu"));
            assertEquals(0, RecoveryBridge.connectivityTests);
            Intent retry = awaitRecoveryLaunch(resumed, 1);
            assertEquals(1, retry.getIntExtra("RecoveryAttempts", 0));
            assertEquals(failedAt, retry.getLongExtra("RecoveryStarted", 0));
            assertFalse(retry.hasExtra("rubylight.tripBudget"));
            assertFalse(retry.hasExtra("rubylight.tripResumeStarted"));
        } finally {
            cleanupRecovery(resumed);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aRealFailureAfterUserTripsHasItsFullBudgetAndIsStillRefusedAfterItsMinute() throws Exception {
        Game stream = leaveAndReturn(leaveAndReturn(establishedGame(), 15000), 15000);
        try {
            long firstFailure = 0;
            int recoveries = 0;
            long[] delays = {1, 2, 4, 8, 12};
            while (true) {
                // A flapping connection: up 15 s at a time, never the 30 s that earns a new budget.
                org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(15));
                if (firstFailure == 0) firstFailure = SystemClock.elapsedRealtime();
                ReflectionHelpers.setField(stream, "conn", new MuteConnection() {
                    @Override public boolean hasEndedSession() { return false; }
                });
                stream.connectionTerminated(-1);
                if (ReflectionHelpers.getField(stream, "pendingRecovery") == null) break;
                Intent retry = awaitRecoveryLaunch(stream, (int) delays[recoveries]);
                cleanupRecovery(stream);
                recoveries++;
                assertEquals(recoveries, retry.getIntExtra("RecoveryAttempts", 0));
                // The budget starts at the first failure, not at either trip.
                assertEquals(firstFailure, retry.getLongExtra("RecoveryStarted", 0));
                assertFalse(retry.hasExtra("rubylight.tripBudget"));
                assertFalse(retry.hasExtra("rubylight.tripResumeStarted"));
                stream = establishedGame();
                startStream(stream, retry);
                stream.streamRecovery.onStreamConnected(SystemClock.elapsedRealtime());
            }
            // Failures at 0, 16, 33 and 52 s: the fourth attempt would end past the minute.
            assertEquals(3, recoveries);
            assertTrue(SystemClock.elapsedRealtime() - firstFailure < NightjarPolicyJni.recoveryWindowMs());
            assertFalse(stream.isFinishing());
            android.app.Dialog dialog = ReflectionHelpers.getField(stream, "streamMenu");
            assertNotNull(dialog);
            assertTrue(dialog.isShowing());
        } finally {
            cleanupRecovery(stream);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aFailureWhileTheResumeStartsContinuesTheBudgetFromBeforeTheTrip() throws Exception {
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(100));
        long started = SystemClock.elapsedRealtime() - 5000;
        Game stream = establishedGame();
        setBudget(stream, 2, started);
        Game resumed = leaveAndReturn(stream, 10000);
        long budgetStart = resumed.recoveryStartedAt;
        // connectionStarted has not come yet.
        ReflectionHelpers.setField(resumed, "connected", false);
        ReflectionHelpers.setField(resumed, "connecting", true);
        ReflectionHelpers.setField(resumed.streamRecovery, "connectedAt", -1L);
        ReflectionHelpers.setField(resumed, "conn", new MuteConnection() {
            @Override public boolean hasEndedSession() { return false; }
        });
        try {
            resumed.connectionTerminated(-1);
            assertNotNull(ReflectionHelpers.getField(resumed, "pendingRecovery"));
            Intent retry = awaitRecoveryLaunch(resumed, 4);
            assertEquals(3, retry.getIntExtra("RecoveryAttempts", 0));
            assertEquals(budgetStart, retry.getLongExtra("RecoveryStarted", 0));
            assertFalse(retry.hasExtra("rubylight.tripBudget"));
            assertFalse(retry.hasExtra("rubylight.tripResumeStarted"));
        } finally {
            cleanupRecovery(resumed);
        }
    }

    @Test public void unpluggingTheDisplayWhileAwayIsAUserTrip() {
        Game game = establishedGame();
        try {
            game.onPause();
            ReflectionHelpers.callInstanceMethod(game.videoOutput, "returnVideoToDevice");
            Intent resume = ReflectionHelpers.getField(game, "stoppedStreamResumeIntent");
            assertNotNull(resume);
            assertTrue(resume.hasExtra("rubylight.tripBudget"));
            assertFalse(resume.hasExtra("rubylight.outputDisplay"));
        } finally {
            cleanupRecovery(game);
        }
    }

    @Test public void aFailureReportedAsTheUserLeavesIsNoUserTrip() {
        Game game = establishedGame();
        try {
            game.onPause();
            // connectionTerminated marked the stream on its own thread; its UI half has not run yet.
            ReflectionHelpers.setField(game, "streamTerminated", true);
            game.onStop();
            Intent resume = ReflectionHelpers.getField(game, "stoppedStreamResumeIntent");
            assertNotNull(resume);
            assertFalse(resume.hasExtra("rubylight.tripBudget"));
        } finally {
            cleanupRecovery(game);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void aConnectionLostWhileAwaySpendsTheFailureBudget() {
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(100));
        Game game = establishedGame();
        // Paused for the 10 s away it would have room for one more attempt; counted, it has none.
        long started = SystemClock.elapsedRealtime() - 40000;
        setBudget(game, 4, started);
        ReflectionHelpers.setField(game, "conn", new MuteConnection() {
            @Override public boolean hasEndedSession() { return false; }
        });
        try {
            game.onPause();
            game.connectionTerminated(-1);
            game.onStop();
            Intent resume = ReflectionHelpers.getField(game, "stoppedStreamResumeIntent");
            assertNotNull(resume);
            assertFalse("a lost connection is no user trip", resume.hasExtra("rubylight.tripBudget"));
            org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(10));
            game.onResume();
            assertNull(ReflectionHelpers.getField(game, "pendingRecovery"));
            assertNull(org.robolectric.Shadows.shadowOf(game).getNextStartedActivity());
        } finally {
            cleanupRecovery(game);
        }
    }

    @Test
    @Config(shadows = RecoveryBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    public void timeLockedResumesButCannotResetAnExhaustedUnstableRecoveryWindow() throws Exception {
        org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofSeconds(100));
        Game game = establishedGame();
        long started = SystemClock.elapsedRealtime() - 58000;
        setBudget(game, 4, started);
        // Two minutes locked: the return still resumes, and the nearly spent window stays nearly spent.
        Game resumed = leaveAndReturn(game, 120000);
        assertEquals(4, resumed.recoveryAttempts);
        assertEquals(58000, SystemClock.elapsedRealtime() - resumed.recoveryStartedAt);
        ReflectionHelpers.setField(resumed, "conn", new MuteConnection() {
            @Override public boolean hasEndedSession() { return false; }
        });
        try {
            resumed.connectionTerminated(-1);
            assertNull(ReflectionHelpers.getField(resumed, "pendingRecovery"));
            android.app.Dialog dialog = ReflectionHelpers.getField(resumed, "streamMenu");
            assertNotNull(dialog);
            assertTrue(dialog.isShowing());
            assertNull(org.robolectric.Shadows.shadowOf(resumed).getNextStartedActivity());
        } finally {
            cleanupRecovery(resumed);
        }
    }

    @Test public void aTripResumeAndADropsRecoveryCarryThePacketSizeWithItsNetwork() throws Exception {
        MuteConnection jumbo = new MuteConnection() {
            @Override public int resumePacketSize() { return 8192; }
            @Override public long resumePacketNetwork() { return 42; }
        };
        Game left = establishedGame();
        ReflectionHelpers.setField(left, "conn", jumbo);
        Game dropped = establishedGame();
        ReflectionHelpers.setField(dropped, "conn", jumbo);
        try {
            Intent trip = leaveAndComeBack(left, 1000);
            assertTrue(dropped.streamRecovery.tryAutomaticRecovery());
            for (Intent again : new Intent[] {trip, awaitRecoveryLaunch(dropped, 10)}) {
                assertEquals(8192, again.getIntExtra("rubylight.recoveryPacketSize", 0));
                assertEquals(42, again.getLongExtra("rubylight.recoveryPacketNetwork", 0));
            }
        } finally {
            cleanupRecovery(left);
            cleanupRecovery(dropped);
        }
    }

    @Test public void leavingADelayedLockResumeCancelsItBeforeAnotherActivityStarts() throws Exception {
        Game game = establishedGame();
        try {
            game.onPause();
            game.onStop();
            game.onResume();
            com.limelight.ui.ConnectingScreen spinner = ReflectionHelpers.getField(game, "spinner");
            assertNotNull(spinner);
            spinner.dialog().cancel();
            org.robolectric.shadows.ShadowLooper.idleMainLooper();
            assertTrue(game.isFinishing());
            org.robolectric.shadows.ShadowLooper.idleMainLooper(15, java.util.concurrent.TimeUnit.SECONDS);
            assertNull(org.robolectric.Shadows.shadowOf(game).getNextStartedActivity());
        } finally {
            cleanupRecovery(game);
        }
    }

    @Test public void aReconnectOverAnOpenConnectingWindowRemovesTheOldOne() throws Exception {
        Game game = establishedGame();
        try {
            game.onPause();
            game.onStop();
            game.onResume();
            com.limelight.ui.ConnectingScreen old = ReflectionHelpers.getField(game, "spinner");
            assertNotNull(old);
            org.robolectric.shadows.ShadowLooper.idleMainLooper();
            assertTrue(old.dialog().isShowing());
            ReflectionHelpers.setField(game, "spinner", game.streamConnection.showConnecting("again"));
            org.robolectric.shadows.ShadowLooper.idleMainLooper();
            assertFalse("the first window would be orphaned over the stream", old.dialog().isShowing());
        } finally {
            cleanupRecovery(game);
        }
    }

    // A started stream readies the host-loss watch, which asks MoonBridge whether the native check loaded.
    @Config(shadows = PcViewLifecycleTest.NoNativeMoonBridge.class, instrumentedPackages = "com.limelight.nvstream.jni")
    @Test public void aStartedStreamLetsTheConnectingCoverGoAndKeepsTheMenusCopy() throws Exception {
        Game game = establishedGame();
        try {
            android.graphics.Bitmap full = android.graphics.Bitmap.createBitmap(600, 900, android.graphics.Bitmap.Config.ARGB_8888);
            android.graphics.Bitmap small = android.graphics.Bitmap.createBitmap(60, 90, android.graphics.Bitmap.Config.ARGB_8888);
            ReflectionHelpers.setField(game.streamConnection, "connectingCover", full);
            ReflectionHelpers.setField(game.streamConnection, "menuCover", small);
            assertSame(full, game.streamConnection.connectingCover());
            ReflectionHelpers.setField(game, "connecting", true);
            try {
                game.streamConnection.connectionStarted();
            } catch (NullPointerException later) {
                // Past the connecting window the start goes on to the controllers, which this game has none of.
                assertTrue(later.getMessage(), later.getMessage().contains("controllerHandler"));
            }
            org.robolectric.shadows.ShadowLooper.idleMainLooper();
            assertSame("only the small copy is held while it streams", small, game.streamConnection.connectingCover());
        } finally {
            cleanupRecovery(game);
        }
    }

    @Test public void leavingWhileConnectingRemovesTheConnectingWindow() throws Exception {
        Game game = establishedGame();
        game.onPause();
        game.onStop();
        game.onResume();
        com.limelight.ui.ConnectingScreen spinner = ReflectionHelpers.getField(game, "spinner");
        assertNotNull(spinner);
        android.app.Dialog window = spinner.dialog();
        org.robolectric.shadows.ShadowLooper.idleMainLooper();
        assertTrue(window.isShowing());
        cleanupRecovery(game);
        game.onDestroy();
        org.robolectric.shadows.ShadowLooper.idleMainLooper();
        assertFalse("a stale window would sit over whatever comes next", window.isShowing());
    }

    @Test public void lifecycleResumeRequiresAnEstablishedEligibleSessionAndEnabledRecovery() {
        for (String condition : new String[] {"disabled", "quit", "failure", "uncaptured", "connecting"}) {
            Game game = establishedGame();
            if (condition.equals("disabled")) {
                PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
                config.autoReconnect = false;
            } else if (condition.equals("quit")) game.finish();
            else if (condition.equals("failure")) ReflectionHelpers.setField(game, "displayedFailureDialog", true);
            else if (condition.equals("uncaptured")) ReflectionHelpers.setField(game, "recoveryAppId", 0);
            else {
                ReflectionHelpers.setField(game, "connected", false);
                ReflectionHelpers.setField(game, "connecting", true);
            }
            ReflectionHelpers.callInstanceMethod(game.streamRecovery, "rememberStreamForResume",
                    ReflectionHelpers.ClassParameter.from(boolean.class, true));
            assertNull(condition, ReflectionHelpers.getField(game, "stoppedStreamResumeIntent"));
        }
    }

    @Test public void stoppingDisabledOrIneligibleSessionsStillFinishesTheirActivity() {
        for (String condition : new String[] {"disabled", "monitor", "input", "control", "failure", "uncaptured", "connecting"}) {
            Game game = establishedGame();
            if (condition.equals("disabled")) {
                PreferenceConfiguration config = ReflectionHelpers.getField(game, "prefConfig");
                config.autoReconnect = false;
            } else if (condition.equals("monitor")) {
                ReflectionHelpers.setField(game, "app", new NvApp("Monitor", 2147483505, false));
            } else if (condition.equals("input")) {
                ReflectionHelpers.setField(game, "app", new NvApp("Input", 2147483506, false));
            } else if (condition.equals("control")) {
                ReflectionHelpers.setField(game, "app", new NvApp("Terminate", 2147483504, false));
            } else if (condition.equals("failure")) {
                ReflectionHelpers.setField(game, "displayedFailureDialog", true);
            } else if (condition.equals("uncaptured")) {
                ReflectionHelpers.setField(game, "recoveryAppId", 0);
            } else {
                ReflectionHelpers.setField(game, "connected", false);
                ReflectionHelpers.setField(game, "connecting", true);
            }
            try {
                game.onPause();
                game.onStop();
                assertTrue(condition, game.isFinishing());
                assertNull(condition, ReflectionHelpers.getField(game, "stoppedStreamResumeIntent"));
                assertNull(condition, org.robolectric.Shadows.shadowOf(game).getNextStartedActivity());
            } finally {
                cleanupRecovery(game);
            }
        }
    }

    @Test public void withRecoveryOffLeavingDuringAReconnectEndsTheStream() throws Exception {
        Game game = establishedGame();
        PreferenceConfiguration prefs = ReflectionHelpers.getField(game, "prefConfig");
        prefs.autoReconnect = false;
        java.util.concurrent.CountDownLatch stopEntered = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.CountDownLatch stopReleased = new java.util.concurrent.CountDownLatch(1);
        ReflectionHelpers.setField(game, "conn", new MuteConnection() {
            @Override public void stop() {
                stopEntered.countDown();
                try { stopReleased.await(HANG_GUARD_MS, java.util.concurrent.TimeUnit.MILLISECONDS); }
                catch (InterruptedException interrupted) { Thread.currentThread().interrupt(); }
            }
        });
        try {
            ReflectionHelpers.callInstanceMethod(game.streamRecovery, "reconnectStream");
            assertTrue(stopEntered.await(HANG_GUARD_MS, java.util.concurrent.TimeUnit.MILLISECONDS));
            // Home while the old connection is still closing: the stream ends, as before recovery.
            game.onPause();
            game.onStop();
            assertTrue(game.isFinishing());
            stopReleased.countDown();
            settleHandoff(game);
            assertNull(org.robolectric.Shadows.shadowOf(game).getNextStartedActivity());
        } finally {
            stopReleased.countDown();
            cleanupRecovery(game);
        }
    }

    @Test public void destroyingAnEstablishedSurfaceWaitsForForegroundBeforeResuming() {
        Game game = establishedGame();
        ReflectionHelpers.setField(game, "surfaceCreated", true);
        game.onPause();
        try {
            ReflectionHelpers.callInstanceMethod(game.streamConnection, "destroyVideoSurface");
            assertFalse(game.isFinishing());
            assertNotNull(ReflectionHelpers.getField(game, "stoppedStreamResumeIntent"));
            assertNull(ReflectionHelpers.getField(game, "pendingRecovery"));
            game.onResume();
            assertNotNull(ReflectionHelpers.getField(game, "pendingRecovery"));
        } finally {
            cleanupRecovery(game);
        }
    }
}
