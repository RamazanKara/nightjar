package com.limelight.binding.input.pad;

import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;

/**
 * Draws the non-stick controls for {@link PadPainter} in the 7.0 material ({@link PadSkin70}): D-pad cross and split
 * arms, face cluster and split buttons, shoulder/trigger/grip pills, Steam trackpads and pointer touchpad (with dot
 * grid, mode chip and finger trail), and round system buttons. Every glyph is the label colour whatever the controller
 * family, over its dark hairline. Reuses scratch paints, paths and buffers so frames allocate nothing.
 */
final class PadPainterControls {
    private final PadPainter p;
    private final PadSkin70 skin;
    private final Paint dots = new Paint(Paint.ANTI_ALIAS_FLAG), trail = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path path = new Path(), mark = new Path();
    /** A touchpad's dot grid, refilled for each pad drawn and grown only when a larger pad needs more. */
    private float[] dotPoints = new float[128];

    PadPainterControls(PadPainter painter) {
        this.p = painter;
        this.skin = painter.skin;
        dots.setStyle(Paint.Style.STROKE);
        dots.setStrokeCap(Paint.Cap.ROUND);
        trail.setStyle(Paint.Style.STROKE);
        trail.setStrokeCap(Paint.Cap.ROUND);
    }

    /**
     * The D-pad as one matte cross whose arms reach the box's edges, so a wide or tall box spreads it as
     * {@link PadInput#dpad} reads it; the arms keep the thickness of the box's shorter side. A held arm lights amber,
     * body and rim, and eases back once let go; a ring marks the pivot.
     */
    void drawDpad(Canvas canvas, float left, float top, float width, float height, PadPainter.State state) {
        float size = Math.min(width, height), edge = PadPainter.EDGE_DP * p.density + skin.rimWidth / 2;
        float arm = size * PadInput.DPAD_ARM, r = size * PadInput.DPAD_CORNER;
        float cx = left + width / 2, cy = top + height / 2;
        float l = left + edge, t = top + edge, w = width - 2 * edge, h = height - 2 * edge;
        plusOutline(l, t, w, h, arm, r);
        skin.shape(canvas, path);
        float reachX = (w - arm) / 2, reachY = (h - arm) / 2;
        armFill(canvas, state.press[0], cx - arm / 2, t, cx + arm / 2, t + reachY + r, r);
        armFill(canvas, state.press[1], cx - arm / 2, t + h - reachY - r, cx + arm / 2, t + h, r);
        armFill(canvas, state.press[2], l, cy - arm / 2, l + reachX + r, cy + arm / 2, r);
        armFill(canvas, state.press[3], l + w - reachX - r, cy - arm / 2, l + w, cy + arm / 2, r);
        canvas.drawCircle(cx, cy, arm * .2f, skin.line(PadSkin70.alpha(skin.rim, .4f), p.density));
        skin.shapeRim(canvas, path, 0);
        // A held arm's own stretch of the rim lights with it.
        float out = skin.rimWidth;
        armRim(canvas, state.press[0], l - out, t - out, l + w + out, cy - arm / 2);
        armRim(canvas, state.press[1], l - out, cy + arm / 2, l + w + out, t + h + out);
        armRim(canvas, state.press[2], l - out, t - out, cx - arm / 2, t + h + out);
        armRim(canvas, state.press[3], cx + arm / 2, t - out, l + w + out, t + h + out);
        float chevron = arm * .22f, insetX = reachX * .5f, insetY = reachY * .5f;
        chevron(canvas, cx, t + insetY, 0, -1, chevron);
        chevron(canvas, cx, t + h - insetY, 0, 1, chevron);
        chevron(canvas, l + insetX, cy, -1, 0, chevron);
        chevron(canvas, l + w - insetX, cy, 1, 0, chevron);
    }

