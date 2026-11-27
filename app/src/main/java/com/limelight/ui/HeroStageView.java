package com.limelight.ui;

import android.animation.Animator;
import android.animation.AnimatorListenerAdapter;
import android.animation.ValueAnimator;
import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapShader;
import android.graphics.Canvas;
import android.graphics.ComposeShader;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.PorterDuff;
import android.graphics.RadialGradient;
import android.graphics.Rect;
import android.graphics.Shader;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.util.AttributeSet;
import android.view.View;
import android.view.animation.LinearInterpolator;

import com.google.android.material.color.MaterialColors;
import com.limelight.R;

import java.util.concurrent.Executor;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/**
 * The hero's art on the stage: the cover of the game Home is about, shown wide behind the hero's words and faded into the
 * stage, in the stage's own light.
 *
 * <p>In a wide window the art takes the end side of the hero and is lit from that edge; in a tall one it lies across the top.
 * Either way it is the cover, cropped to its upper part, where most covers have their picture, behind a soft round mask.
 *
 * <p>One rule holds on every form and in every theme: the art is the ambient's recipe on the sharp picture. A copy of the
 * cover is baked once, off the main thread ({@link #held}): its colour lifted, yellows turned toward orange so they never
 * read as Play, and each pixel's luminance held to {@code ambient.textZoneMaxLuma} under light words, or lifted to
 * {@code day.ambient.textZoneMinLuma} under dark ones. The art keeps its colour and is never brighter than the ambient may
 * be, so whatever is readable on the stage is as readable on the art: the hero's name and its fainter line keep WCAG's 4.5:1
 * and a button's glyph more than 3:1 wherever they stand, and no scrim has to be placed under them.
 *
 * <p>A change of game cross-fades over {@code motion.ambient.ms} once its copy is baked; the caller makes it a cut with
 * reduced motion. At rest nothing moves and nothing is allocated. High contrast shows no art: its page is solid.
 */
public class HeroStageView extends View {
    /** A wide window: the art's share of the width, from the end edge. */
    static final float ART_SHARE = .62f;
    /** A tall window: how far the art's mask reaches across, in window widths, so only its foot fades. */
    static final float TALL_REACH = 1.2f;
    /** The art is whole this far from the mask's middle, as a share of its reach, and gone at the reach. */
    static final float HOLD = .5f;
    /** The mask's middle, down the art's height: on the end edge of a wide window, in the middle of a tall one. */
    static final float MIDDLE_DOWN_WIDE = .34f;
    static final float MIDDLE_DOWN_TALL = .25f;
    /** Of the cover's height that does not fit, this share is cut off above: the picture is in the upper part. */
    static final float CROP_ABOVE = .22f;
    /** The baked copy's longer side at most, in pixels: a 2:3 cover of 96x144, blurred wide so cover text dissolves. */
    static final int HELD_SIDE = 144;
    /** Three wide separable box passes on the 96x144 grid melt cover typography and 8x8 JPEG macroblocks into smooth light. */
    static final int HELD_BLUR_RADIUS = 12;
    static final int HELD_BLUR_PASSES = 3;
    /** WCAG's contrast for text, and for the glyph of a control. */
    public static final double TEXT_CONTRAST = 4.5;
    public static final double CONTROL_CONTRAST = 3.0;
    private static final float[] MASK_STOPS = {0f, HOLD, HOLD + (1f - HOLD) * .25f, HOLD + (1f - HOLD) * .5f, HOLD + (1f - HOLD) * .75f, 1f};
    private static final int[] MASK_COLORS = {0xFFFFFFFF, 0xFFFFFFFF, 0xD7FFFFFF, 0x80FFFFFF, 0x28FFFFFF, 0x00FFFFFF};
    /** One thread for every stage, gone again half a minute after its last bake. */
    private static final Executor WORKER = worker();

    private final Paint incoming = new Paint(Paint.FILTER_BITMAP_FLAG | Paint.DITHER_FLAG);
    private final Paint outgoing = new Paint(Paint.FILTER_BITMAP_FLAG | Paint.DITHER_FLAG);
    private final Rect artRect = new Rect();
    private final Matrix matrix = new Matrix();
    private final Handler main = new Handler(Looper.getMainLooper());
    private final boolean highContrast;
    private final long fadeMs;
    private Executor executor = WORKER;
    /** The words over the art are dark: the art is the day's. */
    private boolean day;
    private int canvasColor;
    private Shader mask;
    /** The cover that was asked for, and which asking it was: a bake that comes back late is dropped. */
    private Bitmap source;
    private int asked;
    /** The baked copies: the one shown or coming in, and the one going. */
    private Bitmap art;
    private Bitmap old;
    private ValueAnimator change;
    /** The change's progress: 1 at rest. */
    private float fade = 1f;
    private float heroShare = .5f;

