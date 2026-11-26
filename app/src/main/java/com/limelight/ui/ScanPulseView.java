package com.limelight.ui;

import android.animation.ValueAnimator;
import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.util.AttributeSet;
import android.view.View;
import android.view.animation.LinearInterpolator;

import androidx.core.content.res.ResourcesCompat;

import com.limelight.R;

/**
 * The scan pulse of first run: three thin rings leaving one point in the middle of the stage, each growing and fading, one
 * after the other, while the network is searched for PCs. It moves only while it is asked to, shown and in a visible window;
 * otherwise, and with reduced motion, the three rings stand still, and in high contrast it is one plain ring. A frame is three
 * circles: no layer, no allocation.
 */
public class ScanPulseView extends View {
    static final int RINGS = 3;
    /** One ring's way from the point to the rim. */
    static final long LOOP_MS = 2400;
    /** The rings at their strongest, against full ink: the design's ring colour is that of still lines, a moving one needs more. */
    static final float PEAK = .34f;
    /** Where a ring is strongest on its way, and where the still rings stand: each fainter than the one inside it. */
    private static final float PEAK_AT = .3f;
    private static final float[] STILL = {.25f, .5f, .8f};

    private final Paint ring = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final boolean highContrast;
    private final int ink;
    private ValueAnimator loop;
    private boolean scanning;
    private boolean windowVisible = true;
    private float time;

    public ScanPulseView(Context context) {
        this(context, null);
    }

    public ScanPulseView(Context context, AttributeSet attrs) {
        super(context, attrs);
        float density = getResources().getDisplayMetrics().density;
        AppTheme.Look look = AppTheme.look(context);
        highContrast = look == AppTheme.Look.HIGH_CONTRAST;
        ink = ResourcesCompat.getColor(getResources(), highContrast ? R.color.nj_hc_color_rim
                : look == AppTheme.Look.DAY ? R.color.nj_day_sky_ring : R.color.nj_sky_ring, null) | 0xFF000000;
        ring.setStyle(Paint.Style.STROKE);
        ring.setStrokeWidth(density * (highContrast ? 2f : 1.5f));
        ring.setColor(ink);
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
    }

    /** Where ring {@code index} is on its way at {@code time} of the loop, both 0 to 1: the rings leave a third of a loop apart. */
    static float phase(float time, int index) {
        float at = time + index / (float) RINGS;
        return at - (float) Math.floor(at);
    }

    /** A ring's radius on its way: fast out of the point, slower toward the rim. */
    static float radius(float phase, float rim) {
        float left = 1f - phase;
        return rim * (1f - left * left);
    }

    /** A ring's strength on its way: it comes out of nothing, is strongest early and is gone at the rim. */
    static float strength(float phase) {
        if (phase <= 0f || phase >= 1f) return 0f;
        if (phase < PEAK_AT) {
            float in = phase / PEAK_AT;
            return PEAK * in * in * (3f - 2f * in);
        }
        float out = (1f - phase) / (1f - PEAK_AT);
        return PEAK * out * out;
    }

    /** The rim the rings reach in a view of this size: the shorter side, with the stroke inside it. */
    static float rim(int width, int height, float stroke) {
        return Math.max(0f, Math.min(width, height) / 2f - stroke);
    }

    /**
     * The side of the square the pulse takes where {@code width} and {@code room} are free: as much as it is given up to
     * {@code most}, and nothing where it would be smaller than {@code least}, a mark too small to read as a search.
     */
    public static int side(int width, int room, int most, int least) {
        int side = Math.min(most, Math.min(width, room));
        return side < least ? 0 : side;
    }

    /** Starts the pulse, or brings it to rest: discovery stopped, or a PC was found. */
    public void setScanning(boolean scanning) {
        if (this.scanning == scanning) return;
        this.scanning = scanning;
        updateMotion();
    }

    /** Whether the rings move now. */
    public boolean isPulsing() {
        return shouldRun();
    }

    private boolean shouldRun() {
        return scanning && !highContrast && windowVisible && isAttachedToWindow() && isShown() && !Motion.reduced(getContext());
    }

    @Override
    protected void onWindowVisibilityChanged(int visibility) {
        super.onWindowVisibilityChanged(visibility);
        windowVisible = visibility == VISIBLE;
        updateMotion();
    }

    @Override
    protected void onVisibilityChanged(View changedView, int visibility) {
        super.onVisibilityChanged(changedView, visibility);
        // The constructor's super call can land here before the fields are set.
        if (ring != null) updateMotion();
    }

    @Override
    protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        updateMotion();
    }

    @Override
    protected void onDetachedFromWindow() {
        stop();
        super.onDetachedFromWindow();
    }

    private void updateMotion() {
        if (!shouldRun()) {
            stop();
        } else if (loop == null && !"robolectric".equals(android.os.Build.FINGERPRINT)) {
            // An animator that never ends would keep a test's looper from ever idling.
            loop = ValueAnimator.ofFloat(0f, 1f);
            loop.setDuration(LOOP_MS);
            loop.setInterpolator(new LinearInterpolator());
            loop.setRepeatCount(ValueAnimator.INFINITE);
            loop.addUpdateListener(animation -> {
                time = animation.getAnimatedFraction();
                invalidate();
            });
            loop.start();
        }
    }

    private void stop() {
        if (loop == null) return;
        loop.cancel();
        loop = null;
        invalidate();
    }

    @Override
    protected void onDraw(Canvas canvas) {
        float cx = getWidth() / 2f;
        float cy = getHeight() / 2f;
        float rim = rim(getWidth(), getHeight(), ring.getStrokeWidth());
        if (rim <= 0f) return;
        if (highContrast) {
            ring.setAlpha(255);
            canvas.drawCircle(cx, cy, radius(STILL[1], rim), ring);
            return;
        }
        boolean moving = loop != null;
        for (int i = 0; i < RINGS; i++) {
            float phase = moving ? phase(time, i) : STILL[i];
            int alpha = Math.round(255 * (moving ? strength(phase) : PEAK * (1f - i / (float) RINGS)));
            if (alpha == 0) continue;
            ring.setAlpha(alpha);
            canvas.drawCircle(cx, cy, radius(phase, rim), ring);
        }
    }
}
