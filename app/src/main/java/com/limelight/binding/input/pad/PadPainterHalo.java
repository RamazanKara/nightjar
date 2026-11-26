package com.limelight.binding.input.pad;

import android.graphics.Canvas;
import android.graphics.Path;
import android.graphics.Region;
import android.os.Build;

/**
 * Draws the press cue past a control's edge for {@link PadPainter}: the amber light a held or easing part spills on
 * the glass around it ({@link PadSkin70#glowDisc}), so a thumb covering a small button still shows it answered. It
 * comes on with the press and fades as the part eases back; nothing runs on after that. Plain strokes from the skin's
 * paints, so frames allocate nothing.
 */
final class PadPainterHalo {
    /** Where the WASD stick's keys sit on its ring, in degrees (W up, S down, A left, D right), and how much each lights. */
    private static final float[] WASD_ANGLE = {-90, 90, 180, 0};
    private static final float WASD_SWEEP = 76;

    private final PadPainter p;
    private final PadSkin70 skin;
    private final Path clip = new Path();

    PadPainterHalo(PadPainter painter) {
        this.p = painter;
        this.skin = painter.skin;
    }

    /** Draws the glow of every pressed or easing part of {@code control} in the box; nothing at rest. */
    void drawHalo(Canvas canvas, PadLayout.Control control, float left, float top, float width, float height,
                  PadPainter.State state) {
        float edge = PadPainter.EDGE_DP * p.density;
        switch (control.shape) {
            case FACE: {
                float size = Math.min(width, height), r = size * PadInput.FACE_RADIUS;
                if (!lit(state)) break;
                // Around the held button and between the others, never across them.
                int saved = canvas.save();
                clip.reset();
                for (int i = 0; i < PadPainter.PARTS; i++) {
                    clip.addCircle(left + PadInput.FACE_CENTERS[i][0] * width, top + PadInput.FACE_CENTERS[i][1] * height, r,
                            Path.Direction.CW);
                }
                clipOut(canvas, clip);
                for (int i = 0; i < PadPainter.PARTS; i++) {
                    skin.glowDisc(canvas, left + PadInput.FACE_CENTERS[i][0] * width,
                            top + PadInput.FACE_CENTERS[i][1] * height, r, r, state.press[i]);
                }
                canvas.restoreToCount(saved);
                break;
            }
            case ROUND:
            case FACE_BUTTON:
                skin.glowDisc(canvas, left + width / 2, top + height / 2, width / 2 - edge, height / 2 - edge, state.press[0]);
                break;
            case PILL:
                p.rect.set(left + edge, top + edge, left + width - edge, top + height - edge);
                skin.glowSlab(canvas, p.rect, Math.min(p.rect.width(), p.rect.height()) / 2, state.press[0]);
                break;
            case DPAD_ARM: {
                boolean vertical = control == PadLayout.Control.DPAD_UP || control == PadLayout.Control.DPAD_DOWN;
                p.rect.set(left + edge, top + edge, left + width - edge, top + height - edge);
                skin.glowSlab(canvas, p.rect, PadPainterControls.armRadius(p.rect, vertical), state.press[0]);
                break;
            }
            case DPAD: {
                float size = Math.min(width, height), arm = size * PadInput.DPAD_ARM, r = size * PadInput.DPAD_CORNER;
                float cx = left + width / 2, cy = top + height / 2, reach = skin.glowReach();
                float l = left + edge, t = top + edge, rgt = left + width - edge, b = top + height - edge;
                for (int i = 0; i < PadPainter.PARTS; i++) {
                    if (state.press[i] <= 0) continue;
                    // The arm from its tip to the bar across it; the glow only past that bar, outside the cross.
                    int saved = canvas.save();
                    switch (i) {
                        case 0:
                            p.rect.set(cx - arm / 2, t, cx + arm / 2, cy - arm / 2);
                            canvas.clipRect(l - reach, t - reach, rgt + reach, cy - arm / 2);
                            break;
                        case 1:
                            p.rect.set(cx - arm / 2, cy + arm / 2, cx + arm / 2, b);
                            canvas.clipRect(l - reach, cy + arm / 2, rgt + reach, b + reach);
                            break;
                        case 2:
                            p.rect.set(l, cy - arm / 2, cx - arm / 2, cy + arm / 2);
                            canvas.clipRect(l - reach, t - reach, cx - arm / 2, b + reach);
                            break;
                        default:
                            p.rect.set(cx + arm / 2, cy - arm / 2, rgt, cy + arm / 2);
                            canvas.clipRect(cx + arm / 2, t - reach, rgt + reach, b + reach);
                            break;
                    }
                    skin.glowSlab(canvas, p.rect, r, state.press[i]);
                    canvas.restoreToCount(saved);
                }
                break;
            }
            case STICK: {
                float base = p.stickBaseFor(Math.min(width, height));
                float cx = left + width / 2, cy = top + height / 2;
                if (control != PadLayout.Control.WASD) {
                    skin.glowDisc(canvas, cx, cy, base, base, state.press[0]);
                    break;
                }
                // Each held key lights its own side of the ring, as the D-pad's arms: up, down, left, right.
                for (int i = 0; i < PadPainter.PARTS; i++) {
                    skin.glowArc(canvas, cx, cy, base, WASD_ANGLE[i], WASD_SWEEP, state.press[i]);
                }
                break;
            }
            case PAD:
            case TOUCHPAD: {
                p.rect.set(left + edge, top + edge, left + width - edge, top + height - edge);
                float corner = control.shape == PadLayout.Shape.PAD ? PadPainter.PAD_CORNER : PadPainter.TOUCHPAD_CORNER;
                skin.glowSlab(canvas, p.rect, Math.min(p.rect.width(), p.rect.height()) * corner, state.press[0]);
                break;
            }
            default:
                break;
        }
    }

    /** Whether any part is pressed or easing back. */
    private static boolean lit(PadPainter.State state) {
        for (int i = 0; i < PadPainter.PARTS; i++) {
            if (state.press[i] > 0) return true;
        }
        return false;
    }

    private static void clipOut(Canvas canvas, Path shape) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            canvas.clipOutPath(shape);
        } else {
            canvas.clipPath(shape, Region.Op.DIFFERENCE);
        }
    }
}