    /** Puts the plus's outline, its arms {@code arm} thick with corners of radius {@code r}, in {@link #path}. */
    private void plusOutline(float left, float top, float width, float height, float arm, float r) {
        float ax = (width - arm) / 2, bx = ax + arm, ay = (height - arm) / 2, by = ay + arm;
        float right = left + width, bottom = top + height;
        path.reset();
        path.moveTo(left + ax + r, top);
        path.lineTo(left + bx - r, top);
        path.quadTo(left + bx, top, left + bx, top + r);
        path.lineTo(left + bx, top + ay);
        path.lineTo(right - r, top + ay);
        path.quadTo(right, top + ay, right, top + ay + r);
        path.lineTo(right, top + by - r);
        path.quadTo(right, top + by, right - r, top + by);
        path.lineTo(left + bx, top + by);
        path.lineTo(left + bx, bottom - r);
        path.quadTo(left + bx, bottom, left + bx - r, bottom);
        path.lineTo(left + ax + r, bottom);
        path.quadTo(left + ax, bottom, left + ax, bottom - r);
        path.lineTo(left + ax, top + by);
        path.lineTo(left + r, top + by);
        path.quadTo(left, top + by, left, top + by - r);
        path.lineTo(left, top + ay + r);
        path.quadTo(left, top + ay, left + r, top + ay);
        path.lineTo(left + ax, top + ay);
        path.lineTo(left + ax, top + r);
        path.quadTo(left + ax, top, left + ax + r, top);
        path.close();
    }

    private void armFill(Canvas canvas, float level, float l, float t, float r, float b, float radius) {
        if (level <= 0) return;
        canvas.drawRoundRect(l, t, r, b, radius, radius, skin.fill(PadSkin70.alpha(skin.pressedFill, level)));
    }

    /** The cross's rim inside one arm's part of the box, lit by {@code level}; {@link #path} holds the cross. */
    private void armRim(Canvas canvas, float level, float l, float t, float r, float b) {
        if (level <= 0) return;
        int saved = canvas.save();
        canvas.clipRect(l, t, r, b);
        skin.shapeRim(canvas, path, level);
        canvas.restoreToCount(saved);
    }

    /** A chevron with its tip toward ({@code dx}, {@code dy}), in {@link #mark} so the cross in {@link #path} stays. */
    private void chevron(Canvas canvas, float cx, float cy, int dx, int dy, float s) {
        float tipX = cx + dx * s * .6f, tipY = cy + dy * s * .6f;
        float backX = cx - dx * s * .4f, backY = cy - dy * s * .4f;
        mark.reset();
        mark.moveTo(backX - dy * s, backY + dx * s);
        mark.lineTo(tipX, tipY);
        mark.lineTo(backX + dy * s, backY - dx * s);
        for (int pass = 0; pass < 2; pass++) canvas.drawPath(mark, skin.ink(pass, PadSkin70.INK_DP * p.density));
    }

    /**
     * The face cluster: each button in its own part of the box (top, bottom, left, right), spread over a wide or tall
     * box as {@link PadInput#face} reads it, and sized to the box's shorter side so the four never overlap.
     */
    void drawFace(Canvas canvas, float left, float top, float width, float height, PadPainter.State state) {
        float size = Math.min(width, height), r = size * PadInput.FACE_RADIUS;
        for (int i = 0; i < PadInput.FACE_CENTERS.length; i++) {
            float cx = left + PadInput.FACE_CENTERS[i][0] * width, cy = top + PadInput.FACE_CENTERS[i][1] * height;
            float level = state.press[i];
            skin.disc(canvas, cx, cy, r, r, level);
            faceGlyph(canvas, p.faceButtonAt(i), cx, cy, r - skin.sink(level));
        }
    }

    /** One face button of a split cluster: the cluster's button filling its box, with the glyph set's glyph. */
    void drawFaceButton(Canvas canvas, PadLayout.Control control, float left, float top, float width,
                        float height, PadPainter.State state) {
        float level = state.press[0], edge = PadPainter.EDGE_DP * p.density;
        float cx = left + width / 2, cy = top + height / 2;
        float rx = width / 2 - edge, ry = height / 2 - edge;
        skin.disc(canvas, cx, cy, rx, ry, level);
        faceGlyph(canvas, control.memberIndex(), cx, cy, Math.min(rx, ry) - skin.sink(level));
    }