    public HeroStageView(Context context) {
        this(context, null);
    }

    public HeroStageView(Context context, AttributeSet attrs) {
        super(context, attrs);
        highContrast = AppTheme.isHighContrast(context);
        day = dark(MaterialColors.getColor(context, com.google.android.material.R.attr.colorOnSurface, 0xFFF5F7FA));
        fadeMs = context.getResources().getInteger(R.integer.nj_motion_ambient_ms);
        canvasColor = MaterialColors.getColor(context, android.R.attr.colorBackground, 0xFF050608) | 0xFF000000;
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
        setFocusable(false);
    }

    private static Executor worker() {
        ThreadPoolExecutor pool = new ThreadPoolExecutor(0, 1, 30, TimeUnit.SECONDS, new LinkedBlockingQueue<>(), task -> {
            Thread thread = new Thread(task, "hero-art");
            thread.setDaemon(true);
            thread.setPriority(Thread.NORM_PRIORITY - 1);
            return thread;
        });
        return pool;
    }

    /** WCAG's relative luminance of an opaque colour. */
    public static double luminance(int color) {
        return .2126 * linear((color >> 16) & 0xFF) + .7152 * linear((color >> 8) & 0xFF) + .0722 * linear(color & 0xFF);
    }

    private static double linear(int channel) {
        double value = channel / 255.0;
        return value <= .04045 ? value / 12.92 : Math.pow((value + .055) / 1.055, 2.4);
    }

    /** WCAG's contrast ratio of two opaque colours, 1 to 21. */
    public static double contrast(int a, int b) {
        double first = luminance(a);
        double second = luminance(b);
        return (Math.max(first, second) + .05) / (Math.min(first, second) + .05);
    }

    /** Words of this colour are the dark ones, written on a light page. */
    static boolean dark(int ink) {
        return luminance(ink | 0xFF000000) < .5;
    }

    /** {@code over} drawn at {@code share} of its strength on {@code under}, as the screen blends it. */
    static int mix(int under, int over, float share) {
        int red = Math.round(((under >> 16) & 0xFF) * (1f - share) + ((over >> 16) & 0xFF) * share);
        int green = Math.round(((under >> 8) & 0xFF) * (1f - share) + ((over >> 8) & 0xFF) * share);
        int blue = Math.round((under & 0xFF) * (1f - share) + (over & 0xFF) * share);
        return 0xFF000000 | (red << 16) | (green << 8) | blue;
    }

    /**
     * The ambient's values on each of these pixels: the night's hold, or the day's lift for {@code day}. A pixel that is not
     * opaque lies on {@code canvas} first.
     */
    static void hold(int[] pixels, int canvas, boolean day) {
        float[] hsv = new float[3];
        int last = 0;
        int lastHeld = 0;
        boolean any = false;
        for (int i = 0; i < pixels.length; i++) {
            int color = pixels[i];
            int alpha = color >>> 24;
            color = alpha == 255 ? color : mix(canvas, color, alpha / 255f);
            // Flat art repeats its colours: the one before is the commonest.
            if (!any || color != last) {
                any = true;
                last = color;
                lastHeld = day ? BackdropBlur.bakeDay(color, hsv) : BackdropBlur.bakeNight(color, hsv);
            }
            pixels[i] = lastHeld;
        }
    }

