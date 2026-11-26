package com.limelight.preferences;

import android.graphics.Canvas;
import android.graphics.RectF;

/**
 * Upscaling's preview picture: a diagonal edge on a pixel grid. The neutral line is the true edge; the amber pixels
 * are what the choice makes of it. Off and Smooth spread it over a wide soft band, the sharp modes pull it back to
 * the line, and the sharpness slider narrows the band further. High contrast draws each pixel as an outline whose
 * size is its share of the edge. Illustrative only.
 */
final class PreviewEdge {
    static final int ROWS = 5;
    private static final int MAX_COLUMNS = 16;
    /** The band of a sharp mode at sharpness 0, as a softness; at 100 it is 0. */
    static final float SHARP_SOFTEST = 0.5f;
    /** Plain enlarging without the extra smoothing pass. */
    static final float OFF_SOFTNESS = 0.82f;
    private static final float CRISP_BAND = 0.16f, SOFT_BAND = 2.6f;

    private PreviewEdge() { }

    /** How soft the edge is drawn, 0 (crisp) to 1, for an upscaling_mode value and the sharpness percent. */
    static float softness(String mode, int sharpness) {
        if ("fsr1".equals(mode) || "sgsr1".equals(mode)) {
            return SHARP_SOFTEST * (1f - Math.max(0, Math.min(100, sharpness)) / 100f);
        }
        return "bilinear".equals(mode) ? 1f : OFF_SOFTNESS;
    }

    /** A pixel's share of the picture's bright side, 0 to 1, from its centre's distance to the edge in pixels. */
    static float coverage(float distance, float softness) {
        float band = CRISP_BAND + (SOFT_BAND - CRISP_BAND) * Math.max(0f, Math.min(1f, softness));
        return Math.max(0f, Math.min(1f, 0.5f + distance / band));
    }

    /**
     * The distance in pixels from the centre of a pixel to the edge, which runs from the grid's bottom left corner to
     * its top right one; positive below it, on the bright side.
     */
    static float distance(int column, int row, int columns) {
        float slope = ROWS / (float) columns;
        float edge = ROWS - (column + 0.5f) * slope;
        return (row + 0.5f - edge) / (float) Math.sqrt(1f + slope * slope);
    }

    /** The grid's columns in a box: square pixels, as many as fit up to a cap. */
    static int columns(float width, float height) {
        float cell = height / ROWS;
        return cell <= 0f ? ROWS : Math.max(ROWS, Math.min(MAX_COLUMNS, (int) (width / cell)));
    }

    static void draw(Canvas canvas, RectF box, float from, float to, float t, PreviewInks inks) {
        float softness = PreviewInks.lerp(from, to, t);
        int columns = columns(box.width(), box.height());
        float cell = Math.min(box.height() / ROWS, box.width() / columns);
        float left = box.centerX() - columns * cell / 2f, top = box.centerY() - ROWS * cell / 2f;
        float gap = Math.max(1f, inks.dp) / 2f, radius = 1.5f * inks.dp;
        for (int row = 0; row < ROWS; row++) {
            for (int column = 0; column < columns; column++) {
                float x = left + column * cell, y = top + row * cell;
                float share = coverage(distance(column, row, columns), softness);
                if (inks.linesOnly) {
                    if (share < 0.08f) continue;
                    float inset = gap + (cell - 2f * gap) * (1f - share) / 2f;
                    canvas.drawRect(x + inset, y + inset, x + cell - inset, y + cell - inset, inks.line(inks.accent, 1f, 1.5f));
                } else {
                    canvas.drawRoundRect(x + gap, y + gap, x + cell - gap, y + cell - gap, radius, radius,
                            inks.fill(inks.neutral, 0.1f));
                    if (share > 0.01f) {
                        canvas.drawRoundRect(x + gap, y + gap, x + cell - gap, y + cell - gap, radius, radius,
                                inks.fill(inks.accent, share));
                    }
                }
            }
        }
        canvas.drawLine(left, top + ROWS * cell, left + columns * cell, top, inks.line(inks.neutral, 0.7f, 1f));
    }
}