    /**
     * The glyph on the face button at {@code index} of radius {@code r}: the family's letter, or PlayStation's shapes
     * drawn as lines so they stay crisp at any size. Every family draws in the one label colour: the skin has no
     * coloured glyphs, and PlayStation's shapes never take on the look of Sony's coloured mark.
     */
    private void faceGlyph(Canvas canvas, int index, float cx, float cy, float r) {
        PadLayout.Glyph glyph = p.glyph();
        if (glyph != PadLayout.Glyph.PLAYSTATION) {
            skin.text(canvas, PadGlyphs.face(glyph, index), cx, cy, r * .95f);
            return;
        }
        float s = r * .42f, weight = Math.max(1.5f * p.density, s * .24f);
        for (int pass = 0; pass < 2; pass++) {
            Paint ink = skin.ink(pass, weight);
            switch (index) {
                case 0: // Cross
                    canvas.drawLine(cx - s * .8f, cy - s * .8f, cx + s * .8f, cy + s * .8f, ink);
                    canvas.drawLine(cx - s * .8f, cy + s * .8f, cx + s * .8f, cy - s * .8f, ink);
                    break;
                case 1: // Circle
                    canvas.drawCircle(cx, cy, s * .9f, ink);
                    break;
                case 2: // Square
                    p.rect.set(cx - s * .78f, cy - s * .78f, cx + s * .78f, cy + s * .78f);
                    canvas.drawRoundRect(p.rect, s * .1f, s * .1f, ink);
                    break;
                default: // Triangle, raised a little so it looks centred
                    path.reset();
                    path.moveTo(cx, cy - s);
                    path.lineTo(cx + s * .98f, cy + s * .66f);
                    path.lineTo(cx - s * .98f, cy + s * .66f);
                    path.close();
                    canvas.drawPath(path, ink);
                    break;
            }
        }
    }

    /** The corner radius of a split D-pad arm in {@code shape}, rounded as the cross's arms are. */
    static float armRadius(RectF shape, boolean vertical) {
        float thickness = vertical ? shape.width() : shape.height();
        return Math.min(thickness * PadInput.DPAD_CORNER / PadInput.DPAD_ARM, Math.min(shape.width(), shape.height()) / 2);
    }

    /**
     * One direction of a split D-pad: its arm filling the box, rounded as the D-pad's arms are, with its chevron
     * pointing out; amber while held.
     */
    void drawDpadArm(Canvas canvas, PadLayout.Control control, float left, float top, float width, float height,
                     PadPainter.State state) {
        boolean vertical = control == PadLayout.Control.DPAD_UP || control == PadLayout.Control.DPAD_DOWN;
        float level = state.press[0], edge = PadPainter.EDGE_DP * p.density;
        RectF shape = p.chip;
        shape.set(left + edge, top + edge, left + width - edge, top + height - edge);
        float thickness = vertical ? shape.width() : shape.height();
        skin.slab(canvas, shape, armRadius(shape, vertical), level);
        int dx = control == PadLayout.Control.DPAD_LEFT ? -1 : control == PadLayout.Control.DPAD_RIGHT ? 1 : 0;
        int dy = control == PadLayout.Control.DPAD_UP ? -1 : control == PadLayout.Control.DPAD_DOWN ? 1 : 0;
        float size = Math.min(thickness, vertical ? shape.height() : shape.width());
        chevron(canvas, shape.centerX(), shape.centerY(), dx, dy, size * .22f);
    }

    void drawPill(Canvas canvas, PadLayout.Control control, float left, float top, float width, float height,
                  PadPainter.State state) {
        float level = state.press[0], edge = PadPainter.EDGE_DP * p.density;
        RectF shape = p.chip;
        shape.set(left + edge, top + edge, left + width - edge, top + height - edge);
        // A stadium filling the box, lying or standing: the shorter side is the round end.
        float thickness = Math.min(shape.width(), shape.height());
        skin.slab(canvas, shape, thickness / 2, level);
        RectF inner = skin.inner;
        float sunk = Math.min(inner.width(), inner.height());
        if (control == PadLayout.Control.TOUCHPAD) {
            // Touchpad: a small pad with its click line, like the one on a PlayStation pad.
            float h = sunk * .4f, w = Math.min(inner.width() * .5f, h * 1.8f);
            float l = inner.centerX() - w / 2, t = inner.centerY() - h / 2;
            p.rect.set(l, t, l + w, t + h);
            for (int pass = 0; pass < 2; pass++) {
                Paint ink = skin.ink(pass, 1.5f * p.density);
                canvas.drawRoundRect(p.rect, h * .25f, h * .25f, ink);
                canvas.drawLine(p.rect.centerX(), t + h * .3f, p.rect.centerX(), t + h * .7f, ink);
            }
            return;
        }
        String name = PadGlyphs.shoulder(control, p.glyph());
        if (name == null) name = PadPainter.pillLabel(control);
        // A long label (the grips') shrinks to stay inside the pill.
        float fit = Math.min(1, inner.width() * .8f / Math.max(1, skin.textWidth(name, sunk * .42f)));
        skin.text(canvas, name, inner.centerX(), inner.centerY(), sunk * .42f * fit);
    }

