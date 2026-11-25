package com.limelight.binding.input.touch;

import static org.junit.Assert.*;

import android.app.Application;
import android.os.Looper;

import com.limelight.nvstream.NvConnection;
import com.limelight.nvstream.StreamConfiguration;
import com.limelight.nvstream.http.ComputerDetails;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.TimeUnit;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class TrackpadContextTest {
    @Test
    public void tappingClicksWithoutPressingTheTouchpad() {
        Recorder connection = new Recorder();
        TrackpadContext context = new TrackpadContext(connection, 0, false, 100, 100, 100, null);
        context.setPointerCount(1);
        context.touchDownEvent(500, 500, 1000, true);
        context.touchUpEvent(500, 500, 1060);
        assertEquals(Collections.singletonList("down 1"), connection.buttons);
        Shadows.shadowOf(Looper.getMainLooper()).idleFor(400, TimeUnit.MILLISECONDS);
        assertEquals(Arrays.asList("down 1", "up 1"), connection.buttons);
        assertTrue(connection.moves.isEmpty());
    }

    @Test
    public void cancellingDuringADoubleTapDragReleasesButtonAndResetsDragState() {
        Recorder connection = new Recorder();
        TrackpadContext context = new TrackpadContext(connection, 0, false, 100, 100, 100, null);
        context.setPointerCount(1);
        context.touchDownEvent(500, 500, 1000, true);
        context.touchUpEvent(500, 500, 1050);
        // Second tap within CLICK_RELEASE_DELAY starts a drag.
        context.touchDownEvent(500, 500, 1100, true);
        context.touchMoveEvent(560, 500, 1120);
        assertEquals(Collections.singletonList("down 1"), connection.buttons);

        context.cancelTouch();
        context.setPointerCount(0);
        assertEquals(Arrays.asList("down 1", "up 1"), connection.buttons);
        // Repeated cancel does not send duplicate button up.
        context.cancelTouch();
        assertEquals(Arrays.asList("down 1", "up 1"), connection.buttons);

        // Next gesture is a normal move, not stuck in drag.
        context.setPointerCount(1);
        context.touchDownEvent(500, 500, 2000, true);
        context.touchMoveEvent(560, 500, 2020);
        context.touchUpEvent(560, 500, 2040);
        assertEquals(Arrays.asList("down 1", "up 1"), connection.buttons);
    }

    @Test
    public void cancellingWhileTapReleaseIsPendingReleasesClickedButtonImmediately() {
        Recorder connection = new Recorder();
        // actionIndex 1 with pointerCount 2 triggers a 2-finger right-click tap (button 3).
        TrackpadContext context = new TrackpadContext(connection, 1, false, 100, 100, 100, null);
        context.setPointerCount(2);
        context.touchDownEvent(500, 500, 1000, true);
        context.touchUpEvent(500, 500, 1050);
        assertEquals(Collections.singletonList("down 3"), connection.buttons);

        // Pointer count drops before cancelTouch(), as on focus loss or gesture cancel.
        context.setPointerCount(0);
        context.cancelTouch();
        assertEquals(Arrays.asList("down 3", "up 3"), connection.buttons);

        // Advancing past CLICK_RELEASE_DELAY must not send a second button up.
        Shadows.shadowOf(Looper.getMainLooper()).idleFor(400, TimeUnit.MILLISECONDS);
        assertEquals(Arrays.asList("down 3", "up 3"), connection.buttons);

        // Starting a new 1-finger tap on actionIndex 0 before the old timer would have fired does not enter drag.
        TrackpadContext single = new TrackpadContext(connection, 0, false, 100, 100, 100, null);
        single.setPointerCount(1);
        single.touchDownEvent(500, 500, 1100, true);
        single.touchUpEvent(500, 500, 1140);
        assertEquals(Arrays.asList("down 3", "up 3", "down 1"), connection.buttons);
        single.cancelTouch();
        assertEquals(Arrays.asList("down 3", "up 3", "down 1", "up 1"), connection.buttons);
        single.setPointerCount(1);
        single.touchDownEvent(500, 500, 1200, true);
        single.touchMoveEvent(560, 500, 1220);
        single.touchUpEvent(560, 500, 1240);
        assertEquals(Arrays.asList("down 3", "up 3", "down 1", "up 1"), connection.buttons);
    }

    @Test
    public void doubleTapDragHoldsAndReleasesNormally() {
        Recorder connection = new Recorder();
        TrackpadContext context = new TrackpadContext(connection, 0, false, 100, 100, 100, null);
        context.setPointerCount(1);
        context.touchDownEvent(500, 500, 1000, true);
        context.touchUpEvent(500, 500, 1050);
        context.touchDownEvent(500, 500, 1100, true);
        context.touchMoveEvent(560, 500, 1120);
        assertEquals(Collections.singletonList("down 1"), connection.buttons);
        context.touchUpEvent(560, 500, 1200);
        assertEquals(Arrays.asList("down 1", "up 1"), connection.buttons);
    }

    @Test
    public void multiFingerDragReleasesOriginalButtonEvenWhenPointerCountChanges() {
        Recorder connection = new Recorder();
        TrackpadContext context = new TrackpadContext(connection, 1, false, 100, 100, 100, null);
        // Two-finger tap starts pending right-button click (button 3).
        context.setPointerCount(2);
        context.touchDownEvent(500, 500, 1000, true);
        context.touchUpEvent(500, 500, 1050);
        assertEquals(Collections.singletonList("down 3"), connection.buttons);

        // Second tap within CLICK_RELEASE_DELAY starts drag with activeDragButton = button 3.
        context.setPointerCount(1);
        context.touchDownEvent(500, 500, 1100, true);
        context.touchMoveEvent(560, 500, 1120);
        // Pointer count is now 1 (not 2); lifting or cancelling must still release button 3, never button 1.
        context.touchUpEvent(560, 500, 1200);
        assertEquals(Arrays.asList("down 3", "up 3"), connection.buttons);

        // Same when cancelled after pointerCount drops to 0.
        connection.buttons.clear();
        context.setPointerCount(2);
        context.touchDownEvent(500, 500, 2000, true);
        context.touchUpEvent(500, 500, 2050);
        context.touchDownEvent(500, 500, 2100, true);
        context.touchMoveEvent(560, 500, 2120);
        context.setPointerCount(0);
        context.cancelTouch();
        assertEquals(Arrays.asList("down 3", "up 3"), connection.buttons);
    }

    @Test
    public void aNegativeSensitivityFlipsThatAxisOnly() {
        int[] normal = drag(false, 100, 100, 100);
        int[] flipped = drag(false, -100, 100, 100);
        assertTrue(normal[0] > 0);
        assertEquals(-normal[0], flipped[0]);
        assertEquals(normal[1], flipped[1]);
    }

    @Test
    public void sensitivityAndMouseSpeedScaleMovement() {
        int[] normal = drag(false, 100, 100, 100);
        int[] half = drag(false, 50, 100, 100);
        int[] fast = drag(false, 100, 100, 200);
        assertTrue(Math.abs(half[0] - normal[0] / 2) <= 2);
        assertTrue(Math.abs(fast[0] - normal[0] * 2) <= 4);
    }

    @Test
    public void swappingTheAxesTurnsHorizontalMovementIntoVertical() {
        int[] normal = drag(false, 100, 100, 100);
        int[] swapped = drag(true, 100, 100, 100);
        assertEquals(normal[0], swapped[1]);
        assertEquals(normal[1], swapped[0]);
    }

    @Test
    public void twoFingerSwipeScrollsInsteadOfMoving() {
        Recorder connection = new Recorder();
        TrackpadContext first = new TrackpadContext(connection, 0, false, 100, 100, 100, null);
        TrackpadContext second = new TrackpadContext(connection, 1, false, 100, 100, 100, null);
        first.setPointerCount(1);
        first.touchDownEvent(500, 500, 1000, true);
        first.setPointerCount(2);
        second.setPointerCount(2);
        second.touchDownEvent(600, 500, 1005, true);
        for (int i = 1; i <= 6; i++) {
            first.touchMoveEvent(500, 500 + i * 30, 1005 + i * 10);
            second.touchMoveEvent(600, 500 + i * 30, 1005 + i * 10);
        }
        assertFalse(connection.scrolls.isEmpty());
        assertTrue(connection.moves.isEmpty());
    }

    @Test
    public void aTouchscreenScaleShrinksMovementButNotScrolling() {
        Recorder connection = new Recorder();
        TrackpadContext context = new TrackpadContext(connection, 0, false, 1920, 1080, new TrackpadContext.MoveScale() {
            @Override public float x() { return 0.5f; }
            @Override public float y() { return 0.5f; }
        });
        context.setPointerCount(1);
        context.touchDownEvent(500, 500, 1000, true);
        for (int i = 1; i <= 4; i++) {
            context.touchMoveEvent(500 + i * 10, 500, 1000 + i * 10);
        }
        int x = 0;
        for (int[] move : connection.moves) x += move[0];
        int[] unscaled = drag(false, 100, 100, 100);
        assertTrue(Math.abs(x - unscaled[0] / 2) <= 2);
    }

    @Test
    public void scrollingIsLinearSoFastSwipesDoNotScrollFarther() {
        int slow = scrollTotal(12, 10, 50);
        int fast = scrollTotal(4, 30, 50);
        assertTrue(slow != 0);
        assertTrue("slow " + slow + " fast " + fast, Math.abs(slow - fast) <= 2);
    }

    @Test
    public void scrollSpeedScalesScrolling() {
        int half = scrollTotal(6, 20, 50);
        int full = scrollTotal(6, 20, 100);
        assertTrue("half " + half + " full " + full, Math.abs(full - half * 2) <= 2);
    }

    @Test
    public void scrollingIsMeasuredInMillimetresNotInputUnits() {
        int normal = scrollTotal(6, 20, 100, 10f);
        int finer = scrollTotal(6, 20, 100, 20f);
        assertTrue(normal != 0);
        assertTrue("normal " + normal + " finer " + finer, Math.abs(normal - finer * 2) <= 2);
        // 200 units at 10 units per millimetre is 20 mm, which is 24 scroll units at 100% (1.2 per mm).
        assertTrue(Math.abs(scrollTotal(10, 20, 100, 10f) - 24) <= 2);
    }

    /** Two fingers move down {@code steps} times by {@code step} units each; returns the vertical scroll sent after the swipe starts. */
    private static int scrollTotal(int steps, int step, int speedPercent) {
        return scrollTotal(steps, step, speedPercent, 10f);
    }

    private static int scrollTotal(int steps, int step, int speedPercent, float unitsPerMm) {
        Recorder connection = new Recorder();
        TrackpadContext first = new TrackpadContext(connection, 0, false, 100, 100, 100, null);
        TrackpadContext second = new TrackpadContext(connection, 1, false, 100, 100, 100, null);
        first.setScrollSpeed(speedPercent);
        second.setScrollSpeed(speedPercent);
        first.setUnitsPerMm(unitsPerMm);
        second.setUnitsPerMm(unitsPerMm);
        first.setPointerCount(1);
        first.touchDownEvent(500, 500, 1000, true);
        first.setPointerCount(2);
        second.setPointerCount(2);
        second.touchDownEvent(600, 500, 1005, true);
        int y = 500;
        long time = 1005;
        // The first long movement only starts the scroll.
        y += 35;
        time += 100;
        first.touchMoveEvent(500, y, time);
        second.touchMoveEvent(600, y, time);
        connection.scrolls.clear();
        for (int i = 0; i < steps; i++) {
            y += step;
            time += 100;
            first.touchMoveEvent(500, y, time);
            second.touchMoveEvent(600, y, time);
        }
        int total = 0;
        for (short amount : connection.scrolls) {
            total += amount;
        }
        return total;
    }

    /** Like scrollTotal, for a touchscreen (the MoveScale constructor), which scrolls per pixel. */
    private static int touchscreenScrollTotal(int steps, int step, int speedPercent, float unitsPerMm) {
        Recorder connection = new Recorder();
        TrackpadContext.MoveScale unscaled = new TrackpadContext.MoveScale() {
            @Override public float x() { return 1; }
            @Override public float y() { return 1; }
        };
        TrackpadContext first = new TrackpadContext(connection, 0, false, 100, 100, unscaled);
        TrackpadContext second = new TrackpadContext(connection, 1, false, 100, 100, unscaled);
        first.setScrollSpeed(speedPercent);
        second.setScrollSpeed(speedPercent);
        first.setUnitsPerMm(unitsPerMm);
        second.setUnitsPerMm(unitsPerMm);
        first.setPointerCount(1);
        first.touchDownEvent(500, 500, 1000, true);
        first.setPointerCount(2);
        second.setPointerCount(2);
        second.touchDownEvent(600, 500, 1005, true);
        int y = 500;
        long time = 1005;
        // The first long movement only starts the scroll.
        y += 35;
        time += 100;
        first.touchMoveEvent(500, y, time);
        second.touchMoveEvent(600, y, time);
        connection.scrolls.clear();
        for (int i = 0; i < steps; i++) {
            y += step;
            time += 100;
            first.touchMoveEvent(500, y, time);
            second.touchMoveEvent(600, y, time);
        }
        int total = 0;
        for (short amount : connection.scrolls) {
            total += amount;
        }
        return total;
    }

    @Test
    public void touchscreenScrollsFiveUnitsPerPixelAtTheDefaultSpeed() {
        // 6 steps of 20 px is 120 px; the normal trackpad mode sends 5 units per pixel at 50%.
        int atDefault = touchscreenScrollTotal(6, 20, 50, 10f);
        assertTrue("got " + atDefault, Math.abs(atDefault - 120 * 5) <= 6);
        // Scroll speed doubles it, and the screen's density (units per millimetre) does not change it.
        assertTrue(Math.abs(touchscreenScrollTotal(6, 20, 100, 10f) - 2 * 120 * 5) <= 6);
        assertTrue(Math.abs(touchscreenScrollTotal(6, 20, 50, 30f) - atDefault) <= 6);
    }

    @Test
    public void twoFingerHoldAndThreeFingerHoldDoNotMiscountRightOrMiddleClicks() {
        Recorder connection = new Recorder();
        TrackpadContext first = new TrackpadContext(connection, 0, false, 100, 100, 100, null);
        // Two-finger stationary hold (> TAP_TIME_THRESHOLD) lifting primary first must not click right.
        first.setPointerCount(1);
        first.touchDownEvent(500, 500, 1000, true);
        first.setPointerCount(2);
        first.touchUpEvent(500, 500, 1400);
        first.setPointerCount(1);
        first.touchDownEvent(530, 500, 1400, false);
        assertTrue(connection.buttons.isEmpty());

        // Three-finger quick tap lifting primary first clicks middle once, then lifting second must not click right.
        connection.buttons.clear();
        first.setPointerCount(1);
        first.touchDownEvent(500, 500, 2000, true);
        first.setPointerCount(3);
        first.touchUpEvent(500, 500, 2060);
        first.setPointerCount(2);
        first.touchDownEvent(530, 500, 2060, false);
        first.touchUpEvent(530, 500, 2090);
        first.setPointerCount(1);
        first.touchDownEvent(560, 500, 2090, false);
        assertEquals(Arrays.asList("down 2", "up 2"), connection.buttons);

        // Two-finger quick tap lifting primary first fires one right click without duplicates.
        connection.buttons.clear();
        first.setPointerCount(1);
        first.touchDownEvent(500, 500, 3000, true);
        first.setPointerCount(2);
        first.touchUpEvent(500, 500, 3060);
        first.setPointerCount(1);
        first.touchDownEvent(530, 500, 3060, false);
        first.touchUpEvent(530, 500, 3100);
        assertEquals(Arrays.asList("down 3", "up 3"), connection.buttons);
    }

    /** Drags one finger 40 units right and returns what the host received, summed. */
    private static int[] drag(boolean swap, int sensitivityX, int sensitivityY, int speed) {
        Recorder connection = new Recorder();
        TrackpadContext context = new TrackpadContext(connection, 0, swap, sensitivityX, sensitivityY, speed, null);
        context.setPointerCount(1);
        context.touchDownEvent(500, 500, 1000, true);
        for (int i = 1; i <= 4; i++) {
            context.touchMoveEvent(500 + i * 10, 500, 1000 + i * 10);
        }
        int x = 0;
        int y = 0;
        for (int[] move : connection.moves) {
            x += move[0];
            y += move[1];
        }
        return new int[] {x, y};
    }

    private static class Recorder extends NvConnection {
        final List<int[]> moves = new ArrayList<>();
        final List<String> buttons = new ArrayList<>();
        final List<Short> scrolls = new ArrayList<>();

        Recorder() {
            super(null, new ComputerDetails.AddressTuple("127.0.0.1", 47989), 47984, "test",
                    new StreamConfiguration.Builder().build(), null, null);
        }

        @Override public boolean canSendInput() { return true; }
        @Override public void sendMouseMove(short x, short y) { moves.add(new int[] {x, y}); }
        @Override public void sendMouseButtonDown(byte button) { buttons.add("down " + button); }
        @Override public void sendMouseButtonUp(byte button) { buttons.add("up " + button); }
        @Override public void sendMouseHighResScroll(short amount) { scrolls.add(amount); }
    }
}
