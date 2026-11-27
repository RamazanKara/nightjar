package com.limelight.preferences;

import androidx.appcompat.app.AlertDialog;
import android.app.Application;
import android.content.SharedPreferences;
import android.preference.Preference;
import android.preference.PreferenceManager;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowDialog;
import org.robolectric.shadows.ShadowLooper;

import static org.junit.Assert.*;

/** The rows that were two switches each: pairs saved before read as the stream used them, and a choice saves both. */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class SwitchPairChoiceTest {
    private SharedPreferences prefs;

    @Before
    public void clear() {
        prefs = PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication());
        prefs.edit().clear().commit();
    }

    private int read(SwitchPairChoice pair, boolean first, boolean second) {
        prefs.edit().putBoolean(pair.firstKey, first).putBoolean(pair.secondKey, second).commit();
        return pair.read(prefs);
    }

    @Test
    public void everyPairSavedBeforeReadsAsTheStreamUsedIt() {
        // With both on, the stream menu won.
        assertEquals(0, read(SwitchPairChoice.BACK_BUTTON, false, false));
        assertEquals(1, read(SwitchPairChoice.BACK_BUTTON, true, false));
        assertEquals(2, read(SwitchPairChoice.BACK_BUTTON, false, true));
        assertEquals(1, read(SwitchPairChoice.BACK_BUTTON, true, true));
        // The second switch only counted while the first was on.
        for (SwitchPairChoice pair : new SwitchPairChoice[] {SwitchPairChoice.SYSTEM_KEYS, SwitchPairChoice.ROAMING}) {
            assertEquals(0, read(pair, false, false));
            assertEquals(0, read(pair, false, true));
            assertEquals(1, read(pair, true, false));
            assertEquals(2, read(pair, true, true));
        }
    }

    @Test
    public void nothingStoredReadsAsEachDefault() {
        assertEquals(0, SwitchPairChoice.BACK_BUTTON.read(prefs));
        assertEquals(1, SwitchPairChoice.SYSTEM_KEYS.read(prefs));
        assertEquals(1, SwitchPairChoice.ROAMING.read(prefs));
    }

    @Test
    public void everyChoiceSavesBothSwitchesAndReadsBack() {
        for (SwitchPairChoice pair : SwitchPairChoice.values()) {
            int names = RuntimeEnvironment.getApplication().getResources().getStringArray(pair.names).length;
            assertEquals(pair.name(), 3, names);
            for (int choice = 0; choice < names; choice++) {
                SharedPreferences.Editor editor = prefs.edit().clear();
                pair.save(editor, choice);
                editor.commit();
                assertTrue(prefs.contains(pair.firstKey));
                assertTrue(prefs.contains(pair.secondKey));
                assertEquals(pair.name() + " " + choice, choice, pair.read(prefs));
                assertSame(pair, SwitchPairChoice.forKey(pair.key));
            }
        }
    }

    @Test
    public void theRowsPickerWritesBothSwitches() {
        try (ActivityController<StreamSettings> controller = Robolectric.buildActivity(StreamSettings.class).setup()) {
            StreamSettings.SettingsFragment input = SettingsStructureTest.open(controller.get(), StreamSettings.CONTROLLER_SECTION);
            Preference back = input.findPreference(SwitchPairChoice.BACK_BUTTON.key);
            assertNotNull(back);
            assertTrue(back.getOnPreferenceClickListener().onPreferenceClick(back));
            ((AlertDialog) ShadowDialog.getLatestDialog()).getListView().performItemClick(null, 2, 0);
            ShadowLooper.idleMainLooper();
            assertFalse(prefs.getBoolean(PreferenceConfiguration.BACK_OPENS_MENU_PREF_STRING, true));
            assertTrue(prefs.getBoolean(PreferenceConfiguration.BACK_AS_GUIDE_PREF_STRING, false));
            assertEquals("Guide", String.valueOf(back.getSummary()));
            PreferenceConfiguration config = PreferenceConfiguration.readPreferences(RuntimeEnvironment.getApplication());
            assertFalse(config.backOpensMenu);
            assertTrue(config.backAsGuide);
        }
    }
}
