package com.limelight.binding.input.pad;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.graphics.Typeface;

import androidx.core.content.ContextCompat;
import androidx.core.graphics.ColorUtils;

import com.limelight.R;

/**
 * The 7.0 material of the on-screen controller: console hardware drawn on glass, from the design tokens
 * {@code nj_pad_skin_*}. A part is a matte, faintly obsidian body with a crisp light rim and its glyph in the label
 * colour; pressed, its body and rim light amber the moment the finger lands and ease off as {@link PadPainter} lowers
 * the press level. {@link PadPainter} owns the shapes and calls this for every body, rim, glyph and glow, so the stream,
 * the layout editor and its Add tiles share one look.
 *
 * <p>It has to read over any video without help from a blur or a shadow layer, so contrast is drawn, not composited:
 * a dark hairline runs along both sides of every rim and under every glyph, which keeps the light rim and the white
 * glyph apart from a white sky, while the rim and glyph themselves carry it over a dark scene. Everything is a solid
 * fill or stroke from paints built once; a frame allocates nothing and needs no offscreen layer.
 *
 * <p>{@link #setOpacity} fades whatever is drawn next, for the editor and its tiles; the stream's views fade through
 * their own alpha instead. At 0 nothing is drawn at all ({@link #hidden}).
 */
final class PadSkin70 {
    /**
     * In dp, and not in the tokens yet: the dark hairline beside a rim and around a glyph, how far a press sinks a
     * part, a glyph's line weight and how far a pressed part's light spills past its edge.
     */
    static final float KEYLINE_DP = 1, SINK_DP = 1.5f, INK_DP = 1.75f, GLOW_DP = 12;
    /** The spill's strength in bands from the edge out, as shares of the pressed rim's colour. */
    private static final float[] GLOW = {.34f, .20f, .10f, .04f};
    /**
     * A glyph's hairline is this much denser than a rim's: a white letter on a body that white video shows through,
     * or on a lit amber one, has only its hairline to stand against.
     */
    private static final float GLYPH_KEYLINE = 1.75f;
    /** How many degrees each band of an arc's spill ends before the band inside it, so the light tapers off. */
    private static final float ARC_TAPER = 7;

    final float density, rimWidth, keyline;
    final int fill, rim, label, pressedFill, pressedRim, shadow;
    /** The hairline under a glyph ({@link #GLYPH_KEYLINE}). */
    private final int outline;
    private float opacity = 1;
    private final Paint body = new Paint(Paint.ANTI_ALIAS_FLAG), edge = new Paint(Paint.ANTI_ALIAS_FLAG),
            ink = new Paint(Paint.ANTI_ALIAS_FLAG), text = new Paint(Paint.ANTI_ALIAS_FLAG);
    /** The shape {@link #slab} last drew, sunk by its press; scratch for {@link #glowSlab} and callers otherwise. */
    final RectF inner = new RectF();
    private final RectF rect = new RectF();

    PadSkin70(Context context) {
        density = context.getResources().getDisplayMetrics().density;
        rimWidth = context.getResources().getDimension(R.dimen.nj_pad_skin_rim_width);
        keyline = KEYLINE_DP * density;
        fill = ContextCompat.getColor(context, R.color.nj_pad_skin_fill);
        rim = ContextCompat.getColor(context, R.color.nj_pad_skin_rim);
        label = ContextCompat.getColor(context, R.color.nj_pad_skin_label);
        pressedFill = ContextCompat.getColor(context, R.color.nj_pad_skin_pressed_fill);
        pressedRim = ContextCompat.getColor(context, R.color.nj_pad_skin_pressed_rim);
        shadow = ContextCompat.getColor(context, R.color.nj_pad_skin_shadow);
        outline = alpha(shadow, GLYPH_KEYLINE);
        edge.setStyle(Paint.Style.STROKE);
        edge.setStrokeCap(Paint.Cap.ROUND);
        edge.setStrokeJoin(Paint.Join.ROUND);
        ink.setStyle(Paint.Style.STROKE);
        ink.setStrokeCap(Paint.Cap.ROUND);
        ink.setStrokeJoin(Paint.Join.ROUND);
        text.setTextAlign(Paint.Align.CENTER);
        // The stream's overlays keep the system face, as its menu does.
        text.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        text.setStrokeJoin(Paint.Join.ROUND);
    }

