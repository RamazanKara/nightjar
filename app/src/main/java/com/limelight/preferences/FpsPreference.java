package com.limelight.preferences;

import android.content.Context;
import android.content.SharedPreferences;
import android.os.Bundle;
import android.preference.PreferenceManager;
import android.text.InputType;
import android.util.AttributeSet;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;

import androidx.appcompat.app.AlertDialog;

import com.google.android.material.button.MaterialButton;
import com.google.android.material.dialog.MaterialAlertDialogBuilder;
import com.google.android.material.textfield.TextInputEditText;
import com.google.android.material.textfield.TextInputLayout;
import com.limelight.R;
import com.limelight.ui.NavigationDialogBuilder;
import com.limelight.utils.UiHelper;

import java.util.Arrays;

/**
 * The frame rate list. The list offers only the rates this screen supports unless {@code checkbox_unlock_fps} is
 * on, so the picker itself switches between that list and every rate, instead of a switch elsewhere in Settings.
 * Also allows entering a custom frame rate between 10 and 500 FPS.
 */
public class FpsPreference extends MaterialListPreference {
    public static final int MIN_CUSTOM_FPS = 10;
    public static final int MAX_CUSTOM_FPS = 500;

    public FpsPreference(Context context, AttributeSet attrs) {
        super(context, attrs);
    }

    // Keep a saved custom rate visible after the device-dependent preset list has been built.
    void ensureSelectedFpsEntry() {
        ensureFpsEntry(getValue());
    }

    private void ensureFpsEntry(String value) {
        if (value == null || value.isEmpty() || findIndexOfValue(value) >= 0) return;
        CharSequence[] names = Arrays.copyOf(getEntries(), getEntries().length + 1);
        CharSequence[] values = Arrays.copyOf(getEntryValues(), getEntryValues().length + 1);
        names[names.length - 1] = getContext().getString(R.string.resolution_custom_size,
                value + " " + getContext().getString(R.string.fps_suffix_fps));
        values[values.length - 1] = value;
        setEntries(names);
        setEntryValues(values);
    }

    @Override
    protected void showDialog(Bundle state) {
        SharedPreferences prefs = PreferenceManager.getDefaultSharedPreferences(getContext());
        boolean all = prefs.getBoolean(PreferenceConfiguration.UNLOCK_FPS_STRING, false);
        MaterialAlertDialogBuilder builder = new NavigationDialogBuilder(getContext())
                .setTitle(getTitle())
                .setSingleChoiceItems(getEntries(), findIndexOfValue(getValue()), (dialog, which) -> {
                    String value = getEntryValues()[which].toString();
                    if (callChangeListener(value)) {
                        setValue(value);
                    }
                    dialog.dismiss();
                })
                .setPositiveButton(all ? R.string.fps_show_supported : R.string.title_unlock_fps, (dialog, which) -> {
                    prefs.edit().putBoolean(PreferenceConfiguration.UNLOCK_FPS_STRING, !all).apply();
                    // The list is built from this choice when the screen is created.
                    if (getContext() instanceof StreamSettings) ((StreamSettings) getContext()).reloadSettings();
                })
                .setNeutralButton(R.string.resolution_custom, (dialog, which) -> showCustomFps())
                .setNegativeButton(android.R.string.cancel, null);
        AlertDialog picker = builder.create();
        UiHelper.showDialog(getContext(), picker);
        addHelpButton(picker);
    }

    private void addHelpButton(AlertDialog picker) {
        if (getDialogMessage() == null) return;
        Button positive = picker.getButton(AlertDialog.BUTTON_POSITIVE);
        if (positive == null || !(positive.getParent() instanceof ViewGroup)) return;
        ViewGroup bar = (ViewGroup) positive.getParent();
        MaterialButton help = new MaterialButton(picker.getContext(), null, android.R.attr.buttonBarNeutralButtonStyle);
        help.setText(R.string.help);
        if (positive.getLayoutParams() != null) {
            help.setLayoutParams(new LinearLayout.LayoutParams((LinearLayout.LayoutParams) positive.getLayoutParams()));
        }
        help.setOnClickListener(view -> showHelp());
        bar.addView(help, 0);
    }

    private void showHelp() {
        if (getDialogMessage() == null) return;
        UiHelper.showDialog(getContext(), new NavigationDialogBuilder(getContext())
                .setTitle(getTitle())
                .setMessage(getDialogMessage())
                .setPositiveButton(android.R.string.ok, null)
                .create());
    }

    private void showCustomFps() {
        Context context = getContext();
        LinearLayout form = new LinearLayout(context);
        form.setOrientation(LinearLayout.VERTICAL);
        int padding = Math.round(24 * context.getResources().getDisplayMetrics().density);
        form.setPadding(padding, 0, padding, padding);
        TextInputLayout container = new TextInputLayout(context);
        container.setHint(R.string.title_fps_list);
        EditText input = new TextInputEditText(container.getContext());
        input.setId(android.R.id.edit);
        input.setContentDescription(context.getString(R.string.title_fps_list));
        input.setInputType(InputType.TYPE_CLASS_NUMBER);
        input.setSingleLine(true);
        input.setSelectAllOnFocus(true);
        input.setMinHeight(Math.round(56 * context.getResources().getDisplayMetrics().density));
        if (getValue() != null) input.setText(getValue());
        container.addView(input);
        form.addView(container);
        ScrollView scroll = new ScrollView(context);
        scroll.addView(form);
        AlertDialog editor = new NavigationDialogBuilder(context).setTitle(R.string.title_fps_list)
                .setView(scroll).setPositiveButton(android.R.string.ok, null)
                .setNegativeButton(android.R.string.cancel, null).create();
        UiHelper.showDialog(context, editor);
        editor.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(view -> {
            Integer parsed = readFps(container);
            if (parsed == null) return;
            String fps = Integer.toString(parsed);
            if (!callChangeListener(fps)) return;
            ensureFpsEntry(fps);
            setValue(fps);
            editor.dismiss();
        });
    }

    private Integer readFps(TextInputLayout container) {
        container.setError(null);
        try {
            return HostStreamProfile.requireRange(Integer.parseInt(
                    container.getEditText().getText().toString().trim()), MIN_CUSTOM_FPS, MAX_CUSTOM_FPS);
        } catch (IllegalArgumentException e) {
            container.setError(getContext().getString(R.string.host_profile_invalid_number,
                    MIN_CUSTOM_FPS, MAX_CUSTOM_FPS));
            return null;
        }
    }
}
