package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.content.Context;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowBuild;

/** A probe's verdict stands for this app on this OS build, not for ever. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 33, application = Application.class)
public class GlassTierStoreTest {
    private final Context context = RuntimeEnvironment.getApplication();

    @Test
    public void anOsUpdateProbesAgain() {
        ShadowBuild.setFingerprint("vendor/device:13/one");
        assertFalse(GlassTier.probed(context));
        GlassTier.remember(context, GlassTier.BAKED);
        assertTrue(GlassTier.probed(context));
        assertEquals(GlassTier.BAKED, GlassTier.land(context));

        ShadowBuild.setFingerprint("vendor/device:14/two");
        assertFalse("the driver may have changed", GlassTier.probed(context));
        assertEquals("nothing holds the new build back until its probe", GlassTier.LENS, GlassTier.land(context));
    }

    @Test
    public void aProbeThatNeverFinishedOnThisBuildLandsOnBakedOnce() {
        ShadowBuild.setFingerprint("vendor/device:13/one");
        context.getSharedPreferences("glass_tier", Context.MODE_PRIVATE).edit()
                .putBoolean(GlassTier.GUARD_IN_FLIGHT, true).putString(GlassTier.GUARD_FINGERPRINT, GlassTier.versionKey()).commit();
        assertEquals(GlassTier.BAKED, GlassTier.land(context));
        assertTrue("kept like a verdict: never probed again on this build", GlassTier.probed(context));
        assertFalse(context.getSharedPreferences("glass_tier", Context.MODE_PRIVATE).contains(GlassTier.GUARD_IN_FLIGHT));
        assertEquals(GlassTier.BAKED, GlassTier.land(context));
    }

    @Test
    public void aGuardFromAnotherBuildGivesAFreshTry() {
        ShadowBuild.setFingerprint("vendor/device:14/two");
        context.getSharedPreferences("glass_tier", Context.MODE_PRIVATE).edit()
                .putBoolean(GlassTier.GUARD_IN_FLIGHT, true).putString(GlassTier.GUARD_FINGERPRINT, "v1-vendor/device:13/one").commit();
        assertEquals(GlassTier.LENS, GlassTier.land(context));
        assertFalse(GlassTier.probed(context));
    }

    @Test
    public void armingAndClearingTheGuardKeepsItOutOfTheVerdict() {
        ShadowBuild.setFingerprint("vendor/device:13/one");
        GlassTier.armGuard(context);
        assertTrue(context.getSharedPreferences("glass_tier", Context.MODE_PRIVATE).getBoolean(GlassTier.GUARD_IN_FLIGHT, false));
        assertEquals("this process's own probe is no crash", GlassTier.LENS, GlassTier.land(context));
        GlassTier.remember(context, GlassTier.LIVE);
        assertTrue("a verdict keeps the guard until the probe clears it",
                context.getSharedPreferences("glass_tier", Context.MODE_PRIVATE).contains(GlassTier.GUARD_IN_FLIGHT));
        GlassTier.clearGuard(context);
        assertEquals(1, context.getSharedPreferences("glass_tier", Context.MODE_PRIVATE).getAll().size());
    }

    @Test
    public void onlyTheLatestVerdictIsKept() {
        ShadowBuild.setFingerprint("vendor/device:13/one");
        GlassTier.remember(context, GlassTier.BAKED);
        ShadowBuild.setFingerprint("vendor/device:14/two");
        GlassTier.remember(context, GlassTier.LIVE);
        assertEquals(1, context.getSharedPreferences("glass_tier", Context.MODE_PRIVATE).getAll().size());
    }
}
