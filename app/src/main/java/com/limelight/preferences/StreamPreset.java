package com.limelight.preferences;

import android.content.SharedPreferences;
import android.os.Build;

import com.limelight.binding.video.DisplayFrameRatePolicy;
import com.limelight.binding.video.MediaCodecHelper;

// The chips' order, as nightjar_policy::presets::Preset::ALL; matching takes the first that fits.
public enum StreamPreset {
    // The phone's own resolution at the panel's top refresh rate, with every latency switch on.
    NATIVE,
    // The phone's own resolution at the panel's top refresh rate, paced for the smoothest picture.
    SMOOTH_TV,
    // Two thirds of the phone's resolution at 60 fps (never above the panel), balanced pacing: for a weaker network.
    WIFI_AWAY,
    // The phone's own resolution at 30 fps, with the panel slowed to match and the GPU left alone.
    BATTERY_SAVER;

    // Wi-Fi / Away's frame rate, never above the panel's top rate.
    static final int WIFI_AWAY_FPS = 60;

    // Switches that cut latency. All are on by default, and Native turns them back on; the other presets leave
    // them as they are. Max GPU clocks is not one of them: it is opt-in, and only Battery saver changes it (to off).
    static final String[] LATENCY_SWITCHES = {
            "checkbox_codec_low_latency",
            "checkbox_vendor_low_latency",
            "checkbox_phone_performance_hints",
            "checkbox_drop_late_frames",
            "checkbox_unbatched_input",
            "checkbox_network_priority",
    };

    // Where Android's low-latency mode can corrupt the picture, it starts off and only the person turns it
    // on, with its own switch or the "Low-latency decoding" master that sets it too. No preset sets it or asks for it.
    static boolean setsLatencySwitch(String key) {
        return !"checkbox_codec_low_latency".equals(key) ||
                // The model first: everything else never starts MediaCodecHelper for this.
                !MediaCodecHelper.LOW_LATENCY_CORRUPTION_MODEL.equals(Build.MODEL) ||
                !MediaCodecHelper.hasLowLatencyCorruptingDecoder();
    }

    int fps(float panelMaxHz) {
        int screen = DisplayFrameRatePolicy.streamFrameRate(panelMaxHz);
        switch (this) {
            case BATTERY_SAVER: return 30;
            case WIFI_AWAY: return Math.min(WIFI_AWAY_FPS, screen);
            default: return screen;
        }
    }

    String pacing() {
        switch (this) {
            case SMOOTH_TV: return FramePacingChoice.SMOOTHNESS;
            case WIFI_AWAY: return FramePacingChoice.BALANCED;
            case BATTERY_SAVER: return FramePacingChoice.CAP_FPS;
            default: return PreferenceConfiguration.DEFAULT_FRAME_PACING;
        }
    }

    // The resolution it streams at: the screen's own, or two thirds of it for Wi-Fi / Away.
    String resolution(String nativeResolution) {
        return this == WIFI_AWAY ? twoThirds(nativeResolution) : nativeResolution;
    }

    // Two thirds of a WxH resolution: the Resolution picker's 67 % step (ResolutionSteps, resolution_scale::scaled).
    static String twoThirds(String resolution) {
        int x = resolution.indexOf('x');
        try {
            int[] size = ResolutionSteps.scaled(Integer.parseInt(resolution.substring(0, x)),
                    Integer.parseInt(resolution.substring(x + 1)), 67);
            return size[0] + "x" + size[1];
        } catch (RuntimeException e) {
            return resolution;
        }
    }

