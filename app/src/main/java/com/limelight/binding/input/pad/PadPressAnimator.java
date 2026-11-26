package com.limelight.binding.input.pad;

import android.view.View;
import android.view.animation.AnimationUtils;

import java.util.Arrays;

/**
 * Tracks a {@link PadControlView}'s pressed parts, eases released parts back to rest over
 * {@link PadPainter#RELEASE_MS}, coordinates its resting/pressed alpha with its stick knob and glow layers, and redraws
 * the control's own {@link PadControlView.HaloLayer} when its glow changes. At opacity 0 it only follows the press:
 * nothing eases and nothing is redrawn.
 */
final class PadPressAnimator {
    private final PadControlView view;
    private final PadPainter painter;
    private final PadPainter.State state;
    private float restingAlpha = 1;
    /** The parts held at the last look ({@link PadPainter#heldParts}), and when each let go while easing back, else 0. */
    private int heldParts;
    private final long[] releasedAt = new long[PadPainter.PARTS];
    private final Runnable easing = this::ease;

    PadPressAnimator(PadControlView view, PadPainter painter, PadPainter.State state) {
        this.view = view;
        this.painter = painter;
        this.state = state;
    }

    /** Redraws this control's glow, which only a visible control has. */
    void invalidateHalo() {
        if (restingAlpha > 0) view.haloLayer().invalidate();
    }

    float restingAlpha() {
        return restingAlpha;
    }

    /**
     * The alpha at rest; a press shows at least 60 % so a faint control still answers visibly. At 0, the opacity
     * setting's 0 %, a control stays invisible even while pressed: the player chose controls nobody sees, so a press
     * draws nothing, not the control, a stick's knob, a touchpad's finger nor the pressed amber.
     */
    void setRestingAlpha(float alpha) {
        boolean shown = restingAlpha > 0;
        restingAlpha = alpha;
        applyAlpha();
        // What was drawn must go, and what was skipped must come: the views draw nothing at 0.
        if (shown != alpha > 0) view.redrawAll();
    }

    void applyAlpha() {
        trackPress();
        showAlpha();
    }

    private void showAlpha() {
        boolean active = state.flags != 0 || state.pressed || state.stickActive;
        float alpha = PadControlView.alpha(restingAlpha, active ? 1 : easingLevel());
        show(view, alpha);
        show(view.knobLayer(), alpha);
        show(view.haloLayer(), alpha);
    }

    private static void show(View layer, float alpha) {
        if (layer != null && Float.compare(layer.getAlpha(), alpha) != 0) layer.setAlpha(alpha);
    }

    /** The deepest press still easing back, or 0. */
    private float easingLevel() {
        float level = 0;
        for (int i = 0; i < PadPainter.PARTS; i++) {
            if (releasedAt[i] != 0) level = Math.max(level, state.press[i]);
        }
        return level;
    }

    /**
     * Follows which parts are held: a part going down shows pressed at once, a part let go eases back over the next
     * frames ({@link #ease}). At opacity 0 a part only goes in and out, as nothing of it is drawn.
     */
    private void trackPress() {
        int held = PadPainter.heldParts(view.control, state, painter.facesSwapped());
        if (held == heldParts) return;
        long now = AnimationUtils.currentAnimationTimeMillis();
        boolean visible = restingAlpha > 0, letGo = false;
        for (int i = 0; i < PadPainter.PARTS; i++) {
            boolean on = (held & 1 << i) != 0, was = (heldParts & 1 << i) != 0;
            if (on) {
                state.press[i] = 1;
                releasedAt[i] = 0;
            } else if (was && visible) {
                releasedAt[i] = now;
                letGo = true;
            } else if (was) {
                state.press[i] = 0;
            }
        }
        heldParts = held;
        if (letGo) {
            view.removeCallbacks(easing);
            view.postOnAnimation(easing);
        }
        invalidateHalo();
    }

    /** One frame of the let-go parts easing back; it comes again until all of them are out. */
    private void ease() {
        long now = AnimationUtils.currentAnimationTimeMillis();
        boolean running = false;
        for (int i = 0; i < PadPainter.PARTS; i++) {
            if (releasedAt[i] == 0) continue;
            float level = PadPainter.easeBack(now - releasedAt[i]);
            state.press[i] = level;
            if (level <= 0) {
                releasedAt[i] = 0;
            } else {
                running = true;
            }
        }
        showAlpha();
        view.redraw();
        invalidateHalo();
        if (running) view.postOnAnimation(easing);
    }

    /** Drops every easing press at once: input was taken away, so nothing lingers. What is still held stays. */
    void stopEasing() {
        view.removeCallbacks(easing);
        Arrays.fill(releasedAt, 0);
        heldParts = PadPainter.heldParts(view.control, state, painter.facesSwapped());
        for (int i = 0; i < PadPainter.PARTS; i++) {
            state.press[i] = (heldParts & 1 << i) != 0 ? 1 : 0;
        }
        showAlpha();
        invalidateHalo();
    }

    void detach() {
        view.removeCallbacks(easing);
    }
}
