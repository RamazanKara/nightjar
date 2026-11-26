/**
 * Unit tests for per-controller rumble log throttling in {@link Game.RumbleLogFilter}.
 */
package com.limelight;

import org.junit.Test;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class GameRumbleLogFilterTest {
    @Test
    public void suppressesUnchangedRumbleAndLogsTransitionsPerController() {
        Game.RumbleLogFilter filter = new Game.RumbleLogFilter();

        assertTrue(filter.shouldLogMotor((short) 0, (short) 0x1234, (short) 0xABCD));
        assertFalse(filter.shouldLogMotor((short) 0, (short) 0x1234, (short) 0xABCD));
        // Second controller starts fresh even with identical payload.
        assertTrue(filter.shouldLogMotor((short) 1, (short) 0x1234, (short) 0xABCD));
        assertFalse(filter.shouldLogMotor((short) 1, (short) 0x1234, (short) 0xABCD));

        // Transition back to 0x0000 and back to 0x1234/0xABCD must both log.
        assertTrue(filter.shouldLogMotor((short) 0, (short) 0, (short) 0));
        assertFalse(filter.shouldLogMotor((short) 0, (short) 0, (short) 0));
        assertTrue(filter.shouldLogMotor((short) 0, (short) 0x1234, (short) 0xABCD));

        // Sign-extended high short values (0xFFFF) must stay distinct from sentinel -1L.
        assertTrue(filter.shouldLogMotor((short) 2, (short) 0xFFFF, (short) 0xFFFF));
        assertFalse(filter.shouldLogMotor((short) 2, (short) 0xFFFF, (short) 0xFFFF));

        // Triggers track independently from motors, including controller indices >= 16.
        assertTrue(filter.shouldLogTriggers((short) 0, (short) 0x1234, (short) 0xABCD));
        assertFalse(filter.shouldLogTriggers((short) 0, (short) 0x1234, (short) 0xABCD));
        assertTrue(filter.shouldLogTriggers((short) 20, (short) 0x0010, (short) 0x0020));
        assertFalse(filter.shouldLogTriggers((short) 20, (short) 0x0010, (short) 0x0020));
        assertTrue(filter.shouldLogTriggers((short) 20, (short) 0x0000, (short) 0x0020));
    }
}
