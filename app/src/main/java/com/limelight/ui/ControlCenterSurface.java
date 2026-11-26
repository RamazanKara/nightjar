/**
 * What the open Control Center is drawn on (nightjar#61), as nightjar-policy's {@code control_center::surface} says for
 * this moment: the game blurred behind it, a baked panel, or solid black under high contrast.
 *
 * <p>Closed, it holds nothing: no window, no listener, no frame work. {@link #open} asks the policy once; only a blurred
 * answer opens the one {@link MenuBlurWindow}, sized to the Control Center's own bounds, and only then does it listen for
 * what can take the blur away while it is open: the system's blur switch, battery saver, heat. Any of them drops to the
 * baked panel at once: the opaque fill first, then the blur window goes, so no frame shows a see-through panel over a
 * sharp game. It stays baked until the Control Center closes; a window opened later would stand over the panel, not
 * under it. {@link #close} removes the window and stops listening.
 */
package com.limelight.ui;

import android.app.Activity;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.graphics.Canvas;
import android.graphics.ColorFilter;
import android.graphics.LinearGradient;
import android.graphics.Outline;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PixelFormat;
import android.graphics.Rect;
import android.graphics.RectF;
import android.graphics.Shader;
import android.graphics.drawable.Drawable;
import android.os.Build;
import android.os.PowerManager;
import android.view.View;

import androidx.core.content.ContextCompat;

import com.limelight.R;

import java.util.function.Consumer;
import java.util.function.Function;

final class ControlCenterSurface {
    /** What the device says right now and how to hear it change: the system's in the app, a registry of its own in a test. */
    interface Signals {
        /** The system can blur across windows now (API 31+, its switch on). */
        boolean blurEnabled();

        boolean powerSave();

        /** Thermal status severe or worse. */
        boolean thermalSevere();

        /** Calls {@code changed} on the main thread when any of the three may have changed. */
        void listen(Runnable changed);

        void unlisten();
    }

    /** For a test: its registry in place of the system's. */
    static Function<Activity, Signals> signalsForTest;

    private final Activity owner;
    private final View panel;
    private final Glass glass;
    private final int[] at = new int[2];
    private Signals signals;
    private MenuBlurWindow blur;
    private String kind;
    private boolean listening;

    /** {@code panel} is the Control Center itself: the blur lies under exactly its bounds. */
    ControlCenterSurface(Activity owner, View panel, Glass glass) {
        this.owner = owner;
        this.panel = panel;
        this.glass = glass;
    }

    /** {@link ControlCenterModel#BLURRED}, {@code BAKED} or {@code SOLID} while open; null while closed. */
    String kind() {
        return kind;
    }

    /** The blur under the panel while it is open and blurred; null otherwise. */
    MenuBlurWindow blurWindow() {
        return blur;
    }

    /** Whether it listens to the device now: only while a blur stands under the panel. */
    boolean listening() {
        return listening;
    }

    Glass glass() {
        return glass;
    }

    /** Before the Control Center's own window shows, so the blur stands under it: it blurs the game, not the panel. */
    void open() {
        if (kind != null) return;
        signals = signalsForTest != null ? signalsForTest.apply(owner) : new SystemSignals(owner);
        String wanted = decide();
        if (ControlCenterModel.BLURRED.equals(wanted)) {
            try {
                blur = MenuBlurWindow.open(owner, Math.round(GlassSpec.menuWindow(owner).blur), glass.radii());
            } catch (RuntimeException e) {
                // A stream whose window is going takes no new window: the baked panel needs none.
                blur = null;
            }
            if (blur == null) wanted = ControlCenterModel.BAKED;
        }
        kind = wanted;
        glass.show(kind);
        if (blur != null) {
            listening = true;
            signals.listen(this::changed);
        }
    }

    private String decide() {
        String answer = ControlCenterModel.surface(true, GlassTier.land(owner).key(), signals.blurEnabled(),
                signals.powerSave(), signals.thermalSevere(), AppTheme.isHighContrast(owner));
        return answer == null ? ControlCenterModel.BAKED : answer;
    }

