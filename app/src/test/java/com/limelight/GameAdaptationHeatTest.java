package com.limelight;

import android.app.Application;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

/** Heat changes a stream only through Thermal protection and a controller that follows it. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class,
        shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class GameAdaptationHeatTest {
    @Test
    public void thermalProtectionOffChangesNothing() {
        assertFalse(GameAdaptation.heatEasesLoad(false, true, true));
        assertFalse(GameAdaptation.heatEasesLoad(false, false, false));
        assertFalse(GameAdaptation.heatHoldsBitrate(false, true));
    }

    @Test
    public void thermalProtectionNeedsAController() {
        assertFalse(GameAdaptation.heatEasesLoad(true, false, false));
        assertTrue(GameAdaptation.heatEasesLoad(true, true, false));
        assertTrue(GameAdaptation.heatEasesLoad(true, false, true));
    }

    @Test
    public void onlyABitrateControllerHasABitrateToHold() {
        assertTrue(GameAdaptation.heatHoldsBitrate(true, true));
        assertFalse(GameAdaptation.heatHoldsBitrate(true, false));
    }

    @Test
    public void concurrentHostActionsKeepFlagTrueUntilLastCompletes() {
        Game game = new Game();
        assertFalse(game.hostActionInProgress);
        Object first = GameAdaptation.beginHostAction(game);
        Object second = GameAdaptation.beginHostAction(game);
        assertTrue(game.hostActionInProgress);
        GameAdaptation.endHostActionOnUi(game, first);
        assertTrue(game.hostActionInProgress);
        GameAdaptation.endHostActionOnUi(game, second);
        assertFalse(game.hostActionInProgress);
    }
}
