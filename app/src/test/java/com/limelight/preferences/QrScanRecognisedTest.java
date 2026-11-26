package com.limelight.preferences;

import android.app.Activity;
import android.app.Application;
import android.os.Looper;

import com.limelight.R;
import com.limelight.ui.ViewfinderFrame;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.util.ReflectionHelpers;
import org.robolectric.util.ReflectionHelpers.ClassParameter;

import java.time.Duration;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;
import static org.robolectric.Shadows.shadowOf;

/** A pairing code read in the viewfinder lights its corners in the accent before the scan closes with the link. */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 34, qualifiers = "w412dp-h915dp-port-xhdpi", application = Application.class)
public class QrScanRecognisedTest {
    private static final String LINK = "rubylight://pair?v=1&host=pc&port=47989&otp=1234&fp=" + "00".repeat(32);

    private static void read(QrScanActivity scan, String text) {
        ReflectionHelpers.callInstanceMethod(scan, "read", ClassParameter.from(String.class, text));
    }

    @Test
    public void aPairingCodeLightsTheCornersThenCloses() {
        // No camera permission in the test: the scan starts with its status line, as on a device that refused it.
        try (ActivityController<QrScanActivity> controller = Robolectric.buildActivity(QrScanActivity.class).setup()) {
            QrScanActivity scan = controller.get();
            ViewfinderFrame frame = scan.findViewById(R.id.qr_scan_frame);
            read(scan, "https://example.com");
            assertFalse("a code that is no pairing link lights nothing", frame.isRecognised());
            assertFalse(scan.isFinishing());
            read(scan, LINK);
            assertTrue("the corners light up", frame.isRecognised());
            assertFalse("long enough to be seen", scan.isFinishing());
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(QrScanActivity.RECOGNISED_MS));
            assertTrue("then the scan closes", scan.isFinishing());
            assertEquals(Activity.RESULT_OK, shadowOf(scan).getResultCode());
            assertEquals(LINK, shadowOf(scan).getResultIntent().getStringExtra(QrScanActivity.EXTRA_LINK));
        }
    }
}
