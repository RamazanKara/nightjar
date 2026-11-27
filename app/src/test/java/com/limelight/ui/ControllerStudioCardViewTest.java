package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.preference.PreferenceManager;
import android.view.View;

import org.junit.After;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

/** Verifies {@link ControllerStudioCardView} preference synchronization, responsive measurement, and vector rendering. */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 30, application = Application.class, qualifiers = "xhdpi")
public class ControllerStudioCardViewTest {

    @After
    public void tearDown() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit().clear().commit();
    }

    @Test
    public void syncsDeadzonesAndFaceLayoutAndDrawsCleanlyAtCompactAndWideWidths() {
        Context context = new androidx.appcompat.view.ContextThemeWrapper(
                RuntimeEnvironment.getApplication(), com.limelight.R.style.DarkTheme);
        SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(context);
        prefs.edit()
                .putInt("seekbar_deadzone", 12)
                .putInt("seekbar_trigger_deadzone", 18)
                .putBoolean("checkbox_swap_face_buttons", true)
                .putBoolean("checkbox_enable_motion", true)
                .putBoolean("checkbox_gamepad_touchpad_as_mouse", true)
                .commit();

        ControllerStudioCardView card = new ControllerStudioCardView(context);
        assertEquals(12, card.stickDeadzonePercent());
        assertEquals(18, card.triggerDeadzonePercent());
        assertTrue(card.facesSwapped());
        assertNotNull(card.getContentDescription());
        assertTrue(card.getContentDescription().toString().contains("B A Y X"));
        assertTrue(card.getContentDescription().toString().contains("12%"));

        for (int widthDp : new int[] {360, 640}) {
            int w = Math.round(widthDp * context.getResources().getDisplayMetrics().density);
            card.measure(View.MeasureSpec.makeMeasureSpec(w, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED));
            assertTrue("measured height > 0 at " + widthDp + "dp", card.getMeasuredHeight() > 0);
            card.layout(0, 0, card.getMeasuredWidth(), card.getMeasuredHeight());
            Bitmap out = Bitmap.createBitmap(card.getWidth(), card.getHeight(), Bitmap.Config.ARGB_8888);
            card.draw(new Canvas(out));
            int nonEmpty = 0;
            for (int y = 0; y < out.getHeight(); y += 8) {
                for (int x = 0; x < out.getWidth(); x += 8) {
                    if ((out.getPixel(x, y) >>> 24) > 0) nonEmpty++;
                }
            }
            assertTrue("vector diagram + deadzone curve rendered at " + widthDp + "dp", nonEmpty > 20);
        }

        card.setVisibility(View.GONE);
        card.measure(View.MeasureSpec.makeMeasureSpec(720, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED));
        assertEquals("collapses cleanly when hidden during search", 0, card.getMeasuredHeight());
        assertFalse(card.getVisibility() == View.VISIBLE);
    }
}
