package com.limelight.preferences;

import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;

/**
 * What a preview picture draws with: amber for what the choice gives, a neutral for the reference it is read against,
 * and whether the look wants lines only (high contrast: no tints, full-strength lines in the label colours). One set
 * per card, made once and reused by every frame, so drawing allocates nothing.
 */
final class PreviewInks {
    private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint line = new Paint(Paint.ANTI_ALIAS_FLAG);
    /** A scratch path for the picture that needs one; rewound by its user. */
    final Path path = new Path();
    /** What the choice gives. */
    int accent;
    /** The reference: the screen, its refresh, the true edge. */
    int neutral;
    boolean linesOnly;
    /** Pixels per dp of the card's own (interface-scaled) resources. */
    float dp = 1f;
    /** Below 1 on a row that is switched off. */
    float dim = 1f;

    PreviewInks() {
        fill.setStyle(Paint.Style.FILL);
        line.setStyle(Paint.Style.STROKE);
        line.setStrokeCap(Paint.Cap.ROUND);
        line.setStrokeJoin(Paint.Join.ROUND);
    }

    Paint fill(int color, float alpha) {
        fill.setColor(faded(color, alpha * dim));
        return fill;
    }

    /** A line {@code width} dp wide; high contrast draws every line at full strength. */
    Paint line(int color, float alpha, float width) {
        line.setColor(faded(color, (linesOnly ? 1f : alpha) * dim));
        line.setStrokeWidth(width * dp);
        return line;
    }

    /** A dashed horizontal line, drawn as its dashes: a path effect is not honoured for lines on every device. */
    void dashes(Canvas canvas, float left, float y, float right, Paint paint) {
        float dash = 4f * dp, gap = 3f * dp;
        for (float x = left; x < right; x += dash + gap) canvas.drawLine(x, y, Math.min(right, x + dash), y, paint);
    }

    static int faded(int color, float alpha) {
        int value = Math.round((color >>> 24) * Math.max(0f, Math.min(1f, alpha)));
        return (value << 24) | (color & 0x00FFFFFF);
    }

    static float lerp(float from, float to, float t) {
        return from + (to - from) * t;
    }
}
