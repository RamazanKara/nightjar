package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import android.app.Activity;
import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.view.LayoutInflater;
import android.view.View;
import android.widget.FrameLayout;

import com.limelight.R;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

/**
 * A cover's art ends at its round rim, on the screen and in a picture of the card: the screen clips the art's frame to its
 * outline, and a software canvas, which knows no outline, gets the same round rectangle as a clip.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "night-xhdpi")
public class CoverCardPictureTest {
    private static void assertRoundInAPicture(Activity activity, int layout) {
        FrameLayout root = new FrameLayout(activity);
        activity.setContentView(root);
        CoverCard card = (CoverCard) LayoutInflater.from(activity).inflate(layout, root, false);
        root.addView(card);
        View frame = card.findViewById(R.id.console_cover_frame);
        assertTrue("the screen clips the art to the cover's outline", frame.getClipToOutline());
        // A fill that is certainly opaque, where a cover without art has its placeholder.
        frame.setBackgroundColor(0xFFFF0000);
        card.measure(View.MeasureSpec.makeMeasureSpec(320, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED));
        card.layout(0, 0, card.getMeasuredWidth(), card.getMeasuredHeight());
        assertTrue("the frame has a size: " + frame.getWidth() + "x" + frame.getHeight(), frame.getWidth() > 40 && frame.getHeight() > 40);
        Bitmap picture = Bitmap.createBitmap(card.getWidth(), card.getHeight(), Bitmap.Config.ARGB_8888);
        card.draw(new Canvas(picture));
        int middle = picture.getPixel(frame.getLeft() + frame.getWidth() / 2, frame.getTop() + frame.getHeight() / 2);
        assertEquals("the cover is painted in its middle", 255, Color.alpha(middle));
        int[][] corners = {{frame.getLeft(), frame.getTop()}, {frame.getRight() - 1, frame.getTop()},
                {frame.getLeft(), frame.getBottom() - 1}, {frame.getRight() - 1, frame.getBottom() - 1}};
        for (int[] corner : corners) {
            int pixel = picture.getPixel(corner[0], corner[1]);
            assertTrue("no square corner behind the round rim at " + corner[0] + "," + corner[1] + ": " + Integer.toHexString(pixel),
                    Color.alpha(pixel) < 255 && pixel != 0xFFFF0000);
        }
    }

    @Test
    public void aPictureOfAShelfCoverHasRoundCornersLikeTheScreen() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            assertRoundInAPicture(controller.get(), R.layout.console_card_cover);
        }
    }

    @Test
    public void aPictureOfAHerosCoverHasRoundCornersToo() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            assertRoundInAPicture(controller.get(), R.layout.console_card_hero);
        }
    }

    @Test
    @Config(qualifiers = "television-night-xhdpi")
    public void aPictureOfATelevisionsCoverHasRoundCornersToo() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            assertRoundInAPicture(controller.get(), R.layout.console_card_cover);
        }
    }
}