    /** How visible everything drawn from now on is, 0 to 1; the caller sets it back to 1 when done. */
    void setOpacity(float opacity) {
        this.opacity = Math.max(0, Math.min(1, opacity));
    }

    /** Nothing shows at this opacity, so a caller draws nothing and returns. */
    boolean hidden() {
        return opacity <= 0;
    }

    /** {@code color} at the current opacity. */
    int tint(int color) {
        return opacity >= 1 ? color : alpha(color, opacity);
    }

    /** How far a part pressed in by {@code level} sinks. */
    float sink(float level) {
        return SINK_DP * density * level;
    }

    /** A round part pressed in by {@code level}: its dark hairline, its body and its rim, sunk a little by the press. */
    void disc(Canvas canvas, float cx, float cy, float rx, float ry, float level) {
        disc(canvas, cx, cy, rx, ry, blend(fill, pressedFill, level), blend(rim, pressedRim, level), sink(level));
    }

    /** A round part with a body and a rim of the given colours, {@code sunk} pixels smaller than its place. */
    void disc(Canvas canvas, float cx, float cy, float rx, float ry, int bodyColor, int rimColor, float sunk) {
        float half = rimWidth / 2;
        rx -= sunk + half;
        ry -= sunk + half;
        if (rx <= 0 || ry <= 0) return;
        edge.setStrokeWidth(rimWidth + 2 * keyline);
        edge.setColor(tint(shadow));
        oval(canvas, cx, cy, rx, ry, edge);
        body.setColor(tint(bodyColor));
        oval(canvas, cx, cy, rx, ry, body);
        edge.setStrokeWidth(rimWidth);
        edge.setColor(tint(rimColor));
        oval(canvas, cx, cy, rx, ry, edge);
    }

    /** A rounded part pressed in by {@code level}, as {@link #disc}; afterwards {@link #inner} holds its sunk shape. */
    void slab(Canvas canvas, RectF shape, float radius, float level) {
        slab(canvas, shape, radius, blend(fill, pressedFill, level), blend(rim, pressedRim, level), sink(level));
    }

    /** A rounded part with a body and a rim of the given colours, {@code sunk} pixels smaller than {@code shape}. */
    void slab(Canvas canvas, RectF shape, float radius, int bodyColor, int rimColor, float sunk) {
        float half = rimWidth / 2;
        inner.set(shape);
        inner.inset(sunk, sunk);
        if (inner.width() <= rimWidth || inner.height() <= rimWidth) return;
        float r = Math.max(0, radius - sunk - half);
        rect.set(inner);
        rect.inset(half, half);
        edge.setStrokeWidth(rimWidth + 2 * keyline);
        edge.setColor(tint(shadow));
        canvas.drawRoundRect(rect, r, r, edge);
        body.setColor(tint(bodyColor));
        canvas.drawRoundRect(rect, r, r, body);
        edge.setStrokeWidth(rimWidth);
        edge.setColor(tint(rimColor));
        canvas.drawRoundRect(rect, r, r, edge);
    }

    /** A part of any outline at rest, as {@link #disc}: {@code outline} is the middle of its rim. */
    void shape(Canvas canvas, Path outline) {
        edge.setStrokeWidth(rimWidth + 2 * keyline);
        edge.setColor(tint(shadow));
        canvas.drawPath(outline, edge);
        body.setColor(tint(fill));
        canvas.drawPath(outline, body);
    }

    /** The rim of a {@link #shape}, lit by {@code level}; drawn after what lies on the body. */
    void shapeRim(Canvas canvas, Path outline, float level) {
        edge.setStrokeWidth(rimWidth);
        edge.setColor(tint(blend(rim, pressedRim, level)));
        canvas.drawPath(outline, edge);
    }

    /** The body paint in {@code color} at the current opacity, for a fill on a part. */
    Paint fill(int color) {
        body.setColor(tint(color));
        return body;
    }

    /** A plain line paint {@code width} wide in {@code color} at the current opacity, for rings, ticks and arcs. */
    Paint line(int color, float width) {
        edge.setStrokeWidth(width);
        edge.setColor(tint(color));
        return edge;
    }

    /**
     * The paint for one pass of a glyph drawn as lines {@code width} wide: pass 0 is its dark hairline, a little wider
     * and drawn first, pass 1 the glyph itself in the label colour. Round caps and joins, so a glyph stays soft at any size.
     */
    Paint ink(int pass, float width) {
        return ink(pass, width, label);
    }

