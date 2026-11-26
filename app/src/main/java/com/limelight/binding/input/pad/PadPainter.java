package com.limelight.binding.input.pad;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.DashPathEffect;
import android.graphics.Paint;
import android.graphics.RectF;

import androidx.core.content.ContextCompat;

import com.limelight.R;
import com.limelight.binding.input.LayoutStick;
import com.limelight.binding.input.StickMath;
import com.limelight.nvstream.input.ControllerPacket;

import java.util.Arrays;

/**
 * Draws the modern controls as console hardware on glass, in the 7.0 material {@link PadSkin70}: a control at rest is a
 * matte, faintly dark body with a crisp light rim and a white glyph, the same for every controller family, so it reads
 * over bright and dark games alike. A press lights its body and rim amber at once, sinks it a little and spills a glow
 * past its edge where a thumb leaves it in view ({@link #drawHalo}); let go, it eases back within {@link #RELEASE_MS}.
 * The stream view, the layout editor and its Add tiles share it, so the editor shows exactly what plays.
 *
 * <p>A control draws only inside its own box, since its view cuts there; the glow and a stick's cap, which reach past
 * it, draw in views of their own around that one control. It is all solid fills and strokes from paints built once, so
 * a frame allocates nothing, needs no blur, shadow or offscreen layer, and draws the same on every Android. At opacity
 * 0 nothing is drawn at all.
 */
final class PadPainter {
    /** The parts a press lights on their own: a face cluster's buttons, a D-pad's arms or the WASD keys; one elsewhere. */
    static final int PARTS = 4;
    /** How long a part takes to ease back once let go, and the touchpad finger's trail to fade. */
    static final long RELEASE_MS = 120, TRAIL_MS = 220;
    /** How many of a touchpad finger's last places its trail keeps. */
    static final int TRAIL = 12;

    /** What one control shows besides its shape. */
    static final class State {
        /**
         * Gamepad flags held by this control (face buttons, D-pad directions, a button or a stick click); for the
         * WASD stick its held keys as {@link LayoutStick} direction bits.
         */
        int flags;
        /** A trigger or button held. */
        boolean pressed;
        /** A stick's position (-1..1, up positive) and its drawn center inside the box, in pixels; NaN centers the base. */
        float stickX, stickY, centerX = Float.NaN, centerY = Float.NaN;
        boolean stickActive;
        /**
         * A stick's deadzone as a share of its travel: the thumb leaves this ring around where it landed before the
         * stick answers, and the stick draws it as a guide. A setting, so {@link #clear} keeps it.
         */
        float deadzone;
        /** A trackpad's finger inside the box, in pixels; NaN while none touches it. */
        float touchX = Float.NaN, touchY = Float.NaN;
        /** The pointer touchpad's short mode word ({@link #pointerLabel}); empty for other controls. */
        String label = "";
        /**
         * How far each part is pressed in, 0 to 1: a face cluster's buttons in {@link PadInput#FACE_CENTERS} order, a
         * D-pad's arms and the WASD keys up, down, left, right, and part 0 for every other control. It is 1 the moment
         * a part is held; {@link PadControlView} eases it back to 0 once let go ({@link #easeBack}).
         */
        final float[] press = new float[PARTS];
        /** A touchpad finger's last places and when it was there, the oldest at {@link #trailStart}, for its trail. */
        final float[] trailX = new float[TRAIL], trailY = new float[TRAIL];
        final long[] trailAt = new long[TRAIL];
        int trailStart, trailCount;
        /** The time this frame draws at, on the clock of {@link #trailAt}. */
        long now;

        void clear() {
            flags = 0; pressed = false; stickX = 0; stickY = 0; centerX = Float.NaN; centerY = Float.NaN; stickActive = false;
            touchX = Float.NaN; touchY = Float.NaN;
            Arrays.fill(press, 0);
            trailStart = trailCount = 0;
        }

