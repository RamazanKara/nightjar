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
import android.util.AttributeSet;
import android.view.View;
import android.view.animation.LinearInterpolator;

import com.google.android.material.color.MaterialColors;
import com.limelight.R;

import java.nio.ByteBuffer;

/**
 * The hero's art on the stage: the cover of the game Home is about, shown wide behind the hero's words and faded into the
 * stage, under a scrim that keeps those words readable on any art.
 *
 * <p>In a wide window the art takes the end side of the hero and is lit from that edge; in a tall one it lies across the top.
 * Either way it is the cover itself, cropped to its upper part, where most covers have their picture, behind a soft round
 * mask. No copy and no blur are made of it, so a change of game costs two small shader objects; it cross-fades over
 * {@code motion.ambient.ms}, and the caller makes it a cut with reduced motion.
 *
 * <p>The scrim is the stage's own colour, whole where the words are ({@link #setWords}) and gone a little past them, with a
 * lighter shade under the top edge for the status bar and the top row. Its strength is not chosen by eye: {@link #scrimShare}
 * is the least that gives WCAG's 4.5:1 to the fainter of the words' two colours over the worst art there could be, white
 * under light words and black under dark ones. It is one small alpha map drawn stretched. At rest nothing moves and nothing
 * is allocated. High contrast shows neither art nor scrim: its page is solid.
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
    /** A wide window: the scrim is whole over at least this share of the width from the start edge, and further under longer words. */
    public static final float BLOCK_SHARE = .46f;
    /** How far past the words the scrim runs out: across, as a share of the width; up and down, of the words' height. */
    static final float RAMP = .2f;
    static final float EDGE = .5f;
    /** And up and down no further than this share of the art's height. */
    static final float EDGE_MOST = .2f;
    /** Under the top edge: this share of the scrim, gone this far down the art's height. */
    static final float TOP_SHADE = .6f;
    static final float TOP_BAND = .18f;
    /** WCAG's contrast for text, and what is kept in hand for the rounding of 8-bit alpha. */
    public static final double TEXT_CONTRAST = 4.5;
    static final float SCRIM_MARGIN = .03f;
    /** One texel of the scrim's map covers this many pixels each way. */
    private static final int MAP_STEP = 16;
    private static final float[] MASK_STOPS = {0f, HOLD, HOLD + (1f - HOLD) * .25f, HOLD + (1f - HOLD) * .5f, HOLD + (1f - HOLD) * .75f, 1f};
    private static final int[] MASK_COLORS = {0xFFFFFFFF, 0xFFFFFFFF, 0xD7FFFFFF, 0x80FFFFFF, 0x28FFFFFF, 0x00FFFFFF};

    private final Paint incoming = new Paint(Paint.FILTER_BITMAP_FLAG | Paint.DITHER_FLAG);
    private final Paint outgoing = new Paint(Paint.FILTER_BITMAP_FLAG | Paint.DITHER_FLAG);
    private final Paint scrim = new Paint(Paint.FILTER_BITMAP_FLAG | Paint.DITHER_FLAG);
    private final Rect artRect = new Rect();
    private final Rect window = new Rect();
    private final Rect words = new Rect();
    private final Matrix matrix = new Matrix();
    private final boolean highContrast;
    private final long fadeMs;
    private int ink;
    private int faintInk;
    private Shader mask;
    private Bitmap art;
    private Bitmap old;
    private ValueAnimator change;
    /** The change's progress: 1 at rest. */
    private float fade = 1f;
    private float heroShare = .5f;
    private int canvasColor;
    private float share;
    private Bitmap map;
    private ByteBuffer mapBytes;
    private boolean mapStale = true;

    public HeroStageView(Context context) {
        this(context, null);
    }

    public HeroStageView(Context context, AttributeSet attrs) {
        super(context, attrs);
        highContrast = AppTheme.isHighContrast(context);
        ink = MaterialColors.getColor(context, com.google.android.material.R.attr.colorOnSurface, 0xFFF5F7FA) | 0xFF000000;
        faintInk = MaterialColors.getColor(context, com.google.android.material.R.attr.colorOnSurfaceVariant, 0xFFC7CDD9) | 0xFF000000;
        fadeMs = context.getResources().getInteger(R.integer.nj_motion_ambient_ms);
        setCanvasColor(MaterialColors.getColor(context, android.R.attr.colorBackground, 0xFF050608));
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
        setFocusable(false);
    }

    /** WCAG's relative luminance of an opaque colour. */
    static double luminance(int color) {
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

    /** {@code over} drawn at {@code share} of its strength on {@code under}, as the screen blends it. */
    static int mix(int under, int over, float share) {
        int red = Math.round(((under >> 16) & 0xFF) * (1f - share) + ((over >> 16) & 0xFF) * share);
        int green = Math.round(((under >> 8) & 0xFF) * (1f - share) + ((over >> 8) & 0xFF) * share);
        int blue = Math.round((under & 0xFF) * (1f - share) + (over & 0xFF) * share);
        return 0xFF000000 | (red << 16) | (green << 8) | blue;
    }

    /** The art that is hardest on words of this colour over this scrim: white under light words, black under dark ones. */
    static int worstArt(int text, int scrimColor) {
        return luminance(text) > luminance(scrimColor) ? 0xFFFFFFFF : 0xFF000000;
    }

    /**
     * The least share of the scrim that gives words of colour {@code text} the contrast {@code wanted} over the worst art;
     * 1 where even the bare scrim does not, which a theme's own page colour never leaves its labels at.
     */
    public static float scrimShare(int text, int scrimColor, double wanted) {
        int worst = worstArt(text, scrimColor);
        if (contrast(text, mix(worst, scrimColor, 1f)) < wanted) return 1f;
        float low = 0f;
        float high = 1f;
        for (int i = 0; i < 20; i++) {
            float middle = (low + high) / 2f;
            if (contrast(text, mix(worst, scrimColor, middle)) >= wanted) high = middle;
            else low = middle;
        }
        return high;
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

    /** 0 to 1 with no kink at either end. */
    static float smooth(float value) {
        float t = Math.max(0f, Math.min(1f, value));
        return t * t * (3f - 2f * t);
    }

    /**
     * How much of the scrim stands at a point: whole from the words' top to their foot and, in a wide window, from the start
     * edge to their end; running out over {@code edge} above and below and over {@code ramp} past their end; and never less
     * than the shade under the top edge. {@code fromStart} is the distance from the start edge; a tall window passes no end.
     */
    static float scrimAt(float fromStart, float y, float wordsEnd, float ramp, float wordsTop, float wordsFoot, float edge, float topBand) {
        float shade = topBand <= 0f ? 0f : TOP_SHADE * (1f - smooth(y / topBand));
        if (wordsFoot <= wordsTop) return shade;
        float down = y < wordsTop ? 1f - smooth((wordsTop - y) / edge) : 1f - smooth((y - wordsFoot) / edge);
        float across = ramp <= 0f ? 1f : 1f - smooth((fromStart - wordsEnd) / ramp);
        return Math.max(shade, down * across);
    }

    /** The stage's colour here: the scrim is made of it. */
    public void setCanvasColor(int color) {
        int opaque = color | 0xFF000000;
        if (opaque == canvasColor) return;
        canvasColor = opaque;
        scrim.setColor(opaque);
        weigh();
    }

    /** The colours of the hero's words, the strong and the faint: the scrim is as strong as the harder of them needs. */
    public void setInks(int strong, int faint) {
        if (ink == (strong | 0xFF000000) && faintInk == (faint | 0xFF000000)) return;
        ink = strong | 0xFF000000;
        faintInk = faint | 0xFF000000;
        weigh();
    }

    private void weigh() {
        share = Math.min(1f, Math.max(scrimShare(ink, canvasColor, TEXT_CONTRAST), scrimShare(faintInk, canvasColor, TEXT_CONTRAST)) + SCRIM_MARGIN);
        mapStale = true;
        invalidate();
    }

    /** The hero's share of the stage's height: the art ends where the hero does. */
    public void setHeroShare(float share) {
        if (heroShare == share) return;
        heroShare = share;
        layOut();
    }

    /** Where the hero's words stand in this view: the scrim is whole there. */
    public void setWords(int left, int top, int right, int bottom) {
        if (words.left == left && words.top == top && words.right == right && words.bottom == bottom) return;
        words.set(left, top, right, bottom);
        mapStale = true;
        invalidate();
    }

    /** Where the words stand, as {@link #setWords} was told; empty for none. */
    public void getWords(Rect out) {
        out.set(words);
    }

    /** The hero has no words now: only the shade under the top edge stays. */
    public void clearWords() {
        setWords(0, 0, 0, 0);
    }

    /**
     * The game's cover, or null for none: a PC, or no game at all. {@code animate} cross-fades from what is shown; the caller
     * passes false with reduced motion.
     */
    public void show(Bitmap cover, boolean animate) {
        if (cover == art) return;
        land();
        old = art;
        outgoing.setShader(incoming.getShader());
        art = cover;
        incoming.setShader(shaderOf(cover));
        if (!animate || highContrast || getWidth() == 0) {
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

    /** Whether a cover is shown or coming in. */
    public boolean hasArt() {
        return art != null;
    }

    /** Whether a change is under way. */
    public boolean isChanging() {
        return change != null;
    }

    /** The scrim's share of the stage's colour where the words are. */
    public float scrimShare() {
        return share;
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

    private Shader shaderOf(Bitmap cover) {
        if (cover == null || mask == null || artRect.isEmpty() || cover.isRecycled()) return null;
        float[] at = place(cover.getWidth(), cover.getHeight(), artRect);
        BitmapShader picture = new BitmapShader(cover, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP);
        matrix.setScale(at[0], at[0]);
        matrix.postTranslate(at[1], at[2]);
        picture.setLocalMatrix(matrix);
        return new ComposeShader(picture, mask, PorterDuff.Mode.DST_IN);
    }

    private boolean isRtl() {
        return getLayoutDirection() == LAYOUT_DIRECTION_RTL;
    }

    /** The art's place and mask for this size; a change that is under way lands, its shaders were for the old size. */
    private void layOut() {
        int w = getWidth();
        int h = getHeight();
        window.set(0, 0, w, h);
        mapStale = true;
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

    /** The scrim's map for this size and these words: a byte for each sixteenth pixel each way, strongest where the words are. */
    private void bakeMap() {
        mapStale = false;
        int w = getWidth();
        int h = getHeight();
        int columns = Math.max(2, w / MAP_STEP);
        int rows = Math.max(2, h / MAP_STEP);
        if (map == null || map.getWidth() != columns || map.getHeight() != rows) {
            map = Bitmap.createBitmap(columns, rows, Bitmap.Config.ALPHA_8);
            mapBytes = ByteBuffer.allocate(columns * rows);
        }
        boolean wide = w > h;
        boolean rtl = isRtl();
        float texelW = w / (float) columns;
        float texelH = h / (float) rows;
        // The words' place, a texel and a half wider each way: the map is drawn stretched, which softens its edges by that much.
        float top = words.top - texelH * 1.5f;
        float foot = words.isEmpty() ? top : words.bottom + texelH * 1.5f;
        float end = Math.max(w * BLOCK_SHARE, rtl ? w - words.left : words.right) + texelW * 1.5f;
        float edge = Math.max(texelH, Math.min(words.height() * EDGE, artRect.height() * EDGE_MOST));
        float band = artRect.height() * TOP_BAND;
        byte[] bytes = mapBytes.array();
        for (int row = 0; row < rows; row++) {
            float y = (row + .5f) * texelH;
            for (int column = 0; column < columns; column++) {
                float x = (column + .5f) * texelW;
                float shape = scrimAt(rtl ? w - x : x, y, end, wide ? w * RAMP : 0f, top, foot, edge, band);
                bytes[row * columns + column] = (byte) Math.min(255, (int) Math.ceil(share * shape * 255f));
            }
        }
        mapBytes.rewind();
        map.copyPixelsFromBuffer(mapBytes);
    }

    private static boolean hardware(Bitmap bitmap) {
        return bitmap != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && bitmap.getConfig() == Bitmap.Config.HARDWARE;
    }

    @Override
    protected void onDraw(Canvas canvas) {
        if (highContrast || artRect.isEmpty()) return;
        if ((art != null && art.isRecycled()) || (old != null && old.isRecycled())) {
            // Its cache let the cover go under us: nothing is better than a crash.
            art = null;
            incoming.setShader(null);
            land();
        }
        // A cover that lives on the GPU cannot be drawn into a picture of this view.
        if (!canvas.isHardwareAccelerated() && (hardware(art) || hardware(old))) return;
        if (old != null && outgoing.getShader() != null && fade < 1f) {
            // Under a cover that comes in, the old one stays whole; with nothing coming, it goes.
            outgoing.setAlpha(art != null ? 255 : Math.round(255 * (1f - fade)));
            canvas.drawRect(artRect, outgoing);
        }
        if (art != null && incoming.getShader() != null) {
            incoming.setAlpha(Math.round(255 * fade));
            canvas.drawRect(artRect, incoming);
        }
        float strength = art != null ? (old != null ? 1f : fade) : (old != null ? 1f - fade : 0f);
        if (strength <= 0f) return;
        if (mapStale) bakeMap();
        scrim.setAlpha(Math.round(255 * strength));
        canvas.drawBitmap(map, null, window, scrim);
    }
}
