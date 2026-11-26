package com.limelight.preferences;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;

import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

import android.app.Application;
import android.content.Context;
import android.preference.ListPreference;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

/** The Resolution picker's steps come from resolution_scale; the Java mirror matches it. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class, qualifiers = "en")
public class ResolutionStepsTest {
    @Test
    public void thePickerGainsTheStepsItLacks() {
        Context context = RuntimeEnvironment.getApplication();
        ListPreference pref = new ListPreference(context);
        pref.setEntries(new CharSequence[] {"720p", "1080p"});
        pref.setEntryValues(new CharSequence[] {"1280x720", "1920x1080"});
        ResolutionSteps.addTo(context, pref, "1920x1080");
        assertArrayEquals(new CharSequence[] {"1280x720", "1920x1080", "1632x918", "1440x810", "960x540"}, pref.getEntryValues());
        assertEquals("85% of screen size (1632×918)", pref.getEntries()[2].toString());
        assertEquals("720p keeps its own name", "720p", pref.getEntries()[0].toString());
        ResolutionSteps.addTo(context, pref, null);
        assertEquals(5, pref.getEntryValues().length);
    }

    @Test
    public void aScreensStepsAreItsOwnSizeAtFiveShares() {
        assertArrayEquals(new int[] {100, 1920, 1080, 85, 1632, 918, 75, 1440, 810, 67, 1280, 720, 50, 960, 540},
                ResolutionSteps.choices(1920, 1080));
        assertArrayEquals(new int[] {100, 2400, 1080, 85, 2040, 918, 75, 1800, 810, 67, 1600, 720, 50, 1200, 540},
                ResolutionSteps.choices(2400, 1080));
    }

    @Test
    public void theMirrorMatchesThePolicy() {
        int[][] screens = {{1920, 1080}, {1280, 720}, {2400, 1080}, {2340, 1080}, {3840, 2160}, {2560, 1600}, {1366, 768},
                {2208, 1840}, {3440, 1440}, {1919, 1079}, {480, 320}, {400, 300}, {0, 0}, {1080, 2400}};
        for (int[] screen : screens) {
            assertArrayEquals(screen[0] + "x" + screen[1], NightjarUiPolicyJni.resolutionChoices(screen[0], screen[1]),
                    ResolutionSteps.choicesMirror(screen[0], screen[1]));
        }
    }

    @Test
    public void aSavedSizeStillReadsAsItsStep() {
        // A step is stored as WxH and named back by the policy; any other size is a plain fixed or custom one.
        assertEquals(67, NightjarUiPolicyJni.resolutionStepOf(1920, 1080, 1280, 720));
        assertEquals(0, NightjarUiPolicyJni.resolutionStepOf(1920, 1080, 1600, 900));
        assertEquals(1440, PreferenceConfiguration.getWidthFromResolutionString("1440x810"));
        assertEquals(810, PreferenceConfiguration.getHeightFromResolutionString("1440x810"));
    }

    @Test
    public void theMirrorAnswersWithoutTheLibrary() {
        NightjarJni.forceUnavailableForTest(new UnsatisfiedLinkError("test"));
        try {
            assertArrayEquals(new int[] {100, 1280, 720, 85, 1088, 612, 75, 960, 540, 67, 854, 480, 50, 640, 360},
                    ResolutionSteps.choices(1280, 720));
        } finally {
            NightjarJni.forceUnavailableForTest(null);
        }
    }
}
