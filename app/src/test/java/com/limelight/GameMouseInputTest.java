package com.limelight;

import android.app.Application;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;

import com.limelight.binding.input.capture.InputCaptureProvider;
import com.limelight.nvstream.NvConnection;
import com.limelight.nvstream.StreamConfiguration;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.input.MouseButtonPacket;
import com.limelight.preferences.PreferenceConfiguration;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import org.robolectric.util.ReflectionHelpers;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class,
        shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class GameMouseInputTest {
    @Test
    public void syntheticRightClicksStayLocalWhenInputIsReleased() {
        for (int source : new int[] {InputDevice.SOURCE_MOUSE, InputDevice.SOURCE_MOUSE_RELATIVE}) {
            RecordingConnection connection = new RecordingConnection();
            Game game = game(connection);
            ReflectionHelpers.setField(game, "grabbedInput", false);

            click(game, source);

            assertTrue("Uncaptured right-click reached the PC", connection.rightButton.isEmpty());
            assertFalse("Synthetic Back must not exit the stream", game.isFinishing());
        }
    }

    @Test
    public void releasingCaptureReleasesHeldRightClickAndIgnoresLateMouseKeys() {
        for (int source : new int[] {InputDevice.SOURCE_MOUSE, InputDevice.SOURCE_MOUSE_RELATIVE}) {
            RecordingConnection connection = new RecordingConnection();
            Game game = game(connection);
            assertTrue(game.handleKeyDown(mouseBack(source, KeyEvent.ACTION_DOWN)));

            ReflectionHelpers.callInstanceMethod(game.touchInput, "setInputGrabState",
                    ReflectionHelpers.ClassParameter.from(boolean.class, false));
            assertEquals(Arrays.asList(true, false), connection.rightButton);
            click(game, source);

            assertEquals("Late mouse events must not press the button again",
                    Arrays.asList(true, false), connection.rightButton);
        }
    }

    @Test
    public void capturedRightClicksAreForwardedUnlessMouseNavigationHandlesThem() {
        for (int source : new int[] {InputDevice.SOURCE_MOUSE, InputDevice.SOURCE_MOUSE_RELATIVE}) {
            RecordingConnection connection = new RecordingConnection();
            Game game = game(connection);

            click(game, source);
            assertEquals(Arrays.asList(true, false), connection.rightButton);

            PreferenceConfiguration prefs = ReflectionHelpers.getField(game, "prefConfig");
            prefs.mouseNavButtons = true;
            click(game, source);
            assertEquals("Mouse navigation must not produce duplicate right-clicks",
                    Arrays.asList(true, false), connection.rightButton);
        }
    }

    private static Game game(RecordingConnection connection) {
        Game game = Robolectric.buildActivity(Game.class).get();
        ReflectionHelpers.setField(game, "conn", connection);
        ReflectionHelpers.setField(game, "prefConfig", new PreferenceConfiguration());
        ReflectionHelpers.setField(game, "inputCaptureProvider", new InputCaptureProvider() {});
        ReflectionHelpers.setField(game, "grabbedInput", true);
        ReflectionHelpers.setField(game, "connected", true);
        PreferenceConfiguration prefs = ReflectionHelpers.getField(game, "prefConfig");
        prefs.backOpensMenu = true;
        return game;
    }

    private static void click(Game game, int source) {
        assertTrue(game.handleKeyDown(mouseBack(source, KeyEvent.ACTION_DOWN)));
        assertTrue(game.handleKeyUp(mouseBack(source, KeyEvent.ACTION_UP)));
    }

    private static KeyEvent mouseBack(int source, int action) {
        return new KeyEvent(0, 10, action, KeyEvent.KEYCODE_BACK, 0, 0, -1, 0, 0, source);
    }

    @Test
    public void fractionalScrollAccumulatesSkipsZeroAxesAndResetsWhenCaptureStops() {
        RecordingConnection connection = new RecordingConnection();
        boolean[] capturing = {true};
        Game game = Robolectric.buildActivity(Game.class).get();
        ReflectionHelpers.setField(game, "conn", connection);
        ReflectionHelpers.setField(game, "prefConfig", new PreferenceConfiguration());
        ReflectionHelpers.setField(game, "inputCaptureProvider", new InputCaptureProvider() {
            @Override public boolean isCapturingActive() { return capturing[0]; }
        });
        ReflectionHelpers.setField(game, "grabbedInput", true);
        ReflectionHelpers.setField(game, "connected", true);
        GameMouseInput mouse = new GameMouseInput(game);

        MotionEvent.PointerProperties[] props = {new MotionEvent.PointerProperties()};
        props[0].id = 0;
        props[0].toolType = MotionEvent.TOOL_TYPE_MOUSE;
        MotionEvent.PointerCoords[] coords = {new MotionEvent.PointerCoords()};
        coords[0].setAxisValue(MotionEvent.AXIS_VSCROLL, 0.003f);
        coords[0].setAxisValue(MotionEvent.AXIS_HSCROLL, 0f);
        MotionEvent e1 = MotionEvent.obtain(1000, 1000, MotionEvent.ACTION_SCROLL, 1, props, coords,
                0, 0, 1, 1, 0, 0, InputDevice.SOURCE_MOUSE, 0);
        MotionEvent e2 = MotionEvent.obtain(1000, 1010, MotionEvent.ACTION_SCROLL, 1, props, coords,
                0, 0, 1, 1, 0, 0, InputDevice.SOURCE_MOUSE, 0);
        try {
            mouse.handleMouseOrStylusEvent(null, e1, InputDevice.SOURCE_MOUSE, null);
            assertEquals("0.003 * 120 = 0.36 stays sub-unit until second scroll", 0, connection.vScrolls.size());
            mouse.handleMouseOrStylusEvent(null, e2, InputDevice.SOURCE_MOUSE, null);
            assertEquals(Arrays.asList((short) 1), connection.vScrolls);
            assertTrue("Zero horizontal axis must not send packet", connection.hScrolls.isEmpty());

            // Leave fractional remainder pending, press button, then drop capture: button up and reset occur.
            mouse.handleMouseOrStylusEvent(null, e1, InputDevice.SOURCE_MOUSE, null);
            MotionEvent down = MotionEvent.obtain(1000, 1020, MotionEvent.ACTION_BUTTON_PRESS, 1, props, coords,
                    0, MotionEvent.BUTTON_SECONDARY, 1, 1, 0, 0, InputDevice.SOURCE_MOUSE, 0);
            try {
                mouse.handleMouseOrStylusEvent(null, down, InputDevice.SOURCE_MOUSE, null);
            } finally {
                down.recycle();
            }
            capturing[0] = false;
            mouse.handleMouseOrStylusEvent(null, e1, InputDevice.SOURCE_MOUSE, null);
            assertEquals(Arrays.asList(true, false), connection.rightButton);
            // Re-enable capture: leftover 0.6 fraction was discarded, so one 0.005f event does not fire yet.
            capturing[0] = true;
            connection.vScrolls.clear();
            mouse.handleMouseOrStylusEvent(null, e1, InputDevice.SOURCE_MOUSE, null);
            assertTrue(connection.vScrolls.isEmpty());
        } finally {
            e1.recycle();
            e2.recycle();
        }
    }

    private static class RecordingConnection extends NvConnection {
        final List<Boolean> rightButton = new ArrayList<>();
        final List<Short> vScrolls = new ArrayList<>();
        final List<Short> hScrolls = new ArrayList<>();

        RecordingConnection() {
            super(null, new ComputerDetails.AddressTuple("127.0.0.1", 47989), 47984, "test",
                    new StreamConfiguration.Builder().build(), null, null);
        }

        @Override public boolean canSendInput() { return true; }
        @Override public void sendMouseButtonDown(byte button) {
            if (button == MouseButtonPacket.BUTTON_RIGHT) rightButton.add(true);
        }
        @Override public void sendMouseButtonUp(byte button) {
            if (button == MouseButtonPacket.BUTTON_RIGHT) rightButton.add(false);
        }
        @Override public void sendMouseHighResScroll(short amount) { vScrolls.add(amount); }
        @Override public void sendMouseHighResHScroll(short amount) { hScrolls.add(amount); }
    }
}