        /** The finger was at ({@code x}, {@code y}) at {@code at}: the trail grows, its oldest place giving way. */
        void trail(float x, float y, long at) {
            int index;
            if (trailCount < TRAIL) {
                index = (trailStart + trailCount) % TRAIL;
                trailCount++;
            } else {
                index = trailStart;
                trailStart = (trailStart + 1) % TRAIL;
            }
            trailX[index] = x;
            trailY[index] = y;
            trailAt[index] = at;
        }

        /** Whether some of the trail still shows at {@code now}. */
        boolean trailShows(long now) {
            return trailCount > 1 && now - trailAt[(trailStart + trailCount - 1) % TRAIL] < TRAIL_MS;
        }
    }

    /**
     * In dp: the stroke the stick geometry is measured with, and how far a shape that fills its box stays inside it,
     * which leaves its dark hairline room.
     */
    private static final float STROKE_DP = 1.5f;
    static final float EDGE_DP = 3;
    /** A Steam trackpad's and the pointer touchpad's corner radius, as a share of their shorter side. */
    static final float PAD_CORNER = .2f, TOUCHPAD_CORNER = .18f;
    /** How far outside a control's box the editor draws its outline and, at the outline's corners, its handles. */
    static final float OUTLINE_DP = 3;
    /** The radius of the editor's round corner handles. */
    static final float HANDLE_DP = 6;
    /** A stick's cap is this much denser than a body, so it reads as a cap over its base; and how lit a held one is. */
    private static final float CAP_DENSITY = 1.8f, CAP_HELD = .35f;

