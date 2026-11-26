package com.limelight.ui;

import static org.junit.Assert.assertTrue;

import android.app.Activity;
import android.app.Dialog;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.os.Looper;
import android.view.View;

import org.junit.Assume;
import org.robolectric.Shadows;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;

/**
 * Optional local captures of whole screens on the JVM, at any size. Set NIGHTJAR_PREVIEW_OUTPUT to a directory to get PNGs;
 * without it a preview test is skipped. A dialog is drawn over its activity, in the middle or at the foot as its window asks.
 */
public final class ScreenShots {
    private ScreenShots() {
    }

    /** The directory for a group of pictures, or the test is skipped when no capture was asked for. */
    public static File directory(String group) {
        String output = System.getenv("NIGHTJAR_PREVIEW_OUTPUT");
        Assume.assumeTrue("previews are an explicitly requested local capture", output != null && !output.isBlank());
        File directory = new File(output, group);
        assertTrue(directory.isDirectory() || directory.mkdirs());
        return directory;
    }

    private static void layOut(View root, int w, int h, boolean exactHeight) {
        layOut(root, w, h, true, exactHeight);
    }

    private static void layOut(View root, int w, int h, boolean exactWidth, boolean exactHeight) {
        root.measure(View.MeasureSpec.makeMeasureSpec(w, exactWidth ? View.MeasureSpec.EXACTLY : View.MeasureSpec.AT_MOST),
                View.MeasureSpec.makeMeasureSpec(h, exactHeight ? View.MeasureSpec.EXACTLY : View.MeasureSpec.AT_MOST));
        root.layout(0, 0, root.getMeasuredWidth(), root.getMeasuredHeight());
    }

    /** Lays the activity out at this size in pixels, as a window of that size would. */
    public static void fit(Activity screen, int w, int h) {
        Shadows.shadowOf(Looper.getMainLooper()).idle();
        layOut(screen.getWindow().getDecorView(), w, h, true);
        Shadows.shadowOf(Looper.getMainLooper()).idle();
        layOut(screen.getWindow().getDecorView(), w, h, true);
    }

    public static void capture(Activity screen, int w, int h, File destination) throws IOException {
        capture(screen, null, w, h, destination);
    }

    /** The activity with {@code over} on top of it; a dialog that fills the window is drawn at its full size. */
    public static void capture(Activity screen, Dialog over, int w, int h, File destination) throws IOException {
        fit(screen, w, h);
        Bitmap bitmap = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(bitmap);
        try (FileOutputStream output = new FileOutputStream(destination)) {
            screen.getWindow().getDecorView().draw(canvas);
            if (over != null && over.getWindow() != null) {
                View top = over.getWindow().getDecorView();
                android.view.WindowManager.LayoutParams at = over.getWindow().getAttributes();
                boolean fills = at.height == android.view.ViewGroup.LayoutParams.MATCH_PARENT;
                int width = at.width > 0 ? Math.min(at.width, w) : w;
                // A window that wraps its content is as wide as its theme and its content make it.
                boolean wide = at.width != android.view.ViewGroup.LayoutParams.WRAP_CONTENT;
                layOut(top, width, h, wide, fills);
                Shadows.shadowOf(Looper.getMainLooper()).idle();
                layOut(top, width, h, wide, fills);
                if (at.dimAmount > 0f && (at.flags & android.view.WindowManager.LayoutParams.FLAG_DIM_BEHIND) != 0) {
                    canvas.drawColor(((int) (at.dimAmount * 255) << 24));
                }
                int gravity = at.gravity == 0 ? android.view.Gravity.CENTER : at.gravity;
                int x = (w - top.getWidth()) / 2;
                int y = (gravity & android.view.Gravity.VERTICAL_GRAVITY_MASK) == android.view.Gravity.BOTTOM ? h - top.getHeight()
                        : (gravity & android.view.Gravity.VERTICAL_GRAVITY_MASK) == android.view.Gravity.TOP ? 0 : (h - top.getHeight()) / 2;
                canvas.save();
                canvas.translate(x, y);
                top.draw(canvas);
                canvas.restore();
            }
            assertTrue(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output));
        } finally {
            bitmap.recycle();
        }
    }
}
