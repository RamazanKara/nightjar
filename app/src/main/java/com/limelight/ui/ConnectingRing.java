package com.limelight.ui;

import android.animation.ObjectAnimator;
import android.animation.ValueAnimator;
import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.SweepGradient;
import android.util.AttributeSet;
import android.view.View;
import android.view.animation.LinearInterpolator;
import android.widget.FrameLayout;

import androidx.core.content.res.ResourcesCompat;

import com.google.android.material.color.MaterialColors;
import com.limelight.R;

/**
 * The one thin ring around a cover while a connection is under way: a faint track and a short arc of light turning on it, the
 * only thing that moves. The cover is this view's child, in the middle. The arc is one view turned about the middle, so a frame
 * touches a transform and nothing else; it stops when the window is away, and with reduced motion it rests.
 */
public class ConnectingRing extends FrameLayout {
    private static final long TURN_MS = 2400;
    /** The arc's length, and where its head rests when it does not move: at the top. */
    static final float ARC_DEGREES = 84f;
    private static final float REST_DEGREES = 0f;

    private final Paint track = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Arc arc;
    private final float inset;
    private ObjectAnimator turning;
    private boolean moving;
    private boolean bare;
    private boolean windowVisible = true;

    public ConnectingRing(Context context) {
        this(context, null);
    }

    public ConnectingRing(Context context, AttributeSet attrs) {
        super(context, attrs);
        float density = getResources().getDisplayMetrics().density;
        AppTheme.Look look = AppTheme.look(context);
        boolean highContrast = look == AppTheme.Look.HIGH_CONTRAST;
        float width = density * (highContrast ? 2f : 1.5f);
        inset = density * 4f;
        track.setStyle(Paint.Style.STROKE);
        track.setStrokeWidth(Math.max(1f, highContrast ? width : density));
        track.setColor(ResourcesCompat.getColor(getResources(), highContrast ? R.color.nj_hc_color_rim
                : look == AppTheme.Look.DAY ? R.color.nj_day_sky_ring : R.color.nj_sky_ring, null));
        int ink = MaterialColors.getColor(this, com.google.android.material.R.attr.colorOnSurface, 0xFFF5F7FA);
        arc = new Arc(context, ink, width, highContrast);
        arc.setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
        addView(arc, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
        setClipChildren(false);
        setWillNotDraw(false);
    }

    /** No ring: only what sits in the middle. */
    public void setBare(boolean bare) {
        this.bare = bare;
        arc.setVisibility(bare ? GONE : VISIBLE);
        invalidate();
    }

    /** Starts the arc, or stops it where it is. With reduced motion it rests. */
    public void setMoving(boolean moving) {
        this.moving = moving;
        updateMotion();
    }

    public boolean isTurning() {
        return moving && shouldRun();
    }

    private boolean shouldRun() {
        return moving && windowVisible && isAttachedToWindow() && isShown() && !Motion.reduced(getContext());
    }

    /** A stopped activity's window goes away with its views still "shown": nothing else would stop the turn. */
    @Override
    protected void onWindowVisibilityChanged(int visibility) {
        super.onWindowVisibilityChanged(visibility);
        windowVisible = visibility == VISIBLE;
        if (arc != null) updateMotion();
    }

    /** The ring's radius in a view of this size: it keeps its stroke and the arc's round head inside the view. */
    static float radius(int width, int height, float inset) {
        return Math.max(0f, Math.min(width, height) / 2f - inset);
    }

    @Override
    protected void onLayout(boolean changed, int left, int top, int right, int bottom) {
        super.onLayout(changed, left, top, right, bottom);
        // Over the cover's shadow, which reaches past the cover.
        arc.bringToFront();
        arc.setPivotX((right - left) / 2f);
        arc.setPivotY((bottom - top) / 2f);
        arc.setRadius(radius(right - left, bottom - top, inset));
        if (turning == null) arc.setRotation(REST_DEGREES);
    }

    @Override
    protected void onDraw(Canvas canvas) {
        if (bare) return;
        canvas.drawCircle(getWidth() / 2f, getHeight() / 2f, radius(getWidth(), getHeight(), inset), track);
    }

    @Override
    protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        updateMotion();
    }

    @Override
    protected void onDetachedFromWindow() {
        stopTurning();
        super.onDetachedFromWindow();
    }

    @Override
    protected void onVisibilityChanged(View changedView, int visibility) {
        super.onVisibilityChanged(changedView, visibility);
        // The constructor's addView can land here before the fields are set.
        if (arc != null) updateMotion();
    }

    private void updateMotion() {
        if (!shouldRun()) {
            stopTurning();
        } else if (turning == null && !"robolectric".equals(android.os.Build.FINGERPRINT)) {
            // An animator that never ends would keep a test's looper from ever idling.
            float from = arc.getRotation();
            turning = ObjectAnimator.ofFloat(arc, View.ROTATION, from, from + 360f);
            turning.setDuration(TURN_MS);
            turning.setInterpolator(new LinearInterpolator());
            turning.setRepeatCount(ValueAnimator.INFINITE);
            turning.start();
        }
    }

    private void stopTurning() {
        if (turning != null) {
            turning.cancel();
            turning = null;
        }
    }

    /** The arc itself, drawn once: a tail that fades in toward a round head at the top. Its parent turns it. */
    private static final class Arc extends View {
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Matrix turn = new Matrix();
        private final int ink;
        private final boolean solid;
        private float radius;
        private float shadedFor = -1f;

        Arc(Context context, int ink, float width, boolean solid) {
            super(context);
            this.ink = ink;
            this.solid = solid;
            paint.setStyle(Paint.Style.STROKE);
            paint.setStrokeWidth(width);
            paint.setStrokeCap(Paint.Cap.ROUND);
            paint.setColor(ink);
        }

        void setRadius(float radius) {
            if (this.radius == radius) return;
            this.radius = radius;
            invalidate();
        }

        @Override
        protected void onDraw(Canvas canvas) {
            if (radius <= 0f) return;
            float cx = getWidth() / 2f;
            float cy = getHeight() / 2f;
            if (!solid && shadedFor != radius) {
                shadedFor = radius;
                float part = ARC_DEGREES / 360f;
                // The sweep starts at three o'clock: turned so the arc ends, brightest, at the top. The ink runs a little
                // past the end, under the round head.
                int clear = ink & 0x00FFFFFF;
                SweepGradient fade = new SweepGradient(cx, cy, new int[] {clear, ink, ink, clear},
                        new float[] {0f, part, part + .02f, part + .02f});
                turn.setRotate(-90f - ARC_DEGREES, cx, cy);
                fade.setLocalMatrix(turn);
                paint.setShader(fade);
            }
            canvas.drawArc(cx - radius, cy - radius, cx + radius, cy + radius, -90f - ARC_DEGREES, ARC_DEGREES, false, paint);
        }
    }
}
