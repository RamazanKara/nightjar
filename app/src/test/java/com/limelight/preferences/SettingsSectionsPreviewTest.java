package com.limelight.preferences;

import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.os.Looper;
import android.view.View;
import android.widget.ListView;

import org.junit.Assume;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;
import org.robolectric.shadows.ShadowLooper;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.util.concurrent.TimeUnit;

import static org.junit.Assert.assertTrue;

/**
 * Optional phone previews of every Settings section with its More rows open, each on one tall page cut to its last
 * row, for comparing a settings change before and after. Set NIGHTJAR_PREVIEW_OUTPUT to capture PNGs.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class, qualifiers = "w411dp-h4000dp-port-notnight-xhdpi",
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = {"com.limelight.nvstream.jni", "com.limelight.binding.video"})
@GraphicsMode(GraphicsMode.Mode.NATIVE)
public class SettingsSectionsPreviewTest {
    private static final int W = 822, H = 8000;
    private static final String[] SECTIONS = {"simple", "video", "display", "input", "controller", "audio", "streaming", "app", "advanced", "debug"};

    @Test
    public void everySectionOpen() throws IOException {
        String output = System.getenv("NIGHTJAR_PREVIEW_OUTPUT");
        Assume.assumeTrue("previews are an explicitly requested local capture", output != null && !output.isBlank());
        File directory = new File(output);
        assertTrue(directory.isDirectory() || directory.mkdirs());
        try (ActivityController<StreamSettings> controller = Robolectric.buildActivity(StreamSettings.class).setup().visible()) {
            StreamSettings screen = controller.get();
            int index = 1;
            for (String section : SECTIONS) {
                screen.moreOpen.add(section);
                screen.showSection(section);
                ShadowLooper.idleMainLooper(400, TimeUnit.MILLISECONDS);
                screen.getFragmentManager().executePendingTransactions();
                Shadows.shadowOf(Looper.getMainLooper()).idle();
                View root = screen.getWindow().getDecorView();
                for (int pass = 0; pass < 2; pass++) {
                    root.measure(View.MeasureSpec.makeMeasureSpec(W, View.MeasureSpec.EXACTLY),
                            View.MeasureSpec.makeMeasureSpec(H, View.MeasureSpec.EXACTLY));
                    root.layout(0, 0, W, H);
                    ShadowLooper.idleMainLooper(400, TimeUnit.MILLISECONDS);
                }
                write(root, contentBottom(root), new File(directory, "android-" + index++ + "-" + section + ".png"));
            }
        }
    }

    /** Display arrangement's list in Settings, and a PC's own page, where it holds the virtual display switch. */
    @Test
    public void displayArrangementPickerAndPcPage() throws IOException {
        String output = System.getenv("NIGHTJAR_PREVIEW_OUTPUT");
        Assume.assumeTrue("previews are an explicitly requested local capture", output != null && !output.isBlank());
        File directory = new File(output);
        assertTrue(directory.isDirectory() || directory.mkdirs());
        try (ActivityController<StreamSettings> controller = Robolectric.buildActivity(StreamSettings.class).setup().visible()) {
            android.preference.Preference row = SettingsStructureTest.open(controller.get(), "display").findPreference("host_display_location");
            row.getOnPreferenceClickListener().onPreferenceClick(row);
            ShadowLooper.idleMainLooper(400, TimeUnit.MILLISECONDS);
            View root = org.robolectric.shadows.ShadowDialog.getLatestDialog().getWindow().getDecorView();
            root.measure(View.MeasureSpec.makeMeasureSpec(W, View.MeasureSpec.EXACTLY),
                    View.MeasureSpec.makeMeasureSpec(H, View.MeasureSpec.AT_MOST));
            root.layout(0, 0, W, root.getMeasuredHeight());
            write(root, root.getMeasuredHeight(), new File(directory, "android-display-arrangement-picker.png"));
        }
        android.content.Intent intent = new android.content.Intent(org.robolectric.RuntimeEnvironment.getApplication(), HostStreamSettings.class)
                .putExtra("host_uuid", "preview-pc").putExtra("host_name", "PC");
        ActivityController<HostStreamSettings> controller = Robolectric.buildActivity(HostStreamSettings.class, intent).setup().visible();
        try {
            View root = controller.get().getWindow().getDecorView();
            for (int pass = 0; pass < 2; pass++) {
                root.measure(View.MeasureSpec.makeMeasureSpec(W, View.MeasureSpec.EXACTLY),
                        View.MeasureSpec.makeMeasureSpec(H, View.MeasureSpec.EXACTLY));
                root.layout(0, 0, W, H);
                ShadowLooper.idleMainLooper(400, TimeUnit.MILLISECONDS);
            }
            write(root, contentBottom(root), new File(directory, "android-pc-settings.png"));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /** The bottom of the settings list's last row on the page, plus a margin; the whole page when it can't tell. */
    private static int contentBottom(View root) {
        ListView list = root.findViewById(android.R.id.list);
        if (list == null || list.getChildCount() == 0) return H;
        View last = list.getChildAt(list.getChildCount() - 1);
        int[] at = new int[2];
        last.getLocationInWindow(at);
        return Math.min(H, Math.max(400, at[1] + last.getHeight() + 48));
    }

    private static void write(View root, int height, File destination) throws IOException {
        Bitmap page = Bitmap.createBitmap(W, H, Bitmap.Config.ARGB_8888);
        Bitmap cut = null;
        try (FileOutputStream out = new FileOutputStream(destination)) {
            root.draw(new Canvas(page));
            cut = Bitmap.createBitmap(page, 0, 0, W, height);
            assertTrue(cut.compress(Bitmap.CompressFormat.PNG, 100, out));
        } finally {
            page.recycle();
            if (cut != null) cut.recycle();
        }
    }
}
