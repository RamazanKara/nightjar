package com.limelight.preferences;

import android.graphics.Canvas;
import android.graphics.Path;
import android.graphics.RectF;

/**
 * HDR's preview picture: a tone curve. Scene brightness runs left to right, what the screen shows runs up; the dashed
 * line is where a standard screen ends. Without HDR the amber curve clips flat at that line, a tone-mapped picture
 * bends into it with a shoulder, and HDR keeps rising over the whole range. Android offers the first and the last;
 * the shoulder is the desktop app's third choice and is drawn the same there. Illustrative only.
 */
final class PreviewTone {
    static final int SDR = 0, TONE_MAPPED = 1, HDR = 2;
    /** Where a standard screen's white sits in the picture's range. */
    static final float SDR_WHITE = 0.56f;
    /** Where a tone-mapped picture leaves the straight line for its shoulder. */
    static final float KNEE = 0.34f;
    private static final int STEPS = 28;
    private static final float MAX_ASPECT = 2.6f;

    private PreviewTone() { }

    /** How bright the screen shows a scene brightness, both 0 to 1 of the HDR range. */
    static float level(int mode, float scene) {
        switch (mode) {
            case HDR:
                return scene;
            case TONE_MAPPED:
                if (scene <= KNEE) return scene;
                float room = SDR_WHITE - KNEE;
                return KNEE + room * (1f - (float) Math.exp(-(scene - KNEE) / room));
            default:
                return Math.min(scene, SDR_WHITE);
        }
    }

    static void draw(Canvas canvas, RectF box, int from, int to, float t, PreviewInks inks) {
        float inset = 1.5f * inks.dp;
        float height = box.height() - 2f * inset;
        float width = Math.min(box.width() - 2f * inset, height * MAX_ASPECT);
        float left = box.centerX() - width / 2f, right = left + width, bottom = box.bottom - inset;
        // The reference: the axes, the scene itself, and where a standard screen ends.
        canvas.drawLine(left, bottom, right, bottom, inks.line(inks.neutral, 0.5f, 1f));
        canvas.drawLine(left, bottom, left, bottom - height, inks.line(inks.neutral, 0.5f, 1f));
        canvas.drawLine(left, bottom, right, bottom - height, inks.line(inks.neutral, 0.3f, 1f));
        inks.dashes(canvas, left, bottom - SDR_WHITE * height, right, inks.line(inks.neutral, 0.8f, 1f));
        Path path = inks.path;
        if (!inks.linesOnly) {
            trace(path, left, bottom, width, height, from, to, t);
            path.lineTo(right, bottom);
            path.lineTo(left, bottom);
            path.close();
            canvas.drawPath(path, inks.fill(inks.accent, 0.16f));
        }
        trace(path, left, bottom, width, height, from, to, t);
        canvas.drawPath(path, inks.line(inks.accent, 1f, 2f));
    }

    private static void trace(Path path, float left, float bottom, float width, float height, int from, int to, float t) {
        path.rewind();
        path.moveTo(left, bottom);
        for (int step = 1; step <= STEPS; step++) {
            float scene = step / (float) STEPS;
            float level = PreviewInks.lerp(level(from, scene), level(to, scene), t);
            path.lineTo(left + scene * width, bottom - level * height);
        }
    }
}
