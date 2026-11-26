/**
 * Where the Control Center stands in its window ({@link ControlCenterDock}), as nightjar-policy's
 * {@code control_center::placement} says: a dock at the end edge, as tall as the screen less its margins, on everything
 * wide; a sheet from the bottom on an upright phone. It keeps the panel clear of the system bars, a cutout and a TV's
 * overscan, draws the scrim around it, closes on a tap beside it, and on the sheet follows a finger: a drag down from
 * the grabber, the rail or the footer, or one the page hands over once it is scrolled to its top, moves the sheet and
 * past a threshold closes it; a swipe across the page steps to the next module.
 */
package com.limelight.ui;

import android.animation.Animator;
import android.animation.AnimatorListenerAdapter;
import android.animation.ValueAnimator;
import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Path;
import android.graphics.Rect;
import android.graphics.RectF;
import android.view.MotionEvent;
import android.view.VelocityTracker;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.ViewGroup;

import androidx.core.content.ContextCompat;
import androidx.core.graphics.Insets;
import androidx.core.view.NestedScrollingParent2;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowInsetsCompat;

import com.google.android.material.slider.Slider;
import com.limelight.R;

final class ControlCenterFrame extends ViewGroup implements NestedScrollingParent2 {
    /** What the frame tells the Control Center. */
    interface Listener {
        /** A tap beside the panel. */
        void onOutsideTap();

        /** The sheet was dragged or flung off the screen. */
        void onDraggedAway();

        /** A swipe across the page: to the next module, or to the one before. */
        void onSwipe(boolean forward);

        /** The panel moved or changed its size: what lies under it follows. */
        void onPanelMoved();

        /** The window has another size: the placement may be another now. */
        void onResized(int width, int height);
    }

    /** The share of the window's height the sheet takes, whatever its page holds: it does not jump between modules. */
    private static final float SHEET_SHARE = 0.72f;

    private final View panel;
    private final Listener listener;
    private final Rect insets = new Rect();
    private final int scrim, slop, margin, safeTv, topGap, dismissAt, flingAway;
    private final long settleMs;
    private final Path hole = new Path();
    private final RectF holeRect = new RectF();
    private final int[] at = new int[2], mine = new int[2];
    private boolean sheet, compact, television;
    private float[] radii = new float[8];
    private View page;
    // The sheet's way down from where it rests, in pixels.
    private float offset;
    private ValueAnimator settle;
    private boolean gone;
    private VelocityTracker velocity;
    private float downX, downY, lastY;
    private boolean downOutside, downOnPage, downInPage, dragging, swiping, nested;

    ControlCenterFrame(Context context, View panel, Listener listener) {
        super(context);
        this.panel = panel;
        this.listener = listener;
        android.content.res.Resources res = context.getResources();
        scrim = ContextCompat.getColor(context, R.color.nj_surface_control_center_scrim);
        slop = ViewConfiguration.get(context).getScaledTouchSlop();
        margin = res.getDimensionPixelSize(R.dimen.control_center_margin);
        safeTv = res.getDimensionPixelSize(R.dimen.nj_layout_safe_tv);
        topGap = res.getDimensionPixelSize(R.dimen.control_center_sheet_top_gap);
        dismissAt = res.getDimensionPixelSize(R.dimen.control_center_sheet_dismiss);
        flingAway = Math.round(1200 * res.getDisplayMetrics().density);
        settleMs = Motion.reduced(context) ? 0 : res.getInteger(R.integer.motion_base);
        setWillNotDraw(false);
        setClipChildren(false);
        addView(panel);
        ViewCompat.setOnApplyWindowInsetsListener(this, (view, windowInsets) -> {
            Insets bars = windowInsets.getInsets(WindowInsetsCompat.Type.systemBars() | WindowInsetsCompat.Type.displayCutout());
            setInsets(bars.left, bars.top, bars.right, bars.bottom);
            return windowInsets;
        });
    }

    /** What the window keeps for the system bars and a cutout: the panel stays clear of it. */
    void setInsets(int left, int top, int right, int bottom) {
        if (insets.left == left && insets.top == top && insets.right == right && insets.bottom == bottom) return;
        insets.set(left, top, right, bottom);
        padPanel();
        requestLayout();
    }

    // The sheet runs to the window's bottom edge; its footer stays above the gesture bar.
    private void padPanel() {
        int bottom = sheet ? insets.bottom : 0;
        if (panel.getPaddingBottom() != bottom) panel.setPadding(0, 0, 0, bottom);
    }

    /**
     * A sheet from the bottom, or a dock at the end edge: {@code compact} at the handheld's width, {@code television}
     * inside a TV's safe area. {@code radii} are the panel's corners, {@code page} the part of it that scrolls.
     */
    void place(boolean sheet, boolean compact, boolean television, float[] radii, View page) {
        this.sheet = sheet;
        this.compact = compact;
        this.television = television;
        this.radii = radii.clone();
        this.page = page;
        cancelSettle();
        offset = 0;
        gone = false;
        panel.setTranslationY(0);
        padPanel();
        requestLayout();
        invalidate();
    }

