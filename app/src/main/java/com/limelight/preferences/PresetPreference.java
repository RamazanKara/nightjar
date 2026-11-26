package com.limelight.preferences;

import android.app.Activity;
import android.content.Context;
import android.preference.Preference;
import android.util.AttributeSet;
import android.view.View;
import android.widget.LinearLayout;
import android.widget.TextView;

import com.limelight.ui.NavigationDialogBuilder;
import com.limelight.R;
import com.limelight.utils.UiHelper;
import com.limelight.binding.video.DisplayFrameRatePolicy;

public class PresetPreference extends Preference {
    public PresetPreference(Context context, AttributeSet attrs) {
        super(context, attrs);
        // By form: a TV is past w840dp, which outranks television in resource matching.
        setLayoutResource(com.limelight.ui.GlassForm.of(context) == com.limelight.ui.GlassForm.TELEVISION
                ? R.layout.settings_presets_tv : R.layout.settings_presets);
        setSelectable(true);
    }

    @Override
    protected void onClick() {
        String[] names = getContext().getResources().getStringArray(R.array.stream_preset_names);
        UiHelper.showDialog(getContext(), new NavigationDialogBuilder(getContext())
                .setTitle(R.string.stream_presets)
                .setItems(names, (dialog, which) -> confirmPreset(StreamPreset.values()[which]))
                .setNegativeButton(android.R.string.cancel, null).create());
    }

    private void confirmPreset(StreamPreset preset) {
        String nativeResolution = PreferenceConfiguration.nativeResolution(getContext());
        if (nativeResolution == null) return;
        float refresh = DisplayFrameRatePolicy.maxRefreshRate(((Activity) getContext()).getWindowManager().getDefaultDisplay());
        UiHelper.showDialog(getContext(), new NavigationDialogBuilder(getContext())
                .setTitle(getContext().getResources().getStringArray(R.array.stream_preset_names)[preset.ordinal()])
                .setMessage(R.string.preset_replace_settings)
                .setPositiveButton(android.R.string.ok, (dialog, which) -> {
                    preset.apply(getSharedPreferences(), nativeResolution, refresh);
                    notifyChanged();
                })
                .setNegativeButton(android.R.string.cancel, (dialog, which) -> notifyChanged())
                .setOnCancelListener(dialog -> notifyChanged()).create());
    }

    @Override
    protected void onBindView(View view) {
        super.onBindView(view);
        float refresh = DisplayFrameRatePolicy.maxRefreshRate(((Activity) getContext()).getWindowManager().getDefaultDisplay());
        String nativeResolution = PreferenceConfiguration.nativeResolution(getContext());
        boolean vrrOffered = DisplayFrameRatePolicy.vrrOffered(getContext());
        String[] names = getContext().getResources().getStringArray(R.array.stream_preset_names);
        // The preset the settings match now; none while they are Custom.
        int inUse = -1;
        for (StreamPreset preset : nativeResolution == null ? new StreamPreset[0] : StreamPreset.values()) {
            if (preset.matches(getSharedPreferences(), nativeResolution, refresh, vrrOffered)) inUse = preset.ordinal();
        }
        // A rail's row has no chips or info button: Select opens the list.
        LinearLayout chips = view.findViewById(R.id.preset_chips);
        if (chips != null && nativeResolution == null) chips.removeAllViews();
        else if (chips != null) {
            PresetChips.fill(chips, names, inUse,
                    PresetChips.columns(getContext().getResources().getConfiguration().screenWidthDp, names.length),
                    which -> confirmPreset(StreamPreset.values()[which]));
        }
        // A one-line value like every other row: "Native (recommended)" reads as "Native"; the chip keeps the tag.
        // Custom, with no chip lit, stands in the stronger ink: it is the one state the chips cannot show.
        String selected = inUse < 0 ? getContext().getString(R.string.settings_custom) : names[inUse];
        TextView value = view.findViewById(R.id.preset_value);
        value.setText(selected.replaceFirst(" \\(.*\\)$", ""));
        if (chips != null) {
            value.setTextColor(com.google.android.material.color.MaterialColors.getColor(value,
                    inUse < 0 ? com.google.android.material.R.attr.colorOnSurface : com.google.android.material.R.attr.colorOnSurfaceVariant));
        }
        Runnable showHelp = () ->
                UiHelper.showDialog(getContext(), new NavigationDialogBuilder(getContext()).setTitle(R.string.stream_presets)
                        .setMessage(R.string.stream_presets_help).setPositiveButton(android.R.string.ok, null).create());
        View help = view.findViewById(R.id.preset_help);
        if (help != null) help.setOnClickListener(v -> showHelp.run());
        view.setTag(R.id.preference_help, showHelp);
    }
}
