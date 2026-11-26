package com.limelight.binding.input.pad;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotEquals;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

/**
 * Where the layout editor lands a moved or sized control: the grid, the screen's centre lines, a neighbour's edges and
 * centre, a twin's mirror, and never off the screen. A 1600 x 720 screen at 2 px per dp: an 8 dp grid, 6 dp of reach.
 */
public class PadSnapGridTest {
    private static final float WIDTH = 1600, HEIGHT = 720, GRID = 16, REACH = 12;
    private final PadSnapGrid snap = new PadSnapGrid(GRID, REACH);

    @Test public void aMovedControlLandsOnTheGrid() {
        snap.begin(WIDTH, HEIGHT);
        snap.move(405, 301, 100, 100);
        assertEquals(400, snap.x, 0);
        assertEquals(304, snap.y, 0);
        assertEquals(PadSnapGrid.GRID, snap.kindX);
        assertEquals(PadSnapGrid.GRID, snap.kindY);
        assertEquals("The grid shows no guide", 0, snap.guides());
    }

    @Test public void itLandsOnTheScreensCentreLines() {
        snap.begin(WIDTH, HEIGHT);
        snap.move(793, 368, 100, 60);
        assertEquals(800, snap.x, 0);
        assertEquals(360, snap.y, 0);
        assertEquals(PadSnapGrid.CENTRE, snap.kindX);
        assertEquals(PadSnapGrid.CENTRE, snap.kindY);
        assertEquals(800, snap.guideX, 0);
        assertEquals(360, snap.guideY, 0);
        assertEquals(2, snap.guides());

        // A pixel past the snap distance the grid has it again, and the guide goes.
        snap.move(800 - REACH - 1, 360, 100, 60);
        assertEquals(784, snap.x, 0);
        assertEquals(PadSnapGrid.GRID, snap.kindX);
        assertTrue(Float.isNaN(snap.guideX));
        assertEquals(1, snap.guides());
        snap.release();
        assertEquals("Let go, the guides go", 0, snap.guides());
    }

    @Test public void itLandsOnANeighboursEdgesAndCentre() {
        snap.begin(WIDTH, HEIGHT);
        snap.neighbour(200, 100, 300, 80);
        // Left edge on left edge.
        snap.move(205 + 50, 400, 100, 100);
        assertEquals(250, snap.x, 0);
        assertEquals(PadSnapGrid.NEIGHBOUR, snap.kindX);
        assertEquals(200, snap.guideX, 0);
        assertEquals("The other axis keeps to the grid", PadSnapGrid.GRID, snap.kindY);
        // An edge meets the other side's: its left on the neighbour's right.
        snap.move(503 + 50, 400, 100, 100);
        assertEquals(550, snap.x, 0);
        assertEquals(500, snap.guideX, 0);
        // Centre on centre.
        snap.move(353, 400, 100, 100);
        assertEquals(350, snap.x, 0);
        assertEquals(350, snap.guideX, 0);
        // And level: its middle on the neighbour's, which is nearer than its top to the neighbour's top.
        snap.move(900, 143, 100, 100);
        assertEquals(140, snap.y, 0);
        assertEquals(PadSnapGrid.NEIGHBOUR, snap.kindY);
        assertEquals(140, snap.guideY, 0);
        // A centre never lines up with an edge: 8 px from the neighbour's left edge is nothing to its centre.
        snap.move(208, 400, 60, 100);
        assertNotEquals(200, snap.x, 0);
    }

    @Test public void theNearestLineWinsAndAStrongerOneWinsATie() {
        snap.begin(WIDTH, HEIGHT);
        snap.neighbour(200, 100, 300, 80);
        snap.neighbour(196, 300, 40, 40);
        // 1 px from one left edge, 3 px from the other's.
        snap.move(199 + 50, 600, 100, 100);
        assertEquals(250, snap.x, 0);
        assertEquals(200, snap.guideX, 0);
        snap.move(197 + 50, 600, 100, 100);
        assertEquals(246, snap.x, 0);
        assertEquals(196, snap.guideX, 0);

        // A neighbour centred on the screen: the screen's centre line is the one the control holds.
        snap.begin(WIDTH, HEIGHT);
        snap.neighbour(750, 100, 100, 100);
        snap.move(804, 500, 60, 60);
        assertEquals(800, snap.x, 0);
        assertEquals(PadSnapGrid.CENTRE, snap.kindX);
    }

