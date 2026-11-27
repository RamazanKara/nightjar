package com.limelight;

import android.app.Application;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.pm.PackageManager;
import android.hardware.input.InputManager;
import android.os.Looper;
import android.preference.PreferenceManager;
import android.view.InputDevice;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;

import com.limelight.binding.input.ControllerButtonMap;
import com.limelight.binding.input.TouchLayout;
import com.limelight.binding.input.pad.PadControllerLayouts;
import com.limelight.binding.input.pad.PadLayout;

import org.junit.After;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowInputDevice;
import org.robolectric.util.ReflectionHelpers;

import java.util.ArrayList;
import java.util.concurrent.TimeUnit;

import static org.junit.Assert.*;
import static org.robolectric.Shadows.shadowOf;

/**
 * A controller with its own on-screen layout (Rubylight #154) plugged in when the stream starts: the controls show
 * for it and sit above the game buttons, as they always do, so the game buttons' overlay never takes their touches.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 36, application = Application.class, qualifiers = "w832dp-h750dp-land-420dpi",
        shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class GameControllerLayoutStartTest {
    private static final String PC = "pc-uuid";
    private static final int APP = 11;
    private static final int VENDOR = 0x045e, PRODUCT = 0x0b13;
    private static final String NAME = "Xbox Wireless Controller";
    private ActivityController<GameModernControlsTouchTest.StreamGame> controller;

    @After public void leaveTheStream() {
        if (controller == null) return;
        controller.pause().stop().destroy();
        shadowOf(Looper.getMainLooper()).idleFor(100, TimeUnit.MILLISECONDS);
        controller = null;
    }

    /** An external gamepad with two stick axes, as Android lists a Bluetooth Xbox pad. */
    private static void plugGamepad(Context context) {
        InputDevice device = ShadowInputDevice.makeInputDeviceNamed(NAME);
        int sources = InputDevice.SOURCE_GAMEPAD | InputDevice.SOURCE_JOYSTICK;
        ReflectionHelpers.setField(device, "mVendorId", VENDOR);
        ReflectionHelpers.setField(device, "mProductId", PRODUCT);
        ReflectionHelpers.setField(device, "mId", 7);
        ReflectionHelpers.setField(device, "mDescriptor", "controller-7");
        ReflectionHelpers.setField(device, "mIsExternal", true);
        ReflectionHelpers.setField(device, "mSources", sources);
        ReflectionHelpers.setField(device, "mKeyboardType", InputDevice.KEYBOARD_TYPE_NONE);
        ReflectionHelpers.setField(device, "mMotionRanges", new ArrayList<InputDevice.MotionRange>());
        for (int axis : new int[] {MotionEvent.AXIS_X, MotionEvent.AXIS_Y}) {
            ReflectionHelpers.callInstanceMethod(device, "addMotionRange",
                    ReflectionHelpers.ClassParameter.from(int.class, axis),
                    ReflectionHelpers.ClassParameter.from(int.class, InputDevice.SOURCE_JOYSTICK),
                    ReflectionHelpers.ClassParameter.from(float.class, -1f),
                    ReflectionHelpers.ClassParameter.from(float.class, 1f),
                    ReflectionHelpers.ClassParameter.from(float.class, 0f),
                    ReflectionHelpers.ClassParameter.from(float.class, 0f),
                    ReflectionHelpers.ClassParameter.from(float.class, 0f));
        }
        shadowOf((InputManager) context.getSystemService(Context.INPUT_SERVICE)).addInputDevice(device);
    }

    @Test public void aControllersOwnLayoutAtTheStartSitsAboveTheGameButtons() {
        Context context = RuntimeEnvironment.getApplication();
        PreferenceManager.getDefaultSharedPreferences(context).edit()
                .putBoolean("checkbox_show_onscreen_controls", true)
                .commit();
        TouchLayout.setEnabled(context, TouchLayout.profileKey(PC, null, APP), true);
        plugGamepad(context);
        String model = ControllerButtonMap.deviceKey(VENDOR, PRODUCT, NAME);
        context.getSharedPreferences("osc_modern", Context.MODE_PRIVATE).edit()
                .putString(PadControllerLayouts.key(true, model), new PadLayout().serialize())
                .commit();
        try {
            ActivityInfo info = new ActivityInfo(context.getPackageManager()
                    .getActivityInfo(new ComponentName(context, Game.class), 0));
            info.name = GameModernControlsTouchTest.StreamGame.class.getName();
            shadowOf(context.getPackageManager()).addOrUpdateActivity(info);
        } catch (PackageManager.NameNotFoundException e) {
            throw new AssertionError(e);
        }
        Intent intent = new Intent(context, GameModernControlsTouchTest.StreamGame.class)
                .putExtra(Game.EXTRA_HOST, "192.0.2.1")
                .putExtra(Game.EXTRA_UNIQUEID, "device")
                .putExtra(Game.EXTRA_APP_ID, APP)
                .putExtra(Game.EXTRA_APP_NAME, "Desktop")
                .putExtra(Game.EXTRA_PC_UUID, PC);
        controller = Robolectric.buildActivity(GameModernControlsTouchTest.StreamGame.class, intent).setup();
        shadowOf(Looper.getMainLooper()).idleFor(100, TimeUnit.MILLISECONDS);
        Game game = controller.get();

        assertTrue("the pad counts as connected", game.physicalControllerConnected);
        assertNotNull("its layout keeps the controls despite Hide with a controller", game.modernController);
        assertTrue(game.modernController.isShown());
        ViewGroup frame = (ViewGroup) game.streamView.getParent();
        int buttons = frame.indexOfChild(game.gameTouchControls);
        assertTrue("the game buttons are in the frame", buttons >= 0);
        int controls = 0;
        for (int i = 0; i < frame.getChildCount(); i++) {
            View child = frame.getChildAt(i);
            if (!child.getClass().getSimpleName().equals("PadControlView") || child.getVisibility() != View.VISIBLE) continue;
            controls++;
            assertTrue("a control at " + i + " under the game buttons at " + buttons, i > buttons);
        }
        assertTrue("the controls show", controls > 0);
    }
}
