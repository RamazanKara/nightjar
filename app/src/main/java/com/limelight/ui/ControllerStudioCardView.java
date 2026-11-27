package com.limelight.ui;

import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.RectF;
import android.os.Build;
import android.preference.PreferenceManager;
import android.text.TextPaint;
import android.text.TextUtils;
import android.util.AttributeSet;
import android.view.View;

import androidx.core.content.ContextCompat;
import androidx.core.graphics.ColorUtils;

import com.google.android.material.color.MaterialColors;
import com.limelight.R;

/**
 * Visual Controller Studio header card for the Controller and Controls settings screens,
 * pairing a vector controller hardware diagram (sticks, triggers, face button order, gyro
 * and touchpad badges) with a live stick/trigger deadzone response curve preview.
 */
public final class ControllerStudioCardView extends View {
    static final String PREF_STICK_DZ = "seekbar_deadzone";
    static final String PREF_TRIGGER_DZ = "seekbar_trigger_deadzone";
    static final String PREF_SWAP_FACE = "checkbox_flip_face_buttons";
    static final String PREF_MOTION = "checkbox_gamepad_motion_sensors";
    static final String PREF_TOUCHPAD_MOUSE = "checkbox_gamepad_touchpad_as_mouse";

    private final float dp;
    private final float sp;
    private final Paint fillPaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint strokePaint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final TextPaint textPaint = new TextPaint(Paint.ANTI_ALIAS_FLAG);
    private final Path bodyPath = new Path();
    private final Path curvePath = new Path();
    private final Path curveAreaPath = new Path();
    private final RectF cardRect = new RectF();
    private final RectF padRect = new RectF();
    private final RectF chartRect = new RectF();
    private final RectF pillRect = new RectF();

    private int stickDeadzonePercent = 7;
    private int triggerDeadzonePercent = 2;
    private boolean swapFaceButtons;
    private boolean motionEnabled;
    private boolean touchpadAsMouse;

    public ControllerStudioCardView(Context context) {
        this(context, null);
    }