    boolean sheet() {
        return sheet;
    }

    /** How far the sheet is dragged down now, in pixels. */
    float offset() {
        return offset;
    }

    private static int exactly(int size) {
        return MeasureSpec.makeMeasureSpec(Math.max(0, size), MeasureSpec.EXACTLY);
    }

    /** The dock's width in a window this wide: the token's, or what the window leaves. */
    int dockWidth(int width) {
        int gap = television ? safeTv : margin;
        int wanted = getResources().getDimensionPixelSize(compact ? R.dimen.nj_size_control_center_width_compact
                : R.dimen.nj_size_control_center_width);
        return Math.min(wanted, width - insets.left - insets.right - gap * 2);
    }

    @Override
    protected void onMeasure(int widthSpec, int heightSpec) {
        int width = MeasureSpec.getSize(widthSpec), height = MeasureSpec.getSize(heightSpec);
        setMeasuredDimension(width, height);
        if (sheet) {
            int room = height - insets.top - topGap;
            panel.measure(exactly(width - insets.left - insets.right), exactly(Math.min(room, Math.round(height * SHEET_SHARE))));
        } else {
            int gap = television ? safeTv : margin;
            panel.measure(exactly(dockWidth(width)), exactly(height - insets.top - insets.bottom - gap * 2));
        }
    }

    @Override
    protected void onLayout(boolean changed, int l, int t, int r, int b) {
        int width = r - l, height = b - t;
        int wide = panel.getMeasuredWidth(), tall = panel.getMeasuredHeight();
        if (sheet) {
            panel.layout(insets.left, height - tall, insets.left + wide, height);
        } else {
            int gap = television ? safeTv : margin;
            boolean rtl = getLayoutDirection() == LAYOUT_DIRECTION_RTL;
            int left = rtl ? insets.left + gap : width - insets.right - gap - wide;
            panel.layout(left, insets.top + gap, left + wide, insets.top + gap + tall);
        }
        listener.onPanelMoved();
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldW, int oldH) {
        super.onSizeChanged(w, h, oldW, oldH);
        if (oldW != 0 || oldH != 0) listener.onResized(w, h);
    }

