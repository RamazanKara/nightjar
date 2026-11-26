package com.limelight.ui;

import android.app.Activity;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.view.FrameMetrics;
import android.view.Window;

import androidx.core.content.res.ResourcesCompat;

import com.limelight.R;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

/**
 * Watches the first {@code tier.probe.frames} real frames a live tier draws once its screen has settled. A p90 above
 * {@code tier.probe.p90BudgetFraction} of the frame budget steps the screen down one tier, and the verdict is kept for
 * this app version. It stops as soon as enough slow frames have decided that p90, so a slow device leaves a slow tier
 * within the first moves. Idle screens draw nothing and count nothing, so the probe simply finishes on a later scroll,
 * or the next time.
 *
 * <p>The numbers and the rule are nightjar-policy's {@code glass_probe}, the desktop's too: the budget floors at a 120 Hz
 * frame ({@code MIN_BUDGET_HZ}), so a faster screen keeps its live surfaces while they fit one. The crash guard is armed
 * before the first probed frame and cleared when the probe ends either way ({@link GlassTier#armGuard}). Where the
 * library is not loaded the Java mirrors below decide, pinned equal to the policy by GlassTierPolicyParityTest.
 */
public final class GlassProbe implements Window.OnFrameMetricsAvailableListener {
    /** What to do with the verdict; called on the main thread. */
    public interface Verdict {
        void stepDownTo(GlassTier tier);
    }

    /**
     * What a screen draws as it opens says nothing of how it moves: on a first launch those frames compile the shaders,
     * and all of them fit in under a second at 120 Hz.
     */
    static final long SETTLE_MILLIS = 2000;
    /**
     * A frame counts as slow only if it also came this late against the budget. A 120 Hz handheld whose render thread
     * idles at low clocks runs 6-7 ms of work for frames that all land on time, and the design asks for the display rate.
     */
    static final float LATE_FRACTION = 1.25f;
    /** The budget never asks for more than this rate ({@code tier.probe.minBudgetHz}). */
    static final float MIN_BUDGET_HZ = 120f;

    private final Activity activity;
    private final GlassTier tier;
    private final Verdict verdict;
    private final int frames;
    private final float budgetFraction;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final long started = SystemClock.uptimeMillis();
    private final long allowed;
    private final long budget;
    private final float refresh;
    private int count;
    private int slow;
    private boolean done;
    private boolean cancelled;
    private boolean listenerRemoved;

    private GlassProbe(Activity activity, GlassTier tier, Verdict verdict) {
        this.activity = activity;
        this.tier = tier;
        this.verdict = verdict;
        frames = activity.getResources().getInteger(R.integer.nj_tier_probe_frames);
        budgetFraction = ResourcesCompat.getFloat(activity.getResources(), R.dimen.nj_tier_probe_p90_budget_fraction);
        refresh = activity.getWindowManager().getDefaultDisplay().getRefreshRate();
        budget = budgetNs(refresh);
        allowed = (long) (budget * (double) budgetFraction);
    }

    private static GlassProbe watch(Activity activity, GlassTier tier, Verdict verdict) {
        GlassProbe probe = new GlassProbe(activity, tier, verdict);
        GlassTier.armGuard(activity);
        activity.getWindow().addOnFrameMetricsAvailableListener(probe, new Handler(Looper.getMainLooper()));
        return probe;
    }

    /** Starts watching, unless this tier has nothing to probe or this app version already has its verdict. */
    public static GlassProbe start(Activity activity, GlassTier tier, Verdict verdict) {
        if (activity.isFinishing() || activity.isDestroyed()) return null;
        if ((tier != GlassTier.LENS && tier != GlassTier.LIVE) || GlassTier.probed(activity)) return null;
        return watch(activity, tier, verdict);
    }

    /** After a step down: the lower tier is watched too, though a verdict is already kept. */
    public static GlassProbe continueAt(Activity activity, GlassTier tier, Verdict verdict) {
        if (activity.isFinishing() || activity.isDestroyed()) return null;
        if (tier != GlassTier.LIVE) return null;
        return watch(activity, tier, verdict);
    }

    /** Stops watching; what was seen so far is dropped and any queued verdict is cancelled (T9-17). */
    public void stop() {
        boolean running = !done || !listenerRemoved;
        done = true;
        cancelled = true;
        main.removeCallbacksAndMessages(null);
        removeListener();
        // The probe ended without a crash: the guard is done either way.
        if (running) GlassTier.clearGuard(activity);
    }

