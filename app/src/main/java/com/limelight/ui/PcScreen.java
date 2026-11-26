package com.limelight.ui;

import android.content.Context;
import android.content.res.Resources;
import android.graphics.Canvas;
import android.graphics.Outline;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.util.AttributeSet;
import android.view.View;
import android.view.ViewOutlineProvider;
import android.widget.FrameLayout;

import androidx.core.content.res.ResourcesCompat;

import com.limelight.R;

/**
 * A PC drawn as a small display on its stand, in one ink: a matte face that catches a little of the stage's light, a thin rim,
 * the neck and the foot. While the PC plays, the game's cover lies on the face; the face is dark while the PC is away. The
 * children lie on the face: the cover, the glyph and the spinner.
 */
public class PcScreen extends FrameLayout {
    private static final float FACE = .62f;
    private static final float RADIUS = .075f;
    private static final float NECK_W = .1f;
    private static final float NECK_H = .07f;
    private static final float BASE_W = .3f;
    private static final float BASE_H = .028f;
    /** How much of the ink the face holds where the light lands, at its far corner and while the PC is away. */
    static final float LIT = .18f;
    static final float DIM = .05f;
    static final float AWAY = .06f;
    /** The rim's share of the ink, and an away PC's. */
    static final float RIM = .32f;
    static final float RIM_AWAY = .16f;

    private final Paint face = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.DITHER_FLAG);
    private final Paint rim = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint neck = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint base = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF rect = new RectF();
    private final Path softClip = new Path();
    private final RectF softRect = new RectF();
    private View backdrop;
    private final float density;
    private final int ink;
    private final int onlineRim;
    private final int offlineRim;
    private final boolean highContrast;
    private boolean away;
    private int shaderW;
    private int shaderH;
    private int faceW;
    private int faceH;
    private float radius;

    public PcScreen(Context context) {
        this(context, null);
    }

    public PcScreen(Context context, AttributeSet attrs) {
        super(context, attrs);
        Resources res = context.getResources();
        density = res.getDisplayMetrics().density;
        highContrast = AppTheme.isHighContrast(context);
        boolean day = AppTheme.look(context) == AppTheme.Look.DAY;
        setWillNotDraw(false);
        setClipChildren(false);
        setClipToPadding(false);
        ink = com.google.android.material.color.MaterialColors.getColor(this, com.google.android.material.R.attr.colorOnSurface, 0xFFF5F7FA);
        onlineRim = highContrast ? ResourcesCompat.getColor(res, R.color.nj_hc_color_rim, null) : ink(ink, RIM);
        offlineRim = highContrast ? onlineRim : ink(ink, RIM_AWAY);
        rim.setStyle(Paint.Style.STROKE);
        rim.setStrokeWidth(highContrast ? res.getDimension(R.dimen.nj_glass_chrome_solid_rim_width) : Math.max(1f, density));
        neck.setColor(ResourcesCompat.getColor(res, day ? R.color.nj_day_tint_track_fill : R.color.nj_tint_track_fill, null));
        int label = com.google.android.material.color.MaterialColors.getColor(this, com.google.android.material.R.attr.colorOnSurfaceVariant, 0xFF8D90A0);
        base.setColor((label & 0xFFFFFF) | 0x80000000);
    }

    /** The ink at this share of its strength. */
    static int ink(int color, float share) {
        return (color & 0xFFFFFF) | (Math.round(255 * share) << 24);
    }

    @Override
    protected void onFinishInflate() {
        super.onFinishInflate();
        View cover = findViewById(R.id.grid_backdrop);
        if (cover == null) return;
        backdrop = cover;
        cover.setOutlineProvider(new ViewOutlineProvider() {
            @Override public void getOutline(View view, Outline outline) {
                outline.setRoundRect(0, 0, view.getWidth(), view.getHeight(), radius);
            }
        });
        cover.setClipToOutline(true);
    }

    /** The screen clips the cover to its outline; a canvas without a GPU knows none and gets the same round rectangle. */
    @Override
    protected boolean drawChild(Canvas canvas, View child, long drawingTime) {
        if (child != backdrop || canvas.isHardwareAccelerated()) return super.drawChild(canvas, child, drawingTime);
        int saved = canvas.save();
        softClip.rewind();
        softRect.set(child.getLeft(), child.getTop(), child.getRight(), child.getBottom());
        softClip.addRoundRect(softRect, radius, radius, Path.Direction.CW);
        canvas.clipPath(softClip);
        boolean more = super.drawChild(canvas, child, drawingTime);
        canvas.restoreToCount(saved);
        return more;
    }

    /** Whether the PC is away: a dark face and a fainter rim. */
    public void setAway(boolean isAway) {
        if (away == isAway) return;
        away = isAway;
        invalidate();
    }

    @Override
    protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
        int w = MeasureSpec.getSize(widthMeasureSpec);
        faceW = w;
        faceH = Math.round(w * FACE);
        int stand = Math.round(w * NECK_H) + standBase(w);
        int total = faceH + stand;
        radius = Math.max(5 * density, Math.round(w * RADIUS));
        super.onMeasure(MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(faceH, MeasureSpec.EXACTLY));
        setMeasuredDimension(w, total);
    }

    private int standBase(int w) {
        return Math.max(Math.round(2 * density), Math.round(w * BASE_H));
    }

    @Override
    protected void onLayout(boolean changed, int left, int top, int right, int bottom) {
        // The children fill the face; the stand under it is this view's own.
        super.onLayout(changed, left, top, right, top + faceH);
    }

    @Override
    protected void dispatchDraw(Canvas canvas) {
        int w = faceW;
        int h = faceH;
        if (w == 0) {
            super.dispatchDraw(canvas);
            return;
        }
        drawStand(canvas, w, h);
        // A drawing casts no shadow. The face lies under a cover too: this view is not told when one arrives.
        if (!highContrast) drawFace(canvas, w, h);
        super.dispatchDraw(canvas);
        float inset = rim.getStrokeWidth() / 2f;
        rect.set(inset, inset, w - inset, h - inset);
        rim.setColor(away ? offlineRim : onlineRim);
        canvas.drawRoundRect(rect, radius - inset, radius - inset, rim);
    }

    private void drawStand(Canvas canvas, int w, int h) {
        float neckW = w * NECK_W;
        float neckH = Math.round(w * NECK_H);
        canvas.drawRect((w - neckW) / 2f, h, (w + neckW) / 2f, h + neckH, neck);
        float baseW = w * BASE_W;
        float baseH = standBase(w);
        rect.set((w - baseW) / 2f, h + neckH, (w + baseW) / 2f, h + neckH + baseH);
        canvas.drawRoundRect(rect, 2 * density, 2 * density, base);
    }

    /** The bare face: the ink, strongest at the top corner the light comes from; one flat faint tone while the PC is away. */
    private void drawFace(Canvas canvas, int w, int h) {
        rect.set(0, 0, w, h);
        if (away) {
            face.setShader(null);
            face.setColor(ink(ink, AWAY));
            shaderW = 0;
        } else if (w != shaderW || h != shaderH) {
            shaderW = w;
            shaderH = h;
            face.setColor(0xFF000000);
            face.setShader(new android.graphics.LinearGradient(0, 0, w * .4f, h, ink(ink, LIT), ink(ink, DIM),
                    android.graphics.Shader.TileMode.CLAMP));
        }
        canvas.drawRoundRect(rect, radius, radius, face);
    }

    /** The face's height as measured, for tests. */
    public int faceHeight() {
        return faceH;
    }
}
