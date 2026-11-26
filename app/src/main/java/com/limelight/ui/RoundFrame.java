package com.limelight.ui;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Path;
import android.graphics.RectF;
import android.util.AttributeSet;
import android.view.View;
import android.view.ViewParent;
import android.widget.FrameLayout;

import com.google.android.material.card.MaterialCardView;

/**
 * A frame whose round corners a picture of it has too. The screen's renderer clips a view to its outline, or to the outline
 * of the card it lies in; a canvas without a GPU (a picture drawn of the view, a screenshot made on the JVM) knows no
 * outline, and the frame's square corners would show behind the round rim. There this frame clips itself by path: to its
 * own radius once {@link #setRadius} gave it one, else to the shape of the card around it. What the screen draws is
 * unchanged: the outline still does the clipping there.
 */
public class RoundFrame extends FrameLayout {
    private final Path clip = new Path();
    private final RectF rect = new RectF();
    private float radius = -1f;

    public RoundFrame(Context context) {
        this(context, null);
    }

    public RoundFrame(Context context, AttributeSet attrs) {
        super(context, attrs);
        // The clip is set in draw(), which a frame without a background would be spared.
        setWillNotDraw(false);
    }

    /** The frame's own corner radius in pixels: the one its outline has on the screen. */
    public void setRadius(float px) {
        radius = px;
        invalidate();
    }

    /**
     * The round rectangle a canvas without a GPU is clipped to, in this view's coordinates.
     *
     * @return its corner radius, or a negative number when the frame has no round shape to keep
     */
    float shape(RectF into) {
        if (radius >= 0f) {
            into.set(0, 0, getWidth(), getHeight());
            return radius;
        }
        // Where this frame lies in its card: the card's shape is what the screen clips it to.
        int x = getLeft();
        int y = getTop();
        ViewParent parent = getParent();
        while (parent instanceof View && !(parent instanceof MaterialCardView)) {
            x += ((View) parent).getLeft();
            y += ((View) parent).getTop();
            parent = parent.getParent();
        }
        if (!(parent instanceof MaterialCardView)) return -1f;
        MaterialCardView card = (MaterialCardView) parent;
        into.set(-x, -y, card.getWidth() - x, card.getHeight() - y);
        return card.getRadius();
    }

    @Override
    public void draw(Canvas canvas) {
        float round = canvas.isHardwareAccelerated() ? -1f : shape(rect);
        if (round < 0f) {
            super.draw(canvas);
            return;
        }
        int saved = canvas.save();
        clip.rewind();
        clip.addRoundRect(rect, round, round, Path.Direction.CW);
        canvas.clipPath(clip);
        super.draw(canvas);
        canvas.restoreToCount(saved);
    }
}
