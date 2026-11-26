package com.limelight.binding.input;

import android.app.Application;
import android.os.Looper;

import com.limelight.nvstream.NvConnection;
import com.limelight.nvstream.StreamConfiguration;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.preferences.PreferenceConfiguration;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.util.ReflectionHelpers;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.junit.Assert.*;

/**
 * NJ#187: verifies 100 Hz controller mouse polling, dt compensation, per-controller fractional pixel
 * accumulators (preserved on migration, reset on toggle/destroy), removal of the 1-px move gate,
 * deadzone and scroll gating, and sensitivity scaling (25-200%).
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class ControllerMouseEmulationTest {
    private static final class MouseRecordingConnection extends NvConnection {
        final List<int[]> moves = new ArrayList<>();
        final List<Short> vScrolls = new ArrayList<>();
        final List<Short> hScrolls = new ArrayList<>();

        MouseRecordingConnection() {
            super(null, new ComputerDetails.AddressTuple("127.0.0.1", 47989), 47984, "test",
                    new StreamConfiguration.Builder().build(), null, null);
        }

        @Override public boolean canSendInput() { return true; }

        @Override public void sendMouseMove(short deltaX, short deltaY) {
            moves.add(new int[] { deltaX, deltaY });
        }

        @Override public void sendMouseHighResScroll(short scrollAmount) {
            vScrolls.add(scrollAmount);
        }

        @Override public void sendMouseHighResHScroll(short scrollAmount) {
            hScrolls.add(scrollAmount);
        }
    }

    @Test
    public void controllerMouseTickRemovesOnePxMoveGateKeepsScrollThresholdAndScalesSensitivity() {
        float[] low = new float[6];
        // Slight stick tilt (~0.35 of range -> x=1.4 -> curve 1.4^3=2.744 * 0.2 = 0.5488 px/tick < 1 px).
        short smallStickX = 11468;
        int flags100 = ControllerMouseTick.compute(smallStickX, (short) 0, (short) 0, (short) 0,
                0, 0, 100, low);
        assertEquals("small non-zero move below 1 px is NOT gated out", 1, flags100);
        assertTrue("expected sub-pixel fractional delta in (0, 1): " + low[0], low[0] > 0.0f && low[0] < 1.0f);

        float[] dbl = new float[6];
        ControllerMouseTick.compute(smallStickX, (short) 0, (short) 0, (short) 0, 0, 0, 200, dbl);
        assertEquals(low[0] * 2.0f, dbl[0], 0.01f);

        float[] quarter = new float[6];
        ControllerMouseTick.compute(smallStickX, (short) 0, (short) 0, (short) 0, 0, 0, 25, quarter);
        assertEquals(low[0] * 0.25f, quarter[0], 0.01f);

        // Scroll stick keeps mag < 1.0 threshold (x=0.2 of full -> 0.8 -> mag 0.8 < 1.0 -> gated).
        float[] scrollOut = new float[6];
        short smallScroll = 6553;
        int smallScrollFlags = ControllerMouseTick.compute(
                (short) 0, (short) 0, (short) 0, smallScroll, 1, 0, 200, scrollOut);
        assertEquals("scroll below mag 1.0 remains gated even at 200% mouse sensitivity", 0, smallScrollFlags);

        // Scroll above threshold ignores mouse sensitivity.
        float[] scroll100 = new float[6];
        float[] scroll200 = new float[6];
        int sFlags = ControllerMouseTick.compute(
                (short) 0, (short) 0, (short) 0, (short) 24576, 1, 0, 100, scroll100);
        ControllerMouseTick.compute(
                (short) 0, (short) 0, (short) 0, (short) 24576, 1, 0, 200, scroll200);
        assertEquals(4, sFlags & 4);
        assertEquals(scroll100[4], scroll200[4], 0.001f);
        assertEquals(scroll100[5], scroll200[5], 0.001f);
    }

    @Test
    public void dtCompensationScalesLinearlyAndClampsBounds() {
        assertEquals(1.0f, ControllerMouseTick.dtCompensation(0L, 10_000_000L, 10), 0.001f);
        assertEquals(1.0f, ControllerMouseTick.dtCompensation(1_000_000L, 11_000_000L, 10), 0.001f);
        assertEquals(2.0f, ControllerMouseTick.dtCompensation(1_000_000L, 21_000_000L, 10), 0.001f);
        assertEquals("clamped lower bound 0.1", 0.1f, ControllerMouseTick.dtCompensation(1_000_000L, 1_100_000L, 10), 0.001f);
        assertEquals("clamped upper bound 5.0", 5.0f, ControllerMouseTick.dtCompensation(1_000_000L, 500_000_000L, 10), 0.001f);
    }

    @Test
    public void handlerRunsAt100HzAccumulatesSubPixelDeltasAndResetsOnToggleAndDestroy() {
        try (ActivityController<ControllerInputReleaseTest.MenuGame> activity =
                     Robolectric.buildActivity(ControllerInputReleaseTest.MenuGame.class).create()) {
            MouseRecordingConnection conn = new MouseRecordingConnection();
            PreferenceConfiguration prefs = new PreferenceConfiguration();
            prefs.mouseEmulation = true;
            prefs.mouseEmulationSensitivity = 100;
            ControllerHandler handler = new ControllerHandler(activity.get(), conn, null, prefs);
            ControllerHandler.GenericControllerContext context =
                    ReflectionHelpers.getField(handler, "defaultContext");
            assertEquals("NJ#187 100 Hz mouse tick period", 10, context.mouseEmulationReportPeriod);

            final long[] nowNs = { 1_000_000_000L };
            handler.gamepad.setNanoClockForTest(() -> nowNs[0]);

            context.toggleMouseEmulation();
            assertTrue(context.mouseEmulationActive);

            // Sub-pixel tilt (~0.31 px per 10 ms tick): first tick accumulates 0.31 px (< 0.5 -> 0 emitted),
            // second tick (+10 ms) reaches 0.62 px and emits 1 px!
            context.leftStickX = 9500;
            nowNs[0] += 10_000_000L;
            context.mouseEmulationRunnable.run();
            int afterFirst = conn.moves.size();
            assertEquals("first 0.31 px tick remains in fractional accumulator", 0, afterFirst);

            nowNs[0] += 10_000_000L;
            context.mouseEmulationRunnable.run();
            assertEquals("second 0.31 px tick crosses 0.5 px and emits accumulated 1 px move", 1, conn.moves.size());
            assertEquals(1, conn.moves.get(0)[0]);
            assertEquals(0, conn.moves.get(0)[1]);

            // Toggling mouse emulation off resets all accumulators and last tick time.
            context.toggleMouseEmulation();
            assertFalse(context.mouseEmulationActive);
            assertEquals(0L, context.mouseEmulationLastTickNs);

            // Non-zero remainder (+0.31) before toggle off must not leak into next toggle session (+0.31).
            context.toggleMouseEmulation();
            nowNs[0] += 10_000_000L;
            context.mouseEmulationRunnable.run();
            assertEquals(1, conn.moves.size());
            context.toggleMouseEmulation();
            context.toggleMouseEmulation();
            nowNs[0] += 10_000_000L;
            context.mouseEmulationRunnable.run();
            assertEquals("reset accumulator must not carry old +0.31 px over toggle", 1, conn.moves.size());

            // Destroy also clears accumulators.
            context.mouseMoveAccumX.scale(0.45f, 1.0f);
            handler.destroy();
            assertEquals(0L, context.mouseEmulationLastTickNs);
            assertEquals(0, context.mouseMoveAccumX.scale(0.4f, 1.0f));
        }
    }

    @Test
    public void contextMigrationPreservesMouseAccumulatorsAndTimestamp() {
        try (ActivityController<ControllerInputReleaseTest.MenuGame> activity =
                     Robolectric.buildActivity(ControllerInputReleaseTest.MenuGame.class).create()) {
            MouseRecordingConnection conn = new MouseRecordingConnection();
            ControllerHandler handler = new ControllerHandler(activity.get(), conn, null, new PreferenceConfiguration());
            try {
                ControllerHandler.InputDeviceContext oldCtx = handler.new InputDeviceContext();
                assertEquals(0, oldCtx.mouseMoveAccumX.scale(0.4f, 1.0f));
                assertEquals(0, oldCtx.mouseMoveAccumY.scale(-0.4f, 1.0f));
                assertEquals(0, oldCtx.mouseScrollVAccum.scale(0.3f, 1.0f));
                assertEquals(0, oldCtx.mouseScrollHAccum.scale(-0.4f, 1.0f));
                oldCtx.mouseEmulationLastTickNs = 987_654_321L;

                ControllerHandler.InputDeviceContext migrated = handler.new InputDeviceContext();
                handler.lifecycle.migrateInputDeviceContext(migrated, oldCtx);
                assertEquals(987_654_321L, migrated.mouseEmulationLastTickNs);
                assertEquals("preserved X remainder 0.4 + 0.3 crosses 0.7 -> 1", 1, migrated.mouseMoveAccumX.scale(0.3f, 1.0f));
                assertEquals("preserved Y remainder -0.4 + -0.3 crosses -0.7 -> -1", -1, migrated.mouseMoveAccumY.scale(-0.3f, 1.0f));
                assertEquals("preserved V remainder 0.3 + 0.4 crosses 0.7 -> 1", 1, migrated.mouseScrollVAccum.scale(0.4f, 1.0f));
                assertEquals("preserved H remainder -0.4 + -0.3 crosses -0.7 -> -1", -1, migrated.mouseScrollHAccum.scale(-0.3f, 1.0f));
            } finally {
                handler.destroy();
            }
        }
    }
}
