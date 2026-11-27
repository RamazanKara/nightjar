/**
 * Requests unbuffered {@link InputDevice#SOURCE_ANY} motion dispatch on stream views so
 * Android's InputDispatcher delivers touch, mouse, stylus, and analog gamepad events on HID
 * arrival instead of batching them to Choreographer VSync.
 */
package com.limelight.binding.input;

import android.os.Build;
import android.view.InputDevice;
import android.view.View;

public final class UnbufferedInputDispatch {
    private UnbufferedInputDispatch() {}

    /**
     * Enables unbuffered dispatch for {@link InputDevice#SOURCE_ANY} on each non-null view.
     * {@link View#requestUnbufferedDispatch(int)} requires Android 11 (API 30); older versions
     * continue using the per-gesture {@code MotionEvent} API 21 call during touch dispatch.
     */
    public static void enable(View... views) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R || views == null) {
            return;
        }
        for (View view : views) {
            if (view != null) {
                view.requestUnbufferedDispatch(InputDevice.SOURCE_ANY);
            }
        }
    }
}
