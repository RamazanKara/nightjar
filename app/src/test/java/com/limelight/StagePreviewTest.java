package com.limelight;

import android.app.Application;
import android.os.Build;

import android.preference.PreferenceManager;

import com.limelight.PcViewLifecycleTest.Library;
import com.limelight.preferences.GlPreferences;
import com.limelight.ui.AppTheme;
import com.limelight.ui.ConnectingScreen;
import com.limelight.ui.ScreenShots;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;
import org.robolectric.shadows.ShadowLooper;

import java.io.File;

/**
 * Optional pictures of the screens that stand on the stage, on a phone, a handheld and a TV and in each look. Set
 * NIGHTJAR_PREVIEW_OUTPUT to capture PNGs under {@code stage/}; without it every test here is skipped.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class StagePreviewTest {
    static final String PHONE = "w412dp-h915dp-port";
    static final String HANDHELD = "w960dp-h540dp-land";
    static final String TV = "w960dp-h540dp-land-television";

    private static ActivityController<Library> firstRun() {
        Application application = RuntimeEnvironment.getApplication();
        GlPreferences gl = GlPreferences.readPreferences(application);
        gl.savedFingerprint = Build.FINGERPRINT;
        gl.glRenderer = "Test renderer";
        gl.writePreferences();
        application.getSharedPreferences("FirstRun", 0).edit().putBoolean("pairing_guide_shown", true).commit();
        return Robolectric.buildActivity(Library.class).setup().visible();
    }

    private static void firstRun(String name, int w, int h) throws Exception {
        File directory = ScreenShots.directory("stage");
        ActivityController<Library> controller = firstRun();
        try {
            ShadowLooper.idleMainLooper();
            ScreenShots.capture(controller.get(), w, h, new File(directory, "first-run-" + name + ".png"));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    private static void set(String key, Object value) {
        var edit = PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit();
        if (value instanceof Boolean) edit.putBoolean(key, (Boolean) value);
        else edit.putString(key, (String) value);
        edit.commit();
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void firstRunPhoneNight() throws Exception {
        firstRun("phone-night", 824, 1830);
    }

    @Test @Config(qualifiers = PHONE + "-xhdpi")
    public void firstRunPhoneDay() throws Exception {
        firstRun("phone-day", 824, 1830);
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void firstRunPhoneOled() throws Exception {
        set(AppTheme.THEME, AppTheme.THEME_OLED_BLACK);
        firstRun("phone-oled", 824, 1830);
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void firstRunPhoneHighContrast() throws Exception {
        set(AppTheme.HIGH_CONTRAST, true);
        firstRun("phone-high-contrast", 824, 1830);
    }

    @Test @Config(qualifiers = HANDHELD + "-night-xhdpi")
    public void firstRunHandheldNight() throws Exception {
        firstRun("handheld-night", 1920, 1080);
    }

    @Test @Config(qualifiers = HANDHELD + "-xhdpi")
    public void firstRunHandheldDay() throws Exception {
        firstRun("handheld-day", 1920, 1080);
    }

    @Test @Config(qualifiers = TV + "-night-xhdpi")
    public void firstRunTv() throws Exception {
        firstRun("tv-night", 1920, 1080);
    }

    private static void connecting(String name, int w, int h) throws Exception {
        File directory = ScreenShots.directory("stage");
        try (var controller = Robolectric.buildActivity(com.limelight.ui.ConnectingScreenTest.Stream.class).setup().visible()) {
            ConnectingScreen screen = ConnectingScreen.show(controller.get(), "Racing Game", "Living-room PC", "Connecting",
                    "Starting connection", () -> { });
            ShadowLooper.idleMainLooper();
            ScreenShots.capture(controller.get(), screen.dialog(), w, h, new File(directory, "connecting-" + name + ".png"));
            screen.dismiss();
        }
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void connectingPhone() throws Exception {
        connecting("phone-night", 824, 1830);
    }

    @Test @Config(qualifiers = HANDHELD + "-night-xhdpi")
    public void connectingHandheld() throws Exception {
        connecting("handheld-night", 1920, 1080);
    }

    @Test @Config(qualifiers = TV + "-night-xhdpi")
    public void connectingTv() throws Exception {
        connecting("tv-night", 1920, 1080);
    }

    private static void pin(String name, int w, int h) throws Exception {
        File directory = ScreenShots.directory("stage");
        try (var controller = Robolectric.buildActivity(com.limelight.pin.PinLockActivity.class,
                com.limelight.pin.PinLockActivity.forSet(RuntimeEnvironment.getApplication())).setup().visible()) {
            ShadowLooper.idleMainLooper();
            ScreenShots.capture(controller.get(), w, h, new File(directory, "pin-" + name + ".png"));
        }
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void pinPhone() throws Exception {
        pin("phone-night", 824, 1830);
    }

    @Test @Config(qualifiers = HANDHELD + "-night-xhdpi")
    public void pinHandheld() throws Exception {
        pin("handheld-night", 1920, 1080);
    }
}
