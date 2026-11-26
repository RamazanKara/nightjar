package com.limelight.binding.input.pad;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

/**
 * A layout saved by Nightjar 6.x loads in 7.0 to the same places and sizes. The fixtures are layouts as 6.4 wrote
 * them (its codec is the one 7.0 ships), and the boxes they must give on a 1600 x 720 screen at 2 px per dp are
 * worked out by hand from 6.x's sizes, so a change to the saved format, the default sizes or the clamping shows here.
 * The Studio added no field: its snapping, mirror lock and Undo change only where a control stands.
 */
public class PadLayoutSaved6xTest {
    private static final float WIDTH = 1600, HEIGHT = 720, DENSITY = 2;
    /** Every kind of entry 6.4 wrote: settings, a moved and resized control, a hidden one, one with two scales. */
    private static final String SAVED_6_4 = "v1|glyph=playstation|mode=camera|faces=swapped|pad=below"
            + "|LEFT_STICK,0.2000,0.6500,1.200,80,0"
            + "|DPAD,-,-,0.900,100,1"
            + "|FACE,0.8400,0.5500,1.500,100,0,1.500,0.800"
            + "|LB,0.1000,0.2500,2.000,35,0"
            + "|START,0.5600,0.0900,0.500,0,0"
            + "|POINTER_PAD,-,-,1.000,100,0"
            + "|LEFT_PAD,0.3000,0.4000,1.250,60,0,1.250,1.000";
    /** As 6.0 to 6.2 wrote it, before a control had a width and a height of its own. */
    private static final String SAVED_6_0 = "v1|FACE,0.7083,0.5556,1.300,100,0|LT,-,-,0.700,80,0|POINTER_PAD,-,-,1.000,100,0";

    @Test public void aLayoutSavedBy64LoadsToTheSamePlacesAndSizes() {
        PadLayout layout = PadLayout.parse(SAVED_6_4);
        assertEquals(PadLayout.Glyph.PLAYSTATION, layout.glyph());
        assertEquals(PadLayout.PointerMode.CAMERA, layout.pointerMode());
        assertTrue(layout.facesSwapped());

        // The left stick: 0.42 units of 720 px at 120 %, centred on 20 % and 65 % of the screen.
        assertBox(layout, PadLayout.Control.LEFT_STICK, 138.56f, 286.56f, 362.88f, 362.88f);
        assertEquals(80, layout.spec(PadLayout.Control.LEFT_STICK).opacity);
        // The face cluster 150 % wide and 80 % high.
        assertBox(layout, PadLayout.Control.FACE, 1128, 280.8f, 432, 230.4f);
        // A shoulder at 200 % reaches past the left edge where it was put, and is shown flush with it as 6.x did.
        assertBox(layout, PadLayout.Control.LB, 0, 93.6f, 360, 172.8f);
        assertEquals(35, layout.spec(PadLayout.Control.LB).opacity);
        // Menu at 50 % is still a full 48 dp touch, and invisible at opacity 0.
        assertBox(layout, PadLayout.Control.START, 848, 16.8f, 96, 96);
        assertEquals(0, layout.spec(PadLayout.Control.START).opacity);
        // The hidden D-pad keeps its size at its default place.
        assertBox(layout, PadLayout.Control.DPAD, 378, 500.4f, 194.4f, 194.4f);
        assertTrue(layout.spec(PadLayout.Control.DPAD).hidden);
        // A Steam Controller trackpad with a width of its own.
        assertBox(layout, PadLayout.Control.LEFT_PAD, 381, 208.8f, 198, 158.4f);
        assertEquals(60, layout.spec(PadLayout.Control.LEFT_PAD).opacity);
        // The pointer touchpad was added and never moved: it follows its default place.
        assertFalse(layout.spec(PadLayout.Control.POINTER_PAD).hidden);
        assertFalse(layout.spec(PadLayout.Control.POINTER_PAD).placed());
        // What it never touched is as a layout nobody edited has it.
        PadLayout fresh = new PadLayout();
        for (PadLayout.Control control : new PadLayout.Control[] {PadLayout.Control.RIGHT_STICK, PadLayout.Control.RB,
                PadLayout.Control.LT, PadLayout.Control.RT, PadLayout.Control.L3, PadLayout.Control.R3,
                PadLayout.Control.SELECT, PadLayout.Control.HOME, PadLayout.Control.TOUCHPAD}) {
            float[] expected = fresh.bounds(control, WIDTH, HEIGHT, DENSITY, new float[4]);
            assertBox(layout, control, expected[0], expected[1], expected[2], expected[3]);
            assertTrue(control.name(), layout.spec(control).isDefault());
        }
        assertEquals("Saved again, it is what 6.4 wrote, so 6.x still reads what 7.0 saves", SAVED_6_4, layout.serialize());
    }

    @Test public void aLayoutSavedBeforeFreeResizingKeepsItsOneScale() {
        PadLayout layout = PadLayout.parse(SAVED_6_0);
        // 0.40 units at 130 %, one scale for both sides.
        assertBox(layout, PadLayout.Control.FACE, 1133.28f - 187.2f, 400.032f - 187.2f, 374.4f, 374.4f);
        PadLayout.Spec trigger = layout.spec(PadLayout.Control.LT);
        assertEquals(.7f, trigger.widthScale, 1e-6);
        assertEquals(.7f, trigger.heightScale, 1e-6);
        assertEquals(80, trigger.opacity);
        assertFalse(trigger.placed());
        // Added before the touchpad's default place moved below the buttons: it stays in the middle, where it was.
        PadLayout.Spec pad = layout.spec(PadLayout.Control.POINTER_PAD);
        assertEquals(.5f, pad.cx, 1e-6);
        assertEquals(.5f, pad.cy, 1e-6);
        // The prefix a short-lived build wrote loads the same.
        assertEquals(layout.serialize(), PadLayout.parse("v2" + SAVED_6_0.substring(2)).serialize());
    }

    private static void assertBox(PadLayout layout, PadLayout.Control control, float left, float top, float width,
                                  float height) {
        float[] box = layout.bounds(control, WIDTH, HEIGHT, DENSITY, new float[4]);
        assertEquals(control + " left", left, box[0], .05f);
        assertEquals(control + " top", top, box[1], .05f);
        assertEquals(control + " width", width, box[2], .05f);
        assertEquals(control + " height", height, box[3], .05f);
    }
}
