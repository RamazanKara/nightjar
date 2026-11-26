package com.limelight.preferences;

import android.content.Context;
import android.os.Bundle;
import android.preference.ListPreference;
import android.util.AttributeSet;
import android.view.View;
import android.view.ViewGroup;
import androidx.appcompat.app.AlertDialog;

import com.google.android.material.dialog.MaterialAlertDialogBuilder;
import com.limelight.ui.NavigationDialogBuilder;
import com.limelight.R;
import com.limelight.utils.UiHelper;

public class MaterialListPreference extends ListPreference {
    public MaterialListPreference(Context context, AttributeSet attrs) {
        super(context, attrs);
        // The row shows the chosen value; the explanation opens from the picker's Help button.
        if (getDialogMessage() == null) {
            setDialogMessage(getSummary());
        }
    }

    /** A setting with a preview card shows the picture of its value in its row (SettingsPreviews). */
    @Override
    public View getView(View convertView, ViewGroup parent) {
        View view = super.getView(convertView, parent);
        SettingsPreviews.bind(this, view, parent);
        return view;
    }

    private void showHelp() {
        UiHelper.showDialog(getContext(), new NavigationDialogBuilder(getContext()).setTitle(getTitle())
                .setMessage(getDialogMessage()).setPositiveButton(android.R.string.ok, null).create());
    }

    @Override
    protected void showDialog(Bundle state) {
        if (SettingsPreviews.has(getKey()) && getEntries() != null) {
            // Its choices are cards with the picture of what each does; the value is stored as from the list.
            SettingsPreviews.pick(this, getDialogMessage() == null ? null : this::showHelp, which -> {
                String value = getEntryValues()[which].toString();
                if (callChangeListener(value)) setValue(value);
            });
            return;
        }
        MaterialAlertDialogBuilder builder = new NavigationDialogBuilder(getContext())
                .setTitle(getTitle())
                .setSingleChoiceItems(getEntries(), findIndexOfValue(getValue()), (dialog, which) -> {
                    String value = getEntryValues()[which].toString();
                    if (callChangeListener(value)) {
                        setValue(value);
                    }
                    dialog.dismiss();
                })
                .setNegativeButton(android.R.string.cancel, null);
        if (getDialogMessage() != null) {
            builder.setNeutralButton(R.string.help, null);
        }
        customizePicker(builder);
        AlertDialog picker = builder.create();
        UiHelper.showDialog(getContext(), picker);
        if (getDialogMessage() != null) {
            picker.getButton(AlertDialog.BUTTON_NEUTRAL).setOnClickListener(view -> showHelp());
        }
    }

    protected void customizePicker(MaterialAlertDialogBuilder builder) {
    }
}
