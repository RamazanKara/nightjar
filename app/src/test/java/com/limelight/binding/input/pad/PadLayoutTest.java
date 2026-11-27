package com.limelight.binding.input.pad;

import org.junit.Test;

import java.util.Map;

import java.util.HashMap;
import com.limelight.nvstream.input.ControllerPacket;

import static org.junit.Assert.*;

/**
 * Verifies {@link PadLayout} serialization, versioning, sizing, default non-overlap,
 * WASD stick placement, presets, glyphs, and pointer-touchpad migration.
 */
public class PadLayoutTest {
    static final float DENSITY = 2.75f;

    /** The preview test's surfaces and the Galaxy Z Fold7's inner screen both ways: width, height and density. */
    static final float[][] SURFACES = {{1600, 720, 2}, {720, 1600, 2}, {1280, 800, 1}, {2184, 1968, 2.625f},
            {1968, 2184, 2.625f}};

    static boolean overlap(float[] a, float[] b) {
        return a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3];
    }

    @Test public void anUneditedLayoutSavesAsTheVersionAlone() {
        PadLayout layout = new PadLayout();
        assertEquals("v1", layout.serialize());
        assertTrue(PadLayout.parse("v1").isDefault());
        assertTrue("A layout the short-lived v2 build wrote still loads", PadLayout.parse("v2").isDefault());
        assertTrue(PadLayout.parse(null).isDefault());
    }

    @Test public void savedLayoutsComeBackAsTheyWere() {
        PadLayout layout = new PadLayout();
        layout.moveTo(PadLayout.Control.FACE, 1700, 600, 2400, 1080, DENSITY);
        layout.setScale(PadLayout.Control.FACE, 1.3f);
        layout.setOpacity(PadLayout.Control.LB, 40);
        layout.setOpacity(PadLayout.Control.RB, 0);
        layout.spec(PadLayout.Control.HOME).hidden = true;
        PadLayout back = PadLayout.parse(layout.serialize());
        PadLayout.Spec face = back.spec(PadLayout.Control.FACE);
        assertEquals(layout.spec(PadLayout.Control.FACE).cx, face.cx, 1e-4);
        assertEquals(layout.spec(PadLayout.Control.FACE).cy, face.cy, 1e-4);
        assertEquals(1.3f, face.scale(), 1e-3);
        assertEquals(40, back.spec(PadLayout.Control.LB).opacity);
        assertEquals(0, back.spec(PadLayout.Control.RB).opacity);
        assertTrue(back.spec(PadLayout.Control.HOME).hidden);
        // Only edited controls are written; the rest follow the default for the screen.
        assertFalse(back.spec(PadLayout.Control.LEFT_STICK).placed());
        assertEquals(layout.serialize(), back.serialize());
    }

    /** A layout saved before free resizing has one scale per control: it loads as that size in both directions. */
    @Test public void aLayoutSavedBeforeFreeResizingLoadsWithItsShape() {
        PadLayout old = PadLayout.parse("v1|FACE,0.7083,0.5556,1.300,100,0|LT,-,-,0.700,80,0");
        PadLayout.Spec face = old.spec(PadLayout.Control.FACE);
        assertEquals(1.3f, face.widthScale, 1e-6);
        assertEquals(1.3f, face.heightScale, 1e-6);
        assertEquals(.7f, old.spec(PadLayout.Control.LT).widthScale, 1e-6);
        assertEquals(.7f, old.spec(PadLayout.Control.LT).heightScale, 1e-6);
        // Saved again, it is written exactly as before: six fields, no width and height of its own.
        assertEquals("v1|FACE,0.7083,0.5556,1.300,100,0|LT,-,-,0.700,80,0", old.serialize());
        float[] box = old.bounds(PadLayout.Control.FACE, 2400, 1080, DENSITY, new float[4]);
        assertEquals("Square, as it always was", box[2], box[3], 0);
    }

    /**
     * A width and a height of their own come back as they were. They follow the six fields an older build reads, and
     * the one scale it reads is the larger, clamped to its own 0.5 to 2, so it shows a sensible size.
     */
    @Test public void independentWidthAndHeightComeBackAndAnOlderBuildReadsSixFields() {
        PadLayout layout = new PadLayout();
        layout.setSize(PadLayout.Control.FACE, 2.5f, 1.2f);
        layout.setSize(PadLayout.Control.LB, .6f, 1.4f);
        String text = layout.serialize();
        assertEquals("v1|FACE,-,-,2.500,100,0,2.500,1.200|LB,-,-,1.400,100,0,0.600,1.400", text);
        PadLayout back = PadLayout.parse(text);
        assertEquals(2.5f, back.spec(PadLayout.Control.FACE).widthScale, 1e-6);
        assertEquals(1.2f, back.spec(PadLayout.Control.FACE).heightScale, 1e-6);
        assertEquals(.6f, back.spec(PadLayout.Control.LB).widthScale, 1e-6);
        assertEquals(1.4f, back.spec(PadLayout.Control.LB).heightScale, 1e-6);
        assertEquals(text, back.serialize());

        // How the build before this one reads each entry: the first six fields, the scale clamped to 0.5 to 2.
        for (String part : text.split("\\|")) {
            String[] fields = part.split(",", -1);
            if (fields.length < 6) continue;
            float olderScale = Math.max(.5f, Math.min(2f, Float.parseFloat(fields[3])));
            PadLayout.Spec spec = back.spec(PadLayout.Control.valueOf(fields[0]));
            assertEquals(part, Math.min(2f, spec.scale()), olderScale, 1e-6);
            assertEquals(part, String.valueOf(spec.opacity), fields[4]);
            assertEquals(part, spec.hidden ? "1" : "0", fields[5]);
        }

        // A broken width or height falls back to the one scale rather than dropping the entry.
        PadLayout broken = PadLayout.parse("v1|RB,-,-,1.500,70,0,x,0.800|RT,-,-,1.200,100,0,1.100");
        assertEquals(1.5f, broken.spec(PadLayout.Control.RB).widthScale, 1e-6);
        assertEquals(1.5f, broken.spec(PadLayout.Control.RB).heightScale, 1e-6);
        assertEquals(70, broken.spec(PadLayout.Control.RB).opacity);
        assertEquals("Seven fields: the one scale", 1.2f, broken.spec(PadLayout.Control.RT).heightScale, 1e-6);
    }

    /**
     * A control can be sized up to the whole surface in each direction, and no further: what was sized on a larger
     * surface fits again. The default unit's tablet cap sets the default sizes only.
     */
    @Test public void aControlGrowsToTheWholeSurfaceAndNoFurther() {
        float[][] surfaces = {{2400, 1080}, {1080, 2400}, {2560, 1600}};
        float[] box = new float[4];
        for (float[] surface : surfaces) {
            for (PadLayout.Control control : PadLayout.Control.values()) {
                PadLayout layout = new PadLayout();
                layout.setBounds(control, 0, 0, surface[0], surface[1], surface[0], surface[1], DENSITY);
                layout.bounds(control, surface[0], surface[1], DENSITY, box);
                String where = control + " on " + surface[0] + "x" + surface[1];
                assertEquals(where, 0, box[0], .5f);
                assertEquals(where, 0, box[1], .5f);
                assertEquals(where, surface[0], box[2], .5f);
                assertEquals(where, surface[1], box[3], .5f);
                // Saved and loaded, it still fills the surface.
                PadLayout.parse(layout.serialize()).bounds(control, surface[0], surface[1], DENSITY, box);
                assertEquals(where + " saved", surface[0], box[2], 1.5f);
                assertEquals(where + " saved", surface[1], box[3], 1.5f);
                // Sized past the surface (on a larger one), each side stops at the surface.
                layout.setSize(control, PadLayout.MAX_SCALE, PadLayout.MAX_SCALE);
                layout.bounds(control, surface[0], surface[1], DENSITY, box);
                assertEquals(where + " past it", surface[0], box[2], .5f);
                assertEquals(where + " past it", surface[1], box[3], .5f);
            }
        }

        // Growing in both directions keeps the shape and stops where the first side meets the surface.
        PadLayout layout = new PadLayout();
        layout.setSize(PadLayout.Control.FACE, 1.5f, 1f);
        layout.scaleFrom(PadLayout.Control.FACE, 1.5f, 1f, 1000, 2400, 1080, DENSITY);
        layout.bounds(PadLayout.Control.FACE, 2400, 1080, DENSITY, box);
        PadLayout.Spec face = layout.spec(PadLayout.Control.FACE);
        assertEquals("Its shape kept", 1.5f, face.widthScale / face.heightScale, 1e-4);
        assertEquals("as tall as the surface", 1080, box[3], .5f);
        assertTrue(box[2] < 2400);
        // A tablet's control grows past the default unit's cap, up to the whole surface.
        PadLayout tablet = new PadLayout();
        tablet.scaleFrom(PadLayout.Control.LEFT_STICK, 1, 1, 1000, 2560, 1600, 2);
        assertEquals(1600, tablet.bounds(PadLayout.Control.LEFT_STICK, 2560, 1600, 2, box)[3], .5f);
        // Shrinking stops at the smallest scale.
        layout.scaleFrom(PadLayout.Control.FACE, 1, 2, .01f, 2400, 1080, DENSITY);
        assertEquals(PadLayout.MIN_SCALE, face.widthScale, 1e-6);
        assertEquals(2 * PadLayout.MIN_SCALE, face.heightScale, 1e-6);
    }

    /** Every control, the split members too, holds its own slot of the on-screen gamepad report. */
    @Test public void everyControlHasAGamepadSlotOfItsOwn() {
        assertTrue(PadLayout.Control.values().length <= com.limelight.binding.input.TouchLayout.MAX_BUTTONS);
    }

    /**
     * The pointer touchpad's default place is clear of the stream's microphone and menu buttons, which sit below the
     * top row, and of every control of the default layout, on every surface above; where there is room it keeps the
     * middle of the surface.
     */
    @Test public void thePointerTouchpadsDefaultIsClearOfTheMicrophoneAndMenuButtons() {
        for (float[] surface : SURFACES) {
            float width = surface[0], height = surface[1], density = surface[2];
            String where = width + "x" + height;
            PadLayout layout = new PadLayout();
            float[] pad = layout.bounds(PadLayout.Control.POINTER_PAD, width, height, density, new float[4]);
            assertTrue(where + " on the surface", pad[0] >= 0 && pad[1] >= 0 && pad[0] + pad[2] <= width + .01f
                    && pad[1] + pad[3] <= height + .01f);
            // Where the controller puts the buttons: centred below the top row's bottom, the menu button beside it.
            float rowBottom = 0;
            for (PadLayout.Control row : new PadLayout.Control[] {PadLayout.Control.SELECT, PadLayout.Control.HOME,
                    PadLayout.Control.START, PadLayout.Control.TOUCHPAD}) {
                float[] box = layout.bounds(row, width, height, density, new float[4]);
                rowBottom = Math.max(rowBottom, box[1] + box[3]);
            }
            float size = com.limelight.ui.MicButton.SIZE_DP * density, margin = 12 * density;
            float[] mic = com.limelight.ui.MicButton.Placement.center(Float.NaN, Float.NaN, width, height, size, margin,
                    rowBottom);
            float[] menu = com.limelight.ui.MenuButton.Placement.center(Float.NaN, Float.NaN, width, height, size, margin,
                    rowBottom);
            for (float[] button : new float[][] {mic, menu}) {
                float[] box = {button[0] - size / 2, button[1] - size / 2, size, size};
                assertFalse(where + ": under a stream button", overlap(pad, box));
            }
            for (PadLayout.Control control : PadLayout.Control.values()) {
                if (control == PadLayout.Control.POINTER_PAD || control.steam || !layout.shown(control)) continue;
                float[] box = layout.bounds(control, width, height, density, new float[4]);
                assertFalse(where + ": over " + control, overlap(pad, box));
            }
            if (height / 2 - pad[3] / 2 > rowBottom + (PadLayout.TOP_BUTTONS_DP + 8) * density) {
                assertEquals(where + ": the middle where there is room", height / 2, pad[1] + pad[3] / 2, .5f);
            }
        }
    }

    @Test public void aResizedButUnmovedControlKeepsFollowingTheDefaultPlace() {
        PadLayout layout = new PadLayout();
        layout.setScale(PadLayout.Control.DPAD, .8f);
        PadLayout back = PadLayout.parse(layout.serialize());
        assertFalse(back.spec(PadLayout.Control.DPAD).placed());
        assertEquals(.8f, back.spec(PadLayout.Control.DPAD).scale(), 1e-3);
    }

    @Test public void defaultLeftTriggerClearsCompactTopLeftStatsHudInLandscape() {
        PadLayout layout = new PadLayout();
        // Standard 1080p phone (2400x1080 @ 2.625x) and 720p/1080p handhelds in landscape clear the 40dp stats capsule.
        for (float[] s : new float[][] {{2400, 1080, 2.625f}, {1920, 1080, 2f}, {1280, 720, 1.5f}}) {
            float[] lt = layout.bounds(PadLayout.Control.LT, s[0], s[1], s[2], new float[4]);
            assertTrue("LT top clears 40dp stats HUD on " + s[0] + "x" + s[1] + ": " + (lt[1] / s[2]) + "dp",
                    lt[1] >= 38f * s[2]);
        }
    }

    @Test public void brokenOrUnknownEntriesAreSkippedAndTheRestKept() {
        PadLayout layout = PadLayout.parse("v1|PADDLE,0.5,0.5,1,100,0|LB,x,0.2,1,100,0|RB,-,-,1.5,70,0|LT,0.1|"
                + "RT,7,-3,900,500,1");
        assertTrue(layout.spec(PadLayout.Control.LB).isDefault());
        assertEquals(1.5f, layout.spec(PadLayout.Control.RB).scale(), 1e-3);
        assertEquals(70, layout.spec(PadLayout.Control.RB).opacity);
        assertTrue(layout.spec(PadLayout.Control.LT).isDefault());
        // Values out of range are clamped, not refused.
        PadLayout.Spec rt = layout.spec(PadLayout.Control.RT);
        assertEquals(1, rt.cx, 0);
        assertEquals(0, rt.cy, 0);
        assertEquals(PadLayout.MAX_SCALE, rt.scale(), 0);
        assertEquals(PadLayout.MAX_OPACITY, rt.opacity);
        assertTrue(rt.hidden);
    }

    @Test public void theWasdStickIsLeftOutUntilShownAndThenKept() {
        PadLayout layout = new PadLayout();
        assertTrue("Not in the default layout", layout.spec(PadLayout.Control.WASD).hidden);
        assertTrue(layout.isDefault());
        assertEquals("v1", layout.serialize());
        layout.spec(PadLayout.Control.WASD).hidden = false;
        layout.moveTo(PadLayout.Control.WASD, 600, 700, 2400, 1080, DENSITY);
        layout.setScale(PadLayout.Control.WASD, 1.2f);
        layout.setOpacity(PadLayout.Control.WASD, 60);
        assertFalse(layout.isDefault());
        PadLayout back = PadLayout.parse(layout.serialize());
        PadLayout.Spec wasd = back.spec(PadLayout.Control.WASD);
        assertFalse(wasd.hidden);
        assertEquals(layout.spec(PadLayout.Control.WASD).cx, wasd.cx, 1e-4);
        assertEquals(layout.spec(PadLayout.Control.WASD).cy, wasd.cy, 1e-4);
        assertEquals(1.2f, wasd.scale(), 1e-3);
        assertEquals(60, wasd.opacity);
        assertEquals(layout.serialize(), back.serialize());
        // Shown but otherwise untouched is still an edit worth saving.
        PadLayout shown = new PadLayout();
        shown.spec(PadLayout.Control.WASD).hidden = false;
        assertFalse(PadLayout.parse(shown.serialize()).spec(PadLayout.Control.WASD).hidden);
        // Reset leaves it out again; a copy keeps it shown.
        assertFalse(back.copy().spec(PadLayout.Control.WASD).hidden);
        back.reset();
        assertTrue(back.spec(PadLayout.Control.WASD).hidden);
        assertTrue(back.isDefault());
    }

    /** With Show Touchpad button on, the microphone and menu buttons sit below the touchpad; the stick clears both. */
    @Test public void theWasdStickClearsTheMicrophoneAndMenuButtons() {
        // Phones as short as 392 dp, both ways up, and a tablet. A 16:10 or 16:9 phone has no room below the buttons, so
        // there the stick clears the controls instead (theDefaultClustersDoNotOverlapOnPhonesInBothOrientations).
        float[][] surfaces = {{2400, 1078, 2.75f}, {1600, 720, 2}, {2340, 1080, 2.75f}, {1080, 2400, 2.75f}, {2560, 1600, 2}};
        PadLayout layout = new PadLayout();
        layout.spec(PadLayout.Control.WASD).hidden = false;
        float[] box = new float[4], wasd = new float[4];
        for (float[] surface : surfaces) {
            float width = surface[0], height = surface[1], density = surface[2];
            // The top row's bottom as ModernController.topRowBottom finds it, touchpad included.
            float rowBottom = 0;
            for (PadLayout.Control control : new PadLayout.Control[] {PadLayout.Control.SELECT, PadLayout.Control.HOME,
                    PadLayout.Control.START, PadLayout.Control.TOUCHPAD}) {
                layout.bounds(control, width, height, density, box);
                rowBottom = Math.max(rowBottom, box[1] + box[3]);
            }
            layout.bounds(PadLayout.Control.WASD, width, height, density, wasd);
            String where = width + "x" + height;
            assertTrue(where + " stays on the surface", wasd[1] + wasd[3] <= height + .01f);
            float size = 48 * density, margin = 12 * density;
            float[][] buttons = {
                    com.limelight.ui.MicButton.Placement.center(Float.NaN, Float.NaN, width, height, size, margin, rowBottom),
                    com.limelight.ui.MenuButton.Placement.center(Float.NaN, Float.NaN, width, height, size, margin, rowBottom)};
            for (float[] center : buttons) {
                float l = center[0] - size / 2, t = center[1] - size / 2;
                boolean overlap = l < wasd[0] + wasd[2] && wasd[0] < l + size && t < wasd[1] + wasd[3] && wasd[1] < t + size;
                assertFalse("The WASD stick overlaps a button at " + center[0] + "," + center[1] + " on " + where, overlap);
            }
        }
        // A 1080p phone at 420 dpi (411 dp tall) has no room below the buttons: the stick keeps its spot below the
        // touchpad there.
        layout.bounds(PadLayout.Control.TOUCHPAD, 1920, 1080, 2.625f, box);
        layout.bounds(PadLayout.Control.WASD, 1920, 1080, 2.625f, wasd);
        assertTrue(wasd[1] >= box[1] + box[3]);
        assertTrue(wasd[1] < box[1] + box[3] + PadLayout.TOP_BUTTONS_DP * 2.625f);
    }

    @Test public void aLayoutSavedBeforeTheWasdStickLoadsWithoutIt() {
        // As 4.5.0 saved it: a moved face cluster and a hidden Home, no WASD entry.
        PadLayout layout = PadLayout.parse("v1|FACE,0.7083,0.5556,1.300,100,0|HOME,-,-,1.000,100,1");
        assertEquals(.7083f, layout.spec(PadLayout.Control.FACE).cx, 1e-4);
        assertTrue(layout.spec(PadLayout.Control.HOME).hidden);
        assertTrue(layout.spec(PadLayout.Control.WASD).hidden);
        assertTrue(layout.spec(PadLayout.Control.WASD).isDefault());
        // The stick saves as an entry of its own, which an older build skips as an unknown control.
        PadLayout withStick = new PadLayout();
        withStick.spec(PadLayout.Control.WASD).hidden = false;
        String text = withStick.serialize();
        assertEquals("v1|WASD,-,-,1.000,100,0", text);
    }

    @Test public void anotherVersionLoadsAsTheDefault() {
        assertTrue(PadLayout.parse("v3|FACE,0.5,0.5,1,100,0").isDefault());
        assertTrue(PadLayout.parse("").isDefault());
    }

    /**
     * L3: layouts stay in the v1 format so a downgraded build keeps the controls it knows, while the v2 prefix this
     * version briefly wrote still loads.
     */
    @Test public void layoutsStayV1SoADowngradeKeepsThemAndV2StillLoads() {
        PadLayout layout = new PadLayout();
        layout.setGlyph(PadLayout.Glyph.PLAYSTATION);
        layout.setPointerMode(PadLayout.PointerMode.CAMERA);
        layout.spec(PadLayout.Control.WASD).hidden = false;
        layout.setScale(PadLayout.Control.FACE, 1.3f);
        String text = layout.serialize();
        assertTrue("v1, which an older parser accepts", text.startsWith("v1|"));
        // The glyph and mode entries have no comma fields, so the older parser skips them and keeps its controls.
        assertTrue(text.contains("|glyph=playstation"));
        assertTrue(text.contains("|mode=camera"));
        PadLayout back = PadLayout.parse(text);
        assertEquals(PadLayout.Glyph.PLAYSTATION, back.glyph());
        assertEquals(PadLayout.PointerMode.CAMERA, back.pointerMode());
        assertEquals(1.3f, back.spec(PadLayout.Control.FACE).scale(), 1e-3);
        // A layout the short-lived v2 build wrote still loads, settings and all.
        PadLayout v2 = PadLayout.parse("v2|FACE,0.7083,0.5556,1.300,100,0|mode=camera");
        assertEquals(1.3f, v2.spec(PadLayout.Control.FACE).scale(), 1e-3);
        assertEquals(PadLayout.PointerMode.CAMERA, v2.pointerMode());
    }

    @Test public void everyControlLiesOnTheSurfaceAtEverySize() {
        float[][] surfaces = {{2400, 1080}, {1080, 2400}, {2560, 1600}, {800, 480}};
        float[] box = new float[4];
        for (float[] surface : surfaces) {
            PadLayout layout = new PadLayout();
            for (PadLayout.Control control : PadLayout.Control.values()) layout.setScale(control, PadLayout.MAX_SCALE);
            for (PadLayout.Control control : PadLayout.Control.values()) {
                layout.bounds(control, surface[0], surface[1], DENSITY, box);
                assertTrue(control + " left", box[0] >= 0);
                assertTrue(control + " top", box[1] >= 0);
                assertTrue(control + " right", box[0] + box[2] <= surface[0] + .01f);
                assertTrue(control + " bottom", box[1] + box[3] <= surface[1] + .01f);
            }
        }
    }

    /**
     * Two controls the default layout leaves out (or the Steam setting gates) may take the same place when both are
     * shown, so the editor moves one rather than the default layout having to fit every combination. A split
     * cluster's members show only in their cluster's place, over its buttons; splitMembersSitOverTheClustersButtons
     * checks where.
     */
    private static boolean bothOptional(PadLayout.Control a, PadLayout.Control b) {
        if (a.cluster() != null || b.cluster() != null) return true;
        return (a.hiddenByDefault || a.steam) && (b.hiddenByDefault || b.steam);
    }

    @Test public void theDefaultClustersDoNotOverlapOnPhonesInBothOrientations() {
        float[][] surfaces = {{2400, 1080}, {1080, 2400}, {1920, 1200}};
        PadLayout layout = new PadLayout();
        float[] a = new float[4], b = new float[4];
        PadLayout.Control[] controls = PadLayout.Control.values();
        for (float[] surface : surfaces) {
            for (int i = 0; i < controls.length; i++) {
                for (int j = i + 1; j < controls.length; j++) {
                    layout.bounds(controls[i], surface[0], surface[1], DENSITY, a);
                    layout.bounds(controls[j], surface[0], surface[1], DENSITY, b);
                    if (bothOptional(controls[i], controls[j])) continue;
                    boolean overlap = a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3];
                    assertFalse(controls[i] + " overlaps " + controls[j] + " on " + surface[0] + "x" + surface[1], overlap);
                }
            }
        }
    }

    /** Small phones (800x360 dp at 2x, either way up) and the smallest scale: every control still takes a 48 dp touch, apart. */
    @Test public void everyDefaultControlTakesAFullTouchOnASmallPhone() {
        float density = 2;
        float[][] surfaces = {{1600, 720}, {720, 1600}, {1280, 800}};
        PadLayout.Control[] controls = PadLayout.Control.values();
        float[] a = new float[4], b = new float[4];
        for (float[] surface : surfaces) {
            PadLayout layout = new PadLayout();
            for (PadLayout.Control control : controls) {
                layout.bounds(control, surface[0], surface[1], density, a);
                String where = control + " on " + surface[0] + "x" + surface[1];
                assertTrue(where + " is " + a[2] / density + " dp wide", a[2] >= 48 * density - .01f);
                assertTrue(where + " is " + a[3] / density + " dp tall", a[3] >= 48 * density - .01f);
                if (control.shape == PadLayout.Shape.PILL) assertTrue(where + " pill", a[2] >= 72 * density - .01f);
                for (PadLayout.Control other : controls) {
                    if (other.ordinal() <= control.ordinal() || bothOptional(control, other)) continue;
                    layout.bounds(other, surface[0], surface[1], density, b);
                    boolean overlap = a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3];
                    assertFalse(where + " overlaps " + other, overlap);
                }
            }
            layout.setScale(PadLayout.Control.HOME, PadLayout.MIN_SCALE);
            layout.bounds(PadLayout.Control.HOME, surface[0], surface[1], density, a);
            assertTrue("Shrinking stops at a full touch", a[2] >= 48 * density - .01f);
        }
    }

    @Test public void aTabletsControlsStopGrowingAtTheUnitCap() {
        float tablet = PadLayout.unit(2560, 1600, 2);
        assertEquals(PadLayout.MAX_UNIT_DP * 2, tablet, 0);
        assertEquals(1080, PadLayout.unit(2400, 1080, DENSITY), 0);
        // Portrait: narrow enough that the two sides fit next to each other.
        assertEquals(1080 / 1.75f, PadLayout.unit(1080, 2400, DENSITY), .01f);
    }

    @Test public void movingKeepsTheWholeControlOnTheSurface() {
        PadLayout layout = new PadLayout();
        layout.moveTo(PadLayout.Control.FACE, -500, 5000, 2400, 1080, DENSITY);
        float[] box = layout.bounds(PadLayout.Control.FACE, 2400, 1080, DENSITY, new float[4]);
        assertEquals(0, box[0], .5f);
        assertEquals(1080 - box[3], box[1], .5f);
        // The saved center is a fraction, so it lands in the same place after a rotation back and forth.
        PadLayout.Spec spec = layout.spec(PadLayout.Control.FACE);
        assertTrue(spec.cx > 0 && spec.cx < 1 && spec.cy > 0 && spec.cy < 1);
    }

    @Test public void resetDropsEveryEdit() {
        PadLayout layout = new PadLayout();
        layout.setScale(PadLayout.Control.LB, 2);
        layout.spec(PadLayout.Control.RB).hidden = true;
        PadLayout copy = layout.copy();
        layout.reset();
        assertTrue(layout.isDefault());
        assertFalse("A copy is its own", copy.isDefault());
    }

    @Test public void glyphsAndTheTouchpadModeAreSavedAndMigrateFromV1() {
        PadLayout layout = new PadLayout();
        layout.setGlyph(PadLayout.Glyph.PLAYSTATION);
        layout.setPointerMode(PadLayout.PointerMode.CAMERA);
        assertEquals("v1|glyph=playstation|mode=camera", layout.serialize());
        PadLayout back = PadLayout.parse(layout.serialize());
        assertEquals(PadLayout.Glyph.PLAYSTATION, back.glyph());
        assertEquals(PadLayout.PointerMode.CAMERA, back.pointerMode());
        assertFalse(back.isDefault());
        back.reset();
        assertEquals(PadLayout.Glyph.XBOX, back.glyph());
        assertEquals(PadLayout.PointerMode.MOUSE, back.pointerMode());
        assertTrue(back.isDefault());

        // A layout from the version before these fields: Xbox glyphs and a mouse touchpad.
        PadLayout old = PadLayout.parse("v1|FACE,0.7083,0.5556,1.300,100,0");
        assertEquals(PadLayout.Glyph.XBOX, old.glyph());
        assertEquals(PadLayout.PointerMode.MOUSE, old.pointerMode());
        assertEquals(1.3f, old.spec(PadLayout.Control.FACE).scale(), 1e-3);
        // An unknown glyph or mode reads as the default rather than dropping the whole layout.
        PadLayout unknown = PadLayout.parse("v2|glyph=atari|mode=laser|LB,-,-,1,100,0");
        assertEquals(PadLayout.Glyph.XBOX, unknown.glyph());
        assertEquals(PadLayout.PointerMode.MOUSE, unknown.pointerMode());
        assertEquals(PadLayout.MAX_OPACITY, unknown.spec(PadLayout.Control.LB).opacity);
    }

    @Test public void presetsShowTheControlsTheyName() {
        PadLayout minimal = new PadLayout();
        minimal.applyPreset(PadLayout.Preset.MINIMAL);
        for (PadLayout.Control control : PadLayout.Control.values()) {
            boolean wanted = control == PadLayout.Control.LEFT_STICK || control == PadLayout.Control.RIGHT_STICK ||
                    control == PadLayout.Control.DPAD || control == PadLayout.Control.FACE ||
                    control == PadLayout.Control.SELECT || control == PadLayout.Control.START;
            if (!control.steam) assertEquals(control.name(), !wanted, minimal.spec(control).hidden);
        }
        // The pointer touchpad and the WASD stick are opt-in controls; Minimal leaves both out.
        assertTrue(minimal.spec(PadLayout.Control.POINTER_PAD).hidden);
        assertTrue(minimal.spec(PadLayout.Control.WASD).hidden);

        PadLayout full = new PadLayout();
        full.applyPreset(PadLayout.Preset.FULL);
        for (PadLayout.Control control : PadLayout.Control.values()) {
            if (control.steam) continue;
            // Every gamepad control and the pointer touchpad show; the WASD stick is a keyboard control and stays
            // opt-in, and the face cluster and the D-pad stay whole.
            assertEquals(control.name(), control == PadLayout.Control.WASD || control.cluster() != null,
                    full.spec(control).hidden);
        }
        assertFalse(full.spec(PadLayout.Control.POINTER_PAD).hidden);

        PadLayout back = new PadLayout();
        back.applyPreset(PadLayout.Preset.FULL);
        back.applyPreset(PadLayout.Preset.DEFAULT);
        assertTrue(back.isDefault());
        assertEquals("v1", back.serialize());
    }

    @Test public void aPresetLeavesTheSteamControlsAsTheyAre() {
        PadLayout.Control steam = null;
        for (PadLayout.Control control : PadLayout.Control.values()) {
            if (control.steam) { steam = control; break; }
        }
        assertNotNull(steam);
        for (PadLayout.Preset preset : PadLayout.Preset.values()) {
            PadLayout layout = new PadLayout();
            layout.spec(steam).cx = 120;
            layout.spec(steam).cy = 340;
            layout.setScale(steam, 1.5f);
            layout.setOpacity(steam, 60);
            layout.spec(steam).hidden = !layout.spec(steam).defaultHidden;
            layout.setScale(PadLayout.Control.FACE, 1.4f);
            layout.applyPreset(preset);
            PadLayout.Spec kept = layout.spec(steam);
            String name = preset + " " + steam;
            assertEquals(name, 120f, kept.cx, 1e-3);
            assertEquals(name, 340f, kept.cy, 1e-3);
            assertEquals(name, 1.5f, kept.scale(), 1e-3);
            assertEquals(name, 60, kept.opacity);
            assertEquals(name, !kept.defaultHidden, kept.hidden);
            assertEquals("the other controls take the preset", 1f, layout.spec(PadLayout.Control.FACE).scale(), 1e-3);
        }
    }

    /** L2: a control preset changes which controls show, not the glyph family or what the pointer touchpad does. */
    @Test public void aPresetKeepsTheGlyphsAndTheTouchpadMode() {
        PadLayout layout = new PadLayout();
        layout.setGlyph(PadLayout.Glyph.PLAYSTATION);
        layout.setPointerMode(PadLayout.PointerMode.CAMERA);
        layout.applyPreset(PadLayout.Preset.MINIMAL);
        assertEquals(PadLayout.Glyph.PLAYSTATION, layout.glyph());
        assertEquals(PadLayout.PointerMode.CAMERA, layout.pointerMode());
        layout.applyPreset(PadLayout.Preset.FULL);
        assertEquals(PadLayout.Glyph.PLAYSTATION, layout.glyph());
        assertEquals(PadLayout.PointerMode.CAMERA, layout.pointerMode());
        layout.applyPreset(PadLayout.Preset.DEFAULT);
        assertEquals(PadLayout.Glyph.PLAYSTATION, layout.glyph());
        assertEquals(PadLayout.PointerMode.CAMERA, layout.pointerMode());
    }

    @Test public void aTouchpadAddedBeforeItsPlaceMovedStaysInTheMiddle() {
        float width = 2400, height = 1080;
        float[] box = new float[4];
        // The Add touchpad button of an older build showed it at its default place, then the middle, and never
        // placed it; the layout it saved has the touchpad shown and unplaced.
        PadLayout old = PadLayout.parse("v1|POINTER_PAD,-,-,1.000,100,0");
        old.bounds(PadLayout.Control.POINTER_PAD, width, height, DENSITY, box);
        assertEquals(width / 2, box[0] + box[2] / 2, 1);
        assertEquals(height / 2, box[1] + box[3] / 2, 1);
        // Saved again, it keeps that place.
        PadLayout again = PadLayout.parse(old.serialize());
        again.bounds(PadLayout.Control.POINTER_PAD, width, height, DENSITY, box);
        assertEquals(height / 2, box[1] + box[3] / 2, 1);

        // A touchpad added now shows at the new default place, below the microphone and menu buttons, and keeps it
        // through a save; an older build ignores the marker.
        PadLayout added = new PadLayout();
        added.spec(PadLayout.Control.POINTER_PAD).hidden = false;
        float[] fresh = added.bounds(PadLayout.Control.POINTER_PAD, width, height, DENSITY, new float[4]);
        assertTrue("the new place is below the middle", fresh[1] + fresh[3] / 2 > height / 2 + 1);
        String text = added.serialize();
        assertEquals("v1|pad=below|POINTER_PAD,-,-,1.000,100,0", text);
        PadLayout loaded = PadLayout.parse(text);
        assertFalse(loaded.spec(PadLayout.Control.POINTER_PAD).placed());
        assertArrayEquals(fresh, loaded.bounds(PadLayout.Control.POINTER_PAD, width, height, DENSITY, box), .01f);

        // The Full preset shows it too, at the new place.
        PadLayout full = new PadLayout();
        full.applyPreset(PadLayout.Preset.FULL);
        assertFalse(PadLayout.parse(full.serialize()).spec(PadLayout.Control.POINTER_PAD).placed());

        // An older layout with the touchpad left out shows it at the new place once it is added.
        PadLayout hidden = PadLayout.parse("v1|FACE,0.7083,0.5556,1.300,100,0");
        assertTrue(hidden.spec(PadLayout.Control.POINTER_PAD).hidden);
        assertFalse(hidden.spec(PadLayout.Control.POINTER_PAD).placed());
        assertEquals("v1", new PadLayout().serialize());
    }
}
