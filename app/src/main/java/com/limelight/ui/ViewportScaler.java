/**
 * How the interface sizes itself for the screen it is on (Nightjar #144, #146): nightjar-policy's
 * {@code viewport_scale}, the desktop's too, through {@link NightjarUiPolicyJni}, with a Java mirror where the library
 * is not loaded (pinned equal by ViewportScalerTest).
 *
 * <p>{@link AppTheme#scaled} takes the density factor from here: a TV is normalised to the 960x540 dp canvas whatever
 * density its vendor reports (a no-op on a TV that already reports it), and the user's Interface size and the TV
 * compaction of Settings apply on top, a target never under 72 lines of a 1080-line picture. Phones and handhelds get
 * no automatic density change: only the user's size. The rest ({@link Scale}: spacing compaction, margins, the hero's
 * share, columns, the smallest target and text widths) is for layouts to use.
 */
package com.limelight.ui;

import android.content.Context;
import android.content.res.Configuration;
import android.content.res.Resources;

import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

public final class ViewportScaler {
    private ViewportScaler() { }

    public static final String PHONE = "phone";
    public static final String HANDHELD = "handheld";
    public static final String TV = "tv";
    public static final String DESKTOP = "desktop";

    /** Smallest target where a finger or a pointer meets it, in system dp ({@code layout.touchMin}). */
    public static final float TOUCH_MIN_DP = 48f;
    static final float TV_CANVAS_WIDTH_DP = 960f;
    static final float TV_CANVAS_HEIGHT_DP = 540f;
    static final float TV_COMPACT = AppTheme.TV_COMPACT_PERCENT / 100f;
    static final float STAGE_MAX_WIDTH_DP = 1760f;
    static final float TEXT_MAX_WIDTH_DP = 720f;
    static final int MAX_COLUMNS = 8;

    /** How the interface is sized and placed; sizes in design dp (multiply by {@link #ui} for system dp). */
    public static final class Scale {
        /** The factor on the system's density. */
        public final float ui;
        /** How much paddings and gaps tighten, 0.75 to 1; targets and type keep their size. */
        public final float density;
        /** The factor on display type, 0.72 to 1. */
        public final float typeScale;
        /** The stage's margin at each side: overscan-safe on a TV. */
        public final float marginDp;
        /** The hero's share of the stage's height. */
        public final float heroHeight;
        public final int columns;
        /** The smallest target in design dp: 48 system dp or more once multiplied by {@link #ui}. */
        public final float touchMinDp;
        public final float stageMaxWidthDp;
        public final float textMaxWidthDp;

        Scale(float[] v) {
            ui = v[0];
            density = v[1];
            typeScale = v[2];
            marginDp = v[3];
            heroHeight = v[4];
            columns = Math.round(v[5]);
            touchMinDp = v[6];
            stageMaxWidthDp = v[7];
            textMaxWidthDp = v[8];
        }

        float[] values() {
            return new float[] {ui, density, typeScale, marginDp, heroHeight, columns, touchMinDp, stageMaxWidthDp, textMaxWidthDp};
        }
    }

