/**
 * The Resolution picker's steps relative to this screen (Nightjar #137): its own size at 85, 75, 67 and 50 %, beside
 * the fixed sizes. nightjar-policy's {@code resolution_scale}, the desktop's too, decides the sizes (even, the screen's
 * shape, never under 320x240) through {@link NightjarUiPolicyJni}, with a Java mirror where the library is not loaded
 * (pinned equal by ResolutionStepsTest). A step is stored as the plain {@code WxH} resolution value: no new key.
 */
package com.limelight.preferences;

import android.content.Context;
import android.preference.ListPreference;

import com.limelight.R;
import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

import java.text.NumberFormat;
import java.util.ArrayList;
import java.util.List;

final class ResolutionSteps {
    private ResolutionSteps() { }

    static final int[] SCALES = {100, 85, 75, 67, 50};
    static final int MIN_WIDTH = 320;
    static final int MIN_HEIGHT = 240;

    /** This screen's steps, flattened {percent, width, height, ...}, largest first. */
    static int[] choices(int nativeWidth, int nativeHeight) {
        if (NightjarJni.isAvailable()) {
            try {
                int[] choices = NightjarUiPolicyJni.resolutionChoices(nativeWidth, nativeHeight);
                if (choices != null) return choices;
            } catch (LinkageError ignored) {
                // Falls back to the mirror below.
            }
        }
        return choicesMirror(nativeWidth, nativeHeight);
    }

    /** resolution_scale::choices in Java. */
    static int[] choicesMirror(int nativeWidth, int nativeHeight) {
        List<int[]> out = new ArrayList<>();
        for (int percent : SCALES) {
            int[] size = scaled(Math.max(0, nativeWidth), Math.max(0, nativeHeight), percent);
            int[] last = out.isEmpty() ? null : out.get(out.size() - 1);
            if (last == null || last[1] != size[0] || last[2] != size[1]) out.add(new int[] {percent, size[0], size[1]});
        }
        int[] flat = new int[out.size() * 3];
        for (int i = 0; i < out.size(); i++) System.arraycopy(out.get(i), 0, flat, i * 3, 3);
        return flat;
    }

    /** resolution_scale::scaled in Java for the steps above: 67 % is exactly two thirds. */
    static int[] scaled(int width, int height, int percent) {
        long num = percent == 67 ? 2 : percent;
        long den = percent == 67 ? 3 : 100;
        long w = percent >= 100 ? width : (width * num + den / 2) / den;
        long h = percent >= 100 ? height : (height * num + den / 2) / den;
        return new int[] {even((int) Math.max(w, MIN_WIDTH)), even((int) Math.max(h, MIN_HEIGHT))};
    }

    private static int even(int value) {
        return (value + 1) & ~1;
    }

    /**
     * Adds this screen's steps under 100 % to the picker, each named by its share of the screen; a size the list already
     * has (720p is two thirds of 1080p) keeps its own entry.
     */
    static void addTo(Context context, ListPreference pref, String panel) {
        if (panel == null) return;
        int width = PreferenceConfiguration.getWidthFromResolutionString(panel);
        int height = PreferenceConfiguration.getHeightFromResolutionString(panel);
        if (width <= 0 || height <= 0) return;
        int[] steps = choices(width, height);
        NumberFormat percent = NumberFormat.getPercentInstance(context.getResources().getConfiguration().locale);
        for (int i = 0; i + 2 < steps.length; i += 3) {
            if (steps[i] >= 100) continue;
            String value = steps[i + 1] + "x" + steps[i + 2];
            if (contains(pref, value)) continue;
            String name = context.getString(R.string.resolution_screen_percent, percent.format(steps[i] / 100.0),
                    steps[i + 1] + "×" + steps[i + 2]);
            StreamSettingsDisplayOptions.appendPreferenceEntry(pref, name, value);
        }
    }

    private static boolean contains(ListPreference pref, String value) {
        for (CharSequence entry : pref.getEntryValues()) if (value.equals(entry.toString())) return true;
        return false;
    }
}
