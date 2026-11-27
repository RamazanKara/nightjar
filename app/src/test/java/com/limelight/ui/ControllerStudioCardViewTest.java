/**
 * Unit tests for {@link ControllerStudioCardView} WCAG contrast across Night/Day/High Contrast
 * themes, canonical controller preference keys, and localized TalkBack summaries.
 */
package com.limelight.ui;

import android.content.Context;
import android.content.SharedPreferences;
import android.content.res.Configuration;
import android.preference.PreferenceManager;

import androidx.core.graphics.ColorUtils;

import com.limelight.R;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

import java.util.Locale;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34)
public class ControllerStudioCardViewTest {
    @Test
    public void primaryButtonInkMeetsWcagContrastAcrossNightDayAndHighContrast() {
        int nightMoon = 0xFFF0B866;
        int dayMoonInk = 0xFF7A5010;
        int highContrastYellow = 0xFFFFD60A;

        int nightInk = ControllerStudioCardView.resolvePrimaryButtonInk(0xFF111318, nightMoon);
        assertTrue(ColorUtils.calculateContrast(nightInk, nightMoon) >= 4.5);

        int dayInk = ControllerStudioCardView.resolvePrimaryButtonInk(0xFFFFFFFF, dayMoonInk);
        assertEquals(0xFFFFFFFF, dayInk);
        assertTrue(ColorUtils.calculateContrast(dayInk, dayMoonInk) >= 4.5);

        // Even if a theme supplied low-contrast dark ink over dark bronze, fallback lifts to white.
        int fallbackDayInk = ControllerStudioCardView.resolvePrimaryButtonInk(0xFF111318, dayMoonInk);
        assertEquals(0xFFFFFFFF, fallbackDayInk);
        assertTrue(ColorUtils.calculateContrast(fallbackDayInk, dayMoonInk) >= 4.5);

        int hcInk = ControllerStudioCardView.resolvePrimaryButtonInk(0xFF000000, highContrastYellow);
        assertTrue(ColorUtils.calculateContrast(hcInk, highContrastYellow) >= 4.5);
    }

    @Test
    public void syncFromPreferencesReadsCanonicalKeysAndFormatsLocalizedSummary() {
        Context base = RuntimeEnvironment.getApplication();
        Configuration cfg = new Configuration(base.getResources().getConfiguration());
        cfg.setLocale(Locale.GERMAN);
        Context deContext = base.createConfigurationContext(cfg);
        deContext.setTheme(R.style.AppTheme);

        SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(base);
        prefs.edit().clear().commit();

        ControllerStudioCardView freshCard = new ControllerStudioCardView(deContext);
        freshCard.syncFromPreferences(prefs);
        assertTrue("motion sensors default to enabled on Android 13+", freshCard.motionEnabled());

        prefs.edit()
                .putInt(ControllerStudioCardView.PREF_STICK_DZ, -10)
                .putInt(ControllerStudioCardView.PREF_TRIGGER_DZ, 5)
                .putBoolean(ControllerStudioCardView.PREF_SWAP_FACE, true)
                .putBoolean(ControllerStudioCardView.PREF_MOTION, false)
                .putBoolean(ControllerStudioCardView.PREF_TOUCHPAD_MOUSE, true)
                .commit();

        ControllerStudioCardView card = new ControllerStudioCardView(deContext);
        card.syncFromPreferences(prefs);

        assertEquals(-10, card.stickDeadzonePercent());
        assertEquals(5, card.triggerDeadzonePercent());
        assertTrue(card.facesSwapped());
        assertEquals(0f, ControllerStudioCardView.stickOutput(0.005f, -10), 1e-4f);
        assertTrue(ControllerStudioCardView.stickOutput(0.10f, -10) > 0.18f);
        assertEquals(0f, ControllerStudioCardView.stickOutput(0.10f, 10), 1e-4f);
        assertEquals(0.25f, ControllerStudioCardView.stickOutput(0.25f, 10), 1e-4f);
        String desc = String.valueOf(card.getContentDescription());
        assertTrue("expected German TalkBack summary, got: " + desc,
                desc.contains("Controller-Studio") && desc.contains("B A Y X") && desc.contains("-10%") && desc.contains("5%"));
    }
}
