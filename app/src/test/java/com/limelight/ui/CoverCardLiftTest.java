package com.limelight.ui;

import static org.junit.Assert.assertEquals;

import org.junit.Test;

/**
 * Where a cover's lift of focus grows from: about its middle, or away from the margin it rests on, so that the focus ring's
 * outer side comes to rest on the cover's own edge instead of reaching into the margin.
 */
public class CoverCardLiftTest {
    /** Where a point of the card is drawn once the card is scaled about a pivot. */
    private static float drawn(float point, float pivot, float scale) {
        return pivot + (point - pivot) * scale;
    }

    @Test
    public void aCardInARowLiftsAboutItsMiddle() {
        assertEquals(140f, CoverCard.liftPivot(280, 12, 268, 8, 1.1f, CoverCard.LIFT_MIDDLE, false), 0f);
        assertEquals("and so does any card that does not scale", 140f, CoverCard.liftPivot(280, 12, 268, 8, 1f, CoverCard.LIFT_FROM_START, false), 0f);
    }

    @Test
    public void theFirstCardsRingStaysOnItsStartEdge() {
        for (float scale : new float[] {1.04f, 1.08f, 1.12f}) {
            float pivot = CoverCard.liftPivot(280, 12, 268, 8, scale, CoverCard.LIFT_FROM_START, false);
            assertEquals("the ring's outer side, 8 outside the frame, lands on the frame's resting edge at " + scale,
                    12f, drawn(12 - 8, pivot, scale), .01f);
            float mirrored = CoverCard.liftPivot(280, 12, 268, 8, scale, CoverCard.LIFT_FROM_START, true);
            assertEquals("right to left the start is the right edge", 268f, drawn(268 + 8, mirrored, scale), .01f);
        }
    }

    @Test
    public void theLastCardsRingStaysOnItsEndEdge() {
        float pivot = CoverCard.liftPivot(280, 12, 268, 8, 1.1f, CoverCard.LIFT_FROM_END, false);
        assertEquals(268f, drawn(268 + 8, pivot, 1.1f), .01f);
        float mirrored = CoverCard.liftPivot(280, 12, 268, 8, 1.1f, CoverCard.LIFT_FROM_END, true);
        assertEquals(12f, drawn(12 - 8, mirrored, 1.1f), .01f);
    }
}