    final PadSkin70 skin;
    final int accent;
    final float density;
    private final float edgeStroke;
    private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG), stroke = new Paint(Paint.ANTI_ALIAS_FLAG),
            dashed = new Paint(Paint.ANTI_ALIAS_FLAG);
    final RectF rect = new RectF(), chip = new RectF();
    /** The face letters and shoulder names; the layout editor changes it and every view redraws. */
    private PadLayout.Glyph glyph = PadLayout.Glyph.XBOX;
    /** The face cluster's buttons stand in the Nintendo arrangement ({@link PadLayout#facesSwapped}). */
    private boolean facesSwapped;
    /** The stick geometry for the last size drawn, recomputed only when a stick's size changes. */
    private float stickSize = Float.NaN, stickBase, stickKnob;
    private final float[] knobAt = new float[2];
    /** The pointer touchpad's mode words in its chip ({@link #pointerLabel}). */
    private final String mouseWord, cameraWord;
    private final PadPainterHalo halo;
    private final PadPainterControls controls;

    PadPainter(Context context) {
        skin = new PadSkin70(context);
        density = skin.density;
        mouseWord = context.getString(R.string.pad_touchpad_chip_mouse);
        cameraWord = context.getString(R.string.pad_touchpad_chip_camera);
        edgeStroke = STROKE_DP * density;
        accent = ContextCompat.getColor(context, R.color.night_primary);
        stroke.setStyle(Paint.Style.STROKE);
        dashed.setStyle(Paint.Style.STROKE);
        dashed.setStrokeWidth(2 * density);
        dashed.setPathEffect(new DashPathEffect(new float[] {6 * density, 4 * density}, 0));
        halo = new PadPainterHalo(this);
        controls = new PadPainterControls(this);
    }

    int accent() {
        return accent;
    }

    /**
     * How visible the controls drawn from now on are, 0 to 1, for the editor and its tiles; the caller sets it back
     * to 1. The stream's views stay at 1 and fade through their own alpha.
     */
    void setOpacity(float opacity) {
        skin.setOpacity(opacity);
    }

    /** The word the pointer touchpad shows for its mode, in play and in the editor. */
    String pointerLabel(boolean camera) {
        return camera ? cameraWord : mouseWord;
    }

    PadLayout.Glyph glyph() {
        return glyph;
    }

    void setGlyph(PadLayout.Glyph glyph) {
        this.glyph = glyph;
    }

    /** Draws and reads the face cluster with its buttons in the Nintendo arrangement, or the standard one. */
    void setFacesSwapped(boolean swapped) {
        facesSwapped = swapped;
    }

    boolean facesSwapped() {
        return facesSwapped;
    }

    /** The face button drawn at {@code place} of the cluster, in {@link PadInput#FACE_CENTERS} order. */
    int faceButtonAt(int place) {
        return PadInput.faceOrder(facesSwapped)[place];
    }

    /** The parts {@code state} holds now, as bits in {@link State#press} order, with the standard face buttons. */
    static int heldParts(PadLayout.Control control, State state) {
        return heldParts(control, state, false);
    }

    /** The parts {@code state} holds now; a face cluster's places hold its buttons in the Nintendo order if swapped. */
    static int heldParts(PadLayout.Control control, State state, boolean facesSwapped) {
        switch (control.shape) {
            case FACE: {
                int[] order = PadInput.faceOrder(facesSwapped);
                int bits = 0;
                for (int i = 0; i < PARTS; i++) {
                    if ((state.flags & PadInput.FACE_FLAGS[order[i]]) != 0) bits |= 1 << i;
                }
                return bits;
            }
            case DPAD:
                return directions(state.flags, ControllerPacket.UP_FLAG, ControllerPacket.DOWN_FLAG,
                        ControllerPacket.LEFT_FLAG, ControllerPacket.RIGHT_FLAG);
            case STICK:
                if (control == PadLayout.Control.WASD) {
                    return directions(state.flags, LayoutStick.UP, LayoutStick.DOWN, LayoutStick.LEFT, LayoutStick.RIGHT);
                }
                return (state.flags & PadInput.buttonFlag(control)) != 0 ? 1 : 0;
            default:
                return state.pressed ? 1 : 0;
        }
    }

    private static int directions(int flags, int up, int down, int left, int right) {
        return ((flags & up) != 0 ? 1 : 0) | ((flags & down) != 0 ? 2 : 0) | ((flags & left) != 0 ? 4 : 0)
                | ((flags & right) != 0 ? 8 : 0);
    }

    /** How far a part let go {@code sinceMs} ago is still pressed in: from 1 back to 0 over {@link #RELEASE_MS}, fast at first. */
    static float easeBack(long sinceMs) {
        if (sinceMs <= 0) return 1;
        if (sinceMs >= RELEASE_MS) return 0;
        float rest = 1 - sinceMs / (float) RELEASE_MS;
        return rest * rest;
    }

    /** Draws {@code control} filling the box ({@code left}, {@code top}, {@code width}, {@code height}). */
    void draw(Canvas canvas, PadLayout.Control control, float left, float top, float width, float height, State state) {
        if (skin.hidden()) return;
        switch (control.shape) {
            case STICK:
                drawStickBase(canvas, control, left, top, width, height, state);
                drawStickKnob(canvas, control, left, top, width, height, state);
                break;
            case DPAD: controls.drawDpad(canvas, left, top, width, height, state); break;
            case FACE: controls.drawFace(canvas, left, top, width, height, state); break;
            case PILL: controls.drawPill(canvas, control, left, top, width, height, state); break;
            case PAD: controls.drawPad(canvas, left, top, width, height, state, PAD_CORNER,
                    control == PadLayout.Control.LEFT_PAD ? "L" : "R", null); break;
            case TOUCHPAD: controls.drawPad(canvas, left, top, width, height, state, TOUCHPAD_CORNER, null, state.label); break;
            case FACE_BUTTON: controls.drawFaceButton(canvas, control, left, top, width, height, state); break;
            case DPAD_ARM: controls.drawDpadArm(canvas, control, left, top, width, height, state); break;
            default: controls.drawRound(canvas, control, left, top, width, height, state); break;
        }
    }

    /** The editor's outline around a control: dashed amber with round corner handles when selected, a faint line otherwise. */
    void drawOutline(Canvas canvas, float left, float top, float width, float height, boolean selected) {
        float pad = OUTLINE_DP * density, radius = 12 * density;
        rect.set(left - pad, top - pad, left + width + pad, top + height + pad);
        if (selected) {
            dashed.setColor(accent);
            canvas.drawRoundRect(rect, radius, radius, dashed);
            // Corner handles: dragging one sizes the control's width and height (PadEditor).
            float h = HANDLE_DP * density;
            fill.setColor(0xFFFFFFFF);
            stroke.setColor(accent);
            stroke.setStrokeWidth(2 * density);
            for (int corner = 0; corner < 4; corner++) {
                float x = (corner & 1) != 0 ? rect.right : rect.left, y = (corner & 2) != 0 ? rect.bottom : rect.top;
                canvas.drawCircle(x, y, h, fill);
                canvas.drawCircle(x, y, h, stroke);
            }
        } else {
            stroke.setColor(0x38FFFFFF);
            stroke.setStrokeWidth(density);
            canvas.drawRoundRect(rect, radius, radius, stroke);
        }
    }

    /**
     * The press cue past a control's edge, drawn under the controls by that control's {@link PadControlView.HaloLayer}:
     * an amber glow around every held part that fades as it eases back, so a thumb covering a small button still shows
     * it answered. Every part a player presses has one: face buttons, the D-pad's arms, pills, round buttons, a stick's
     * click, each held key of the WASD stick on its side of the ring and a finger on a touchpad; pushing an analog
     * stick shows in its arc instead. It reaches {@link #haloReach} past the box at most.
     */
    void drawHalo(Canvas canvas, PadLayout.Control control, float left, float top, float width, float height, State state) {
        if (skin.hidden()) return;
        halo.drawHalo(canvas, control, left, top, width, height, state);
    }

    /** How far past each edge of a control's box its glow can draw, in whole pixels. */
    int haloReach() {
        return (int) Math.ceil(skin.glowReach());
    }

    /**
     * A stick's base: a matte disc whose rim is the ring the cap travels to at full tilt, with ticks at the four
     * directions; while pushed, an amber arc on the rim points where and grows with the push, and a click lights the
     * rim. The WASD stick shows its four keys instead of the ticks, a held one lit amber.
     */
    void drawStickBase(Canvas canvas, PadLayout.Control control, float left, float top, float width, float height,
                       State state) {
        if (skin.hidden()) return;
        stickGeometry(Math.min(width, height));
        float base = stickBase;
        float cx = left + width / 2, cy = top + height / 2;
        boolean wasd = control == PadLayout.Control.WASD;
        float rim = skin.rimWidth, ring = base - rim / 2;
        skin.disc(canvas, cx, cy, base, base, skin.fill,
                PadSkin70.blend(skin.rim, skin.pressedRim, wasd ? 0 : state.press[0]), 0);
        if (wasd) {
            float reach = base * .68f, key = base * .21f;
            wasdKey(canvas, "W", cx, cy - reach, key, state.press[0]);
            wasdKey(canvas, "S", cx, cy + reach, key, state.press[1]);
            wasdKey(canvas, "A", cx - reach, cy, key, state.press[2]);
            wasdKey(canvas, "D", cx + reach, cy, key, state.press[3]);
        } else {
            Paint tick = skin.line(PadSkin70.alpha(skin.label, .55f), 1.5f * density);
            for (int i = 0; i < 4; i++) {
                int dx = i == 0 ? 1 : i == 2 ? -1 : 0, dy = i == 1 ? 1 : i == 3 ? -1 : 0;
                canvas.drawLine(cx + dx * base * .80f, cy + dy * base * .80f, cx + dx * base * .89f, cy + dy * base * .89f, tick);
            }
        }
        float magnitude = (float) Math.min(1, Math.hypot(state.stickX, state.stickY));
        if (magnitude > 0) {
            float angle = (float) Math.toDegrees(Math.atan2(-state.stickY, state.stickX));
            float sweep = 30 + 60 * magnitude;
            rect.set(cx - ring, cy - ring, cx + ring, cy + ring);
            canvas.drawArc(rect, angle - sweep / 2, sweep, false,
                    skin.line(skin.pressedRim, rim + (1 + 2 * magnitude) * density));
        }
    }

    /** One key of the WASD stick: a small cap that lights amber while its key is held, under its letter. */
    private void wasdKey(Canvas canvas, String key, float cx, float cy, float half, float level) {
        rect.set(cx - half, cy - half, cx + half, cy + half);
        float corner = half * .45f;
        if (level > 0) canvas.drawRoundRect(rect, corner, corner, skin.fill(PadSkin70.alpha(skin.pressedFill, level)));
        canvas.drawRoundRect(rect, corner, corner,
                skin.line(PadSkin70.blend(PadSkin70.alpha(skin.rim, .4f), skin.pressedRim, level), density));
        skin.text(canvas, key, cx, cy, half * 1.25f);
    }

    /**
     * A stick's cap for the control in the box ({@code left}, {@code top}, {@code width}, {@code height}): a denser
     * matte disc with a thumb ring, its rim amber while a thumb holds it and the whole cap amber when clicked. Its
     * centre travels to the base's rim at full tilt, so the canvas must reach {@link #knobOverhang} past the box on
     * every side. Under it, where the thumb landed, a small ring marks the deadzone the thumb leaves before the stick
     * answers; it comes into view as the cap moves away.
     */
    void drawStickKnob(Canvas canvas, PadLayout.Control control, float left, float top, float width, float height, State state) {
        if (skin.hidden()) return;
        float size = Math.min(width, height);
        stickGeometry(size);
        float base = stickBase, knob = stickKnob;
        // The base stays put; a floating stick's cap starts where the thumb landed.
        float cx = left + width / 2, cy = top + height / 2;
        float originX = cx, originY = cy;
        if (state.stickActive && !Float.isNaN(state.centerX)) {
            originX = left + state.centerX;
            originY = top + state.centerY;
        }
        if (state.deadzone > 0) {
            float dead = Math.max(3 * density, state.deadzone * StickMath.travel(size));
            for (int pass = 0; pass < 2; pass++) {
                canvas.drawCircle(originX, originY, dead, skin.ink(pass, density, PadSkin70.alpha(skin.label, .7f)));
            }
        }
        StickMath.knob(cx, cy, originX, originY, state.stickX, state.stickY, base, knobAt);
        float kx = knobAt[0], ky = knobAt[1];
        boolean wasd = control == PadLayout.Control.WASD;
        float click = wasd ? 0 : state.press[0];
        int body = PadSkin70.blend(PadSkin70.alpha(skin.fill, CAP_DENSITY), skin.pressedFill,
                Math.max(click, state.stickActive ? CAP_HELD : 0));
        int rim = state.stickActive ? skin.pressedRim : PadSkin70.blend(skin.rim, skin.pressedRim, click);
        skin.disc(canvas, kx, ky, knob, knob, body, rim, skin.sink(click));
        if (!wasd && (state.flags & PadInput.buttonFlag(control)) != 0) {
            skin.text(canvas, control == PadLayout.Control.LEFT_STICK ? "L3" : "R3", kx, ky, knob * .7f);
        } else {
            // A ring in the cap, like a thumb grip.
            canvas.drawCircle(kx, ky, knob * .55f, skin.line(PadSkin70.alpha(skin.rim, .45f), density));
        }
    }

    float stickBaseFor(float size) {
        stickGeometry(size);
        return stickBase;
    }

    private void stickGeometry(float size) {
        if (size == stickSize) return;
        stickSize = size;
        stickBase = StickMath.baseRadius(size, edgeStroke);
        stickKnob = StickMath.knobRadius(size);
    }

    /** How far past each edge of a stick of this box its cap, with its dark hairline, can draw. */
    float knobOverhang(float width, float height) {
        return StickMath.overhang(Math.min(width, height), edgeStroke, skin.keyline + 1);
    }

    /** How far the finger travels for full deflection on a stick of this size; ModernController uses it as the stick radius. */
    static float travel(float size) {
        return StickMath.travel(size);
    }

    static String pillLabel(PadLayout.Control control) {
        switch (control) {
            case LEFT_GRIP: return "L GRIP";
            case RIGHT_GRIP: return "R GRIP";
            default: return control.name();
        }
    }
}
