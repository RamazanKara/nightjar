package com.limelight.ui;

import android.app.Activity;
import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.ComposeShader;
import android.graphics.LinearGradient;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.PorterDuff;
import android.graphics.RadialGradient;
import android.graphics.Shader;
import android.util.AttributeSet;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewParent;

import androidx.core.content.res.ResourcesCompat;

import com.limelight.R;

/**
 * The stage a page without art stands on: the canvas, one soft wide light from the top centre, and the page's own colour taking
 * over toward the foot (the design's {@code sky.*} tokens). Day is a light page under a faint cool light, OLED and a TV the darker
 * canvas under a fainter one, high contrast solid black. The light is a wide ellipse whose middle lies above the window and
 * whose falloff is a bell curve, so it reads as light on a matte floor and never as a disc or a cone with an edge.
 *
 * <p>It is one opaque, dithered draw, shaded once for each size: nothing moves, nothing is allocated while it rests, nothing blurs.
 */
public class StageView extends View {
    /** Where the page's own colour is reached, top to bottom; the desktop app's stage has the same stop. */
    static final float PAGE_AT = .46f;
    /** The light's middle, above the window's top edge, in window heights: the brightest point is never on screen. */
    static final float LIGHT_ABOVE = .16f;
    /** How far down the light reaches from its middle, in window heights, and no further than this many window widths. */
    static final float LIGHT_DOWN = .86f;
    static final float LIGHT_DOWN_OF_WIDTH = 1.1f;
    /** Its half width in window widths: wider than the window on a phone held upright, inside it on a wide screen. */
    static final float LIGHT_ACROSS_WIDE = .6f;
    static final float LIGHT_ACROSS_TALL = .95f;
    /** The light on an OLED page and a TV, against the token's strength: a dark room needs less of it. */
    static final float LIGHT_DIM = .6f;
    /** The bell curve's width against the reach, and how many stops draw it. */
    private static final float BELL = .36f;
    private static final int BELL_STOPS = 9;
    private static final int BAKED_WIDTH = 64;
    private static final int BAKED_HEIGHT = 96;

    private final Paint fill = new Paint(Paint.DITHER_FLAG);
    private final Matrix across = new Matrix();
    private final AppTheme.Look look;
    private final boolean television;
    private int shadedW;
    private int shadedH;
    private boolean bleed;

    public StageView(Context context) {
        this(context, null);
    }

    public StageView(Context context, AttributeSet attrs) {
        super(context, attrs);
        look = AppTheme.look(context);
        television = GlassForm.of(context) == GlassForm.TELEVISION;
        setWillNotDraw(false);
    }

    private static int color(Context context, int id) {
        return ResourcesCompat.getColor(context.getResources(), id, null);
    }

    /** The flat fill of a PC's card or a group of rows on this look's stage. */
    public static int groupFill(Context context) {
        switch (AppTheme.look(context)) {
            case DAY:
                return color(context, R.color.nj_day_tint_group_on_sky_fill);
            case OLED:
                return color(context, R.color.nj_oled_tint_group_on_sky_fill);
            case HIGH_CONTRAST:
                return color(context, R.color.nj_solid_group_fill);
            default:
                return color(context, R.color.nj_tint_group_on_sky_fill);
        }
    }

    /** The lines between a group's rows on this look. */
    public static int separator(Context context) {
        return AppTheme.look(context) == AppTheme.Look.DAY ? color(context, R.color.nj_day_tint_separator) : color(context, R.color.nj_tint_separator);
    }

    /** The accent as ink, for an action that is not filled with it. */
    public static int accentInk(Context context) {
        switch (AppTheme.look(context)) {
            case DAY:
                return color(context, R.color.nj_day_color_moon_ink);
            case HIGH_CONTRAST:
                return color(context, R.color.nj_hc_color_moon);
            default:
                return color(context, R.color.nj_color_moon_ink);
        }
    }

    /**
     * Puts the stage behind an art-less page that is built on a plain content view, such as Settings' sub-pages. Call after
     * setContentView.
     */
    public static StageView behind(Activity page) {
        return placeBehind(page, new StageView(page));
    }