    private void removeListener() {
        if (listenerRemoved) return;
        listenerRemoved = true;
        try {
            activity.getWindow().removeOnFrameMetricsAvailableListener(this);
        } catch (IllegalArgumentException ignored) {
            // Already removed or window detached.
        }
    }

    @Override
    public void onFrameMetricsAvailable(Window window, FrameMetrics metrics, int dropCount) {
        if (done || metrics.getMetric(FrameMetrics.FIRST_DRAW_FRAME) == 1) return;
        if (!settled(started, SystemClock.uptimeMillis())) return;
        if (slowFrame(work(metrics), metrics.getMetric(FrameMetrics.TOTAL_DURATION))) slow++;
        count++;
        boolean fail = slow >= slowFramesToFail(frames);
        if (!fail && count < frames) return;
        done = true;
        com.limelight.LimeLog.info("Glass probe at " + tier + ": " + (fail ? "steps down" : "keeps it") + " after " + count + " frames, " + slow + " slow");
        // Removing the listener during its own callback is not allowed on every version: finish on the next turn.
        main.post(() -> {
            removeListener();
            if (cancelled) return;
            GlassTier.clearGuard(activity);
            if (activity.isFinishing() || activity.isDestroyed()) return;
            GlassTier next = fail ? tier.stepDown() : tier;
            GlassTier.remember(activity, next);
            if (fail) verdict.stepDownTo(next);
        });
    }

    /** The app's and the GPU's own work on the frame, without the wait for its vsync. */
    private static long work(FrameMetrics metrics) {
        long cpu = metrics.getMetric(FrameMetrics.ANIMATION_DURATION) + metrics.getMetric(FrameMetrics.LAYOUT_MEASURE_DURATION)
                + metrics.getMetric(FrameMetrics.DRAW_DURATION) + metrics.getMetric(FrameMetrics.SYNC_DURATION)
                + metrics.getMetric(FrameMetrics.COMMAND_ISSUE_DURATION) + metrics.getMetric(FrameMetrics.SWAP_BUFFERS_DURATION);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            return Math.max(cpu, metrics.getMetric(FrameMetrics.GPU_DURATION));
        }
        return cpu;
    }

    /** This frame, by the policy where it can be asked. */
    private boolean slowFrame(long work, long total) {
        if (GlassTier.Policy.available()) {
            try {
                return NightjarUiPolicyJni.probeSlow(refresh, work, total);
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return slow(work, total, allowed, budget);
    }

    /** The frame budget at {@code refreshHz}, floored at a {@link #MIN_BUDGET_HZ} frame; an unknown rate is 60 Hz. */
    static long budgetNs(float refreshHz) {
        if (GlassTier.Policy.available()) {
            try {
                return NightjarUiPolicyJni.probeBudgetNs(refreshHz);
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return budgetNsMirror(refreshHz);
    }

    /** glass_probe::budget_ns in Java. */
    static long budgetNsMirror(float refreshHz) {
        float hz = !Float.isNaN(refreshHz) && !Float.isInfinite(refreshHz) && refreshHz >= 1f
                ? Math.min(refreshHz, MIN_BUDGET_HZ) : 60f;
        return Math.round(1e9 / hz);
    }

    /** Whether a frame was over its share of the budget in work and also late on screen (glass_probe's Probe::slow). */
    static boolean slow(long work, long total, long allowed, long budget) {
        return work > allowed && total > (long) (budget * (double) LATE_FRACTION);
    }

    /** How many slow frames put the 90th percentile of {@code frames} frames over the line. */
    static int slowFramesToFail(int frames) {
        if (GlassTier.Policy.available()) {
            try {
                return NightjarUiPolicyJni.probeSlowFramesToFail(frames);
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return slowFramesToFailMirror(frames);
    }

    static int slowFramesToFailMirror(int frames) {
        if (frames <= 0) return 1;
        return frames - Math.min(frames - 1, (int) Math.ceil(frames * 0.9) - 1);
    }

    /** Whether a screen opened at {@code started} has settled by {@code now}. */
    static boolean settled(long started, long now) {
        return now - started >= SETTLE_MILLIS;
    }
}