    public ControllerStudioCardView(Context context, AttributeSet attrs) {
        super(context, attrs);
        dp = context.getResources().getDisplayMetrics().density;
        sp = context.getResources().getDisplayMetrics().scaledDensity;
        strokePaint.setStyle(Paint.Style.STROKE);
        strokePaint.setStrokeCap(Paint.Cap.ROUND);
        strokePaint.setStrokeJoin(Paint.Join.ROUND);
        textPaint.setTypeface(AppTheme.uiTypeface(context, true,
                android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)));
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);
        syncFromPreferences(PreferenceManager.getDefaultSharedPreferences(context));
    }

    /** Synchronizes diagram highlights and response curve with the current controller preferences. */
    public void syncFromPreferences(SharedPreferences prefs) {
        if (prefs == null) return;
        stickDeadzonePercent = clamp(prefs.getInt(PREF_STICK_DZ, 7), -20, 20);
        triggerDeadzonePercent = clamp(prefs.getInt(PREF_TRIGGER_DZ, 2), 0, 30);
        swapFaceButtons = prefs.getBoolean(PREF_SWAP_FACE, false);
        motionEnabled = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
                && prefs.getBoolean(PREF_MOTION, Build.VERSION.SDK_INT != Build.VERSION_CODES.S);
        touchpadAsMouse = prefs.getBoolean(PREF_TOUCHPAD_MOUSE, false);
        updateAccessibilitySummary();
        requestLayout();
        invalidate();
    }

    public int stickDeadzonePercent() {
        return stickDeadzonePercent;
    }

    public int triggerDeadzonePercent() {
        return triggerDeadzonePercent;
    }

    public boolean facesSwapped() {
        return swapFaceButtons;
    }

    public boolean motionEnabled() {
        return motionEnabled;
    }

    /**
     * Piecewise stick magnitude response matching {@code nightjar_policy::controller::deadzone::apply_stick_deadzone}:
     * positive {@code deadzonePercent} drops {@code <= r} to 0 and passes outer deflections raw;
     * zero or negative anti-deadzone drops {@code <= 0.01} to 0 and remaps {@code [0.01, 1]} to {@code [|r|, 1]}.
     */
    static float stickOutput(float input, int deadzonePercent) {
        float x = Math.max(0f, Math.min(1f, input));
        float r = deadzonePercent / 100f;
        if (r > 0f) {
            return x <= r ? 0f : x;
        }
        if (x <= 0.01f) {
            return 0f;
        }
        float anti = -r;
        return Math.min(1f, anti + x * (1f - anti));
    }

    static int resolvePrimaryButtonInk(int candidateOnPrimary, int moonFill) {
        int opaqueFill = ColorUtils.setAlphaComponent(moonFill, 0xFF);
        int opaqueCandidate = ColorUtils.setAlphaComponent(candidateOnPrimary, 0xFF);
        if (ColorUtils.calculateContrast(opaqueCandidate, opaqueFill) >= 4.5) {
            return opaqueCandidate;
        }
        return ColorUtils.calculateContrast(0xFFFFFFFF, opaqueFill)
                >= ColorUtils.calculateContrast(0xFF111318, opaqueFill)
                ? 0xFFFFFFFF
                : 0xFF111318;
    }

    private void updateAccessibilitySummary() {
        String faces = swapFaceButtons ? "B A Y X" : "A B X Y";
        setContentDescription(getResources().getString(
                R.string.controller_studio_a11y_summary,
                faces, stickDeadzonePercent, triggerDeadzonePercent));
    }

    @Override
    protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
        int width = MeasureSpec.getSize(widthMeasureSpec);
        if (getVisibility() == GONE) {
            setMeasuredDimension(width, 0);
            return;
        }
        boolean wide = width >= Math.round(520 * dp);
        int desiredHeight = Math.round((wide ? 196 : 284) * dp);
        setMeasuredDimension(width, resolveSize(desiredHeight, heightMeasureSpec));
    }

    @Override
    protected void onDraw(Canvas canvas) {
        super.onDraw(canvas);
        if (getWidth() <= 0 || getHeight() <= 0 || getVisibility() == GONE) return;

        Context ctx = getContext();
        int ink = MaterialColors.getColor(this, com.google.android.material.R.attr.colorOnSurface, 0xFFF5F7FA);
        int muted = MaterialColors.getColor(this, com.google.android.material.R.attr.colorOnSurfaceVariant, 0xFFB8C0CC);
        int moon = MaterialColors.getColor(this, androidx.appcompat.R.attr.colorPrimary,
                ContextCompat.getColor(ctx, R.color.nj_color_moon));
        int onMoon = resolvePrimaryButtonInk(
                MaterialColors.getColor(this, com.google.android.material.R.attr.colorOnPrimary, 0xFF111318),
                moon);
        int surface = MaterialColors.getColor(this, com.google.android.material.R.attr.colorSurfaceContainerHigh, 0xFF171B24);
        int rim = MaterialColors.getColor(this, R.attr.glassGroupRim, ColorUtils.setAlphaComponent(ink, 0x24));
        int cardFill = AppTheme.isHighContrast(ctx) ? 0xFF000000 : ColorUtils.setAlphaComponent(surface, 0xEB);

        float gutterH = 8 * dp;
        float gutterTop = 6 * dp;
        float gutterBottom = 10 * dp;
        cardRect.set(gutterH, gutterTop, getWidth() - gutterH, getHeight() - gutterBottom);
        float radius = 18 * dp;

        fillPaint.setColor(cardFill);
        canvas.drawRoundRect(cardRect, radius, radius, fillPaint);
        strokePaint.setStrokeWidth(AppTheme.isHighContrast(ctx) ? 2 * dp : dp);
        strokePaint.setColor(rim);
        canvas.drawRoundRect(cardRect, radius, radius, strokePaint);

        float innerPad = 14 * dp;
        boolean wide = cardRect.width() >= 500 * dp;
        if (wide) {
            float split = cardRect.left + cardRect.width() * 0.54f;
            padRect.set(cardRect.left + innerPad, cardRect.top + innerPad, split - 8 * dp, cardRect.bottom - innerPad);
            chartRect.set(split + 10 * dp, cardRect.top + innerPad, cardRect.right - innerPad, cardRect.bottom - innerPad);
            strokePaint.setStrokeWidth(dp);
            strokePaint.setColor(ColorUtils.setAlphaComponent(ink, 0x16));
            canvas.drawLine(split, cardRect.top + innerPad, split, cardRect.bottom - innerPad, strokePaint);
        } else {
            float midY = cardRect.top + cardRect.height() * 0.54f;
            padRect.set(cardRect.left + innerPad, cardRect.top + innerPad, cardRect.right - innerPad, midY - 6 * dp);
            chartRect.set(cardRect.left + innerPad, midY + 6 * dp, cardRect.right - innerPad, cardRect.bottom - innerPad);
            strokePaint.setStrokeWidth(dp);
            strokePaint.setColor(ColorUtils.setAlphaComponent(ink, 0x16));
            canvas.drawLine(cardRect.left + innerPad, midY, cardRect.right - innerPad, midY, strokePaint);
        }

        drawControllerDiagram(canvas, padRect, ink, muted, moon, onMoon);
        drawDeadzoneCurve(canvas, chartRect, ink, muted, moon);
    }

    private void drawControllerDiagram(Canvas canvas, RectF area, int ink, int muted, int moon, int onMoon) {
        float badgeH = 20 * dp;
        float badgeGap = 6 * dp;
        float rowGap = 4 * dp;
        String[] badgeLabels = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
                ? new String[] {
                        swapFaceButtons ? "B·A·Y·X" : "A·B·X·Y",
                        getResources().getString(R.string.controller_studio_gyro),
                        getResources().getString(R.string.controller_studio_touchpad),
                }
                : new String[] {
                        swapFaceButtons ? "B·A·Y·X" : "A·B·X·Y",
                        getResources().getString(R.string.controller_studio_touchpad),
                };
        boolean[] badgeStates = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
                ? new boolean[] {swapFaceButtons, motionEnabled, touchpadAsMouse}
                : new boolean[] {swapFaceButtons, touchpadAsMouse};

        textPaint.setTextSize(10.5f * sp);
        int badgeRows = 1;
        float probeX = area.left;
        for (String label : badgeLabels) {
            float w = Math.min(area.width(), textPaint.measureText(label) + 14 * dp);
            if (probeX > area.left && probeX + w > area.right) {
                badgeRows++;
                probeX = area.left;
            }
            probeX += w + badgeGap;
        }
        float badgeBlockH = badgeRows * badgeH + (badgeRows - 1) * rowGap;
        float diagramBottom = area.bottom - badgeBlockH - 6 * dp;
        float cx = area.centerX();
        float cy = (area.top + diagramBottom) * 0.52f;
        float hw = Math.min(area.width() * 0.44f, (diagramBottom - area.top) * 0.86f);
        float hh = hw * 0.58f;

        // Triggers (LT / RT) above shoulders, tinted with trigger deadzone fill.
        float trigW = hw * 0.34f;
        float trigH = hh * 0.22f;
        for (int side : new int[] {-1, 1}) {
            float tx = cx + side * hw * 0.54f;
            float ty = cy - hh * 0.90f;
            pillRect.set(tx - trigW / 2f, ty - trigH / 2f, tx + trigW / 2f, ty + trigH / 2f);
            fillPaint.setColor(ColorUtils.setAlphaComponent(moon, 0x26));
            canvas.drawRoundRect(pillRect, 5 * dp, 5 * dp, fillPaint);
            strokePaint.setStrokeWidth(1.25f * dp);
            strokePaint.setColor(ColorUtils.setAlphaComponent(moon, 0xB8));
            canvas.drawRoundRect(pillRect, 5 * dp, 5 * dp, strokePaint);
        }

        // Ergonomic controller shell silhouette.
        bodyPath.reset();
        bodyPath.moveTo(cx - hw * 0.58f, cy - hh * 0.72f);
        bodyPath.lineTo(cx + hw * 0.58f, cy - hh * 0.72f);
        bodyPath.quadTo(cx + hw * 0.96f, cy - hh * 0.68f, cx + hw, cy - hh * 0.05f);
        bodyPath.quadTo(cx + hw * 1.02f, cy + hh * 0.88f, cx + hw * 0.68f, cy + hh * 0.92f);
        bodyPath.quadTo(cx + hw * 0.46f, cy + hh * 0.92f, cx + hw * 0.34f, cy + hh * 0.42f);
        bodyPath.lineTo(cx - hw * 0.34f, cy + hh * 0.42f);
        bodyPath.quadTo(cx - hw * 0.46f, cy + hh * 0.92f, cx - hw * 0.68f, cy + hh * 0.92f);
        bodyPath.quadTo(cx - hw * 1.02f, cy + hh * 0.88f, cx - hw, cy - hh * 0.05f);
        bodyPath.quadTo(cx - hw * 0.96f, cy - hh * 0.68f, cx - hw * 0.58f, cy - hh * 0.72f);
        bodyPath.close();

        fillPaint.setColor(ColorUtils.setAlphaComponent(ink, 0x0D));
        canvas.drawPath(bodyPath, fillPaint);
        strokePaint.setStrokeWidth(1.5f * dp);
        strokePaint.setColor(ColorUtils.setAlphaComponent(ink, 0x4D));
        canvas.drawPath(bodyPath, strokePaint);

        // Central touchpad slab.
        float padW = hw * 0.42f;
        float padH = hh * 0.34f;
        pillRect.set(cx - padW / 2f, cy - hh * 0.58f, cx + padW / 2f, cy - hh * 0.58f + padH);
        fillPaint.setColor(touchpadAsMouse ? ColorUtils.setAlphaComponent(moon, 0x2E) : ColorUtils.setAlphaComponent(ink, 0x12));
        canvas.drawRoundRect(pillRect, 4 * dp, 4 * dp, fillPaint);
        strokePaint.setStrokeWidth(dp);
        strokePaint.setColor(touchpadAsMouse ? moon : ColorUtils.setAlphaComponent(ink, 0x38));
        canvas.drawRoundRect(pillRect, 4 * dp, 4 * dp, strokePaint);

        // Left stick (upper-left) & Right stick (lower-right) with live deadzone ring.
        float stickR = hh * 0.28f;
        float dzShare = Math.max(0.14f, Math.min(0.82f, Math.abs(stickDeadzonePercent) / 30f));
        drawStickWell(canvas, cx - hw * 0.52f, cy - hh * 0.22f, stickR, dzShare, ink, moon);
        drawStickWell(canvas, cx + hw * 0.26f, cy + hh * 0.08f, stickR * 0.92f, dzShare, ink, moon);

        // D-pad cross (lower-left).
        float dpadX = cx - hw * 0.26f;
        float dpadY = cy + hh * 0.08f;
        float arm = hh * 0.17f;
        strokePaint.setStrokeWidth(hh * 0.12f);
        strokePaint.setColor(ColorUtils.setAlphaComponent(ink, 0x55));
        canvas.drawLine(dpadX - arm, dpadY, dpadX + arm, dpadY, strokePaint);
        canvas.drawLine(dpadX, dpadY - arm, dpadX, dpadY + arm, strokePaint);

        // Face buttons cluster (upper-right) showing Xbox (A bottom) vs Nintendo (B bottom).
        float fx = cx + hw * 0.54f;
        float fy = cy - hh * 0.22f;
        float spread = hh * 0.20f;
        float btnR = Math.max(3.5f * dp, hh * 0.095f);
        String[] glyphs = swapFaceButtons ? new String[] {"X", "A", "B", "Y"} : new String[] {"Y", "B", "A", "X"};
        float[][] offsets = {{0, -spread}, {spread, 0}, {0, spread}, {-spread, 0}};
        textPaint.setTextAlign(Paint.Align.CENTER);
        textPaint.setTextSize(Math.max(7.5f * sp, btnR * 1.15f));
        for (int i = 0; i < 4; i++) {
            float bx = fx + offsets[i][0];
            float by = fy + offsets[i][1];
            boolean primary = i == 2;
            fillPaint.setColor(primary ? moon : ColorUtils.setAlphaComponent(ink, 0x22));
            canvas.drawCircle(bx, by, btnR, fillPaint);
            textPaint.setColor(primary ? onMoon : ink);
            float ty = by - (textPaint.ascent() + textPaint.descent()) * 0.5f;
            canvas.drawText(glyphs[i], bx, ty, textPaint);
        }

        // Studio capability pills along the bottom of the diagram pane (wrapping cleanly when needed).
        float py = area.bottom - badgeBlockH;
        float px = area.left;
        for (int i = 0; i < badgeLabels.length; i++) {
            textPaint.setTextSize(10.5f * sp);
            float maxPillW = area.width();
            String fitted = TextUtils.ellipsize(
                    badgeLabels[i], textPaint, Math.max(0f, maxPillW - 14 * dp), TextUtils.TruncateAt.END).toString();
            float w = Math.min(maxPillW, textPaint.measureText(fitted) + 14 * dp);
            if (px > area.left && px + w > area.right) {
                px = area.left;
                py += badgeH + rowGap;
            }
            px = drawBadge(canvas, px, py, w, badgeH, fitted, badgeStates[i], ink, muted, moon) + badgeGap;
        }
    }

    private void drawStickWell(Canvas canvas, float cx, float cy, float r, float dzShare, int ink, int moon) {
        fillPaint.setColor(ColorUtils.setAlphaComponent(ink, 0x14));
        canvas.drawCircle(cx, cy, r, fillPaint);
        strokePaint.setStrokeWidth(1.2f * dp);
        strokePaint.setColor(ColorUtils.setAlphaComponent(ink, 0x48));
        canvas.drawCircle(cx, cy, r, strokePaint);
        float dzR = r * dzShare;
        fillPaint.setColor(ColorUtils.setAlphaComponent(moon, 0x28));
        canvas.drawCircle(cx, cy, dzR, fillPaint);
        strokePaint.setStrokeWidth(1.25f * dp);
        strokePaint.setColor(moon);
        canvas.drawCircle(cx, cy, dzR, strokePaint);
    }

    private float drawBadge(Canvas canvas, float x, float y, float w, float h, String label, boolean active,
                            int ink, int muted, int moon) {
        textPaint.setTextAlign(Paint.Align.LEFT);
        textPaint.setTextSize(10.5f * sp);
        pillRect.set(x, y, x + w, y + h);
        fillPaint.setColor(active ? ColorUtils.setAlphaComponent(moon, 0x28) : ColorUtils.setAlphaComponent(ink, 0x10));
        canvas.drawRoundRect(pillRect, h / 2f, h / 2f, fillPaint);
        strokePaint.setStrokeWidth(dp);
        strokePaint.setColor(active ? ColorUtils.setAlphaComponent(moon, 0xAA) : ColorUtils.setAlphaComponent(ink, 0x22));
        canvas.drawRoundRect(pillRect, h / 2f, h / 2f, strokePaint);
        textPaint.setColor(active ? ink : muted);
        float ty = pillRect.centerY() - (textPaint.ascent() + textPaint.descent()) * 0.5f;
        canvas.drawText(label, x + 7 * dp, ty, textPaint);
        return x + w;
    }

    private void drawDeadzoneCurve(Canvas canvas, RectF area, int ink, int muted, int moon) {
        String title = getResources().getString(R.string.title_seekbar_deadzone);
        String readout = getResources().getString(
                R.string.controller_studio_deadzone_readout,
                stickDeadzonePercent, triggerDeadzonePercent);

        textPaint.setTextSize(12f * sp);
        float titleW = textPaint.measureText(title);
        float titleAscent = textPaint.ascent();
        float titleDescent = textPaint.descent();

        textPaint.setTextSize(11f * sp);
        float readoutW = textPaint.measureText(readout);

        float headerBaseline = area.top - titleAscent;
        float plotTop;
        if (titleW + readoutW + 8 * dp <= area.width()) {
            textPaint.setTextAlign(Paint.Align.LEFT);
            textPaint.setTextSize(12f * sp);
            textPaint.setColor(ink);
            canvas.drawText(title, area.left, headerBaseline, textPaint);

            textPaint.setTextAlign(Paint.Align.RIGHT);
            textPaint.setTextSize(11f * sp);
            textPaint.setColor(moon);
            canvas.drawText(readout, area.right, headerBaseline, textPaint);
            plotTop = headerBaseline + titleDescent + 8 * dp;
        } else {
            textPaint.setTextAlign(Paint.Align.LEFT);
            textPaint.setTextSize(12f * sp);
            textPaint.setColor(ink);
            String fittedTitle = TextUtils.ellipsize(title, textPaint, area.width(), TextUtils.TruncateAt.END).toString();
            canvas.drawText(fittedTitle, area.left, headerBaseline, textPaint);

            textPaint.setTextSize(11f * sp);
            textPaint.setColor(moon);
            float readoutBaseline = headerBaseline + titleDescent - textPaint.ascent() + 2 * dp;
            String fittedReadout = TextUtils.ellipsize(readout, textPaint, area.width(), TextUtils.TruncateAt.END).toString();
            canvas.drawText(fittedReadout, area.left, readoutBaseline, textPaint);
            plotTop = readoutBaseline + textPaint.descent() + 6 * dp;
        }

        float plotBottom = area.bottom - 2 * dp;
        float plotLeft = area.left;
        float plotRight = area.right;
        float plotW = plotRight - plotLeft;
        float plotH = plotBottom - plotTop;
        if (plotH < 24 * dp || plotW < 40 * dp) return;

        pillRect.set(plotLeft, plotTop, plotRight, plotBottom);
        fillPaint.setColor(ColorUtils.setAlphaComponent(ink, 0x0A));
        canvas.drawRoundRect(pillRect, 8 * dp, 8 * dp, fillPaint);

        strokePaint.setStrokeWidth(dp);
        strokePaint.setColor(ColorUtils.setAlphaComponent(ink, 0x14));
        for (int i = 1; i <= 3; i++) {
            float gx = plotLeft + plotW * (i / 4f);
            float gy = plotTop + plotH * (i / 4f);
            canvas.drawLine(gx, plotTop, gx, plotBottom, strokePaint);
            canvas.drawLine(plotLeft, gy, plotRight, gy, strokePaint);
        }

        float threshold = stickDeadzonePercent > 0 ? stickDeadzonePercent / 100f : 0.01f;
        float kneeX = plotLeft + plotW * threshold;
        float kneeY = plotBottom - plotH * stickOutput(Math.min(1f, threshold + 0.001f), stickDeadzonePercent);

        pillRect.set(plotLeft, plotTop, kneeX, plotBottom);
        fillPaint.setColor(ColorUtils.setAlphaComponent(muted, 0x18));
        canvas.drawRect(pillRect, fillPaint);

        curveAreaPath.reset();
        curveAreaPath.moveTo(kneeX, plotBottom);
        curveAreaPath.lineTo(kneeX, kneeY);
        curveAreaPath.lineTo(plotRight, plotTop);
        curveAreaPath.lineTo(plotRight, plotBottom);
        curveAreaPath.close();
        fillPaint.setColor(ColorUtils.setAlphaComponent(moon, 0x22));
        canvas.drawPath(curveAreaPath, fillPaint);

        curvePath.reset();
        curvePath.moveTo(plotLeft, plotBottom);
        curvePath.lineTo(kneeX, plotBottom);
        curvePath.lineTo(kneeX, kneeY);
        curvePath.lineTo(plotRight, plotTop);
        strokePaint.setStrokeWidth(2f * dp);
        strokePaint.setColor(moon);
        canvas.drawPath(curvePath, strokePaint);

        fillPaint.setColor(moon);
        canvas.drawCircle(kneeX, kneeY, 3.2f * dp, fillPaint);
    }

    private static int clamp(int v, int lo, int hi) {
        return Math.max(lo, Math.min(hi, v));
    }
}
