package com.limelight.ui;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Paint;
import android.graphics.Path;
import android.util.AttributeSet;
import android.view.View;

import androidx.core.content.res.ResourcesCompat;

import com.limelight.R;

/**
 * The viewfinder over the camera that reads a pairing code: four corner brackets around a square in the middle of the
 * picture, in the rim colour, and in the accent once a code was recognised. A camera's picture is dark or bright as it comes,
 * so the brackets are night's ink in every theme, over a thin shadow of their own shape. The path is built once per size and
 * nothing moves.
 */
public class ViewfinderFrame extends View {
    /** The square's side at most, and the room it keeps to the picture's edges, in dp. */
    private static final float MOST_DP = 280f;
    private static final float MARGIN_DP = 32f;
    /** A bracket's arm against the square's side, and its corner's radius against the arm. */
    static final float ARM = .16f;
    private static final float CORNER = .45f;

    private final Paint line = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Paint shadow = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path brackets = new Path();
    private final int rim;
    private final int lit;
    private boolean recognised;

    public ViewfinderFrame(Context context) {
        this(context, null);
    }

    public ViewfinderFrame(Context context, AttributeSet attrs) {
        super(context, attrs);
        float density = getResources().getDisplayMetrics().density;
        boolean highContrast = AppTheme.look(context) == AppTheme.Look.HIGH_CONTRAST;
        rim = highContrast ? 0xFFFFFFFF : (ResourcesCompat.getColor(getResources(), R.color.nj_sky_ring, null) & 0x00FFFFFF) | 0xE6000000;
        lit = ResourcesCompat.getColor(getResources(), highContrast ? R.color.nj_hc_color_moon : R.color.nj_color_moon, null);
        line.setStyle(Paint.Style.STROKE);
        line.setStrokeCap(Paint.Cap.ROUND);
        line.setStrokeWidth(density * (highContrast ? 4f : 3f));
        line.setColor(rim);
        shadow.setStyle(Paint.Style.STROKE);
        shadow.setStrokeCap(Paint.Cap.ROUND);
        shadow.setStrokeWidth(line.getStrokeWidth() + density * 2f);
        shadow.setColor(0x66000000);
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_NO);
    }

    /**
     * The square the brackets stand around in a picture of this size, as {left, top, side}: in the middle, at most {@code most}
     * wide, and {@code margin} clear of the nearer edges.
     */
    static float[] square(int width, int height, float margin, float most) {
        float side = Math.max(0f, Math.min(most, Math.min(width, height) - 2f * margin));
        return new float[] {(width - side) / 2f, (height - side) / 2f, side};
    }

    /** A pairing code was read: the brackets light up in the accent. */
    public void setRecognised(boolean recognised) {
        if (this.recognised == recognised) return;
        this.recognised = recognised;
        line.setColor(recognised ? lit : rim);
        invalidate();
    }

    /** Whether the brackets are lit. */
    public boolean isRecognised() {
        return recognised;
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldW, int oldH) {
        super.onSizeChanged(w, h, oldW, oldH);
        float density = getResources().getDisplayMetrics().density;
        float[] square = square(w, h, MARGIN_DP * density, MOST_DP * density);
        brackets.rewind();
        float side = square[2];
        if (side <= 0f) return;
        float arm = side * ARM;
        float round = arm * CORNER;
        float left = square[0];
        float top = square[1];
        float right = left + side;
        float bottom = top + side;
        corner(left, top, arm, round, 1f, 1f);
        corner(right, top, arm, round, -1f, 1f);
        corner(right, bottom, arm, round, -1f, -1f);
        corner(left, bottom, arm, round, 1f, -1f);
    }

    /** One bracket at the corner (x, y), its arms running toward the square's inside along {@code dx} and {@code dy}. */
    private void corner(float x, float y, float arm, float round, float dx, float dy) {
        brackets.moveTo(x, y + dy * arm);
        brackets.lineTo(x, y + dy * round);
        brackets.quadTo(x, y, x + dx * round, y);
        brackets.lineTo(x + dx * arm, y);
    }

    @Override
    protected void onDraw(Canvas canvas) {
        if (brackets.isEmpty()) return;
        canvas.drawPath(brackets, shadow);
        canvas.drawPath(brackets, line);
    }
}