    // The scrim around the panel, never under it: the panel's own fill is all that lies over the game there.
    @Override
    protected void onDraw(Canvas canvas) {
        if (android.graphics.Color.alpha(scrim) == 0) return;
        int save = canvas.save();
        holeRect.set(panel.getLeft(), panel.getTop() + offset, panel.getRight(), panel.getBottom() + offset);
        hole.rewind();
        hole.addRoundRect(holeRect, radii, Path.Direction.CW);
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) canvas.clipOutPath(hole);
        else canvas.clipPath(hole, android.graphics.Region.Op.DIFFERENCE);
        canvas.drawColor(scrim);
        canvas.restoreToCount(save);
    }

    private boolean inPanel(float x, float y) {
        return x >= panel.getLeft() && x < panel.getRight() && y >= panel.getTop() + offset && y < panel.getBottom() + offset;
    }

    private boolean inside(View view, float x, float y) {
        if (view == null || !view.isShown()) return false;
        getLocationInWindow(mine);
        view.getLocationInWindow(at);
        float wx = x + mine[0], wy = y + mine[1];
        return wx >= at[0] && wx < at[0] + view.getWidth() && wy >= at[1] && wy < at[1] + view.getHeight();
    }

    // A slider's own drag is sideways: a finger that starts on one never steps to another module.
    private boolean onSlider(View view, float x, float y) {
        if (view.getVisibility() != VISIBLE) return false;
        if (view instanceof Slider) return inside(view, x, y);
        if (view instanceof ViewGroup group) {
            for (int i = 0; i < group.getChildCount(); i++) if (onSlider(group.getChildAt(i), x, y)) return true;
        }
        return false;
    }

    private void track(MotionEvent event) {
        if (velocity == null) velocity = VelocityTracker.obtain();
        velocity.addMovement(event);
    }

    private void down(MotionEvent event) {
        downX = event.getX();
        downY = lastY = event.getY();
        downOutside = !inPanel(downX, downY);
        downInPage = !downOutside && inside(page, downX, downY);
        downOnPage = downInPage && !onSlider(panel, downX, downY);
        dragging = swiping = false;
        cancelSettle();
        if (velocity != null) velocity.clear();
        track(event);
    }

    // Whether the finger's move became the sheet's drag or the page's swipe.
    private boolean claims(MotionEvent event) {
        if (!sheet || downOutside || dragging || swiping) return dragging || swiping;
        float dx = event.getX() - downX, dy = event.getY() - downY;
        // A drag that starts in the page is the page's own scroll, which hands over what it cannot use (onNestedScroll).
        if (!downInPage && dy > slop && Math.abs(dy) > Math.abs(dx)) {
            dragging = true;
            lastY = event.getY();
        } else if (downOnPage && Math.abs(dx) > slop * 2 && Math.abs(dx) > Math.abs(dy) * 2) {
            swiping = true;
        }
        return dragging || swiping;
    }

    @Override
    public boolean onInterceptTouchEvent(MotionEvent event) {
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                down(event);
                return downOutside;
            case MotionEvent.ACTION_MOVE:
                track(event);
                return downOutside || claims(event);
            default:
                return false;
        }
    }

    @Override
    public boolean onTouchEvent(MotionEvent event) {
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                // No child took it: beside the panel, or on a part of it with nothing to press.
                if (!dragging && !swiping && event.getX() == downX && event.getY() == downY) return true;
                down(event);
                return true;
            case MotionEvent.ACTION_MOVE:
                track(event);
                if (claims(event) && dragging) {
                    dragBy(event.getY() - lastY);
                    lastY = event.getY();
                }
                return true;
            case MotionEvent.ACTION_UP:
                track(event);
                if (dragging) {
                    velocity.computeCurrentVelocity(1000);
                    release(velocity.getYVelocity());
                } else if (swiping) {
                    boolean towardStart = event.getX() < downX;
                    listener.onSwipe(towardStart != (getLayoutDirection() == LAYOUT_DIRECTION_RTL));
                } else if (downOutside && !inPanel(event.getX(), event.getY())
                        && Math.abs(event.getX() - downX) <= slop && Math.abs(event.getY() - downY) <= slop) {
                    listener.onOutsideTap();
                }
                dragging = swiping = false;
                return true;
            case MotionEvent.ACTION_CANCEL:
                if (dragging) release(0);
                dragging = swiping = false;
                return true;
            default:
                return true;
        }
    }

    /** Moves the sheet with a finger: down by {@code dy} pixels, or back up, never above where it rests. */
    void dragBy(float dy) {
        if (!sheet || gone) return;
        cancelSettle();
        moveTo(Math.max(0f, offset + dy));
    }

    private void moveTo(float to) {
        if (to == offset) return;
        offset = to;
        panel.setTranslationY(to);
        invalidate();
        listener.onPanelMoved();
    }

    /**
     * The finger let go at {@code velocityY} pixels a second, down being positive: past the threshold or flung down the
     * sheet leaves and the Control Center closes; else it returns to where it rests.
     */
    void release(float velocityY) {
        if (!sheet || gone) return;
        boolean away = velocityY > flingAway || (offset > dismissAt && velocityY > -flingAway);
        if (!away && offset == 0f) return;
        float to = away ? panel.getHeight() : 0f;
        if (away) gone = true;
        cancelSettle();
        if (settleMs <= 0 || to == offset) {
            moveTo(to);
            if (away) listener.onDraggedAway();
            return;
        }
        settle = ValueAnimator.ofFloat(offset, to);
        settle.setDuration(settleMs);
        settle.addUpdateListener(animation -> moveTo((float) animation.getAnimatedValue()));
        settle.addListener(new AnimatorListenerAdapter() {
            private boolean cancelled;

            @Override public void onAnimationCancel(Animator animation) {
                cancelled = true;
            }

            @Override public void onAnimationEnd(Animator animation) {
                settle = null;
                if (away && !cancelled) listener.onDraggedAway();
            }
        });
        settle.start();
    }

    private void cancelSettle() {
        if (settle == null) return;
        ValueAnimator running = settle;
        settle = null;
        running.cancel();
    }

    @Override
    protected void onDetachedFromWindow() {
        cancelSettle();
        if (velocity != null) {
            velocity.recycle();
            velocity = null;
        }
        super.onDetachedFromWindow();
    }

    // The page's scroll hands over what it cannot use: at its top, a finger still pulling down moves the sheet.

    @Override
    public boolean onStartNestedScroll(View child, View target, int axes, int type) {
        return sheet && !gone && type == ViewCompat.TYPE_TOUCH && (axes & ViewCompat.SCROLL_AXIS_VERTICAL) != 0;
    }

    @Override
    public void onNestedScrollAccepted(View child, View target, int axes, int type) {
        cancelSettle();
        nested = true;
    }

    @Override
    public void onStopNestedScroll(View target, int type) {
        if (!nested) return;
        nested = false;
        if (offset > 0f && settle == null) release(0);
    }

    @Override
    public void onNestedScroll(View target, int dxConsumed, int dyConsumed, int dxUnconsumed, int dyUnconsumed, int type) {
        if (nested && dyUnconsumed < 0) dragBy(-dyUnconsumed);
    }

    @Override
    public void onNestedPreScroll(View target, int dx, int dy, int[] consumed, int type) {
        // On its way back up the sheet takes the finger's move before the page scrolls again.
        if (!nested || offset <= 0f || dy <= 0) return;
        int take = (int) Math.min(dy, offset);
        dragBy(-take);
        consumed[1] = take;
    }

    @Override
    public boolean onNestedPreFling(View target, float velocityX, float velocityY) {
        if (offset <= 0f) return false;
        // A scroll's fling counts up for a finger moving up.
        release(-velocityY);
        return true;
    }
}