    // The system's switch, battery saver or heat changed while a blur stands under the panel.
    private void changed() {
        if (blur == null || signals == null) return;
        String now = decide();
        if (ControlCenterModel.BLURRED.equals(now)) return;
        // The opaque surface first, then the window: never a see-through panel over a sharp game.
        kind = now;
        glass.show(now);
        removeBlur();
        stopListening();
    }

    /**
     * Drops to the baked panel until the Control Center closes: its shape changed (the screen turned), and the blur
     * window keeps the shape it was opened with.
     */
    void bake() {
        if (blur == null) return;
        kind = ControlCenterModel.BAKED;
        glass.show(kind);
        removeBlur();
        stopListening();
    }

    /** Lays the blur under the panel where it is now; a page that changes its height moves it. */
    void place() {
        if (blur == null) return;
        panel.getLocationOnScreen(at);
        blur.cover(at[0], at[1], panel.getWidth(), panel.getHeight());
    }

    private void removeBlur() {
        if (blur == null) return;
        MenuBlurWindow open = blur;
        blur = null;
        open.close();
    }

    private void stopListening() {
        if (!listening) return;
        listening = false;
        if (signals != null) signals.unlisten();
    }

    /** Removes the blur window, not hides it, and stops listening: a stream with the Control Center closed carries nothing. */
    void close() {
        stopListening();
        removeBlur();
        signals = null;
        kind = null;
    }

    /** The system's own signals. Everything it registers in {@link #listen} is gone after {@link #unlisten}. */
    static final class SystemSignals implements Signals {
        private final Activity owner;
        private final PowerManager power;
        private Consumer<Boolean> blurListener;
        private BroadcastReceiver saver;
        private PowerManager.OnThermalStatusChangedListener thermal;
        // What the system's listener last said; null before it spoke.
        private Boolean blurOn;

        SystemSignals(Activity owner) {
            this.owner = owner;
            power = (PowerManager) owner.getApplicationContext().getSystemService(Context.POWER_SERVICE);
        }

        @Override
        public boolean blurEnabled() {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return false;
            if (MenuBlurWindow.enabledForTest != null) return MenuBlurWindow.enabledForTest;
            return blurOn != null ? blurOn : MenuBlurWindow.systemSwitch(owner);
        }

        @Override
        public boolean powerSave() {
            return power != null && power.isPowerSaveMode();
        }

        @Override
        public boolean thermalSevere() {
            return Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && power != null
                    && power.getCurrentThermalStatus() >= PowerManager.THERMAL_STATUS_SEVERE;
        }