    /**
     * A touchpad: a rounded matte pad with a faint dot grid, its rim lit amber while touched. A Steam Controller
     * trackpad shows its side's {@code letter} in the middle; the pointer touchpad shows its mode's word as a small chip
     * in a corner ({@code word}), so a glance says whether it moves the pointer or the camera. The finger shows as an
     * amber dot with a short trail that fades behind it.
     */
    void drawPad(Canvas canvas, float left, float top, float width, float height, PadPainter.State state, float corner,
                 String letter, String word) {
        float edge = PadPainter.EDGE_DP * p.density, level = state.press[0];
        RectF shape = p.chip;
        shape.set(left + edge, top + edge, left + width - edge, top + height - edge);
        float size = Math.min(shape.width(), shape.height()), radius = size * corner;
        // A surface, not a key: a finger lights its rim and leaves the body clear for the game behind it.
        skin.slab(canvas, shape, radius, skin.fill, PadSkin70.blend(skin.rim, skin.pressedRim, level), 0);
        drawDots(canvas, shape, radius);
        if (letter != null) skin.text(canvas, letter, shape.centerX(), shape.centerY(), size * .22f, PadSkin70.alpha(skin.label, .7f));
        if (word != null && !word.isEmpty()) drawChip(canvas, word, shape, size, radius);
        drawTrail(canvas, left, top, width, height, state);
        if (!Float.isNaN(state.touchX)) {
            float fx = left + Math.max(0, Math.min(width, state.touchX)), fy = top + Math.max(0, Math.min(height, state.touchY));
            float dot = Math.min(7 * p.density, size * .1f);
            canvas.drawCircle(fx, fy, dot * 1.9f, skin.line(PadSkin70.alpha(skin.pressedRim, .3f), dot * .8f));
            canvas.drawCircle(fx, fy, dot, skin.fill(skin.pressedRim));
            canvas.drawCircle(fx, fy, dot, skin.line(skin.label, 1.5f * p.density));
        }
    }

    /** A faint grid of dots over a touchpad's surface, one draw for all of them. */
    private void drawDots(Canvas canvas, RectF area, float radius) {
        float step = 16 * p.density, margin = Math.max(radius * .7f, 12 * p.density);
        float l = area.left + margin, t = area.top + margin, r = area.right - margin, b = area.bottom - margin;
        if (r < l || b < t) return;
        int columns = (int) ((r - l) / step) + 1, rows = (int) ((b - t) / step) + 1;
        float x0 = (l + r - (columns - 1) * step) / 2, y0 = (t + b - (rows - 1) * step) / 2;
        int count = columns * rows * 2;
        if (dotPoints.length < count) dotPoints = new float[count];
        int k = 0;
        for (int row = 0; row < rows; row++) {
            for (int column = 0; column < columns; column++) {
                dotPoints[k++] = x0 + column * step;
                dotPoints[k++] = y0 + row * step;
            }
        }
        dots.setStrokeWidth(2 * p.density);
        dots.setColor(skin.tint(PadSkin70.alpha(skin.rim, .32f)));
        canvas.drawPoints(dotPoints, 0, k, dots);
    }

    /** The pointer touchpad's mode word as a small dark chip in its upper left corner, sized to the pad. */
    private void drawChip(Canvas canvas, String word, RectF area, float size, float radius) {
        float textSize = Math.max(7 * p.density, Math.min(12 * p.density, size * .14f));
        float pad = textSize * .7f, w = skin.textWidth(word, textSize) + 2 * pad, h = textSize * 1.75f;
        float inset = Math.max(4 * p.density, Math.min(radius * .45f, size * .14f));
        p.rect.set(area.left + inset, area.top + inset, area.left + inset + w, area.top + inset + h);
        canvas.drawRoundRect(p.rect, h / 2, h / 2, skin.fill(skin.shadow));
        canvas.drawRoundRect(p.rect, h / 2, h / 2, skin.line(PadSkin70.alpha(skin.rim, .6f), p.density));
        skin.text(canvas, word, p.rect.centerX(), p.rect.centerY(), textSize);
    }