    /** As {@link #ink(int, float)} in {@code color}. */
    Paint ink(int pass, float width, int color) {
        ink.setStrokeWidth(pass == 0 ? width + 2 * keyline : width);
        ink.setColor(tint(pass == 0 ? outline : color));
        return ink;
    }

    /** A glyph of text centred on ({@code cx}, {@code cy}) in the label colour, over its dark hairline. */
    void text(Canvas canvas, String glyph, float cx, float cy, float size) {
        text(canvas, glyph, cx, cy, size, label);
    }

    /** As {@link #text(Canvas, String, float, float, float)} in {@code color}. */
    void text(Canvas canvas, String glyph, float cx, float cy, float size, int color) {
        text.setTextSize(size);
        float baseline = cy - (text.descent() + text.ascent()) / 2;
        text.setStyle(Paint.Style.STROKE);
        text.setStrokeWidth(2 * keyline);
        text.setColor(tint(outline));
        canvas.drawText(glyph, cx, baseline, text);
        text.setStyle(Paint.Style.FILL);
        text.setColor(tint(color));
        canvas.drawText(glyph, cx, baseline, text);
    }

    /** How wide {@code glyph} draws at {@code size}. */
    float textWidth(String glyph, float size) {
        text.setTextSize(size);
        return text.measureText(glyph);
    }

    /** How far past a part's edge its light spills, in pixels. */
    float glowReach() {
        return GLOW_DP * density;
    }

    /** The light a round part pressed in by {@code level} spills past its edge: amber bands, fainter the farther out. */
    void glowDisc(Canvas canvas, float cx, float cy, float rx, float ry, float level) {
        if (level <= 0 || hidden()) return;
        float step = glowReach() / GLOW.length;
        edge.setStrokeWidth(step);
        for (int band = 0; band < GLOW.length; band++) {
            float out = (band + .5f) * step;
            edge.setColor(tint(alpha(pressedRim, level * GLOW[band])));
            oval(canvas, cx, cy, rx + out, ry + out, edge);
        }
    }

    /** The light a rounded part pressed in by {@code level} spills past its edge, as {@link #glowDisc}. */
    void glowSlab(Canvas canvas, RectF shape, float radius, float level) {
        if (level <= 0 || hidden()) return;
        float step = glowReach() / GLOW.length;
        edge.setStrokeWidth(step);
        for (int band = 0; band < GLOW.length; band++) {
            float out = (band + .5f) * step;
            edge.setColor(tint(alpha(pressedRim, level * GLOW[band])));
            rect.set(shape);
            rect.inset(-out, -out);
            canvas.drawRoundRect(rect, radius + out, radius + out, edge);
        }
    }

    /**
     * The light one side of a round part's edge spills when only that side is lit: {@code sweep} degrees around
     * {@code angle} (0 is right, 90 down), each band ending a little before the one inside it.
     */
    void glowArc(Canvas canvas, float cx, float cy, float radius, float angle, float sweep, float level) {
        if (level <= 0 || hidden()) return;
        float step = glowReach() / GLOW.length;
        edge.setStrokeWidth(step);
        for (int band = 0; band < GLOW.length; band++) {
            float out = radius + (band + .5f) * step, span = sweep - 2 * ARC_TAPER * band;
            if (span <= 0) break;
            edge.setColor(tint(alpha(pressedRim, level * GLOW[band])));
            rect.set(cx - out, cy - out, cx + out, cy + out);
            canvas.drawArc(rect, angle - span / 2, span, false, edge);
        }
    }

    private void oval(Canvas canvas, float cx, float cy, float rx, float ry, Paint paint) {
        if (rx == ry) {
            canvas.drawCircle(cx, cy, rx, paint);
        } else {
            rect.set(cx - rx, cy - ry, cx + rx, cy + ry);
            canvas.drawOval(rect, paint);
        }
    }

    static int blend(int from, int to, float t) {
        if (t <= 0) return from;
        if (t >= 1) return to;
        return ColorUtils.blendARGB(from, to, t);
    }

    static int alpha(int color, float share) {
        return ColorUtils.setAlphaComponent(color, Math.max(0, Math.min(255, Math.round(Color.alpha(color) * share))));
    }
}