    // A VRR choice saved where it isn't offered reads as off (SettingRules.vrrOn), so it doesn't make a preset custom.
    // Balanced reads as Balanced whichever newest-frame switch is on: that switch is a latency switch, which only
    // Native sets.
    boolean matches(SharedPreferences preferences, String nativeResolution, float panelMaxHz, boolean vrrOffered) {
        String resolution = resolution(nativeResolution);
        String fps = Integer.toString(fps(panelMaxHz));
        int bitrate = PreferenceConfiguration.getShownDefaultBitrate(preferences, resolution, fps, null);
        boolean followsScreen = preferences.getBoolean(
                PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING, false);
        String saved = preferences.getString(PreferenceConfiguration.RESOLUTION_PREF_STRING, PreferenceConfiguration.DEFAULT_RESOLUTION);
        boolean resolutionMatches = this == WIFI_AWAY ? !followsScreen && resolution.equals(saved)
                : followsScreen || resolution.equals(saved);
        if (!resolutionMatches ||
                !fps.equals(preferences.getString(PreferenceConfiguration.FPS_PREF_STRING, PreferenceConfiguration.DEFAULT_FPS)) ||
                preferences.getInt(PreferenceConfiguration.BITRATE_PREF_STRING, bitrate) != bitrate ||
                vrrOffered && preferences.getBoolean("checkbox_vrr", false) ||
                !pacing().equals(FramePacingChoice.pacing(FramePacingChoice.read(preferences))) ||
                !"off".equals(preferences.getString(PreferenceConfiguration.UPSCALING_PREF_STRING, "off")) ||
                preferences.getBoolean("checkbox_reduce_refresh_rate", false) != (this == BATTERY_SAVER) ||
                preferences.getBoolean("checkbox_codec_performance", true) != (this != BATTERY_SAVER)) {
            return false;
        }
        if (this == BATTERY_SAVER) {
            return !preferences.getBoolean(PreferenceConfiguration.GPU_MAX_CLOCKS_PREF_STRING,
                    PreferenceConfiguration.DEFAULT_GPU_MAX_CLOCKS);
        }
        if (this == NATIVE) {
            for (String key : LATENCY_SWITCHES) {
                if (setsLatencySwitch(key) && !preferences.getBoolean(key, true)) {
                    return false;
                }
            }
        }
        return true;
    }

    // The codec, HDR and 4:4:4 choices stay as they are; the bitrate is the default for that codec, size and rate.
    void apply(SharedPreferences preferences, String nativeResolution, float panelMaxHz) {
        String resolution = resolution(nativeResolution);
        String fps = Integer.toString(fps(panelMaxHz));
        SharedPreferences.Editor editor = preferences.edit()
                .putString(PreferenceConfiguration.RESOLUTION_PREF_STRING, resolution)
                // Wi-Fi / Away's two thirds is a fixed size, so it doesn't follow the screen.
                .putBoolean(PreferenceConfiguration.RESOLUTION_FOLLOWS_SCREEN_PREF_STRING, this != WIFI_AWAY)
                .putString(PreferenceConfiguration.FPS_PREF_STRING, fps)
                .putInt(PreferenceConfiguration.BITRATE_PREF_STRING,
                        PreferenceConfiguration.getShownDefaultBitrate(preferences, resolution, fps, null))
                .putBoolean(PreferenceConfiguration.BITRATE_USER_SET_PREF_STRING, false)
                // Variable refresh rate Off: Real VRR stays as it was (VrrChoice).
                .putBoolean("checkbox_vrr", false)
                .putString(PreferenceConfiguration.UPSCALING_PREF_STRING, "off")
                // Frame pacing as its row shows it: this pacing, not the PC's frame times (FramePacingChoice).
                .putString(PreferenceConfiguration.FRAME_PACING_PREF_STRING, pacing())
                .putBoolean(PreferenceConfiguration.HOST_FRAME_TIMES_PREF_STRING, false)
                .putBoolean("checkbox_reduce_refresh_rate", this == BATTERY_SAVER)
                .putBoolean("checkbox_codec_performance", this != BATTERY_SAVER);
        if (this == BATTERY_SAVER) {
            editor.putBoolean(PreferenceConfiguration.GPU_MAX_CLOCKS_PREF_STRING, false);
        } else if (this == NATIVE) {
            for (String key : LATENCY_SWITCHES) {
                if (setsLatencySwitch(key)) editor.putBoolean(key, true);
            }
        }
        editor.apply();
    }
}
