package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.app.Activity;
import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.RectF;
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
 * A round frame is round in a picture too: a PC's block on Home takes the shape of the card it lies in, a PC's cover on the
 * PCs page its own radius, where the screen's renderer would clip them to an outline that a picture knows nothing of.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "night-xhdpi")
public class RoundFrameTest {
    private static final int FILL = 0xFFFF0000;

    /** The frame's own fill, with nothing over it and nothing cut out of it. */
    private static boolean isFill(int pixel) {
        return Color.alpha(pixel) == 255 && Color.red(pixel) > 200 && Color.green(pixel) < 60 && Color.blue(pixel) < 60;
    }

    /** A picture of the view at this width, and at this height or, for 0, as high as it wants to be. */
    private static Bitmap pictureOf(View view, int width, int height) {
        view.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, height > 0 ? View.MeasureSpec.EXACTLY : View.MeasureSpec.UNSPECIFIED));
        view.layout(0, 0, view.getMeasuredWidth(), view.getMeasuredHeight());
        Bitmap picture = Bitmap.createBitmap(view.getWidth(), view.getHeight(), Bitmap.Config.ARGB_8888);
        view.draw(new Canvas(picture));
        return picture;
    }

    /** Where a view lies in an ancestor. */
    private static int[] placeIn(View view, View ancestor) {
        int[] at = new int[2];
        for (View v = view; v != ancestor; v = (View) v.getParent()) {
            at[0] += v.getLeft();
            at[1] += v.getTop();
        }
        return at;
    }

    @Test
    public void aFrameWithARadiusOfItsOwnIsRoundInAPicture() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            Activity activity = controller.get();
            FrameLayout root = new FrameLayout(activity);
            activity.setContentView(root);
            RoundFrame frame = new RoundFrame(activity);
            frame.setBackgroundColor(FILL);
            root.addView(frame, new FrameLayout.LayoutParams(200, 300));
            RectF shape = new RectF();
            assertTrue("no radius and no card: nothing to keep round", frame.shape(shape) < 0f);
            Bitmap square = pictureOf(frame, 200, 300);
            assertTrue("and its corner is its own", isFill(square.getPixel(0, 0)));

            frame.setRadius(28f);
            assertEquals(28f, frame.shape(shape), 0f);
            Bitmap round = pictureOf(frame, 200, 300);
            assertTrue("the frame is painted", isFill(round.getPixel(100, 150)));
            for (int[] corner : new int[][] {{0, 0}, {199, 0}, {0, 299}, {199, 299}}) {
                assertFalse("no square corner at " + corner[0] + "," + corner[1], isFill(round.getPixel(corner[0], corner[1])));
            }
            assertTrue("the round corner starts where its radius does", isFill(round.getPixel(28, 2)));
        }
    }

    @Test
    public void aPcsBlockOnHomeTakesTheShapeOfItsCard() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            Activity activity = controller.get();
            FrameLayout root = new FrameLayout(activity);
            activity.setContentView(root);
            View card = LayoutInflater.from(activity).inflate(R.layout.console_card_host, root, false);
            root.addView(card);
            assertTrue("the screen clips the block to the card's outline", card.getClipToOutline());
            View block = card.findViewById(R.id.console_thumb_frame);
            assertTrue(block instanceof RoundFrame);
            block.setBackgroundColor(FILL);
            card.findViewById(R.id.console_thumb_text).setVisibility(View.GONE);
            Bitmap picture = pictureOf(card, 640, 0);
            int[] at = placeIn(block, card);
            assertEquals("the block starts in the card's corner", 0, at[0]);
            assertEquals(0, at[1]);
            assertTrue("the block is painted", isFill(picture.getPixel(block.getWidth() / 2, block.getHeight() / 2)));
            assertFalse("the card's top corner is round", isFill(picture.getPixel(0, 0)));
            assertFalse("and its bottom corner", isFill(picture.getPixel(0, card.getHeight() - 1)));
            // The block's other side lies inside the card: square, as on the screen.
            assertTrue("the inner side stays square", isFill(picture.getPixel(block.getWidth() - 2, 10)));
        }
    }

    @Test
    public void aPcsCoverOnThePcsPageHasItsOwnRadiusInAPicture() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            Activity activity = controller.get();
            FrameLayout root = new FrameLayout(activity);
            activity.setContentView(root);
            PcRow row = (PcRow) LayoutInflater.from(activity).inflate(R.layout.pc_grid_item, root, false);
            root.addView(row);
            View cover = row.findViewById(R.id.grid_cover_frame);
            assertTrue(cover instanceof RoundFrame);
            assertTrue("the screen clips the cover to its outline", cover.getClipToOutline());
            float density = activity.getResources().getDisplayMetrics().density;
            assertEquals("a picture clips it to the same radius", 14 * density, ((RoundFrame) cover).shape(new RectF()), 0f);
        }
    }
}
