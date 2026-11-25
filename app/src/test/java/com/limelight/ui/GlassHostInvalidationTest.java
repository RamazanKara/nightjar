package com.limelight.ui;

import android.app.Activity;
import android.app.Application;
import android.graphics.Canvas;
import android.view.MotionEvent;
import android.view.View;
import android.widget.FrameLayout;

import com.limelight.R;
import com.limelight.binding.input.keyboard.PcKeyboardController;
import com.limelight.binding.input.keyboard.PcKeyboardEngine;
import com.limelight.binding.input.keyboard.PcKeyboardLayout;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.util.ReflectionHelpers;

import static org.junit.Assert.*;

/**
 * Item 25 & Item 24: verifies {@link GlassHost} skips sibling {@link GlassSurface} invalidations for unrelated
 * non-intersecting children, coalesces repeated intersecting invalidations until draw resets them, and
 * {@link PcKeyboardView} releases active touches on cancel and detach.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 33, application = Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class GlassHostInvalidationTest {
    private static final class CountingGlassSurface extends GlassSurface {
        int invalidations;

        CountingGlassSurface(android.content.Context context) {
            super(context);
        }

        @Override
        public void invalidate() {
            invalidations++;
            super.invalidate();
        }
    }

    @Test
    public void unrelatedChildInvalidationSkipsSurfaceAndIntersectingCoalescesPerFrame() {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).setup()) {
            GlassHost host = new GlassHost(activity.get());
            FrameLayout content = new FrameLayout(activity.get());
            View topChild = new View(activity.get());
            View bottomChild = new View(activity.get());
            content.addView(topChild);
            content.addView(bottomChild);
            CountingGlassSurface bottomDock = new CountingGlassSurface(activity.get());

            host.addView(content);
            host.addView(bottomDock);
            activity.get().setContentView(host);

            host.measure(
                    View.MeasureSpec.makeMeasureSpec(400, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(800, View.MeasureSpec.EXACTLY));
            host.layout(0, 0, 400, 800);
            content.layout(0, 0, 400, 800);
            topChild.layout(0, 0, 400, 100);
            bottomChild.layout(0, 680, 400, 780);
            bottomDock.layout(0, 700, 400, 800);

            host.setTier(GlassTier.LIVE);
            ReflectionHelpers.setField(host, "liveReady", true);
            bottomDock.invalidations = 0;

            // Unrelated child far above the bottom dock (+32dp blur margin) must not invalidate it.
            host.onDescendantInvalidated(content, topChild);
            assertEquals("unrelated child far outside blur margin must not redraw GlassSurface", 0, bottomDock.invalidations);

            // Overlapping child invalidates the dock once per draw frame even across multiple bursts.
            host.onDescendantInvalidated(content, bottomChild);
            host.onDescendantInvalidated(content, bottomChild);
            assertEquals("intersecting child invalidates surface once per draw pass", 1, bottomDock.invalidations);

            // Drawing clears invalidatedThisDraw so the next frame's overlapping invalidation works.
            host.dispatchDraw(new Canvas());
            host.onDescendantInvalidated(content, bottomChild);
            assertEquals("next frame after dispatchDraw can invalidate surface again", 2, bottomDock.invalidations);
        }
    }

    @Test
    public void pcKeyboardViewReleasesAllTouchesOnCancelAndWindowDetach() {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).setup()) {
            activity.get().setTheme(R.style.AppTheme);
            int[] downs = {0};
            int[] ups = {0};
            PcKeyboardEngine.Output out = (vk, down, modifiers) -> {
                if (down) downs[0]++;
                else ups[0]++;
            };
            PcKeyboardController pcController = new PcKeyboardController(
                    PcKeyboardLayout.byId(PcKeyboardLayout.US), out, new PcKeyboardController.Host() {
                @Override public void close() { }
                @Override public void changed() { }
            });
            PcKeyboardView view = new PcKeyboardView(activity.get(), pcController);
            FrameLayout root = new FrameLayout(activity.get());
            root.addView(view, new FrameLayout.LayoutParams(900, 400));
            activity.get().setContentView(root);
            view.measure(
                    View.MeasureSpec.makeMeasureSpec(900, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(400, View.MeasureSpec.EXACTLY));
            view.layout(0, 0, 900, 400);

            MotionEvent down1 = MotionEvent.obtain(0, 10, MotionEvent.ACTION_DOWN, 25f, 25f, 0);
            view.onTouchEvent(down1);
            down1.recycle();
            assertEquals(1, downs[0]);
            assertEquals(0, ups[0]);

            MotionEvent cancel = MotionEvent.obtain(0, 20, MotionEvent.ACTION_CANCEL, 25f, 25f, 0);
            view.onTouchEvent(cancel);
            cancel.recycle();
            assertEquals("ACTION_CANCEL must release held touches", 1, ups[0]);

            MotionEvent down2 = MotionEvent.obtain(30, 30, MotionEvent.ACTION_DOWN, 25f, 25f, 0);
            view.onTouchEvent(down2);
            down2.recycle();
            assertEquals(2, downs[0]);

            root.removeView(view);
            assertEquals("detaching PcKeyboardView must release held touches", 2, ups[0]);
        }
    }
}
