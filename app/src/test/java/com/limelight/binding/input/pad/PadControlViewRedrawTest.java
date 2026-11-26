package com.limelight.binding.input.pad;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.view.View;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;
import org.robolectric.shadows.ShadowLooper;

import java.util.Arrays;
import java.util.HashSet;
import java.util.Set;
import java.util.concurrent.TimeUnit;

/**
 * What a touch makes the stream's frame draw again: a pressed control redraws its own views and nothing else, a stick
 * its cap and its base only while the arc shows, and controls at opacity 0 nothing at all.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 30, application = Application.class, qualifiers = "w800dp-h360dp-land-xhdpi")
public class PadControlViewRedrawTest extends ModernControllerHarness {
    @Test
    public void aStickMoveRedrawsTheBaseOnlyWhileItsArcIsOrWasDrawn() {
        // Still at rest and no button changed: the base has nothing new to show.
        assertFalse(PadControlView.baseNeedsRedraw(false, false, false));
        // The arc starts or stops with the move, so the base must redraw either way.
        assertTrue(PadControlView.baseNeedsRedraw(false, false, true));
        assertTrue(PadControlView.baseNeedsRedraw(false, true, false));
        // A button shown on the base changed.
        assertTrue(PadControlView.baseNeedsRedraw(true, false, false));
    }

    /** A press, its release and its easing back redraw the pressed control and its glow, never a neighbour. */
    @Test
    public void aPressRedrawsOnlyThePressedControlsOwnViews() {
        ModernController controller = controller(GAME);
        PadControlView start = controller.view(PadLayout.Control.START), face = controller.view(PadLayout.Control.FACE);
        drawFrame();
        assertEquals("Nothing to draw at rest", views(), dirty());

        start.down(0, 5, 5, 100);
        assertEquals(views(start, start.haloLayer()), dirty());
        // The glow's view is the control's box grown by the glow's reach, so that is all a press can damage.
        View halo = start.haloLayer();
        int reach = start.haloReach();
        assertTrue("The glow reaches past the box", reach > 0);
        assertEquals(start.getLeft() - reach, halo.getLeft());
        assertEquals(start.getTop() - reach, halo.getTop());
        assertEquals(start.getRight() + reach, halo.getRight());
        assertEquals(start.getBottom() + reach, halo.getBottom());

        drawFrame();
        start.up(0, 200);
        assertEquals(views(start, start.haloLayer()), dirty());
        ShadowLooper.idleMainLooper(PadPainter.RELEASE_MS + 40, TimeUnit.MILLISECONDS);
        assertEquals("Easing back redraws no more than the press", views(start, start.haloLayer()), dirty());

        drawFrame();
        face.down(1, face.getWidth() / 2f, face.getHeight() * .78f, 300);
        assertEquals(views(face, face.haloLayer()), dirty());
        controller.destroy();
    }

    /** A thumb on a stick redraws its cap; its base only once the arc shows; a neighbour never. */
    @Test
    public void aStickRedrawsItsCapAndItsBaseOnlyForTheArc() {
        prefs.oscFloatingSticks = false;
        ModernController controller = controller(GAME);
        PadControlView stick = controller.view(PadLayout.Control.LEFT_STICK);
        float w = stick.getWidth(), h = stick.getHeight();
        drawFrame();
        stick.down(0, w / 2, h / 2, 100);
        assertEquals("Held at its centre, only the cap changes", views(stick.knobLayer()), dirty());
        drawFrame();
        stick.move(0, w / 2 + PadPainter.travel(stick.size()) * .8f, h / 2);
        assertEquals("Pushed, the arc on the base as well", views(stick, stick.knobLayer()), dirty());
        // The cap's view is the box grown by the cap's overhang.
        int overhang = stick.knobOverhang(w, h);
        assertEquals(stick.getLeft() - overhang, stick.knobLayer().getLeft());
        assertEquals(stick.getBottom() + overhang, stick.knobLayer().getBottom());
        controller.destroy();
    }

    /** At opacity 0 the controls take every touch and draw nothing, so a touch redraws nothing. */
    @Test
    public void atOpacityZeroATouchRedrawsNothing() {
        prefs.oscOpacity = 0;
        ModernController controller = controller(GAME);
        PadControlView start = controller.view(PadLayout.Control.START), stick = controller.view(PadLayout.Control.LEFT_STICK);
        Bitmap frameAtRest = drawFrame();
        for (int x = 0; x < frameAtRest.getWidth(); x += 8) {
            for (int y = 0; y < frameAtRest.getHeight(); y += 8) assertEquals("Nothing at " + x + "," + y, 0, frameAtRest.getPixel(x, y));
        }
        int reports = this.reports.size();
        start.down(0, 5, 5, 100);
        stick.down(1, stick.getWidth() / 2f, stick.getHeight() / 2f, 100);
        stick.move(1, stick.getWidth() * .9f, stick.getHeight() / 2f);
        assertTrue("The controls still work", this.reports.size() > reports);
        assertEquals(views(), dirty());
        start.up(0, 200);
        stick.up(1, 200);
        ShadowLooper.idleMainLooper(PadPainter.RELEASE_MS + 40, TimeUnit.MILLISECONDS);
        assertEquals(views(), dirty());
        controller.destroy();
    }

    /** No view of a control needs an offscreen layer to fade: the skin's fills and strokes take the alpha themselves. */
    @Test
    public void fadingAControlNeedsNoLayer() {
        ModernController controller = controller(GAME);
        PadControlView stick = controller.view(PadLayout.Control.LEFT_STICK);
        assertFalse(stick.hasOverlappingRendering());
        assertFalse(stick.knobLayer().hasOverlappingRendering());
        assertFalse(stick.haloLayer().hasOverlappingRendering());
        assertEquals(View.LAYER_TYPE_NONE, stick.getLayerType());
        controller.destroy();
    }

    /** Draws the frame as a frame of the stream would, which leaves every shown view clean, and returns its pixels. */
    private Bitmap drawFrame() {
        ShadowLooper.idleMainLooper();
        layout();
        Bitmap pixels = Bitmap.createBitmap(frame.getWidth(), frame.getHeight(), Bitmap.Config.ARGB_8888);
        frame.draw(new Canvas(pixels));
        return pixels;
    }

    /** The shown views of the frame that asked to be drawn again since the last frame. */
    private Set<View> dirty() {
        Set<View> dirty = new HashSet<>();
        for (int i = 0; i < frame.getChildCount(); i++) {
            View child = frame.getChildAt(i);
            if (child.getVisibility() == View.VISIBLE && child.isDirty()) dirty.add(child);
        }
        return dirty;
    }

    private static Set<View> views(View... views) {
        return new HashSet<>(Arrays.asList(views));
    }
}
