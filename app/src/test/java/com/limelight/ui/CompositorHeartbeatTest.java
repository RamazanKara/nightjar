package com.limelight.ui;

import android.app.Application;
import android.graphics.drawable.ColorDrawable;
import android.graphics.drawable.Drawable;
import android.os.Looper;
import android.view.View;
import android.widget.FrameLayout;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

import java.util.concurrent.TimeUnit;

import static org.junit.Assert.*;
import static org.robolectric.Shadows.shadowOf;

/**
 * Ported from the derflacco fork's CompositorHeartbeatTest (branch Artemide_Experimental_Async, GPL-3.0, commit
 * 784b4eca), on the looper's clock; the dot is added on start and removed on stop here.
 */
@Config(sdk = 33, application = Application.class)
@RunWith(RobolectricTestRunner.class)
public class CompositorHeartbeatTest {
    private static int color(View view) {
        return ((ColorDrawable) view.getBackground()).getColor();
    }

    private static void idle(long ms) {
        shadowOf(Looper.getMainLooper()).idleFor(ms, TimeUnit.MILLISECONDS);
    }

    @Test
    public void startAndStopAddAlternateAndRemoveTheDot() {
        FrameLayout window = new FrameLayout(RuntimeEnvironment.getApplication());
        CompositorHeartbeat heartbeat = new CompositorHeartbeat(window);
        View dot = heartbeat.dot();
        assertNull("nothing before start", dot.getParent());

        heartbeat.start();
        shadowOf(Looper.getMainLooper()).idle();
        assertSame(window, dot.getParent());
        assertEquals(View.VISIBLE, dot.getVisibility());
        assertEquals(CompositorHeartbeat.COLOR_PHASE_ONE, color(dot));
        Drawable drawable = dot.getBackground();

        idle(CompositorHeartbeat.INTERVAL_MS);
        assertEquals(CompositorHeartbeat.COLOR_PHASE_TWO, color(dot));
        idle(CompositorHeartbeat.INTERVAL_MS);
        assertEquals(CompositorHeartbeat.COLOR_PHASE_ONE, color(dot));
        assertSame("a tick changes the colour of the one drawable", drawable, dot.getBackground());

        heartbeat.stop();
        assertFalse(heartbeat.running());
        assertEquals(View.GONE, dot.getVisibility());
        shadowOf(Looper.getMainLooper()).idle();
        assertNull("gone when stopped", dot.getParent());
        int stopped = color(dot);
        idle(CompositorHeartbeat.INTERVAL_MS * 3);
        assertEquals("no tick after stop", stopped, color(dot));
    }

    @Test
    public void restartingBeforeDeferredRemovalReusesTheDotWithoutDuplicateAdd() {
        FrameLayout window = new FrameLayout(RuntimeEnvironment.getApplication());
        CompositorHeartbeat heartbeat = new CompositorHeartbeat(window);
        View dot = heartbeat.dot();

        heartbeat.start();
        heartbeat.stop();
        assertEquals(View.GONE, dot.getVisibility());
        heartbeat.start();
        shadowOf(Looper.getMainLooper()).idle();

        assertSame(window, dot.getParent());
        assertEquals(1, window.getChildCount());
        assertEquals(View.VISIBLE, dot.getVisibility());
        heartbeat.stop();
        shadowOf(Looper.getMainLooper()).idle();
    }

    @Test
    public void stoppingDuringDispatchWindowVisibilityChangedDoesNotCrash() {
        FrameLayout window = new FrameLayout(RuntimeEnvironment.getApplication());
        CompositorHeartbeat heartbeat = new CompositorHeartbeat(window);
        View dot = heartbeat.dot();
        heartbeat.start();
        View sibling = new View(RuntimeEnvironment.getApplication()) {
            @Override
            protected void onWindowVisibilityChanged(int visibility) {
                super.onWindowVisibilityChanged(visibility);
                if (visibility == View.GONE) {
                    heartbeat.stop();
                }
            }
        };
        window.addView(sibling, 0);

        window.dispatchWindowVisibilityChanged(View.GONE);
        assertEquals(View.GONE, dot.getVisibility());
        shadowOf(Looper.getMainLooper()).idle();
        assertNull(dot.getParent());
        assertEquals(1, window.getChildCount());
    }

    @Test
    @Config(sdk = 35)
    public void setRequestedFrameRateTargetsTheDotOnApi35() {
        FrameLayout window = new FrameLayout(RuntimeEnvironment.getApplication());
        CompositorHeartbeat heartbeat = new CompositorHeartbeat(window);
        heartbeat.setRequestedFrameRate(144f);
        assertEquals(144f, heartbeat.dot().getRequestedFrameRate(), 0f);
    }

    @Test
    public void theDotTakesNoFocusTouchOrAccessibility() {
        FrameLayout window = new FrameLayout(RuntimeEnvironment.getApplication());
        CompositorHeartbeat heartbeat = new CompositorHeartbeat(window);
        heartbeat.start();
        View dot = heartbeat.dot();
        assertFalse(dot.isFocusable());
        assertFalse(dot.isFocusableInTouchMode());
        assertFalse(dot.isClickable());
        assertFalse(dot.isLongClickable());
        assertEquals(View.IMPORTANT_FOR_ACCESSIBILITY_NO, dot.getImportantForAccessibility());
        assertFalse(dot.isImportantForAccessibility());
        heartbeat.stop();
    }
}
