/**
 * Unit tests for {@link UnbufferedInputDispatch} against real Android {@link View} /
 * {@link android.view.ViewGroup} unbuffered source-class propagation and window attachment.
 */
package com.limelight.binding.input;

import android.app.Activity;
import android.app.Application;
import android.content.Context;
import android.view.InputDevice;
import android.view.View;
import android.widget.FrameLayout;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

import java.lang.reflect.Field;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotEquals;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 30, application = Application.class)
public class UnbufferedInputDispatchTest {
    private Context context;

    @Before
    public void setUp() {
        context = RuntimeEnvironment.getApplication();
    }

    private static int readUnbufferedSource(View view) throws Exception {
        Field field = View.class.getDeclaredField("mUnbufferedInputSource");
        field.setAccessible(true);
        return field.getInt(view);
    }

    @Test
    public void handlesNullVarargsAndNullEntriesSafely() {
        UnbufferedInputDispatch.enable((View[]) null);
        UnbufferedInputDispatch.enable((View) null);
    }

    @Test
    public void propagatesPointerClassToParentViewGroupAndReappliesOnWindowAttachAtApi30() throws Exception {
        assertEquals(0x1f, UnbufferedInputDispatch.ALL_SOURCE_CLASSES & InputDevice.SOURCE_CLASS_MASK);

        FrameLayout parent = new FrameLayout(context);
        View child = new View(context);
        // Enable before attaching to a window (mirrors Game.onCreate before DecorView attach).
        UnbufferedInputDispatch.enable(null, parent, null, child);
        parent.addView(child);

        assertEquals(UnbufferedInputDispatch.ALL_SOURCE_CLASSES, readUnbufferedSource(child));

        Activity activity = Robolectric.buildActivity(Activity.class).create().start().resume().visible().get();
        activity.setContentView(parent);

        // Even with no focused child (touch mode), ViewGroup retains SOURCE_CLASS_POINTER once attached.
        assertNotEquals(0, readUnbufferedSource(parent) & InputDevice.SOURCE_CLASS_POINTER);
        assertEquals(UnbufferedInputDispatch.ALL_SOURCE_CLASSES, readUnbufferedSource(child));
    }

    @Test
    @Config(sdk = 29)
    public void isNoOpBeforeAndroidR() {
        // API 29 View lacks requestUnbufferedDispatch(int) and mUnbufferedInputSource; both
        // immediate enable() and later window attachment must be safe no-ops without linkage error.
        View view = new View(context);
        UnbufferedInputDispatch.enable(view);
        Activity activity = Robolectric.buildActivity(Activity.class).create().start().resume().visible().get();
        activity.setContentView(view);
    }
}