    /**
     * The cover in the stage's light: no longer than {@link #HELD_SIDE} on its longer side, with the ambient's values on every
     * pixel. A new opaque bitmap, and the cover is left as it is; null when the cover cannot be read or there is no memory for
     * the copy, which leaves the stage without art rather than with art that is too bright.
     */
    static Bitmap held(Bitmap cover, int canvas, boolean day) {
        Bitmap readable = null;
        Bitmap small = null;
        try {
            if (cover == null || cover.isRecycled() || cover.getWidth() <= 0 || cover.getHeight() <= 0) return null;
            // The artwork loader decodes straight to the GPU on newer Android: its pixels cannot be read there.
            readable = hardware(cover) ? cover.copy(Bitmap.Config.ARGB_8888, false) : cover;
            if (readable == null) return null;
            float shrink = Math.min(1f, HELD_SIDE / (float) Math.max(readable.getWidth(), readable.getHeight()));
            int width = Math.max(1, Math.round(readable.getWidth() * shrink));
            int height = Math.max(1, Math.round(readable.getHeight() * shrink));
            int opaqueCanvas = canvas | 0xFF000000;
            small = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
            small.eraseColor(opaqueCanvas);
            BackdropBlur.smooth(readable, small);
            int[] pixels = new int[width * height];
            small.getPixels(pixels, 0, width, 0, 0, width, height);
            int radius = Math.max(1, Math.round(HELD_BLUR_RADIUS * Math.max(width, height) / (float) HELD_SIDE));
            BackdropBlur.dissolve(pixels, width, height, radius, HELD_BLUR_PASSES);
            hold(pixels, opaqueCanvas, day);
            Bitmap result = Bitmap.createBitmap(pixels, width, height, Bitmap.Config.ARGB_8888);
            result.setHasAlpha(false);
            return result;
        } catch (RuntimeException | OutOfMemoryError e) {
            return null;
        } finally {
            // Only the copies made here are let go: the cover is its cache's.
            if (small != null && small != readable && small != cover) small.recycle();
            if (readable != null && readable != cover) readable.recycle();
        }
    }

    /** Where the art goes in a window of this size, whose hero takes {@code heroShare} of the height. */
    static void artRect(int width, int height, float heroShare, boolean rtl, Rect out) {
        int bottom = Math.round(height * heroShare);
        if (width > height) {
            int across = Math.round(width * ART_SHARE);
            out.set(rtl ? 0 : width - across, 0, rtl ? across : width, bottom);
        } else {
            out.set(0, 0, width, bottom);
        }
    }

    /**
     * How a cover fills {@code rect}: {scale, left, top} of the scaled cover. It covers the rect, centred across; down, most of
     * what does not fit is cut off below.
     */
    static float[] place(int coverWidth, int coverHeight, Rect rect) {
        float scale = Math.max(rect.width() / (float) coverWidth, rect.height() / (float) coverHeight);
        float left = rect.left + (rect.width() - coverWidth * scale) / 2f;
        float top = rect.top - (coverHeight * scale - rect.height()) * CROP_ABOVE;
        return new float[] {scale, left, top};
    }

    /** The art's mask in {@code rect}: {middle x, middle y, reach across, reach down}. It is whole to {@link #HOLD} of the reach. */
    static float[] mask(Rect rect, int width, boolean wide, boolean rtl) {
        float down = wide ? MIDDLE_DOWN_WIDE : MIDDLE_DOWN_TALL;
        float x = wide ? (rtl ? rect.left : rect.right) : width / 2f;
        return new float[] {x, rect.top + rect.height() * down, wide ? rect.width() : width * TALL_REACH, rect.height() * (1f - down)};
    }

    /** The stage's colour here: what a cover with transparency lies on. */
    public void setCanvasColor(int color) {
        int opaque = color | 0xFF000000;
        if (opaque == canvasColor) return;
        canvasColor = opaque;
        if (source != null) bake(false);
    }

    /** The colour of the words that stand on the art: light words get the night's art, dark ones the day's. */
    public void setInk(int color) {
        boolean wanted = dark(color);
        if (wanted == day) return;
        day = wanted;
        if (source != null) bake(false);
    }

    /** Whether the art is the day's: lifted under dark words instead of held down under light ones. */
    public boolean isDay() {
        return day;
    }

    /** The hero's share of the stage's height: the art ends where the hero does. */
    public void setHeroShare(float share) {
        if (heroShare == share) return;
        heroShare = share;
        layOut();
    }

    /** For tests: bakes in the caller's thread. */
    public void setWorker(Executor worker) {
        executor = worker;
    }

    /**
     * The game's cover, or null for none: a PC, or no game at all. {@code animate} cross-fades from what is shown, once the
     * cover's copy is baked; the caller passes false with reduced motion. The cover itself is never drawn and never changed.
     */
    public void show(Bitmap cover, boolean animate) {
        if (cover == source) return;
        source = cover;
        bake(animate);
    }

