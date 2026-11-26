package com.limelight.ui;

import android.app.Activity;
import android.app.Application;
import android.os.SystemClock;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;

import androidx.core.view.ViewCompat;

import com.google.android.material.button.MaterialButton;
import com.limelight.R;
import com.limelight.nvstream.http.NvApp;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static com.limelight.ui.ControlCenterTest.*;
import static com.limelight.ui.StreamMenu.Item.*;
import static com.limelight.ui.StreamMenuTestSupport.*;
import static org.junit.Assert.*;

/**
 * Steering the Control Center. The triggers step through the modules that have something and stop at both ends, one
 * step a pull however the pad says it, while Y, X, LB and RB stay the user's own buttons; Guide and the menu chord close
 * it; a remote reaches the rail, steps along it and enters the page with its arrows and OK alone; it opens again on the
 * module and card it was left on, never on a way out; and on an upright phone the sheet follows a finger down, takes
 * what its page cannot scroll, and a swipe across the page steps.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class, qualifiers = ControlCenterTest.HANDHELD)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
public class ControlCenterNavigationTest {
    @Before
    public void aDeviceThatDoesNotBlur() {
        quietDevice();
    }

    @After
    public void forget() {
        forgetDevice();
    }

    @Test
    public void theTriggersStepThroughTheModulesThatHaveSomethingAndStopAtBothEnds() throws Exception {
        StreamMenu.gamepadForTest = true;
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = open(activity, everything(NvApp.Role.STREAM), chosen, null);
            ControlCenterDock dock = menu.sheet();
            List<String> modules = menu.modules();
            assertEquals(Arrays.asList(ControlCenterModel.QUICK, ControlCenterModel.PICTURE, ControlCenterModel.AUDIO,
                    ControlCenterModel.PC, ControlCenterModel.CONTROLLER, ControlCenterModel.GUESTS, ControlCenterModel.POWER), modules);
            assertEquals("the rail is the model's", menu.dock().modules(), modules);
            assertEquals(ControlCenterModel.QUICK, menu.module());
            // The rail says what the triggers do, and where there is nothing further.
            assertEquals(View.VISIBLE, dock.hint(false).getVisibility());
            assertEquals(View.VISIBLE, dock.hint(true).getVisibility());
            assertEquals("LT", dock.hint(false).getText().toString());
            assertEquals("RT", dock.hint(true).getText().toString());
            assertTrue("nothing before Quick", dock.hint(false).getAlpha() < 1f);
            assertEquals(1f, dock.hint(true).getAlpha(), 0f);

            press(menu, KeyEvent.KEYCODE_BUTTON_L2);
            assertEquals("it stops at the first", ControlCenterModel.QUICK, menu.module());
            for (int at = 1; at < modules.size(); at++) {
                press(menu, KeyEvent.KEYCODE_BUTTON_R2);
                assertEquals(modules.get(at), menu.module());
                assertEquals("as the policy steps", menu.dock().step(modules.get(at - 1), true), menu.module());
                assertTrue(dock.railEntry(modules.get(at)).isSelected());
                assertFalse(dock.railEntry(modules.get(at - 1)).isSelected());
                assertFalse(modules.get(at) + " has something to choose", menu.entries().isEmpty());
            }
            assertEquals("Clips has nothing yet, so Audio leads to the PC", ControlCenterModel.PC,
                    menu.dock().step(ControlCenterModel.AUDIO, true));
            press(menu, KeyEvent.KEYCODE_BUTTON_R2);
            assertEquals("it stops at the last", ControlCenterModel.POWER, menu.module());
            assertTrue("nothing after Power", dock.hint(true).getAlpha() < 1f);
            assertEquals(1f, dock.hint(false).getAlpha(), 0f);

            // A held trigger is one step: its repeats step nothing.
            dock.dispatchGenericMotionEvent(triggers(0f, 0f));
            long now = SystemClock.uptimeMillis();
            dock.dispatchKeyEvent(new KeyEvent(now, now, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_L2, 0));
            dock.dispatchKeyEvent(new KeyEvent(now, now + 400, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_L2, 1));
            dock.dispatchKeyEvent(new KeyEvent(now, now + 450, KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_BUTTON_L2, 2));
            assertEquals(ControlCenterModel.GUESTS, menu.module());
            // A pad that says the same pull as an axis too still takes one step.
            dock.dispatchGenericMotionEvent(triggers(1f, 0f));
            assertEquals(ControlCenterModel.GUESTS, menu.module());
            dock.dispatchKeyEvent(new KeyEvent(now, now + 500, KeyEvent.ACTION_UP, KeyEvent.KEYCODE_BUTTON_L2, 0));
            dock.dispatchGenericMotionEvent(triggers(0f, 0f));
            // The axis alone, as most pads say it.
            dock.dispatchGenericMotionEvent(triggers(1f, 0f));
            assertEquals(ControlCenterModel.CONTROLLER, menu.module());
            dock.dispatchGenericMotionEvent(triggers(1f, 0f));
            assertEquals("held, it steps once", ControlCenterModel.CONTROLLER, menu.module());
            dock.dispatchGenericMotionEvent(triggers(0f, 0f));
            dock.dispatchGenericMotionEvent(triggers(0f, 1f));
            assertEquals(ControlCenterModel.GUESTS, menu.module());
            dock.dispatchGenericMotionEvent(triggers(0f, 0f));
            assertTrue("a step chooses nothing", chosen.isEmpty());

            // Y, X, LB and RB stay the user's own: Y runs its card on whichever module it is, which then shows.
            press(menu, KeyEvent.KEYCODE_BUTTON_Y);
            assertEquals(Arrays.asList(KEYBOARD), chosen);
            assertEquals(ControlCenterModel.QUICK, menu.module());
            chosen.clear();
            press(menu, KeyEvent.KEYCODE_BUTTON_R1);
            assertEquals("RB is no step", Arrays.asList(OVERLAY), chosen);
            assertEquals(ControlCenterModel.QUICK, menu.module());

            // Guide: the release of the hold that opened it closes nothing; a press of its own does.
            dock.dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_BUTTON_MODE));
            assertTrue(dock.isShowing());
            press(menu, KeyEvent.KEYCODE_BUTTON_MODE);
            settle();
            assertFalse(dock.isShowing());
        }
    }

    /** A joystick's move with the two triggers at these values. */
    private static MotionEvent triggers(float left, float right) {
        MotionEvent.PointerProperties[] pointer = {new MotionEvent.PointerProperties()};
        pointer[0].id = 0;
        MotionEvent.PointerCoords[] axes = {new MotionEvent.PointerCoords()};
        axes[0].setAxisValue(MotionEvent.AXIS_LTRIGGER, left);
        axes[0].setAxisValue(MotionEvent.AXIS_RTRIGGER, right);
        long now = SystemClock.uptimeMillis();
        return MotionEvent.obtain(now, now, MotionEvent.ACTION_MOVE, 1, pointer, axes, 0, 0, 1f, 1f, 0, 0,
                InputDevice.SOURCE_JOYSTICK, 0);
    }

    @Test
    public void aTriggerHeldWhileItOpensStepsNothingAndTheMenuChordClosesIt() throws Exception {
        StreamMenu.gamepadForTest = true;
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = open(activity, everything(NvApp.Role.STREAM), chosen, null);
            ControlCenterDock dock = menu.sheet();
            dock.dispatchGenericMotionEvent(triggers(0f, 1f));
            assertEquals("the first reading only says where the trigger is", ControlCenterModel.QUICK, menu.module());
            dock.dispatchGenericMotionEvent(triggers(0f, 0f));
            dock.dispatchGenericMotionEvent(triggers(0f, 1f));
            assertEquals(ControlCenterModel.PICTURE, menu.module());

            // The chord that opens the menu closes it once every button of it is let go; its LB and RB run no card.
            int[] chord = {KeyEvent.KEYCODE_BUTTON_START, KeyEvent.KEYCODE_BUTTON_SELECT, KeyEvent.KEYCODE_BUTTON_L1,
                    KeyEvent.KEYCODE_BUTTON_R1};
            for (int key : chord) dock.dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_DOWN, key));
            assertTrue(dock.isShowing());
            assertEquals(StreamMenu.Page.MAIN, menu.page());
            for (int i = 0; i < chord.length - 1; i++) dock.dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, chord[i]));
            assertTrue("until the last of them is up", dock.isShowing());
            dock.dispatchKeyEvent(new KeyEvent(KeyEvent.ACTION_UP, chord[chord.length - 1]));
            assertFalse(dock.isShowing());
            assertTrue(chosen.isEmpty());
        }
    }

    @Test
    public void aModuleWithNothingIsLeftOffTheRailAndSteppedOver() throws Exception {
        StreamMenu.gamepadForTest = true;
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            // Nobody to play with and nothing recorded: Guests and Clips have nothing.
            StreamMenu menu = open(activity, options("rubylight-serverinfo.xml", NvApp.Role.STREAM), new ArrayList<>(), null);
            List<String> modules = menu.modules();
            assertFalse(modules.contains(ControlCenterModel.GUESTS));
            assertFalse(modules.contains(ControlCenterModel.CLIPS));
            assertNull(menu.sheet().railEntry(ControlCenterModel.GUESTS));
            assertEquals(modules.size(), ((ViewGroup) menu.sheet().railEntry(modules.get(0)).getParent()).getChildCount());
            menu.select(ControlCenterModel.GUESTS);
            assertEquals("a module the rail does not show cannot be chosen", ControlCenterModel.QUICK, menu.module());
            for (int at = 1; at < modules.size(); at++) {
                press(menu, KeyEvent.KEYCODE_BUTTON_R2);
                assertEquals(modules.get(at), menu.module());
                assertFalse(modules.get(at) + " has something", menu.entries().isEmpty());
            }
            assertEquals("past Guests, from the controller to the ways to end", modules.indexOf(ControlCenterModel.CONTROLLER) + 1,
                    modules.indexOf(ControlCenterModel.POWER));
            assertEquals(ControlCenterModel.POWER, menu.module());
            menu.sheet().dismiss();

            // A viewer who cannot type has nothing on Quick: the rail starts at the first module that has something.
            StreamMenu viewer = open(activity, options("butterpollo-viewer-serverinfo.xml", NvApp.Role.INPUT_ONLY),
                    new ArrayList<>(), null);
            List<String> shown = viewer.modules();
            assertFalse(shown.toString(), shown.isEmpty() || shown.contains(ControlCenterModel.QUICK));
            assertEquals(shown.get(0), viewer.module());
            assertFalse(viewer.entries().isEmpty());
            press(viewer, KeyEvent.KEYCODE_BUTTON_L2);
            assertEquals("back from the first shown module stays", shown.get(0), viewer.module());
            for (int at = 1; at < shown.size(); at++) {
                press(viewer, KeyEvent.KEYCODE_BUTTON_R2);
                assertEquals(shown.get(at), viewer.module());
            }
            press(viewer, KeyEvent.KEYCODE_BUTTON_R2);
            assertEquals(shown.get(shown.size() - 1), viewer.module());
            viewer.sheet().dismiss();
        }
    }

    @Test @Config(qualifiers = TELEVISION)
    public void aRemoteReachesTheRailStepsAlongItAndEntersThePage() throws Exception {
        StreamMenu.gamepadForTest = false;
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = open(activity, everything(NvApp.Role.STREAM), chosen, null);
            ControlCenterDock dock = menu.sheet();
            byKeys(dock.panel());
            layOut(activity, menu);
            List<String> modules = menu.modules();
            assertEquals("a remote has no triggers to show", View.GONE, dock.hint(true).getVisibility());
            assertEquals(View.GONE, dock.hint(false).getVisibility());
            dock.focusFirstAction();
            View card = dock.getCurrentFocus();
            assertSame(menu.grid().getChildAt(0), card);
            MaterialButton entry = dock.railEntry(modules.get(0));
            assertSame("Left from the first column is the rail", entry, card.focusSearch(View.FOCUS_LEFT));
            assertTrue(entry.requestFocus());
            assertTrue(dock.railFocused());

            // Along the rail the arrows step, and the focus stays on the rail, on the module that shows.
            press(menu, KeyEvent.KEYCODE_DPAD_UP);
            assertEquals("it stops at the first", modules.get(0), menu.module());
            press(menu, KeyEvent.KEYCODE_DPAD_DOWN);
            assertEquals(modules.get(1), menu.module());
            assertSame(dock.railEntry(modules.get(1)), dock.getCurrentFocus());
            assertFalse("only the lit module is a stop for the D-pad", dock.railEntry(modules.get(0)).isFocusable());
            press(menu, KeyEvent.KEYCODE_DPAD_DOWN);
            assertEquals(modules.get(2), menu.module());
            press(menu, KeyEvent.KEYCODE_DPAD_UP);
            assertEquals(modules.get(1), menu.module());
            assertTrue(dock.railFocused());
            // The arrow toward the page enters it, on its first card or row; so does OK.
            press(menu, KeyEvent.KEYCODE_DPAD_RIGHT);
            assertFalse(dock.railFocused());
            assertNotNull(dock.page().firstAction());
            assertSame(dock.page().firstAction(), dock.getCurrentFocus());
            assertTrue(dock.railEntry(modules.get(1)).requestFocus());
            press(menu, KeyEvent.KEYCODE_DPAD_CENTER);
            assertSame(dock.page().firstAction(), dock.getCurrentFocus());
            assertEquals(modules.get(1), menu.module());
            assertTrue("moving about chooses nothing", chosen.isEmpty());

            // Back leaves a page one level down, then the Control Center.
            menu.reveal(OVERLAY_PAGE).performClick();
            assertEquals(StreamMenu.Page.OVERLAY, menu.page());
            dock.onBackPressed();
            assertEquals(StreamMenu.Page.MAIN, menu.page());
            assertTrue(dock.isShowing());
            dock.onBackPressed();
            settle();
            assertFalse(dock.isShowing());
            assertTrue(chosen.isEmpty());
        }
    }

    @Test
    public void reopenedInTheSameStreamItShowsTheModuleAndTheCardItWasLeftOnButNeverAWayOut() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            // A pad or a remote opened it: the stream's own window is out of touch mode.
            byKeys(activity.getWindow().getDecorView());
            StreamMenu.Options state = everything(NvApp.Role.STREAM);
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu.Session session = new StreamMenu.Session();
            StreamMenu menu = open(activity, state, chosen, session);
            assertEquals("a stream's first opening is on Quick", ControlCenterModel.QUICK, menu.module());
            byKeys(menu.sheet().panel());
            layOut(activity, menu);
            MaterialButton left = menu.reveal(HOST_STATUS);
            assertEquals(ControlCenterModel.PC, menu.module());
            assertNotSame("not the row a page starts on anyway", menu.sheet().page().firstAction(), left);
            assertTrue(left.requestFocus());
            menu.sheet().dismiss();
            settle();
            assertEquals(ControlCenterModel.PC, session.module);
            assertEquals(HOST_STATUS, session.focused);

            StreamMenu again = open(activity, state, chosen, session);
            assertEquals(ControlCenterModel.PC, again.module());
            assertTrue(again.sheet().railEntry(ControlCenterModel.PC).isSelected());
            again.sheet().onWindowFocusChanged(true);
            assertSame("the focus is back where it was", again.row(HOST_STATUS), again.sheet().getCurrentFocus());
            // Left on a way out, it never opens there: OK right after opening must not disconnect.
            byKeys(again.sheet().panel());
            assertTrue(again.row(DISCONNECT).requestFocus());
            again.sheet().onBackPressed();
            settle();
            assertEquals(ControlCenterModel.PC, session.module);
            assertNull(session.focused);
            StreamMenu third = open(activity, state, chosen, session);
            third.sheet().onWindowFocusChanged(true);
            assertEquals(ControlCenterModel.PC, third.module());
            assertNotNull(third.sheet().page().firstAction());
            assertSame(third.sheet().page().firstAction(), third.sheet().getCurrentFocus());
            third.sheet().dismiss();
            settle();

            // Another stream starts on Quick again.
            StreamMenu fresh = open(activity, state, chosen, new StreamMenu.Session());
            assertEquals(ControlCenterModel.QUICK, fresh.module());
            fresh.sheet().dismiss();
            assertTrue(chosen.isEmpty());
        }
    }

    @Test @Config(qualifiers = PHONE_UPRIGHT)
    public void theSheetFollowsAFingerDownAndClosesPastItsThreshold() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            StreamMenu menu = open(activity, everything(NvApp.Role.STREAM), new ArrayList<>(), null);
            layOut(activity, menu);
            ControlCenterDock dock = menu.sheet();
            ControlCenterFrame frame = dock.frame();
            View panel = dock.panel(), scroll = dock.page().scroll();
            int dismiss = dock.getContext().getResources().getDimensionPixelSize(R.dimen.control_center_sheet_dismiss);
            // A short pull follows the finger and returns.
            frame.dragBy(dismiss / 2f);
            assertEquals(dismiss / 2f, frame.offset(), 0.5f);
            assertEquals(frame.offset(), panel.getTranslationY(), 0.5f);
            frame.dragBy(-dismiss * 2f);
            assertEquals("never above where it rests", 0f, frame.offset(), 0f);
            frame.dragBy(dismiss / 2f);
            frame.release(0f);
            settle();
            assertEquals(0f, frame.offset(), 0f);
            assertEquals(0f, panel.getTranslationY(), 0f);
            assertTrue(dock.isShowing());

            // The page hands over what it cannot scroll: at its top, a finger still pulling down moves the sheet.
            assertTrue(frame.onStartNestedScroll(scroll, scroll, ViewCompat.SCROLL_AXIS_VERTICAL, ViewCompat.TYPE_TOUCH));
            assertFalse("a fling's own scroll moves nothing",
                    frame.onStartNestedScroll(scroll, scroll, ViewCompat.SCROLL_AXIS_VERTICAL, ViewCompat.TYPE_NON_TOUCH));
            frame.onNestedScrollAccepted(scroll, scroll, ViewCompat.SCROLL_AXIS_VERTICAL, ViewCompat.TYPE_TOUCH);
            frame.onNestedScroll(scroll, 0, 30, 0, 0, ViewCompat.TYPE_TOUCH);
            assertEquals("while the page scrolls the sheet stays", 0f, frame.offset(), 0f);
            frame.onNestedScroll(scroll, 0, 0, 0, -60, ViewCompat.TYPE_TOUCH);
            assertEquals(60f, frame.offset(), 0f);
            int[] consumed = new int[2];
            frame.onNestedPreScroll(scroll, 0, 25, consumed, ViewCompat.TYPE_TOUCH);
            assertEquals("on its way back up the sheet takes the move first", 25, consumed[1]);
            assertEquals(35f, frame.offset(), 0f);
            frame.onStopNestedScroll(scroll, ViewCompat.TYPE_TOUCH);
            settle();
            assertEquals(0f, frame.offset(), 0f);
            assertTrue(dock.isShowing());

            // Past the threshold, letting go closes it.
            frame.dragBy(dismiss + 10f);
            frame.release(0f);
            settle();
            assertFalse(dock.isShowing());
            assertNull("and nothing of it stays", dock.surface().kind());

            // A fling down closes it from anywhere.
            StreamMenu flung = open(activity, everything(NvApp.Role.STREAM), new ArrayList<>(), null);
            layOut(activity, flung);
            flung.sheet().frame().dragBy(4f);
            flung.sheet().frame().release(100000f);
            settle();
            assertFalse(flung.sheet().isShowing());
        }
    }

    private static long clock = 1000;

    private static void touch(View frame, int action, float x, float y) {
        clock += 16;
        MotionEvent event = MotionEvent.obtain(1000, clock, action, x, y, 0);
        frame.dispatchTouchEvent(event);
        event.recycle();
    }

    private static void swipe(View frame, float fromX, float toX, float y) {
        touch(frame, MotionEvent.ACTION_DOWN, fromX, y);
        for (int step = 1; step <= 4; step++) touch(frame, MotionEvent.ACTION_MOVE, fromX + (toX - fromX) * step / 4f, y);
        touch(frame, MotionEvent.ACTION_UP, toX, y);
    }

    @Test @Config(qualifiers = PHONE_UPRIGHT)
    public void aSwipeAcrossThePageStepsAndATapBesideTheSheetClosesIt() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu.Options state = everything(NvApp.Role.STREAM);
            // A PC that changes the stream's rate in place: Picture carries the bitrate's track.
            state.rateChange = true;
            StreamMenu menu = open(activity, state, chosen, null);
            layOut(activity, menu);
            ControlCenterDock dock = menu.sheet();
            ControlCenterFrame frame = dock.frame();
            View panel = dock.panel(), scroll = dock.page().scroll();
            List<String> modules = menu.modules();
            float width = frame.getWidth();
            // Across the top of the page: its cards, never the bitrate's track further down.
            float y = inWindow(scroll)[1] - inWindow(frame)[1] + 6f;
            swipe(frame, width * 0.2f, width * 0.8f, y);
            assertEquals("nothing before the first", modules.get(0), menu.module());
            swipe(frame, width * 0.8f, width * 0.2f, y);
            layOut(activity, menu);
            assertEquals("toward the start is the next module", modules.get(1), menu.module());
            // A finger that starts on a slider moves the slider, never the page.
            assertEquals(ControlCenterModel.PICTURE, menu.module());
            View track = menu.bitrateSlider();
            assertNotNull(track);
            assertTrue(track.isShown());
            // Under the cards, so the page is scrolled to it: into its lower half, clear of the row the swipes run along.
            int inPage = inWindow(track)[1] - inWindow(scroll)[1] + scroll.getScrollY();
            scroll.scrollTo(0, Math.max(0, inPage - scroll.getHeight() / 2));
            float left = inWindow(track)[0] - inWindow(frame)[0];
            float middle = inWindow(track)[1] - inWindow(frame)[1] + track.getHeight() / 2f;
            assertTrue("in sight", middle > y + track.getHeight() / 2f
                    && middle < inWindow(scroll)[1] - inWindow(frame)[1] + scroll.getHeight());
            swipe(frame, left + track.getWidth() * 0.8f, left + track.getWidth() * 0.2f, middle);
            assertEquals(ControlCenterModel.PICTURE, menu.module());
            swipe(frame, width * 0.8f, width * 0.2f, y);
            layOut(activity, menu);
            assertEquals(modules.get(2), menu.module());
            swipe(frame, width * 0.2f, width * 0.8f, y);
            layOut(activity, menu);
            assertEquals(modules.get(1), menu.module());
            assertTrue("a swipe chooses nothing", chosen.isEmpty());
            assertTrue(dock.isShowing());
            assertEquals(0f, frame.offset(), 0f);

            // A tap on the sheet's own top edge is the sheet's; one on the picture beside it closes.
            touch(frame, MotionEvent.ACTION_DOWN, width / 2f, panel.getTop() + 4f);
            touch(frame, MotionEvent.ACTION_UP, width / 2f, panel.getTop() + 4f);
            assertTrue(dock.isShowing());
            touch(frame, MotionEvent.ACTION_DOWN, width / 2f, panel.getTop() / 2f);
            touch(frame, MotionEvent.ACTION_UP, width / 2f, panel.getTop() / 2f);
            settle();
            assertFalse(dock.isShowing());
            assertTrue(chosen.isEmpty());
        }
    }
}
