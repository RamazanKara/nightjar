package com.limelight.preferences;

import android.content.Context;
import android.preference.Preference;
import android.util.AttributeSet;
import android.view.View;
import android.view.ViewGroup;

/**
 * A plain row that carries a preview card (SettingsPreviews): Video size, whose choice is stored through
 * VideoScaling and picked through its click listener, as before.
 */
public class PreviewRowPreference extends Preference {
    public PreviewRowPreference(Context context, AttributeSet attrs) {
        super(context, attrs);
    }

    @Override
    public View getView(View convertView, ViewGroup parent) {
        View view = super.getView(convertView, parent);
        SettingsPreviews.bind(this, view, parent);
        return view;
    }
}
