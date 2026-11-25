package com.limelight.binding.video;

import android.content.Context;
import android.os.Build;
import android.os.PowerManager;
import android.provider.Settings;
import android.view.Display;


import com.limelight.LimeLog;

import java.util.ArrayList;
import java.util.List;

/**
 * Reads the display and the system around it once a second while a stream holds a frame rate vote,
 * and returns the text for the overlay's status line when the screen has not followed the vote for
 * long enough. The decision is {@link RefreshSwitchDiagnosis}; this only gathers its inputs.
 */
public final class RefreshSwitchMonitor {
    private final Context context;
    private final RefreshSwitchDiagnosis.Watcher watcher = new RefreshSwitchDiagnosis.Watcher();
    private RefreshSwitchDiagnosis.Result lastSettled;
    private String logged;

    public RefreshSwitchMonitor(Context context) {
        this.context = context.getApplicationContext() != null ? context.getApplicationContext() : context;
    }

    /** The settled mismatch from the latest {@link #poll}, or null when holding or unsettled. */
    public RefreshSwitchDiagnosis.Result lastSettled() {
        return lastSettled;
    }

    /**
     * @param voteHz the rate last voted on the video surface, null or 0 when there is no vote to
     *               follow (none made yet, or the zero vote that leaves ARR in charge)
     * @return the status line text, or null when the screen follows the vote or it is too soon to say
     */
    public String poll(long nowMs, Display display, Float voteHz, boolean seamlessOnly) {
        if (voteHz == null || voteHz <= 0 || Build.VERSION.SDK_INT < Build.VERSION_CODES.R || display == null) {
            watcher.observe(nowMs, null);
            lastSettled = null;
            return null;
        }
        RefreshSwitchDiagnosis.PanelMode active = activeMode(display);
        if (RefreshSwitchDiagnosis.holds(voteHz, active.hz)) {
            watcher.observe(nowMs, null);
            lastSettled = null;
            logged = null;
            return null;
        }
        RefreshSwitchDiagnosis.Result result = RefreshSwitchDiagnosis.diagnose(Build.VERSION.SDK_INT, voteHz,
                seamlessOnly, active, modes(display), conditions(context));
        RefreshSwitchDiagnosis.Result settled = watcher.observe(nowMs, result);
        lastSettled = settled;
        if (settled == null) {
            if (!result.mismatch()) logged = null;
            return null;
        }
        String line = settled.logLine(seamlessOnly);
        if (!line.equals(logged)) {
            logged = line;
            LimeLog.warning(line);
        }
        return RefreshSwitchDiagnosis.describe(context, settled);
    }

    static RefreshSwitchDiagnosis.PanelMode activeMode(Display display) {
        return panelMode(display.getMode());
    }

    static List<RefreshSwitchDiagnosis.PanelMode> modes(Display display) {
        List<RefreshSwitchDiagnosis.PanelMode> modes = new ArrayList<>();
        for (Display.Mode mode : display.getSupportedModes()) modes.add(panelMode(mode));
        return modes;
    }

    private static RefreshSwitchDiagnosis.PanelMode panelMode(Display.Mode mode) {
        return new RefreshSwitchDiagnosis.PanelMode(mode.getModeId(), mode.getPhysicalWidth(),
                mode.getPhysicalHeight(), mode.getRefreshRate(),
                Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ? mode.getAlternativeRefreshRates() : null);
    }

    /** What may hold the screen below a vote: battery saver, critical heat, the user's peak refresh setting. */
    public static RefreshSwitchDiagnosis.Conditions conditions(Context context) {
        boolean batterySaver = false;
        boolean thermalCritical = false;
        PowerManager power = (PowerManager) context.getSystemService(Context.POWER_SERVICE);
        if (power != null) {
            batterySaver = power.isPowerSaveMode();
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                thermalCritical = power.getCurrentThermalStatus() >= PowerManager.THERMAL_STATUS_CRITICAL;
            }
        }
        float peak = 0f;
        try {
            // The user's Smooth display or refresh rate choice; the key is not public API, so an
            // absent or unreadable value only means "unknown".
            peak = Settings.System.getFloat(context.getContentResolver(), "peak_refresh_rate", 0f);
        } catch (RuntimeException ignored) {
            // A vendor ROM may refuse the read; the diagnosis then has no user limit to name.
        }
        return new RefreshSwitchDiagnosis.Conditions(batterySaver, thermalCritical, peak);
    }
}