    /** The finger's last places as amber strokes, thinner and fainter the older they are, gone after {@link PadPainter#TRAIL_MS}. */
    private void drawTrail(Canvas canvas, float left, float top, float width, float height, PadPainter.State state) {
        if (state.trailCount < 2) return;
        for (int i = 1; i < state.trailCount; i++) {
            int from = (state.trailStart + i - 1) % PadPainter.TRAIL, to = (state.trailStart + i) % PadPainter.TRAIL;
            long age = Math.max(0, state.now - state.trailAt[to]);
            if (age >= PadPainter.TRAIL_MS) continue;
            float fade = 1 - age / (float) PadPainter.TRAIL_MS, along = i / (float) (state.trailCount - 1);
            trail.setStrokeWidth((1.5f + 3.5f * along) * p.density);
            trail.setColor(skin.tint(PadSkin70.alpha(skin.pressedRim, .7f * fade * (.3f + .7f * along))));
            canvas.drawLine(left + Math.max(0, Math.min(width, state.trailX[from])), top + Math.max(0, Math.min(height, state.trailY[from])),
                    left + Math.max(0, Math.min(width, state.trailX[to])), top + Math.max(0, Math.min(height, state.trailY[to])), trail);
        }
    }

    /** A round button filling its box: a disc in a square one, an oval in a resized one, its mark in the middle. */
    void drawRound(Canvas canvas, PadLayout.Control control, float left, float top, float width, float height,
                   PadPainter.State state) {
        float level = state.press[0], edge = PadPainter.EDGE_DP * p.density;
        float cx = left + width / 2, cy = top + height / 2;
        float rx = width / 2 - edge, ry = height / 2 - edge;
        skin.disc(canvas, cx, cy, rx, ry, level);
        float r = Math.min(rx, ry) - skin.sink(level);
        float g = r * .42f;
        switch (control) {
            case SELECT: {
                // View: two windows, the front one over the back one's corner. Only the back one's part in view is
                // drawn, so nothing has to cover it.
                float w = g * 1.1f, h = g * .85f, o = g * .35f;
                float l = cx - w / 2 - o, t = cy - h / 2 - o;
                path.reset();
                path.moveTo(l + 2 * o, t + h);
                path.lineTo(l, t + h);
                path.lineTo(l, t);
                path.lineTo(l + w, t);
                path.lineTo(l + w, t + 2 * o);
                p.rect.set(l + 2 * o, t + 2 * o, l + 2 * o + w, t + 2 * o + h);
                for (int pass = 0; pass < 2; pass++) {
                    Paint ink = skin.ink(pass, PadSkin70.INK_DP * p.density);
                    canvas.drawPath(path, ink);
                    canvas.drawRoundRect(p.rect, g * .15f, g * .15f, ink);
                }
                break;
            }
            case START: {
                // Menu: three lines.
                for (int pass = 0; pass < 2; pass++) {
                    Paint ink = skin.ink(pass, PadSkin70.INK_DP * p.density);
                    for (int i = -1; i <= 1; i++) {
                        canvas.drawLine(cx - g * .75f, cy + i * g * .5f, cx + g * .75f, cy + i * g * .5f, ink);
                    }
                }
                break;
            }
            case HOME: {
                // Home: a house.
                path.reset();
                path.moveTo(cx - g * .8f, cy - g * .05f);
                path.lineTo(cx, cy - g * .8f);
                path.lineTo(cx + g * .8f, cy - g * .05f);
                path.moveTo(cx - g * .55f, cy - g * .25f);
                path.lineTo(cx - g * .55f, cy + g * .7f);
                path.lineTo(cx + g * .55f, cy + g * .7f);
                path.lineTo(cx + g * .55f, cy - g * .25f);
                for (int pass = 0; pass < 2; pass++) canvas.drawPath(path, skin.ink(pass, PadSkin70.INK_DP * p.density));
                break;
            }
            default:
                skin.text(canvas, control.name(), cx, cy, r * .7f);
                break;
        }
    }
}
