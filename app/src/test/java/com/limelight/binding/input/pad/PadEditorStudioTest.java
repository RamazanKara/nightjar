package com.limelight.binding.input.pad;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;
import static org.robolectric.Shadows.shadowOf;

import android.app.Application;
import android.view.HapticFeedbackConstants;
import android.view.MotionEvent;
import android.view.View;

import com.limelight.ui.Haptics;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import java.lang.reflect.Method;

/**
 * Precise editing in the layout editor: a dragged control lands on the screen's centre lines and its twin's mirror
 * and shows the guide it holds, a drag straight along one axis leaves the other alone, the mirror lock carries a
 * control's twin, and Undo takes changes back one at a time.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 30, application = Application.class, qualifiers = "w800dp-h360dp-land-xhdpi")
public class PadEditorStudioTest extends ModernControllerHarness {
    private ModernController controller;
    private PadEditor editor;
    private View surface;
    private float density;

    private void open() {
        controller = controller(GAME);
        controller.startEditing();
        settle();
        editor = (PadEditor) frame.getChildAt(frame.getChildCount() - 1);
        surface = editor.getChildAt(0);
        density = frame.getResources().getDisplayMetrics().density;
    }

    /** Leaves only these controls on the surface, so nothing else offers a line. */
    private void only(PadLayout.Control... kept) {
        for (PadLayout.Control control : PadLayout.Control.values()) editor.draft().spec(control).hidden = true;
        for (PadLayout.Control control : kept) editor.draft().spec(control).hidden = false;
        editor.changed();
    }

    private float[] box(PadLayout.Control control) {
        return editor.draft().bounds(control, surface.getWidth(), surface.getHeight(), density, new float[4]);
    }

    private float centreX(PadLayout.Control control) {
        float[] box = box(control);
        return box[0] + box[2] / 2;
    }

    private float centreY(PadLayout.Control control) {
        float[] box = box(control);
        return box[1] + box[3] / 2;
    }

    private void touch(int action, float x, float y) {
        long now = android.os.SystemClock.uptimeMillis();
        MotionEvent event = MotionEvent.obtain(now, now, action, x, y, 0);
        surface.dispatchTouchEvent(event);
        event.recycle();
    }

    /** Takes hold of {@code control} by its centre and moves the finger there to ({@code x}, {@code y}), still down. */
    private void hold(PadLayout.Control control, float x, float y) {
        touch(MotionEvent.ACTION_DOWN, centreX(control), centreY(control));
        touch(MotionEvent.ACTION_MOVE, x, y);
    }

    @Test public void aDraggedControlLandsOnTheScreensCentreLinesAndShowsTheGuidesItHolds() {
        open();
        only(PadLayout.Control.LB);
        float midX = surface.getWidth() / 2f, midY = surface.getHeight() / 2f;
        hold(PadLayout.Control.LB, midX + 9, midY - 7);
        assertEquals(midX, centreX(PadLayout.Control.LB), .5f);
        assertEquals(midY, centreY(PadLayout.Control.LB), .5f);
        assertEquals("One guide for each line it holds", 2, editor.guides());
        // Pulled away from the upright line, that guide goes and the grid has the control.
        touch(MotionEvent.ACTION_MOVE, midX + 40, midY - 7);
        assertEquals(1, editor.guides());
        float grid = PadEditor.GRID_DP * density, x = centreX(PadLayout.Control.LB);
        assertEquals("On a grid line", Math.round(x / grid) * grid, x, .01f);
        assertTrue(x > midX + 30);
        touch(MotionEvent.ACTION_UP, midX + 40, midY - 7);
        assertEquals("Let go, the guides go", 0, editor.guides());
        controller.finishEditing(false);
    }

    @Test public void aDraggedControlLandsOnItsTwinsMirror() {
        open();
        only(PadLayout.Control.LB, PadLayout.Control.RB);
        float width = surface.getWidth();
        drag(surface, centreX(PadLayout.Control.RB), centreY(PadLayout.Control.RB), 1200, 400);
        float twinX = centreX(PadLayout.Control.RB), twinY = centreY(PadLayout.Control.RB);
        assertEquals(1200, twinX, .5f);
        hold(PadLayout.Control.LB, width - twinX + 5, twinY - 4);
        assertEquals("As far from the middle as its twin", width - twinX, centreX(PadLayout.Control.LB), .5f);
        assertEquals("and level with it", twinY, centreY(PadLayout.Control.LB), .5f);
        assertEquals(2, editor.guides());
        touch(MotionEvent.ACTION_UP, width - twinX + 5, twinY - 4);
        controller.finishEditing(false);
    }

    @Test public void aDragStraightAlongOneAxisLeavesTheOtherAlone() {
        open();
        // View stands on no grid line: dragged straight down, with the tremble of a finger, it moves not a pixel
        // sideways.
        float x = centreX(PadLayout.Control.SELECT), y = centreY(PadLayout.Control.SELECT);
        float grid = PadEditor.GRID_DP * density;
        assertTrue(Math.abs(x - Math.round(x / grid) * grid) > 1);
        touch(MotionEvent.ACTION_DOWN, x, y);
        touch(MotionEvent.ACTION_MOVE, x + 3, y + 150);
        assertEquals(x, centreX(PadLayout.Control.SELECT), .01f);
        assertTrue(centreY(PadLayout.Control.SELECT) > y + 100);
        assertTrue("No upright guide for an axis that stays", editor.guides() <= 1);
        touch(MotionEvent.ACTION_UP, x + 3, y + 150);
        // A tap that trembles moves nothing at all.
        PadLayout.Spec spec = editor.draft().spec(PadLayout.Control.START);
        touch(MotionEvent.ACTION_DOWN, centreX(PadLayout.Control.START), centreY(PadLayout.Control.START));
        touch(MotionEvent.ACTION_MOVE, centreX(PadLayout.Control.START) + 4, centreY(PadLayout.Control.START) - 5);
        touch(MotionEvent.ACTION_UP, centreX(PadLayout.Control.START) + 4, centreY(PadLayout.Control.START) - 5);
        assertEquals(PadLayout.Control.START, editor.selected());
        assertFalse("Still where a layout nobody edited has it", spec.placed());
        controller.finishEditing(false);
    }

    @Test public void theMirrorLockMovesAControlsTwinWithIt() {
        open();
        float width = surface.getWidth();
        // Without the lock a twin stays where it is.
        float triggerX = centreX(PadLayout.Control.RT), triggerY = centreY(PadLayout.Control.RT);
        drag(surface, centreX(PadLayout.Control.LT), centreY(PadLayout.Control.LT), 500, 300);
        assertEquals(triggerX, centreX(PadLayout.Control.RT), .01f);
        assertEquals(triggerY, centreY(PadLayout.Control.RT), .01f);

        assertFalse(editor.mirror());
        editor.setMirror(true);
        // The shoulders mirror each other already: they stay mirrored to the pixel, wherever one goes.
        drag(surface, centreX(PadLayout.Control.LB), centreY(PadLayout.Control.LB), 420, 520);
        assertTrue("It moved", centreX(PadLayout.Control.LB) > 300);
        assertEquals(width - centreX(PadLayout.Control.LB), centreX(PadLayout.Control.RB), .5f);
        assertEquals(centreY(PadLayout.Control.LB), centreY(PadLayout.Control.RB), .5f);
        // Sized, by a corner handle or the buttons, the twin takes the same size.
        editor.select(PadLayout.Control.LB);
        editor.scaleBy(.3f);
        assertEquals(1.3f, editor.draft().spec(PadLayout.Control.RB).scale(), 1e-3);
        assertEquals(width - centreX(PadLayout.Control.LB), centreX(PadLayout.Control.RB), .5f);

        // The sticks do not mirror each other in the default layout: the right one keeps its own place beside the
        // mirror and moves as far, the other way.
        float leftX = centreX(PadLayout.Control.LEFT_STICK), leftY = centreY(PadLayout.Control.LEFT_STICK);
        float rightX = centreX(PadLayout.Control.RIGHT_STICK), rightY = centreY(PadLayout.Control.RIGHT_STICK);
        drag(surface, leftX, leftY, leftX + 80, leftY - 48);
        float movedX = centreX(PadLayout.Control.LEFT_STICK) - leftX, movedY = centreY(PadLayout.Control.LEFT_STICK) - leftY;
        assertTrue(movedX > 60 && movedY < -30);
        assertEquals(rightX - movedX, centreX(PadLayout.Control.RIGHT_STICK), .5f);
        assertEquals(rightY + movedY, centreY(PadLayout.Control.RIGHT_STICK), .5f);

        // One Undo takes the control and its twin back together.
        editor.undo();
        assertEquals(leftX, centreX(PadLayout.Control.LEFT_STICK), .5f);
        assertEquals(rightX, centreX(PadLayout.Control.RIGHT_STICK), .5f);
        assertEquals(rightY, centreY(PadLayout.Control.RIGHT_STICK), .5f);
        controller.finishEditing(false);
    }

    @Test public void undoTakesChangesBackOneAtATime() {
        open();
        assertFalse(editor.canUndo());
        float homeY = centreY(PadLayout.Control.HOME);
        drag(surface, centreX(PadLayout.Control.HOME), homeY, centreX(PadLayout.Control.HOME), homeY + 200);
        assertTrue(editor.canUndo());
        editor.select(PadLayout.Control.DPAD);
        editor.toggleHidden();
        editor.select(PadLayout.Control.LB);
        // A slider's run is one step, however many values it passed.
        editor.setOpacity(60);
        editor.setOpacity(45);
        editor.setOpacity(30);
        // Only a cluster splits: nothing changed, so there is nothing to take back.
        editor.split();

        editor.undo();
        assertEquals(100, editor.draft().spec(PadLayout.Control.LB).opacity);
        assertTrue(editor.draft().spec(PadLayout.Control.DPAD).hidden);
        editor.undo();
        assertFalse(editor.draft().spec(PadLayout.Control.DPAD).hidden);
        assertTrue(centreY(PadLayout.Control.HOME) > homeY + 100);
        editor.undo();
        assertEquals(homeY, centreY(PadLayout.Control.HOME), .5f);
        assertTrue("All taken back, it is the layout the editor opened with", editor.draft().isDefault());
        assertFalse(editor.canUndo());
        editor.undo();
        assertTrue(editor.draft().isDefault());

        // A split is one step too, and taking it back picks nothing that left the surface.
        editor.select(PadLayout.Control.FACE);
        editor.split();
        assertEquals(PadLayout.members(PadLayout.Control.FACE)[0], editor.selected());
        editor.undo();
        assertFalse(editor.draft().isSplit(PadLayout.Control.FACE));
        assertNull(editor.selected());
        // Reset can be taken back as well.
        editor.select(PadLayout.Control.RT);
        editor.scaleBy(.2f);
        editor.resetDraft();
        assertTrue(editor.draft().isDefault());
        editor.undo();
        assertEquals(1.2f, editor.draft().spec(PadLayout.Control.RT).scale(), 1e-3);
        controller.finishEditing(false);
    }

    @Test public void takingHoldOfALineIsOneLightTick() throws Exception {
        // Whatever ticked before in this process is forgotten: the interface's limiter is the process's.
        Method forget = Haptics.class.getDeclaredMethod("resetForTests");
        forget.setAccessible(true);
        forget.invoke(null);
        open();
        only(PadLayout.Control.LB);
        float midX = surface.getWidth() / 2f;
        hold(PadLayout.Control.LB, 400, 500);
        assertEquals("The grid is silent", -1, shadowOf(surface).lastHapticFeedbackPerformed());
        touch(MotionEvent.ACTION_MOVE, midX + 6, 500);
        assertEquals(1, editor.guides());
        assertEquals(HapticFeedbackConstants.CLOCK_TICK, shadowOf(surface).lastHapticFeedbackPerformed());
        touch(MotionEvent.ACTION_UP, midX + 6, 500);
        controller.finishEditing(false);
    }
}