    @Test public void itLandsOnItsTwinsMirror() {
        snap.begin(WIDTH, HEIGHT);
        // The twin on the left, also a neighbour as every other control is: its mirror is 1300 to 1500, centred on 1400.
        snap.neighbour(100, 500, 200, 150);
        snap.twin(100, 500, 200, 150);
        snap.move(1407, 569, 200, 150);
        assertEquals(1400, snap.x, 0);
        assertEquals(575, snap.y, 0);
        assertEquals(PadSnapGrid.MIRROR, snap.kindX);
        assertEquals("Level with its twin, which the mirror says before the neighbour does", PadSnapGrid.MIRROR, snap.kindY);
        assertEquals("The guide is the mirror's axis", 800, snap.guideX, 0);
        assertEquals(575, snap.guideY, 0);
        // Out of reach of the mirror, it is a control like any other.
        snap.move(1380, 569, 200, 150);
        assertEquals(1376, snap.x, 0);
        assertEquals(PadSnapGrid.GRID, snap.kindX);

        // Sized, its edge lands where the twin's mirrored edge is: as wide as its twin.
        snap.size(1493, 650, 1300, 500, true, true, 96, 96);
        assertEquals(1500, snap.x, 0);
        assertEquals(PadSnapGrid.MIRROR, snap.kindX);
        assertEquals("Its bottom on the twin's", 650, snap.y, 0);
    }

    @Test public void nothingSnapsOutsideTheScreen() {
        snap.begin(WIDTH, HEIGHT);
        snap.neighbour(30, 300, 60, 60);
        // Its right edge is 5 px from the neighbour's, but that would push it 10 px off the screen: the edge has it.
        snap.move(45, 400, 100, 100);
        assertEquals(50, snap.x, 0);
        assertEquals(PadSnapGrid.EDGE, snap.kindX);
        assertEquals(0, snap.guideX, 0);

        // No line in reach keeps it on the screen: the grid's place is off it too, so it stops at the edge.
        snap.begin(WIDTH, HEIGHT);
        snap.neighbour(0, 300, 60, 60);
        snap.move(33, 400, 100, 100);
        assertEquals(50, snap.x, 0);

        // Wherever the finger goes, far off the screen too, the whole control stays on it.
        snap.neighbour(1500, 20, 90, 90);
        snap.twin(40, 600, 130, 110);
        for (float fx = -600; fx <= WIDTH + 600; fx += 37) {
            for (float fy = -600; fy <= HEIGHT + 600; fy += 41) {
                snap.move(fx, fy, 130, 110);
                assertTrue(fx + "," + fy + " lands at " + snap.x + "," + snap.y, snap.x - 65 >= 0 && snap.x + 65 <= WIDTH
                        && snap.y - 55 >= 0 && snap.y + 55 <= HEIGHT);
                snap.size(fx, fy, 800, 360, true, true, 96, 96);
                assertTrue(snap.x >= 800 + 96 && snap.x <= WIDTH && snap.y >= 360 + 96 && snap.y <= HEIGHT);
                snap.size(fx, fy, 800, 360, false, false, 96, 96);
                assertTrue(snap.x >= 0 && snap.x <= 800 - 96 && snap.y >= 0 && snap.y <= 360 - 96);
            }
        }
    }

    @Test public void aControlCanStandAnywhereOnTheWholeScreen() {
        snap.begin(WIDTH, HEIGHT);
        // Flush in each corner.
        snap.move(0, 0, 100, 60);
        assertEquals(50, snap.x, 0);
        assertEquals(30, snap.y, 0);
        snap.move(WIDTH, HEIGHT, 100, 60);
        assertEquals(WIDTH - 50, snap.x, 0);
        assertEquals(HEIGHT - 30, snap.y, 0);
        // And on every grid line between, no margin kept free.
        for (float centre = 64; centre <= WIDTH - 64; centre += GRID) {
            snap.move(centre + 3, 304, 100, 60);
            if (snap.kindX == PadSnapGrid.GRID) assertEquals(centre, snap.x, 0);
            assertTrue(Math.abs(snap.x - centre - 3) <= REACH);
        }
        // A control as large as the screen fills it.
        snap.move(300, 200, WIDTH + 40, HEIGHT);
        assertEquals(WIDTH / 2, snap.x, 0);
        assertEquals(HEIGHT / 2, snap.y, 0);
    }

