package com.limelight.preferences;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.RectF;
import android.util.AttributeSet;
import android.util.TypedValue;
import android.view.View;
import android.widget.LinearLayout;

import androidx.core.content.res.ResourcesCompat;
import androidx.dynamicanimation.animation.FloatValueHolder;
import androidx.dynamicanimation.animation.SpringAnimation;

import com.google.android.material.color.MaterialColors;
import com.limelight.R;
import com.limelight.ui.AppTheme;
import com.limelight.ui.Motion;
import com.limelight.ui.Springs;

/**
 * A settings preview card: a small drawn picture of what a choice does (a frame timeline, a tone curve, a pixel edge,
 * a screen with bars), on the control surface with its 1 dp rim, over the card's own text. A setting's row shows the
 * picture of its current choice; its picker shows one card per choice, the chosen one in the amber container and the
 * focused one in the ring (SettingsPreviews). The picture is a few shapes drawn at run time: amber is what the choice
 * gives, neutral is the reference, high contrast draws lines only. Nothing moves at rest; a changed choice springs to
 * its new picture, or cuts with Reduce motion. The geometry is PreviewPacing, PreviewTone, PreviewEdge and
 * PreviewScaling.
 */
public class SettingsPreviewCard extends LinearLayout {
    static final int NONE = -1, PACING = 0, TONE = 1, EDGE = 2, SCALING = 3;
    /** The picture's height over a caption; without one the picture takes the card. */
    private static final float PICTURE_DP = 72f, PAD_DP = 12f, GAP_DP = 8f;
    // Material's disabled content opacity, as in color/text_on_surface.xml
    private static final float DISABLED_ALPHA = 0.38f;

    private final PreviewInks inks = new PreviewInks();
    private final Paint surface = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final RectF edge = new RectF();
    private final RectF box = new RectF();
    private final float radius, rim, ring;
    private final int pad, gap;
    private final FloatValueHolder progress = new FloatValueHolder(1f);
    private SpringAnimation spring;
    private int fillColor, rimColor, ringColor;
    private String owner;
    private int kind = NONE;
    private float from, to;
    private boolean chosen;

    public SettingsPreviewCard(Context context, AttributeSet attrs) {
        super(context, attrs);
        setOrientation(VERTICAL);
        setWillNotDraw(false);
        float dp = getResources().getDisplayMetrics().density;
        inks.dp = dp;
        pad = Math.round(PAD_DP * dp);
        gap = Math.round(GAP_DP * dp);
        setPadding(pad, pad + Math.round(PICTURE_DP * dp) + gap, pad, pad);
        setMinimumHeight(getResources().getDimensionPixelSize(R.dimen.nj_size_preview_card));
        radius = getResources().getDimension(R.dimen.nj_radius_control);
        rim = Math.max(1f, dp);
        TypedValue value = new TypedValue();
        ring = context.getTheme().resolveAttribute(R.attr.nightjarFocusRingWidth, value, true)
                ? value.getDimension(getResources().getDisplayMetrics()) : 2f * dp;
        applyLook();
    }

    /** The card's colours for its look, and for the amber container while it is the chosen one. */
    private void applyLook() {
        Context context = getContext();
        AppTheme.Look look = AppTheme.look(context);
        int label = MaterialColors.getColor(context, com.google.android.material.R.attr.colorOnSurface, 0xFFF5F7FA);
        int secondary = MaterialColors.getColor(context, com.google.android.material.R.attr.colorOnSurfaceVariant, 0xFFC7CDD9);
        int onChosen = MaterialColors.getColor(context, R.attr.glassOnSelected, label);
        inks.linesOnly = look == AppTheme.Look.HIGH_CONTRAST;
        if (chosen) {
            // On the amber container the picture is drawn in its own ink: amber on amber would not read.
            inks.accent = onChosen;
            inks.neutral = inks.linesOnly ? onChosen : secondary;
        } else {
            inks.accent = ResourcesCompat.getColor(getResources(), look == AppTheme.Look.DAY ? R.color.nj_day_color_moon_ink
                    : inks.linesOnly ? R.color.nj_hc_color_moon : R.color.nj_color_moon, null);
            inks.neutral = inks.linesOnly ? label : secondary;
        }
        fillColor = MaterialColors.getColor(context, chosen ? R.attr.glassSelectedFill : R.attr.glassControlFill, 0x14FFFFFF);
        rimColor = MaterialColors.getColor(context, chosen ? R.attr.glassSelectedRim : R.attr.glassControlRim, 0x29FFFFFF);
        ringColor = MaterialColors.getColor(context, R.attr.glassRing, label);
        invalidate();
    }

