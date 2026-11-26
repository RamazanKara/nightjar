package com.limelight.binding.input.pad;

import java.util.Arrays;

/**
 * Where a control lands while the layout editor moves or sizes it. Each axis lands on the nearest line worth sharing
 * that lies within the snap distance of where the finger put it: the screen's centre line, another control's edges
 * and centre, the mirror of its left or right twin's place, or the screen's edge. Where no such line is near it lands
 * on the grid. Nothing lands off the screen, and a control can stand anywhere on it, edge to edge.
 *
 * <p>The editor calls {@link #begin}, tells it the other controls ({@link #neighbour}) and the twin ({@link #twin}),
 * then {@link #move} or {@link #size} for every step of the finger, and reads the landing place and the line each axis
 * holds, which it draws as a guide. Plain geometry in pixels with no state between steps but that, so it is tested
 * without a device.
 */
final class PadSnapGrid {
    /**
     * What an axis landed on. Where two lines are equally near the later one wins: a twin's mirror before the screen's
     * centre, that before a neighbour, that before the screen's edge.
     */
    static final int FREE = 0, GRID = 1, EDGE = 2, NEIGHBOUR = 3, CENTRE = 4, MIRROR = 5;
    /** Two nudges this close, in pixels, are equally near. */
    private static final float SAME = .01f;

    private final float grid, reach;
    private float width, height;
    /** Every other control's left, centre and right, and in {@link #ys} its top, middle and bottom. */
    private float[] xs = new float[96], ys = new float[96];
    private int count;
    /** The twin's box mirrored left to right, where this control mirrors it; {@code mirrorLeft} is NaN without a twin. */
    private float mirrorLeft = Float.NaN, mirrorTop, mirrorWidth, mirrorHeight;

    /** Where the last {@link #move} put the centre, or the last {@link #size} the dragged corner. */
    float x, y;
    /** What each axis landed on. */
    int kindX, kindY;
    /** The line each axis holds now: the x of an upright guide and the y of a level one, NaN where it holds none. */
    float guideX = Float.NaN, guideY = Float.NaN;

    private float bestNudge, bestLine;
    private int bestKind;

    /** A grid of {@code grid} pixels; a line within {@code reach} pixels of the finger takes the control. */
    PadSnapGrid(float grid, float reach) {
        this.grid = Math.max(1e-3f, grid);
        this.reach = Math.max(0, reach);
    }

    /** A gesture starts on a {@code width} x {@code height} screen: no neighbours, no twin and no guides yet. */
    void begin(float width, float height) {
        this.width = width;
        this.height = height;
        count = 0;
        mirrorLeft = Float.NaN;
        release();
    }

    /** The finger lifted: the guides go. */
    void release() {
        kindX = kindY = FREE;
        guideX = guideY = Float.NaN;
    }

    /** Another control on the screen, whose edges and centre the moved one can share. */
    void neighbour(float left, float top, float w, float h) {
        if (xs.length < count + 3) {
            xs = Arrays.copyOf(xs, xs.length * 2);
            ys = Arrays.copyOf(ys, ys.length * 2);
        }
        xs[count] = left;
        xs[count + 1] = left + w / 2;
        xs[count + 2] = left + w;
        ys[count] = top;
        ys[count + 1] = top + h / 2;
        ys[count + 2] = top + h;
        count += 3;
    }

    /** The moved control's left or right twin: the place that mirrors it across the screen's middle is a line too. */
    void twin(float left, float top, float w, float h) {
        mirrorLeft = width - left - w;
        mirrorTop = top;
        mirrorWidth = w;
        mirrorHeight = h;
    }

    /** How many guides show now: none, one or two. */
    int guides() {
        return (Float.isNaN(guideX) ? 0 : 1) + (Float.isNaN(guideY) ? 0 : 1);
    }

    /**
     * Lands a {@code w} x {@code h} control whose centre the finger put at ({@code cx}, {@code cy}): {@link #x} and
     * {@link #y} are where its centre goes. Edges meet edges on either side, centres meet centres.
     */
    void move(float cx, float cy, float w, float h) {
        boolean twin = !Float.isNaN(mirrorLeft);
        x = moveAxis(cx, w, width, xs, twin ? mirrorLeft + mirrorWidth / 2 : Float.NaN, true);
        kindX = bestKind;
        guideX = bestLine;
        y = moveAxis(cy, h, height, ys, twin ? mirrorTop + mirrorHeight / 2 : Float.NaN, false);
        kindY = bestKind;
        guideY = bestLine;
    }

    private float moveAxis(float centre, float size, float screen, float[] lines, float mirror, boolean across) {
        float start = centre - size / 2, end = centre + size / 2;
        // The nudges that keep the whole control on the screen.
        float least = -start, most = screen - end;
        bestKind = FREE;
        bestNudge = 0;
        bestLine = Float.NaN;
        if (most < least) {
            // As large as the screen or larger: it fills it.
            return screen / 2;
        }
        // Mirrored left to right, a twin's place shares the screen's centre line as its axis; its height is its own.
        if (!Float.isNaN(mirror)) offer(mirror - centre, MIRROR, across ? screen / 2 : mirror, least, most);
        offer(screen / 2 - centre, CENTRE, screen / 2, least, most);
        for (int i = 0; i + 2 < count; i += 3) {
            float otherStart = lines[i], otherCentre = lines[i + 1], otherEnd = lines[i + 2];
            offer(otherCentre - centre, NEIGHBOUR, otherCentre, least, most);
            offer(otherStart - start, NEIGHBOUR, otherStart, least, most);
            offer(otherEnd - end, NEIGHBOUR, otherEnd, least, most);
            offer(otherStart - end, NEIGHBOUR, otherStart, least, most);
            offer(otherEnd - start, NEIGHBOUR, otherEnd, least, most);
        }
        offer(-start, EDGE, 0, least, most);
        offer(screen - end, EDGE, screen, least, most);
        if (bestKind == FREE) {
            bestKind = GRID;
            bestNudge = clamp(Math.round(centre / grid) * grid - centre, least, most);
        }
        return centre + bestNudge;
    }

