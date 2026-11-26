package com.limelight.preferences;

import android.content.Context;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.CheckedTextView;
import android.widget.LinearLayout;

import com.limelight.R;

/**
 * The preset chips of the Presets row (PresetPreference): four outcomes, one tap each. Two by two on a phone held
 * upright and one row where there is room; every chip of a line is as wide and as tall as its neighbours, and a name
 * too long for its chip wraps inside it, so no translation is cut or runs to a third line of chips. The preset in use
 * wears the amber container; none does while the settings are Custom.
 */
final class PresetChips {
    /** From this width on the four chips share one row. */
    private static final int ONE_ROW_DP = 600;

    private PresetChips() { }

    /** A tapped chip's place in the names. */
    interface OnPick {
        void picked(int which);
    }

    /** Chips beside each other on a screen this wide. */
    static int columns(int screenWidthDp, int count) {
        return screenWidthDp < ONE_ROW_DP ? Math.min(2, count) : count;
    }

    /**
     * Fills {@code container} with one chip per name, {@code checked} (or none, for -1) in the amber container. The
     * chips of a settings row take no focus of their own: the row is the D-pad's stop and Select opens the presets.
     */
    static void fill(LinearLayout container, CharSequence[] names, int checked, int columns, OnPick onPick) {
        container.removeAllViews();
        container.setOrientation(LinearLayout.VERTICAL);
        Context context = container.getContext();
        int gap = context.getResources().getDimensionPixelSize(R.dimen.space_s);
        LayoutInflater inflater = LayoutInflater.from(context);
        for (int start = 0; start < names.length; start += columns) {
            LinearLayout line = new LinearLayout(context);
            line.setBaselineAligned(false);
            for (int column = 0; column < columns; column++) {
                int index = start + column;
                if (index >= names.length) {
                    // A last line with fewer chips keeps their width.
                    LinearLayout.LayoutParams blank = new LinearLayout.LayoutParams(0, 0, 1f);
                    blank.setMarginStart(gap);
                    line.addView(new View(context), blank);
                    continue;
                }
                CheckedTextView chip = (CheckedTextView) inflater.inflate(R.layout.settings_preset_chip, line, false);
                chip.setText(names[index]);
                chip.setChecked(index == checked);
                chip.setOnClickListener(view -> onPick.picked(index));
                // A clickable view would turn focusable by itself.
                chip.setFocusable(false);
                if (column > 0) ((LinearLayout.LayoutParams) chip.getLayoutParams()).setMarginStart(gap);
                line.addView(chip);
            }
            LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.WRAP_CONTENT);
            if (start > 0) params.topMargin = gap;
            container.addView(line, params);
        }
    }
}