    @Test public void aSizedCornerLandsOnTheGridLinesAndEdges() {
        snap.begin(WIDTH, HEIGHT);
        snap.size(403, 297, 200, 100, true, true, 96, 96);
        assertEquals(400, snap.x, 0);
        assertEquals(304, snap.y, 0);
        assertEquals(PadSnapGrid.GRID, snap.kindX);
        // Never smaller than a full touch, never past the screen, whichever corner.
        snap.size(210, 110, 200, 100, true, true, 96, 96);
        assertEquals(296, snap.x, 0);
        assertEquals(196, snap.y, 0);
        snap.size(5000, 5000, 200, 100, true, true, 96, 96);
        assertEquals(WIDTH, snap.x, 0);
        assertEquals(HEIGHT, snap.y, 0);
        snap.size(-50, -50, 500, 400, false, false, 96, 96);
        assertEquals(0, snap.x, 0);
        assertEquals(0, snap.y, 0);
        // Flush with the screen's edge when that is in reach.
        snap.size(WIDTH - 9, 300, 200, 100, true, true, 96, 96);
        assertEquals(WIDTH, snap.x, 0);
        assertEquals(PadSnapGrid.EDGE, snap.kindX);

        // An edge lands on a neighbour's edge, not on its centre, and on the screen's centre lines.
        snap.neighbour(700, 50, 90, 100);
        snap.size(695, 300, 200, 100, true, true, 96, 96);
        assertEquals(700, snap.x, 0);
        assertEquals(PadSnapGrid.NEIGHBOUR, snap.kindX);
        assertEquals(700, snap.guideX, 0);
        snap.size(748, 300, 200, 100, true, true, 96, 96);
        assertEquals("745 is the neighbour's centre: the grid has the edge", 752, snap.x, 0);
        assertEquals(PadSnapGrid.GRID, snap.kindX);
        snap.size(806, 366, 200, 100, true, true, 96, 96);
        assertEquals(800, snap.x, 0);
        assertEquals(360, snap.y, 0);
        assertEquals(PadSnapGrid.CENTRE, snap.kindX);
        assertEquals(PadSnapGrid.CENTRE, snap.kindY);
    }

    @Test public void anAxisTheFingerHasNotMovedAlongStaysWhereItWas() {
        snap.begin(WIDTH, HEIGHT);
        // Dragged straight down from 797, 100: the centre line 3 px away does not take it.
        snap.move(797, 371, 100, 60);
        snap.keepX(797);
        assertEquals(797, snap.x, 0);
        assertEquals(PadSnapGrid.FREE, snap.kindX);
        assertEquals(360, snap.y, 0);
        assertEquals("Only the level guide", 1, snap.guides());
        assertEquals(360, snap.guideY, 0);
        snap.move(793, 303, 100, 60);
        snap.keepY(303);
        assertEquals(800, snap.x, 0);
        assertEquals(303, snap.y, 0);
        assertTrue(Float.isNaN(snap.guideY));
    }

    @Test public void twinsAreEachOthersAndOnlyLeftAndRightPairsHaveOne() {
        int pairs = 0;
        for (PadLayout.Control control : PadLayout.Control.values()) {
            PadLayout.Control twin = PadSnapGrid.twin(control);
            if (twin == null) continue;
            pairs++;
            assertNotEquals(control, twin);
            assertSame(control.name(), control, PadSnapGrid.twin(twin));
            assertEquals("A Steam Controller control mirrors another", control.steam, twin.steam);
        }
        assertEquals(20, pairs);
        assertSame(PadLayout.Control.RB, PadSnapGrid.twin(PadLayout.Control.LB));
        assertSame(PadLayout.Control.START, PadSnapGrid.twin(PadLayout.Control.SELECT));
        assertSame(PadLayout.Control.FACE, PadSnapGrid.twin(PadLayout.Control.DPAD));
        for (PadLayout.Control alone : new PadLayout.Control[] {PadLayout.Control.HOME, PadLayout.Control.TOUCHPAD,
                PadLayout.Control.WASD, PadLayout.Control.POINTER_PAD, PadLayout.Control.A, PadLayout.Control.DPAD_LEFT}) {
            assertNull(alone.name(), PadSnapGrid.twin(alone));
        }
        assertNull(PadSnapGrid.twin(null));
    }
}