    static <T extends StageView> T placeBehind(Activity page, T stage) {
        ViewGroup content = page.findViewById(android.R.id.content);
        stage.setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
        content.addView(stage, 0, new ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        // The content view carries the TV margin and the safe area as padding: the stage goes edge to edge past it, or the
        // page stands as a lighter frame inset from the screen's edge.
        stage.bleedPastPadding();
        return stage;
    }

    /**
     * Lays the stage over the whole window content, past the padding the content view carries for the TV margin and the safe
     * area, whether the stage is its child or stands in the page's own root inside it.
     */
    public void bleedPastPadding() {
        bleed = true;
        for (ViewParent at = getParent(); at instanceof ViewGroup; at = at.getParent()) {
            ViewGroup group = (ViewGroup) at;
            group.setClipChildren(false);
            group.setClipToPadding(false);
            if (group.getId() == android.R.id.content) break;
        }
        requestLayout();
    }

    @Override
    public void layout(int l, int t, int r, int b) {
        if (!bleed) {
            super.layout(l, t, r, b);
            return;
        }
        // The parent's place inside the content view; the stage spans the content view's whole frame from there.
        int x = 0, y = 0;
        View content = null;
        for (ViewParent at = getParent(); at instanceof View; at = at.getParent()) {
            View view = (View) at;
            if (view.getId() == android.R.id.content) {
                content = view;
                break;
            }
            x += view.getLeft();
            y += view.getTop();
        }
        if (content == null) super.layout(l, t, r, b);
        else super.layout(-x, -y, content.getWidth() - x, content.getHeight() - y);
    }

    /**
     * The light's ellipse in a window of this size: its middle's x and y and its reach across and down, in pixels. The middle
     * is above the window, and the reach down ends well above the foot, where lists and docks stand on the page's own colour.
     */
    static float[] light(int w, int h) {
        float down = Math.min(h * LIGHT_DOWN, w * LIGHT_DOWN_OF_WIDTH);
        float acrossBy = w >= h ? LIGHT_ACROSS_WIDE : LIGHT_ACROSS_TALL;
        return new float[] {w / 2f, -h * LIGHT_ABOVE, w * acrossBy, down};
    }

    /** How much of the light is left at {@code t} of its reach: a bell curve brought to nothing at the rim, with no edge. */
    static float falloff(float t) {
        if (t >= 1f) return 0f;
        float rim = (float) Math.exp(-1f / (2f * BELL * BELL));
        float at = (float) Math.exp(-(t * t) / (2f * BELL * BELL));
        return (at - rim) / (1f - rim);
    }

    /** The light's strength on this look, against the token's. */
    private float strength() {
        if (look == AppTheme.Look.HIGH_CONTRAST) return 0f;
        return look == AppTheme.Look.OLED || (television && look != AppTheme.Look.DAY) ? LIGHT_DIM : 1f;
    }

    private static int mix(int from, int to, float by) {
        int r = Math.round(((from >> 16) & 0xFF) + (((to >> 16) & 0xFF) - ((from >> 16) & 0xFF)) * by);
        int g = Math.round(((from >> 8) & 0xFF) + (((to >> 8) & 0xFF) - ((from >> 8) & 0xFF)) * by);
        int b = Math.round((from & 0xFF) + ((to & 0xFF) - (from & 0xFF)) * by);
        return 0xFF000000 | (r << 16) | (g << 8) | b;
    }

    /** The page's colours, top to bottom, on this look. */
    private int[] page() {
        Context context = getContext();
        switch (look) {
            case DAY:
                return new int[] {color(context, R.color.nj_day_sky_top), color(context, R.color.nj_day_sky_mid), color(context, R.color.nj_day_sky_bottom)};
            case OLED:
                return new int[] {color(context, R.color.nj_oled_sky_top), color(context, R.color.nj_oled_sky_mid), color(context, R.color.nj_oled_sky_bottom)};
            case HIGH_CONTRAST:
                return new int[] {0xFF000000, 0xFF000000, 0xFF000000};
            default:
                if (!television) {
                    return new int[] {color(context, R.color.nj_sky_top), color(context, R.color.nj_sky_mid), color(context, R.color.nj_sky_bottom)};
                }
                // A TV's canvas is darker than a phone's: the top comes down with it, and the foot is the canvas itself.
                int canvas = color(context, R.color.nj_color_canvas_tv);
                return new int[] {mix(canvas, color(context, R.color.nj_sky_top), LIGHT_DIM), canvas, canvas};
        }
    }

    private void shade(int w, int h) {
        int[] page = page();
        float strength = strength();
        if (page[0] == page[1] && page[1] == page[2] && strength == 0f) {
            fill.setShader(null);
            fill.setColor(page[0]);
            return;
        }
        Shader floor = new LinearGradient(0, 0, 0, h, page, new float[] {0f, PAGE_AT, 1f}, Shader.TileMode.CLAMP);
        if (strength == 0f) {
            fill.setShader(floor);
            return;
        }
        int token = color(getContext(), look == AppTheme.Look.DAY ? R.color.nj_day_sky_glow : R.color.nj_sky_glow);
        float alpha = (token >>> 24) * strength;
        int[] colors = new int[BELL_STOPS];
        float[] stops = new float[BELL_STOPS];
        for (int i = 0; i < BELL_STOPS; i++) {
            stops[i] = i / (float) (BELL_STOPS - 1);
            colors[i] = (Math.round(alpha * falloff(stops[i])) << 24) | (token & 0xFFFFFF);
        }
        float[] light = light(w, h);
        RadialGradient pool = new RadialGradient(light[0], light[1], light[3], colors, stops, Shader.TileMode.CLAMP);
        // A circle of the reach down, stretched to the reach across about its own middle.
        across.setScale(light[2] / light[3], 1f, light[0], light[1]);
        pool.setLocalMatrix(across);
        fill.setShader(new ComposeShader(floor, pool, PorterDuff.Mode.SRC_OVER));
    }

    @Override
    protected void onDraw(Canvas canvas) {
        int w = getWidth();
        int h = getHeight();
        if (w != shadedW || h != shadedH) {
            shadedW = w;
            shadedH = h;
            shade(w, h);
        }
        canvas.drawRect(0, 0, w, h, fill);
    }

    /** A small picture of this stage for the glass of the baked tier, which samples a bitmap instead of the page. */
    public Bitmap bake(int windowW, int windowH) {
        if (windowW <= 0 || windowH <= 0) return null;
        shade(windowW, windowH);
        shadedW = windowW;
        shadedH = windowH;
        Bitmap made = Bitmap.createBitmap(BAKED_WIDTH, BAKED_HEIGHT, Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(made);
        canvas.scale(BAKED_WIDTH / (float) windowW, BAKED_HEIGHT / (float) windowH);
        canvas.drawRect(0, 0, windowW, windowH, fill);
        return made;
    }
}