    /** The size and placement for a window of {@code widthDp} x {@code heightDp} system dp. */
    public static Scale scale(float widthDp, float heightDp, float dpi, String form, int userScalePct, boolean tvCompact) {
        if (NightjarJni.isAvailable()) {
            try {
                float[] values = NightjarUiPolicyJni.viewportScale(widthDp, heightDp, dpi, form, userScalePct, tvCompact);
                if (values != null && values.length == 9) return new Scale(values);
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return new Scale(mirror(widthDp, heightDp, form, userScalePct, tvCompact));
    }

    /**
     * The layout of the screen {@code context} shows, for layouts: the window in system dp (recovered from an
     * {@link AppTheme#scaled} context too), its form and the user's Interface size. {@code tvCompact} for Settings'
     * screens on a TV.
     */
    public static Scale of(Context context, boolean tvCompact) {
        Configuration config = context.getResources().getConfiguration();
        int system = Resources.getSystem().getConfiguration().densityDpi;
        float back = config.densityDpi > 0 && system > 0 ? config.densityDpi / (float) system : 1f;
        float width = config.screenWidthDp * back;
        float height = config.screenHeightDp * back;
        float dpi = context.getResources().getDisplayMetrics().xdpi;
        return scale(width, height, dpi, form(config, width), AppTheme.uiScalePercent(context), tvCompact);
    }

    /** The policy's form for a window: a TV by its type, a landscape window 600 dp or wider a handheld. */
    static String form(Configuration config, float widthDp) {
        if ((config.uiMode & Configuration.UI_MODE_TYPE_MASK) == Configuration.UI_MODE_TYPE_TELEVISION) return TV;
        return config.orientation == Configuration.ORIENTATION_LANDSCAPE && widthDp >= 600 ? HANDHELD : PHONE;
    }

    /**
     * The factor {@link AppTheme#scaled} puts on the density. A real TV laid out as one takes the policy's: the canvas,
     * the user's size and the compaction. Anything else keeps the user's size alone, and the compaction where the TV
     * layout is chosen on a device that is no TV: a phone or handheld never changes density by itself.
     *
     * @param config   the screen's own configuration, before any scaling
     * @param ownTv    the device itself is a TV
     * @param layoutTv the screen lays out as a TV (a TV, or the TV interface chosen in Settings)
     */
    static float factor(Configuration config, boolean ownTv, boolean layoutTv, int percent, boolean compactOnTelevision) {
        boolean compact = compactOnTelevision && layoutTv;
        if (ownTv && layoutTv) {
            float width = config.screenWidthDp == Configuration.SCREEN_WIDTH_DP_UNDEFINED ? TV_CANVAS_WIDTH_DP : config.screenWidthDp;
            float height = config.screenHeightDp == Configuration.SCREEN_HEIGHT_DP_UNDEFINED ? TV_CANVAS_HEIGHT_DP : config.screenHeightDp;
            // Width and height as the system reports them for the screen, whatever the orientation.
            return scale(Math.max(width, height), Math.min(width, height), 0f, TV, percent, compact).ui;
        }
        float factor = percent / 100f;
        return compact ? factor * TV_COMPACT : factor;
    }

    private static float finite(float value, float fallback) {
        return !Float.isNaN(value) && !Float.isInfinite(value) && value > 0f ? value : fallback;
    }

    private static float clamp(float value, float low, float high) {
        return Math.max(low, Math.min(high, value));
    }

    /** viewport_scale::scale in Java. */
    static float[] mirror(float widthDp, float heightDp, String form, int userScalePct, boolean tvCompact) {
        float width = finite(widthDp, 360f);
        float height = finite(heightDp, 640f);
        float user = Math.max(50, Math.min(300, userScalePct)) / 100f;
        float ui;
        switch (form) {
            case TV: {
                float canvas = Math.min(width / TV_CANVAS_WIDTH_DP, height / TV_CANVAS_HEIGHT_DP);
                float compact = tvCompact ? TV_COMPACT : 1f;
                ui = clamp(Math.max(canvas * user * compact, canvas * 0.75f), 0.5f, 4f);
                break;
            }
            case DESKTOP:
                if (userScalePct != 100) ui = user;
                else if (height > 1600f) ui = clamp(Math.round((height / 1080f) * 20f), 20f, 60f) / 20f;
                else ui = 1f;
                break;
            case HANDHELD:
                ui = user;
                break;
            default:
                form = PHONE;
                ui = user;
        }
        float stageW = width / ui;
        float stageH = height / ui;
        boolean landscape = stageW >= stageH;
        boolean tv = TV.equals(form);
        float density = tv ? 1f : landscape ? clamp(stageH / 540f, 0.75f, 1f) : 1f;
        float typeScale = tv ? 1f : landscape ? clamp(stageH / 600f, 0.72f, 1f) : clamp(stageW / 360f, 0.85f, 1f);
        float margin;
        float hero;
        float cover;
        float gap;
        int floor;
        switch (form) {
            case HANDHELD:
                margin = stageH >= 400f ? 24f : 16f;
                hero = 0.58f;
                cover = 124f;
                gap = 14f;
                floor = 4;
                break;
            case TV:
                margin = 48f;
                hero = tvCompact ? 0.5f : 0.56f;
                cover = 112f;
                gap = 14f;
                floor = 6;
                break;
            case DESKTOP:
                margin = stageW >= 1100f ? 48f : stageW >= 800f ? 32f : 24f;
                hero = clamp(400f / stageH, 0.36f, 0.56f);
                cover = 152f;
                gap = 16f;
                floor = 3;
                break;
            default:
                margin = 16f;
                hero = landscape ? 0.62f : 0.46f;
                cover = 100f;
                gap = 12f;
                floor = 3;
        }
        float content = Math.min(stageW, STAGE_MAX_WIDTH_DP) - 2f * margin;
        int fit = (int) Math.max(1f, (float) Math.floor((content + gap) / (cover + gap)));
        int columns = tv ? (tvCompact ? 8 : 7) : Math.max(floor, Math.min(MAX_COLUMNS, fit));
        float touchMin = tv ? TOUCH_MIN_DP : TOUCH_MIN_DP / Math.min(ui, 1f);
        float text = Math.min(TEXT_MAX_WIDTH_DP, Math.max(content, 240f));
        return new float[] {ui, density, typeScale, margin, hero, columns, touchMin, STAGE_MAX_WIDTH_DP, text};
    }
}
