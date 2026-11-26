package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import android.app.Activity;
import android.content.Intent;
import android.view.View;
import android.view.ViewGroup;

import com.limelight.pin.PinLockSettingsActivity;
import com.limelight.preferences.HostStreamSettings;
import com.limelight.preferences.StreamProfilesActivity;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;

/** Settings' sub-pages stood on the bare canvas while Settings has the stage: every art-less page stands on the same one. */
@RunWith(RobolectricTestRunner.class)
@org.robolectric.annotation.Config(sdk = 34, application = android.app.Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class SubPageStageTest {
    private static void assertStageBehind(Activity page) {
        ViewGroup content = page.findViewById(android.R.id.content);
        View back = content.getChildAt(0);
        assertTrue(page.getClass().getSimpleName() + " has the stage at the back", back instanceof StageView);
        assertEquals("a screen reader passes over it", View.IMPORTANT_FOR_ACCESSIBILITY_NO, back.getImportantForAccessibility());
    }

    @Test
    public void profiles() {
        assertStageBehind(Robolectric.buildActivity(StreamProfilesActivity.class).setup().get());
    }

    @Test
    public void pinLock() {
        assertStageBehind(Robolectric.buildActivity(PinLockSettingsActivity.class).setup().get());
    }

    @Test
    public void hostSettings() {
        Intent intent = new Intent(org.robolectric.RuntimeEnvironment.getApplication(), HostStreamSettings.class)
                .putExtra("host_uuid", "stage-pc").putExtra("host_name", "PC");
        assertStageBehind(Robolectric.buildActivity(HostStreamSettings.class, intent).setup().get());
    }
}
