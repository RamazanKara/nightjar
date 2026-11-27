/**
 * Requests unbuffered motion dispatch across all input source classes on stream views so
 * Android's InputDispatcher delivers touch, mouse, stylus, trackball, and analog gamepad events
 * on HID arrival instead of batching them to Choreographer VSync.
 */
package com.limelight.binding.input;

import android.os.Build;
import android.view.InputDevice;
import android.view.View;

public final class UnbufferedInputDispatch {
    /**
     * All {@code @InputSourceClass} bits accepted by {@link View#requestUnbufferedDispatch(int)}.
     * Note: {@link InputDevice#SOURCE_ANY} ({@code 0xffffff00}) has zero {@code SOURCE_CLASS_MASK}
     * ({@code 0xff}) bits and would not propagate {@code SOURCE_CLASS_POINTER} to {@code ViewRootImpl}.
     */
    public static final int ALL_SOURCE_CLASSES =
            InputDevice.SOURCE_CLASS_BUTTON
                    | InputDevice.SOURCE_CLASS_POINTER
                    | InputDevice.SOURCE_CLASS_TRACKBALL
                    | InputDevice.SOURCE_CLASS_POSITION
                    | InputDevice.SOURCE_CLASS_JOYSTICK;

    private UnbufferedInputDispatch() {}

    /**
     * Enables unbuffered dispatch for {@link #ALL_SOURCE_CLASSES} on each non-null view, and
     * re-applies on {@code onViewAttachedToWindow} when the view is not yet attached to a
     * {@code ViewRootImpl} (needed on Android 11–13 / API 30–33, where {@code ViewRootImpl.setView}
     * did not copy pre-attach unbuffered flags).
     */
    public static void enable(View... views) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R || views == null) {
            return;
        }
        for (View view : views) {
            if (view == null) {
                continue;
            }
            view.requestUnbufferedDispatch(ALL_SOURCE_CLASSES);
            if (!view.isAttachedToWindow()) {
                view.addOnAttachStateChangeListener(new View.OnAttachStateChangeListener() {
                    @Override
                    public void onViewAttachedToWindow(View attached) {
                        attached.removeOnAttachStateChangeListener(this);
                        attached.requestUnbufferedDispatch(ALL_SOURCE_CLASSES);
                    }

                    @Override
                    public void onViewDetachedFromWindow(View detached) {}
                });
            }
        }
    }
}