        @Override
        public void listen(Runnable changed) {
            unlisten();
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                blurListener = on -> {
                    blurOn = on;
                    changed.run();
                };
                owner.getWindowManager().addCrossWindowBlurEnabledListener(blurListener);
            }
            saver = new BroadcastReceiver() {
                @Override public void onReceive(Context context, Intent intent) {
                    changed.run();
                }
            };
            ContextCompat.registerReceiver(owner, saver, new IntentFilter(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED),
                    ContextCompat.RECEIVER_NOT_EXPORTED);
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && power != null) {
                thermal = status -> owner.runOnUiThread(changed);
                power.addThermalStatusListener(thermal);
            }
        }

        @Override
        public void unlisten() {
            if (blurListener != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                owner.getWindowManager().removeCrossWindowBlurEnabledListener(blurListener);
            }
            blurListener = null;
            blurOn = null;
            if (saver != null) {
                try {
                    owner.unregisterReceiver(saver);
                } catch (IllegalArgumentException ignored) {
                    // The activity already dropped its receivers while it went.
                }
                saver = null;
            }
            if (thermal != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && power != null) {
                power.removeThermalStatusListener(thermal);
            }
            thermal = null;
        }
    }

    /**
     * The Control Center's panel: Obsidian Night in every theme. Over a blur the see-through fill, baked the near-opaque
     * one, both under a rim lit from above; under high contrast solid black inside a 2 dp rim. Its colours are the
     * {@code surface.controlCenter.*} tokens, never the theme's.
     */
    static final class Glass extends Drawable {
        private final int blurredFill, bakedFill, solidFill, rimTop, rimBottom, solidRim;
        private final float rimWidth, solidRimWidth;
        private final Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint rim = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Path path = new Path();
        private final RectF rect = new RectF();
        private float[] radii = new float[8];
        private String kind = ControlCenterModel.BAKED;

        Glass(Context context) {
            android.content.res.Resources res = context.getResources();
            blurredFill = ContextCompat.getColor(context, R.color.nj_surface_control_center_fill);
            bakedFill = ContextCompat.getColor(context, R.color.nj_surface_control_center_baked_fill);
            solidFill = ContextCompat.getColor(context, R.color.nj_surface_control_center_solid_fill);
            rimTop = ContextCompat.getColor(context, R.color.nj_surface_control_center_rim_top);
            rimBottom = ContextCompat.getColor(context, R.color.nj_surface_control_center_rim_bottom);
            solidRim = ContextCompat.getColor(context, R.color.nj_hc_color_rim);
            float density = res.getDisplayMetrics().density;
            rimWidth = Math.max(1f, density);
            solidRimWidth = 2 * density;
            rim.setStyle(Paint.Style.STROKE);
        }

        /** The corners, as {@link Path#addRoundRect} takes them: a dock's four, a sheet's upper two. */
        void setRadii(float[] radii) {
            this.radii = radii.clone();
            onBoundsChange(getBounds());
            invalidateSelf();
        }

        float[] radii() {
            return radii.clone();
        }

        /** The surface to draw: {@link ControlCenterModel#BLURRED}, {@code BAKED} or {@code SOLID}. */
        void show(String kind) {
            if (kind == null || kind.equals(this.kind)) return;
            this.kind = kind;
            onBoundsChange(getBounds());
            invalidateSelf();
        }

        String kind() {
            return kind;
        }

        int fillColor() {
            return ControlCenterModel.SOLID.equals(kind) ? solidFill
                    : ControlCenterModel.BLURRED.equals(kind) ? blurredFill : bakedFill;
        }

        /** The rim's width in pixels: 2 dp under high contrast. */
        float rimWidth() {
            return ControlCenterModel.SOLID.equals(kind) ? solidRimWidth : rimWidth;
        }

        int rimTopColor() {
            return ControlCenterModel.SOLID.equals(kind) ? solidRim : rimTop;
        }

        @Override
        protected void onBoundsChange(Rect bounds) {
            if (bounds.isEmpty()) return;
            boolean solid = ControlCenterModel.SOLID.equals(kind);
            rim.setStrokeWidth(rimWidth());
            rim.setShader(solid ? null : new LinearGradient(0, bounds.top, 0, bounds.bottom, rimTop, rimBottom, Shader.TileMode.CLAMP));
            if (solid) rim.setColor(solidRim);
        }

        @Override
        public void draw(Canvas canvas) {
            rect.set(getBounds());
            path.rewind();
            path.addRoundRect(rect, radii, Path.Direction.CW);
            fill.setColor(fillColor());
            canvas.drawPath(path, fill);
            float inset = rimWidth() / 2f;
            rect.inset(inset, inset);
            float[] inner = new float[8];
            for (int i = 0; i < 8; i++) inner[i] = Math.max(0f, radii[i] - inset);
            path.rewind();
            path.addRoundRect(rect, inner, Path.Direction.CW);
            canvas.drawPath(path, rim);
        }

        @Override
        public void getOutline(Outline outline) {
            Rect b = getBounds();
            float top = radii[0], bottom = radii[4];
            // A sheet rounds only its upper corners: the round rect runs on below the view, which cuts it square there.
            if (bottom == 0f && top > 0f) outline.setRoundRect(b.left, b.top, b.right, b.bottom + Math.round(top), top);
            else outline.setRoundRect(b, top);
        }

        @Override
        public void setAlpha(int alpha) {
            fill.setAlpha(alpha);
        }

        @Override
        public void setColorFilter(ColorFilter filter) {
            fill.setColorFilter(filter);
        }

        @Override
        public int getOpacity() {
            return PixelFormat.TRANSLUCENT;
        }
    }
}
