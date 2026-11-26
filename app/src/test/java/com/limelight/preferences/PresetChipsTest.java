package com.limelight.preferences;

import android.app.Application;
import android.preference.PreferenceManager;
import android.view.View;
import android.view.ViewGroup;
import android.widget.CheckedTextView;
import android.widget.LinearLayout;
import android.widget.ListView;
import android.widget.TextView;

import androidx.appcompat.app.AlertDialog;

import com.limelight.R;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowDialog;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

/** The Presets row's chips: two whole lines of two on a phone, never cut, and the row's one line says the preset in use. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class PresetChipsTest {
    @Before
    public void openMoreSettings() {
        StreamSettings.rememberRail(RuntimeEnvironment.getApplication(), true);
    }

    @After
    public void clear() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit().clear().commit();
    }

    private static View presetRow(StreamSettings screen) {
        SettingsStructureTest.settle(screen);
        View root = screen.findViewById(android.R.id.content);
        root.measure(View.MeasureSpec.makeMeasureSpec(1080, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(2400, View.MeasureSpec.EXACTLY));
        root.layout(0, 0, 1080, 2400);
        ListView list = screen.findViewById(android.R.id.list);
        View row = list.getChildAt(StreamSettings.RootFragment.positionOf(list, "stream_presets") - list.getFirstVisiblePosition());
        assertNotNull("the Presets row is on the overview", row);
        return row;
    }

    @Test
    public void twoByTwoOnAPhoneAndOneRowWhereThereIsRoom() {
        assertEquals(2, PresetChips.columns(360, 4));
        assertEquals(2, PresetChips.columns(411, 4));
        assertEquals(4, PresetChips.columns(600, 4));
        assertEquals(1, PresetChips.columns(360, 1));
    }

    @Test
    public void thePhonesPresetRowHasFourWholeChipsInTwoLines() {
        try (ActivityController<StreamSettings> controller = Robolectric.buildActivity(StreamSettings.class).setup().visible()) {
            StreamSettings screen = controller.get();
            View row = presetRow(screen);
            String[] names = screen.getResources().getStringArray(R.array.stream_preset_names);
            LinearLayout chips = row.findViewById(R.id.preset_chips);
            assertEquals("two lines", 2, chips.getChildCount());
            int touch = screen.getResources().getDimensionPixelSize(R.dimen.nj_size_touch_min);
            int index = 0;
            String checked = null;
            for (int at = 0; at < chips.getChildCount(); at++) {
                ViewGroup line = (ViewGroup) chips.getChildAt(at);
                assertEquals("two chips a line", 2, line.getChildCount());
                assertEquals("as wide as each other", line.getChildAt(0).getWidth(), line.getChildAt(1).getWidth());
                assertEquals("as tall as each other", line.getChildAt(0).getHeight(), line.getChildAt(1).getHeight());
                for (int i = 0; i < line.getChildCount(); i++) {
                    CheckedTextView chip = (CheckedTextView) line.getChildAt(i);
                    assertEquals(names[index++], chip.getText().toString());
                    assertNull("a name is never cut", chip.getEllipsize());
                    assertEquals("it wraps inside its chip instead", Integer.MAX_VALUE, chip.getMaxLines());
                    assertTrue(chip.getHeight() >= touch);
                    assertTrue(chip.isClickable());
                    assertFalse("the row is the D-pad's stop", chip.isFocusable());
                    if (chip.isChecked()) {
                        assertNull("one preset at most is in use", checked);
                        checked = chip.getText().toString();
                    }
                }
            }
            assertFalse(row.hasFocusable());
            // The row's one line says the preset in use without its tag, or Custom while no chip is lit.
            String value = ((TextView) row.findViewById(R.id.preset_value)).getText().toString();
            assertEquals(checked == null ? screen.getString(R.string.settings_custom) : checked.replaceFirst(" \\(.*\\)$", ""), value);
        }
    }

    @Test
    public void aChipAsksBeforeItReplacesTheSettings() {
        try (ActivityController<StreamSettings> controller = Robolectric.buildActivity(StreamSettings.class).setup().visible()) {
            StreamSettings screen = controller.get();
            LinearLayout chips = presetRow(screen).findViewById(R.id.preset_chips);
            ((ViewGroup) chips.getChildAt(1)).getChildAt(1).performClick();
            AlertDialog confirmation = (AlertDialog) ShadowDialog.getLatestDialog();
            assertNotNull(confirmation);
            assertEquals(screen.getString(R.string.preset_replace_settings),
                    ((TextView) confirmation.findViewById(android.R.id.message)).getText().toString());
            assertEquals(screen.getResources().getStringArray(R.array.stream_preset_names)[3],
                    ((TextView) confirmation.findViewById(androidx.appcompat.R.id.alertTitle)).getText().toString());
            confirmation.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
        }
    }
}