    /**
     * Lands the corner the finger put at ({@code cornerX}, {@code cornerY}) of a control whose opposite corner stays at
     * ({@code anchorX}, {@code anchorY}): {@link #x} and {@link #y} are where the corner goes. {@code right} and
     * {@code bottom} say which corner it is. The control never gets narrower than {@code leastWide} or lower than
     * {@code leastHigh}, and its edges meet other edges, the screen's centre lines and its edges.
     */
    void size(float cornerX, float cornerY, float anchorX, float anchorY, boolean right, boolean bottom,
              float leastWide, float leastHigh) {
        boolean twin = !Float.isNaN(mirrorLeft);
        x = sizeAxis(cornerX, anchorX, right, leastWide, width, xs, twin ? mirrorLeft : Float.NaN,
                twin ? mirrorLeft + mirrorWidth : Float.NaN);
        kindX = bestKind;
        guideX = bestLine;
        y = sizeAxis(cornerY, anchorY, bottom, leastHigh, height, ys, twin ? mirrorTop : Float.NaN,
                twin ? mirrorTop + mirrorHeight : Float.NaN);
        kindY = bestKind;
        guideY = bestLine;
    }

    private float sizeAxis(float edge, float anchor, boolean after, float leastSize, float screen, float[] lines,
                           float mirrorStart, float mirrorEnd) {
        // Where the dragged edge may stand: a full touch from the edge that stays, and on the screen.
        float from = after ? Math.min(screen, anchor + leastSize) : 0;
        float to = after ? screen : Math.max(0, anchor - leastSize);
        float least = from - edge, most = to - edge;
        bestKind = FREE;
        bestNudge = 0;
        bestLine = Float.NaN;
        if (!Float.isNaN(mirrorStart)) {
            offer(mirrorStart - edge, MIRROR, mirrorStart, least, most);
            offer(mirrorEnd - edge, MIRROR, mirrorEnd, least, most);
        }
        offer(screen / 2 - edge, CENTRE, screen / 2, least, most);
        for (int i = 0; i + 2 < count; i += 3) {
            offer(lines[i] - edge, NEIGHBOUR, lines[i], least, most);
            offer(lines[i + 2] - edge, NEIGHBOUR, lines[i + 2], least, most);
        }
        offer(-edge, EDGE, 0, least, most);
        offer(screen - edge, EDGE, screen, least, most);
        if (bestKind == FREE) {
            bestKind = GRID;
            bestNudge = clamp(Math.round(edge / grid) * grid - edge, least, most);
        }
        return edge + bestNudge;
    }

    /**
     * The finger has not moved left or right in this gesture: what {@link #move} or {@link #size} just landed stays
     * at {@code x} and holds no upright line, so a drag straight down leaves the other axis alone.
     */
    void keepX(float x) {
        this.x = x;
        kindX = FREE;
        guideX = Float.NaN;
    }

    /** As {@link #keepX}, for a finger that has not moved up or down. */
    void keepY(float y) {
        this.y = y;
        kindY = FREE;
        guideY = Float.NaN;
    }

    /**
     * Takes the line {@code nudge} pixels away when it is within reach, keeps the control between {@code least} and
     * {@code most}, and is nearer than the best so far, or as near and of a stronger kind.
     */
    private void offer(float nudge, int kind, float line, float least, float most) {
        float far = Math.abs(nudge);
        if (far > reach || nudge < least - SAME || nudge > most + SAME) return;
        if (bestKind != FREE) {
            float best = Math.abs(bestNudge);
            if (far > best + SAME || (far > best - SAME && kind <= bestKind)) return;
        }
        bestNudge = nudge;
        bestKind = kind;
        bestLine = line;
    }

    /**
     * A control's left or right twin, the one whose place mirrors its own across the screen's middle: the other
     * stick, shoulder, trigger, stick click, trackpad, back button or grip, View for Menu, and the D-pad for the face
     * buttons, the two clusters under the thumbs. Null for a control with none.
     */
    static PadLayout.Control twin(PadLayout.Control control) {
        if (control == null) return null;
        switch (control) {
            case LEFT_STICK: return PadLayout.Control.RIGHT_STICK;
            case RIGHT_STICK: return PadLayout.Control.LEFT_STICK;
            case DPAD: return PadLayout.Control.FACE;
            case FACE: return PadLayout.Control.DPAD;
            case LB: return PadLayout.Control.RB;
            case RB: return PadLayout.Control.LB;
            case LT: return PadLayout.Control.RT;
            case RT: return PadLayout.Control.LT;
            case L3: return PadLayout.Control.R3;
            case R3: return PadLayout.Control.L3;
            case SELECT: return PadLayout.Control.START;
            case START: return PadLayout.Control.SELECT;
            case LEFT_PAD: return PadLayout.Control.RIGHT_PAD;
            case RIGHT_PAD: return PadLayout.Control.LEFT_PAD;
            case L4: return PadLayout.Control.R4;
            case R4: return PadLayout.Control.L4;
            case L5: return PadLayout.Control.R5;
            case R5: return PadLayout.Control.L5;
            case LEFT_GRIP: return PadLayout.Control.RIGHT_GRIP;
            case RIGHT_GRIP: return PadLayout.Control.LEFT_GRIP;
            default: return null;
        }
    }

    private static float clamp(float value, float min, float max) {
        return Math.max(min, Math.min(max, value));
    }
}
