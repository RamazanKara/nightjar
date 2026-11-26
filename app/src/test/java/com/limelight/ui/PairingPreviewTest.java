package com.limelight.ui;

import android.app.Activity;
import android.app.Application;
import android.graphics.drawable.GradientDrawable;
import android.preference.PreferenceManager;
import android.widget.FrameLayout;

import androidx.appcompat.app.AlertDialog;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

import java.io.File;
import java.io.IOException;

/**
 * Pictures of the pairing panel and the QR viewfinder on a phone, a handheld and a TV, written only when
 * NIGHTJAR_PREVIEW_OUTPUT names a directory (see {@link ScreenShots}).
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class)
public class PairingPreviewTest {
    private static final String PHONE = "w412dp-h915dp-port";
    private static final String HANDHELD = "w960dp-h540dp-land";
    private static final String TV = "w960dp-h540dp-land-television";

    private interface Panel {
        AlertDialog show(Activity screen);
    }

    private static void picture(String name, int w, int h, Panel panel) throws IOException {
        File directory = ScreenShots.directory("pairing");
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            AlertDialog dialog = panel.show(controller.get());
            ScreenShots.capture(controller.get(), dialog, w, h, new File(directory, name + ".png"));
            dialog.dismiss();
        }
    }

    private static AlertDialog pin(Activity screen) {
        return PairingCinemaSheet.show(screen, "Living room PC", null, "0417", () -> { });
    }

    private static AlertDialog failure(Activity screen) {
        return PairingCinemaSheet.showFailure(screen, screen.getString(com.limelight.R.string.pair_pairing_title),
                screen.getString(com.limelight.R.string.pair_connection_fix), () -> { });
    }

    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void pinPhoneNight() throws IOException {
        picture("pin-phone-night", 824, 1830, PairingPreviewTest::pin);
    }

    @Test
    @Config(qualifiers = PHONE + "-notnight-xhdpi")
    public void pinPhoneDay() throws IOException {
        picture("pin-phone-day", 824, 1830, PairingPreviewTest::pin);
    }

    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void pinPhoneHighContrast() throws IOException {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putBoolean(AppTheme.HIGH_CONTRAST, true).commit();
        picture("pin-phone-high-contrast", 824, 1830, PairingPreviewTest::pin);
    }

    @Test
    @Config(qualifiers = "w320dp-h568dp-port-night-xhdpi")
    public void pinSmallPhone() throws IOException {
        picture("pin-small-phone", 640, 1136, PairingPreviewTest::pin);
    }

    @Test
    @Config(qualifiers = HANDHELD + "-night-xhdpi")
    public void pinHandheld() throws IOException {
        picture("pin-handheld", 1920, 1080, PairingPreviewTest::pin);
    }

    @Test
    @Config(qualifiers = TV + "-night-xhdpi")
    public void pinTv() throws IOException {
        picture("pin-tv", 1920, 1080, PairingPreviewTest::pin);
    }

    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void waitingPhone() throws IOException {
        picture("waiting-phone", 824, 1830, screen -> PairingCinemaSheet.show(screen, "Living room PC", "9041", "9041", () -> { }));
    }

    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void failurePhone() throws IOException {
        picture("failure-phone", 824, 1830, PairingPreviewTest::failure);
    }

    @Test
    @Config(qualifiers = TV + "-night-xhdpi")
    public void failureTv() throws IOException {
        picture("failure-tv", 1920, 1080, PairingPreviewTest::failure);
    }

    /** The viewfinder over a stand-in for the camera's picture, before and after a code was read. */
    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void viewfinder() throws IOException {
        File directory = ScreenShots.directory("pairing");
        for (boolean recognised : new boolean[] {false, true}) {
            try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
                FrameLayout camera = new FrameLayout(controller.get());
                camera.setBackground(new GradientDrawable(GradientDrawable.Orientation.TL_BR, new int[] {0xFF8A8F99, 0xFF2B2E36, 0xFFD8D2C4}));
                ViewfinderFrame frame = new ViewfinderFrame(controller.get());
                frame.setRecognised(recognised);
                camera.addView(frame);
                controller.get().setContentView(camera);
                ScreenShots.capture(controller.get(), 824, 1200, new File(directory, recognised ? "viewfinder-recognised.png" : "viewfinder.png"));
            }
        }
    }
}
