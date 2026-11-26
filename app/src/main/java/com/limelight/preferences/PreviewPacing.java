package com.limelight.preferences;

import android.graphics.Canvas;
import android.graphics.RectF;

/**
 * Frame pacing's preview picture: a frame timeline. Frames reach the device unevenly (the neutral dots); the amber
 * bars are when the choice shows them, over the screen's refresh ticks. Lowest latency shows each at once, uneven;
 * the PC's frame times shows them at the game's own even pace a moment later; Balanced waits for the next refresh,
 * keeping every frame waits one more, Smoothest video the longest; the FPS limit spaces frames a little wider than
 * the refresh. Illustrative only: the positions are drawn, never measured.
 */
final class PreviewPacing {
    static final int LATENCY = 0, BALANCED = 1, BALANCED_EVERY = 2, CAP_FPS = 3, SMOOTHNESS = 4, HOST = 5;
    /** The frame_pacing values, in the order of the constants above. */
    static final String[] VALUES = {"latency", "balanced", "balanced-every", "cap-fps", "smoothness", "host"};
    static final int FRAMES = 6;
    /** The refresh ticks drawn: enough for the last frame of the choice that waits longest. */
    static final int TICKS = 8;
    /** A capped stream's frame interval, in refreshes: a little under the screen's rate, so no frame ever queues. */
    static final float CAP_INTERVAL = 1.2f;
    /** The PC's frame times add a small constant wait. */
    static final float HOST_WAIT = 0.45f;
    private static final float[] JITTER = {0f, 0.28f, -0.14f, 0.3f, 0.02f, 0.22f};
    private static final float EDGE = 0.4f;
    private static final float MAX_ASPECT = 6f;

    private PreviewPacing() { }

    /** The picture's choice for a stored value; an unknown one draws as Lowest latency, the default. */
    static int choice(String value) {
        for (int i = 0; i < VALUES.length; i++) if (VALUES[i].equals(value)) return i;
        return LATENCY;
    }

    /** When frame {@code index} reaches the device, in refresh intervals. */
    static float arrival(int index) {
        return index + JITTER[index];
    }

    /** When {@code choice} shows frame {@code index}, in refresh intervals; never before it arrived. */
    static float shown(int choice, int index) {
        switch (choice) {
            case BALANCED:
                return index + 1f;
            case BALANCED_EVERY:
                return index + 2f;
            case CAP_FPS:
                return 1f + index * CAP_INTERVAL;
            case SMOOTHNESS:
                return index + 3f;
            case HOST:
                return index + HOST_WAIT;
            default:
                return arrival(index);
        }
    }

    static void draw(Canvas canvas, RectF box, int from, int to, float t, PreviewInks inks) {
        float width = Math.min(box.width(), box.height() * MAX_ASPECT);
        float left = box.centerX() - width / 2f;
        float unit = width / (TICKS + 2f * EDGE);
        float origin = left + EDGE * unit;
        float bar = Math.min(unit * 0.44f, 9f * inks.dp);
        float height = box.height();
        float dotY = box.top + height * 0.08f;
        float barTop = box.top + height * 0.32f;
        float barBottom = box.top + height * 0.78f;
        float tickTop = box.top + height * 0.87f;
        for (int tick = 0; tick <= TICKS; tick++) {
            float x = origin + tick * unit;
            canvas.drawLine(x, tickTop, x, box.bottom, inks.line(inks.neutral, 0.6f, 1.5f));
        }
        float radius = 2f * inks.dp;
        for (int frame = 0; frame < FRAMES; frame++) {
            float arrived = origin + arrival(frame) * unit;
            float shown = origin + PreviewInks.lerp(shown(from, frame), shown(to, frame), t) * unit;
            canvas.drawLine(arrived, dotY, shown, barTop, inks.line(inks.neutral, 0.35f, 1f));
            if (inks.linesOnly) {
                canvas.drawCircle(arrived, dotY, radius, inks.line(inks.neutral, 1f, 1f));
                canvas.drawRoundRect(shown - bar / 2f, barTop, shown + bar / 2f, barBottom, radius, radius,
                        inks.line(inks.accent, 1f, 1.5f));
            } else {
                canvas.drawCircle(arrived, dotY, radius, inks.fill(inks.neutral, 0.75f));
                canvas.drawRoundRect(shown - bar / 2f, barTop, shown + bar / 2f, barBottom, radius, radius,
                        inks.fill(inks.accent, 1f));
            }
        }
    }
}