    /** The cover that was asked for goes to the worker, and what was shown stays until its copy is back. */
    private void bake(boolean animate) {
        int asking = ++asked;
        Bitmap cover = source;
        if (cover == null || highContrast) {
            change(null, animate && !highContrast);
            return;
        }
        int page = canvasColor;
        boolean lifted = day;
        executor.execute(() -> {
            Bitmap made = held(cover, page, lifted);
            if (Looper.myLooper() == Looper.getMainLooper()) arrive(asking, made, animate);
            else main.post(() -> arrive(asking, made, animate));
        });
    }

    private void arrive(int asking, Bitmap made, boolean animate) {
        if (asking == asked) change(made, animate && isAttachedToWindow());
    }

    private void change(Bitmap next, boolean animate) {
        if (next == art) return;
        land();
        old = art;
        outgoing.setShader(incoming.getShader());
        art = next;
        incoming.setShader(shaderOf(next));
        if (!animate || getWidth() == 0) {
            land();
            return;
        }
        fade = 0f;
        ValueAnimator running = ValueAnimator.ofFloat(0f, 1f);
        change = running;
        running.setDuration(fadeMs);
        running.setInterpolator(new LinearInterpolator());
        running.addUpdateListener(animation -> {
            if (change != running) return;
            fade = (float) animation.getAnimatedValue();
            invalidate();
        });
        running.addListener(new AnimatorListenerAdapter() {
            @Override public void onAnimationEnd(Animator animation) {
                if (change == running) land();
            }
        });
        running.start();
    }

    /** Whether a cover was asked for: shown, coming in or still being baked. */
    public boolean hasArt() {
        return source != null;
    }

    /** Whether a change is under way. */
    public boolean isChanging() {
        return change != null;
    }

    /** The baked copy that is shown or coming in; null for none. */
    Bitmap art() {
        return art;
    }

    /** A change lands where it is headed: what was shown is let go. */
    private void land() {
        ValueAnimator running = change;
        change = null;
        if (running != null) running.cancel();
        old = null;
        outgoing.setShader(null);
        fade = 1f;
        invalidate();
    }

    private Shader shaderOf(Bitmap picture) {
        if (picture == null || mask == null || artRect.isEmpty()) return null;
        float[] at = place(picture.getWidth(), picture.getHeight(), artRect);
        BitmapShader shader = new BitmapShader(picture, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP);
        matrix.setScale(at[0], at[0]);
        matrix.postTranslate(at[1], at[2]);
        shader.setLocalMatrix(matrix);
        return new ComposeShader(shader, mask, PorterDuff.Mode.DST_IN);
    }

    private boolean isRtl() {
        return getLayoutDirection() == LAYOUT_DIRECTION_RTL;
    }

    /** The art's place and mask for this size; a change that is under way lands, its shaders were for the old size. */
    private void layOut() {
        int w = getWidth();
        int h = getHeight();
        if (w == 0 || h == 0) return;
        boolean rtl = isRtl();
        artRect(w, h, heroShare, rtl, artRect);
        if (artRect.isEmpty()) return;
        float[] at = mask(artRect, w, w > h, rtl);
        // A circle of the reach down, stretched to the reach across about its own middle.
        RadialGradient gradient = new RadialGradient(at[0], at[1], at[3], MASK_COLORS, MASK_STOPS, Shader.TileMode.CLAMP);
        matrix.setScale(at[2] / at[3], 1f, at[0], at[1]);
        gradient.setLocalMatrix(matrix);
        mask = gradient;
        land();
        incoming.setShader(shaderOf(art));
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldw, int oldh) {
        super.onSizeChanged(w, h, oldw, oldh);
        layOut();
    }

    @Override
    public void onRtlPropertiesChanged(int layoutDirection) {
        super.onRtlPropertiesChanged(layoutDirection);
        if (mask != null) layOut();
    }

    @Override
    protected void onDetachedFromWindow() {
        land();
        super.onDetachedFromWindow();
    }

    private static boolean hardware(Bitmap bitmap) {
        return Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && bitmap.getConfig() == Bitmap.Config.HARDWARE;
    }

    @Override
    protected void onDraw(Canvas canvas) {
        if (highContrast || artRect.isEmpty()) return;
        if (old != null && outgoing.getShader() != null && fade < 1f) {
            // Under a cover that comes in, the old one stays whole; with nothing coming, it goes.
            outgoing.setAlpha(art != null ? 255 : Math.round(255 * (1f - fade)));
            canvas.drawRect(artRect, outgoing);
        }
        if (art != null && incoming.getShader() != null) {
            incoming.setAlpha(Math.round(255 * fade));
            canvas.drawRect(artRect, incoming);
        }
    }
}