    /**
     * Shows a picture's state. The same owner (a setting's key) with another state springs from the picture it showed
     * to the new one; a card that showed nothing yet, or another setting's picture, cuts.
     */
    void show(String owner, int kind, float state) {
        boolean same = owner != null && owner.equals(this.owner) && kind == this.kind;
        this.owner = owner;
        this.kind = kind;
        if (same && state == to) return;
        float shown = kind == EDGE ? PreviewInks.lerp(from, to, progress.getValue()) : to;
        land();
        to = state;
        if (!same || Motion.reduced(getContext())) {
            from = state;
            invalidate();
            return;
        }
        from = shown;
        progress.setValue(0f);
        if (spring == null) {
            spring = new SpringAnimation(progress).setSpring(Springs.force(this, Springs.Kind.FOCUS).setFinalPosition(1f));
            spring.setMinimumVisibleChange(1f / 256f);
            spring.addUpdateListener((animation, value, velocity) -> invalidate());
        }
        spring.setStartValue(0f).start();
        invalidate();
    }

    /** Whether the picture is between two states right now. */
    boolean moving() {
        return spring != null && spring.isRunning();
    }

    int kind() {
        return kind;
    }

    /** The state the picture shows, or is on its way to. */
    float state() {
        return to;
    }

    /** The amber container of the chosen card in a picker. */
    void setChosen(boolean chosen) {
        if (this.chosen == chosen) return;
        this.chosen = chosen;
        setSelected(chosen);
        applyLook();
    }

    boolean chosen() {
        return chosen;
    }

    private void land() {
        if (spring != null && spring.isRunning()) spring.cancel();
        progress.setValue(1f);
    }

    @Override
    protected void onDetachedFromWindow() {
        land();
        super.onDetachedFromWindow();
    }

    @Override
    public void setEnabled(boolean enabled) {
        super.setEnabled(enabled);
        invalidate();
    }

    @Override
    protected void drawableStateChanged() {
        super.drawableStateChanged();
        invalidate();
    }

    @Override
    protected void onLayout(boolean changed, int left, int top, int right, int bottom) {
        super.onLayout(changed, left, top, right, bottom);
        float floor = getHeight() - pad;
        for (int i = 0; i < getChildCount(); i++) {
            View child = getChildAt(i);
            if (child.getVisibility() != GONE) {
                floor = child.getTop() - gap;
                break;
            }
        }
        box.set(pad, pad, getWidth() - pad, Math.max(pad, floor));
    }

    @Override
    protected void onDraw(Canvas canvas) {
        float half = rim / 2f;
        edge.set(half, half, getWidth() - half, getHeight() - half);
        surface.setStyle(Paint.Style.FILL);
        surface.setColor(fillColor);
        canvas.drawRoundRect(edge, radius, radius, surface);
        surface.setStyle(Paint.Style.STROKE);
        surface.setStrokeWidth(rim);
        surface.setColor(rimColor);
        canvas.drawRoundRect(edge, radius, radius, surface);
        if (isPressed()) {
            surface.setStyle(Paint.Style.FILL);
            surface.setColor(PreviewInks.faded(inks.neutral, 0.12f));
            canvas.drawRoundRect(edge, radius, radius, surface);
        }
        if (isFocused()) {
            half = ring / 2f;
            edge.set(half, half, getWidth() - half, getHeight() - half);
            surface.setStyle(Paint.Style.STROKE);
            surface.setStrokeWidth(ring);
            surface.setColor(ringColor);
            canvas.drawRoundRect(edge, radius, radius, surface);
        }
        if (kind == NONE || box.height() <= 0f || box.width() <= 0f) return;
        inks.dim = isEnabled() ? 1f : DISABLED_ALPHA;
        float t = progress.getValue();
        switch (kind) {
            case PACING:
                PreviewPacing.draw(canvas, box, Math.round(from), Math.round(to), t, inks);
                break;
            case TONE:
                PreviewTone.draw(canvas, box, Math.round(from), Math.round(to), t, inks);
                break;
            case EDGE:
                PreviewEdge.draw(canvas, box, from, to, t, inks);
                break;
            default:
                PreviewScaling.draw(canvas, box, Math.round(from), Math.round(to), t, inks);
                break;
        }
    }
}
