package com.limelight.nvstream;

import org.junit.Test;

import static org.junit.Assert.assertEquals;

public class ClientHintBitrateControllerTest {
    @Test
    public void singleModerateLossSampleResetsStableStreak() {
        ClientHintBitrateController controller = new ClientHintBitrateController(20000, 0, 0);
        // Cut to 14000 kbps (0.7x) so upward probing has headroom below the 20000 kbps ceiling.
        assertEquals(14000, controller.sample(3000, true, 6.0f, 0f));
        controller.applied(14000, 3000);
        // 4 clean samples past COOLDOWN_MS (5000 ms), one short of STABLE_SAMPLES (5).
        assertEquals(0, controller.sample(5000, true, 0f, 0f));
        assertEquals(0, controller.sample(6000, true, 0f, 0f));
        assertEquals(0, controller.sample(7000, true, 0f, 0f));
        assertEquals(0, controller.sample(8000, true, 0f, 0f));
        // A single 3% loss sample (2 < loss <= 5) does not cut yet (lossStreak == 1),
        // but must reset stableSamples so the next clean sample does not probe up.
        assertEquals(0, controller.sample(9000, true, 3.0f, 0f));
        assertEquals(0, controller.sample(10000, true, 0f, 0f));
        // 4 more clean samples (5 total since the loss sample) are needed to probe up (+2%).
        assertEquals(0, controller.sample(11000, true, 0f, 0f));
        assertEquals(0, controller.sample(12000, true, 0f, 0f));
        assertEquals(0, controller.sample(13000, true, 0f, 0f));
        assertEquals(14280, controller.sample(14000, true, 0f, 0f));
    }

    @Test
    public void hostLinkCutBoundsPreTakeoverThermalAndLossCuts() {
        ClientHintBitrateController controller = new ClientHintBitrateController(50000, 0, 0);
        // Host Congestion / LinkRate cuts stream to 40% (20000 kbps equivalent) before any client write.
        controller.syncHostLink(15000, 37500);
        assertEquals(20000, controller.currentKbps());
        // Clean samples never probe upward while the host still owns the rate (!client_owned).
        for (int s = 3; s <= 12; s++) {
            assertEquals(0, controller.sample(s * 1000L, true, 0f, 0f));
        }
        // If host cut below min_kbps (8000 < 15000), neither clean samples nor loss jump up to 15000.
        controller.syncHostLink(6000, 37500);
        assertEquals(8000, controller.currentKbps());
        assertEquals(0, controller.sample(13000, true, 10f, 0f));
        // Host recovers to 20000; subsequent thermal hot samples step down 20000 -> 18000, claiming ownership.
        controller.syncHostLink(15000, 37500);
        controller.setThermalLevel(2);
        assertEquals(0, controller.sample(14000, true, 0f, 0f));
        assertEquals(0, controller.sample(15000, true, 0f, 0f));
        assertEquals(18000, controller.sample(16000, true, 0f, 0f));
        controller.applied(18000, 16000);
        controller.setThermalLevel(0);
        for (int s = 18; s < 22; s++) {
            assertEquals(0, controller.sample(s * 1000L, true, 0f, 0f));
        }
        assertEquals(18360, controller.sample(22000, true, 0f, 0f));
    }
}
