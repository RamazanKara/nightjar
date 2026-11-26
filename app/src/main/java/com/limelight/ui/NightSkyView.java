package com.limelight.ui;

import android.app.Activity;
import android.content.Context;
import android.util.AttributeSet;

/**
 * The name Settings' pages and layouts still know the stage by: a {@link StageView} and nothing more, with the calls of the
 * retired 6.x backdrop accepted and ignored. It draws what the stage draws. It goes when those pages name StageView themselves.
 */
@Deprecated
public class NightSkyView extends StageView {
    public NightSkyView(Context context) {
        this(context, null);
    }

    public NightSkyView(Context context, AttributeSet attrs) {
        super(context, attrs);
    }

    /** {@link StageView#behind} under the old name. */
    public static NightSkyView behind(Activity page) {
        return placeBehind(page, new NightSkyView(page));
    }

    /** The stage's light comes from above the window whatever the safe area is. */
    public void setSafeTop(int top) {
    }

    /** The stage has no ornament to show or hide. */
    public void setMoonShown(boolean shown) {
    }

    public void setMoonAtEnd() {
    }

    public boolean isMoonShown() {
        return false;
    }
}
