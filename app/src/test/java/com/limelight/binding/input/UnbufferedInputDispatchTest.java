/**
 * Unit tests for {@link UnbufferedInputDispatch}.
 */
package com.limelight.binding.input;

import android.app.Application;
import android.content.Context;
import android.view.InputDevice;
import android.view.View;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.Implementation;
import org.robolectric.annotation.Implements;
import org.robolectric.annotation.RealObject;
import org.robolectric.shadows.ShadowView;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

import static org.junit.Assert.assertEquals;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 30, application = Application.class, shadows = UnbufferedInputDispatchTest.RecordingViewShadow.class)
public class UnbufferedInputDispatchTest {
    private Context context;

    @Before
    public void setUp() {
        context = RuntimeEnvironment.getApplication();
        RecordingViewShadow.calls.clear();
    }

    @Test
    public void handlesNullVarargsAndNullEntriesSafely() {
        UnbufferedInputDispatch.enable((View[]) null);
        UnbufferedInputDispatch.enable((View) null);
        assertEquals(Collections.emptyList(), RecordingViewShadow.calls);
    }

    @Test
    public void requestsUnbufferedDispatchWithSourceAnyOnNonNullViews() {
        View decor = new View(context);
        View stream = new View(context);

        UnbufferedInputDispatch.enable(null, decor, null, stream);

        assertEquals(
                Arrays.asList(
                        new DispatchCall(decor, InputDevice.SOURCE_ANY),
                        new DispatchCall(stream, InputDevice.SOURCE_ANY)),
                RecordingViewShadow.calls);
    }

    @Test
    @Config(sdk = 29)
    public void isNoOpBeforeAndroidR() {
        View view = new View(context);
        UnbufferedInputDispatch.enable(view);
        assertEquals(Collections.emptyList(), RecordingViewShadow.calls);
    }

    private static final class DispatchCall {
        final View view;
        final int source;

        DispatchCall(View view, int source) {
            this.view = view;
            this.source = source;
        }

        @Override
        public boolean equals(Object other) {
            if (!(other instanceof DispatchCall)) return false;
            DispatchCall that = (DispatchCall) other;
            return view == that.view && source == that.source;
        }

        @Override
        public int hashCode() {
            return System.identityHashCode(view) * 31 + source;
        }
    }

    @Implements(View.class)
    public static class RecordingViewShadow extends ShadowView {
        static final List<DispatchCall> calls = new ArrayList<>();
        @RealObject private View realView;

        @Implementation
        protected void requestUnbufferedDispatch(int source) {
            calls.add(new DispatchCall(realView, source));
        }
    }
}
