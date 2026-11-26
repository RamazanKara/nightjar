package com.limelight.ui;

import android.graphics.drawable.ColorDrawable;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.FrameLayout;

/**
 * Settings › Keep TV video smooth: a 1 dp dot in the window that holds the video, switched between two
 * near-black colours every 100 ms while it runs.
 *
 * Some Android TV firmware takes an unstable direct-composition path when the video surface is the only
 * visible content, and freezes, flashes or restarts. A regularly changing UI pixel keeps the app's UI layer
 * taking part in the composition.
 *
 * Ported from the derflacco fork (branch Artemide_Experimental_Async, GPL-3.0), app/src/main/java/com/limelight/ui/
 * CompositorHeartbeat.java, commit 784b4eca. Changed for Nightjar: the dot is made and added here and removed
 * on stop instead of living hidden in the stream layout, so it can sit in an external display's window too;
 * one drawable changes colour, so a tick allocates nothing.
 */
public final class CompositorHeartbeat {
    static final long INTERVAL_MS = 100L;
    static final int COLOR_PHASE_ONE = 0xFF000000;
    static final int COLOR_PHASE_TWO = 0xFF010101;

    private final ViewGroup holder;
    private final View dot;
    private final ColorDrawable color = new ColorDrawable(COLOR_PHASE_ONE);
    private final Handler handler = new Handler(Looper.getMainLooper());
    private boolean running;
    private boolean alternatePhase;

    private final Runnable heartbeat = new Runnable() {
        @Override
        public void run() {
            if (!running) {
                return;
            }

            color.setColor(alternatePhase ? COLOR_PHASE_TWO : COLOR_PHASE_ONE);
            alternatePhase = !alternatePhase;
            handler.postDelayed(this, INTERVAL_MS);
        }
    };

    private final Runnable removeDot;

    /** The dot for this window; nothing shows until {@link #start()}. Main thread only. */
    public CompositorHeartbeat(ViewGroup holder) {
        if (holder == null) {
            throw new IllegalArgumentException("holder must not be null");
        }

        this.holder = holder;
        dot = new View(holder.getContext());
        dot.setBackground(color);
        // Never takes focus, touch or accessibility focus: touches pass to the views below.
        dot.setFocusable(false);
        dot.setFocusableInTouchMode(false);
        dot.setClickable(false);
        dot.setLongClickable(false);
        dot.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_NO);
        removeDot = () -> {
            if (!running && dot.getParent() == holder) {
                holder.removeView(dot);
            }
        };
    }

    /** The window's view the dot sits in. */
    public ViewGroup holder() {
        return holder;
    }

    /** The dot; in its window only while running. */
    public View dot() {
        return dot;
    }

    public boolean running() {
        return running;
    }

    /** Adds the dot over the video in the top-left corner (left in a right-to-left layout too), and starts its colour change. */
    public void start() {
        if (running) {
            return;
        }

        running = true;
        alternatePhase = false;
        handler.removeCallbacks(removeDot);
        dot.setVisibility(View.VISIBLE);
        if (dot.getParent() != holder) {
            int size = Math.max(1, Math.round(holder.getResources().getDisplayMetrics().density));
            holder.addView(dot, new FrameLayout.LayoutParams(size, size, Gravity.TOP | Gravity.LEFT));
        }
        heartbeat.run();
    }

    /** Stops the colour change and schedules removal of the dot from its window. */
    public void stop() {
        running = false;
        handler.removeCallbacks(heartbeat);
        dot.setVisibility(View.GONE);
        if (dot.getParent() == holder) {
            handler.removeCallbacks(removeDot);
            handler.post(removeDot);
        }
    }

    /** Sets the Android 15+ requested frame rate vote on the dot view so UI refresh recovery carries the pinned rate. */
    public void setRequestedFrameRate(float frameRate) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.VANILLA_ICE_CREAM) {
            dot.setRequestedFrameRate(frameRate);
        }
    }
}
