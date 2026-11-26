package com.limelight.ui;

import android.content.Context;
import android.view.MotionEvent;
import android.view.ScaleGestureDetector;

import com.limelight.binding.input.touch.TwoFingerTapGesture;

/** Two-finger pan/pinch never leaks pointer-up events into a canceled host gesture. */
public final class DesktopGestures {
    private static final float PASSIVE_PINCH_THRESHOLD_DP = 28f;

    private final DesktopViewport viewport = new DesktopViewport();
    private final StreamView view;
    private final ScaleGestureDetector detector;
    private final Runnable cancelInput;
    private final Runnable rightClick;
    private final TwoFingerTapGesture twoFingerTap = new TwoFingerTapGesture();
    private final float density;
    private boolean enabled, consuming, tapAllowed, hasInitialSpan;
    private float lastX, lastY, initialSpan;

    public DesktopGestures(Context context, StreamView view, Runnable cancelInput, Runnable rightClick) {
        this.view = view; this.cancelInput = cancelInput; this.rightClick = rightClick;
        density = context.getResources().getDisplayMetrics().density;
        detector = new ScaleGestureDetector(context, new ScaleGestureDetector.SimpleOnScaleGestureListener() {
            @Override public boolean onScaleBegin(ScaleGestureDetector d) {
                twoFingerTap.invalidate();
                return true;
            }
            @Override public boolean onScale(ScaleGestureDetector d) {
                if (!consuming) {
                    if (!enabled && (!hasInitialSpan || Math.abs(d.getCurrentSpan() - initialSpan) < PASSIVE_PINCH_THRESHOLD_DP * density)) {
                        return false;
                    }
                    startConsuming(d.getFocusX(), d.getFocusY());
                }
                viewport.zoom(d.getScaleFactor(), d.getFocusX(), d.getFocusY()); apply(); return true;
            }
        });
        detector.setQuickScaleEnabled(false);
        view.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> {
            viewport.resize(r - l, b - t); apply();
        });
    }
    public DesktopViewport viewport() { return viewport; }
    public boolean enabled() { return enabled; }
    public void setEnabled(boolean enabled) {
        cancelInput.run(); cancelTouch(); consuming = false; this.enabled = enabled;
        if (!enabled) reset();
    }
    /** Invalidate a pending tap on focus loss, mode changes, or an overlay taking input. */
    public void cancelTouch() { twoFingerTap.reset(); tapAllowed = false; hasInitialSpan = false; }
    public void reset() { viewport.reset(); apply(); }
    private void apply() { view.setVideoTransform(viewport.scale(), viewport.x(), viewport.y()); }

    private void startConsuming(float x, float y) {
        cancelInput.run();
        consuming = true;
        lastX = x;
        lastY = y;
    }

    private static float fingerSpan(MotionEvent event) {
        return (float) Math.hypot(event.getX(0) - event.getX(1), event.getY(0) - event.getY(1));
    }

    public boolean onTouch(MotionEvent event, boolean allowRightClick) {
        return onTouch(event, allowRightClick, false);
    }

    public boolean onTouch(MotionEvent event, boolean allowRightClick, boolean allowScrollThrough) {
        if ((!enabled && !allowScrollThrough) || event.getToolType(0) != MotionEvent.TOOL_TYPE_FINGER) return false;
        int action = event.getActionMasked();
        if (action == MotionEvent.ACTION_DOWN) {
            consuming = false;
            tapAllowed = allowRightClick;
            hasInitialSpan = false;
        }
        boolean tapped = twoFingerTap.onTouch(event, density);
        if (event.getPointerCount() >= 2 && (action == MotionEvent.ACTION_POINTER_DOWN || !hasInitialSpan)) {
            initialSpan = fingerSpan(event);
            hasInitialSpan = true;
        } else if (event.getPointerCount() < 2) {
            hasInitialSpan = false;
        }
        detector.onTouchEvent(event);
        if (event.getPointerCount() >= 2) {
            float x = (event.getX(0) + event.getX(1)) / 2;
            float y = (event.getY(0) + event.getY(1)) / 2;
            boolean passiveWaitAtOneX = allowScrollThrough && !enabled && viewport.scale() == 1.0f;
            if (!consuming) {
                boolean pinchDiverged = detector.isInProgress() && hasInitialSpan
                        && Math.abs(fingerSpan(event) - initialSpan) >= PASSIVE_PINCH_THRESHOLD_DP * density;
                if (!passiveWaitAtOneX || pinchDiverged) {
                    startConsuming(x, y);
                }
            }
            else if (action == MotionEvent.ACTION_MOVE && !twoFingerTap.isPossible()) {
                viewport.pan(x - lastX, y - lastY); apply();
                lastX = x; lastY = y;
            }
            else {
                lastX = x; lastY = y;
            }
        }
        boolean handled = consuming;
        if (action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_CANCEL) {
            consuming = false;
            hasInitialSpan = false;
            if (handled && tapped && tapAllowed) rightClick.run();
            tapAllowed = false;
        }
        return handled;
    }
}
