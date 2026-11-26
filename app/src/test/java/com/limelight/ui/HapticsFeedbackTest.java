package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotEquals;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import android.content.Context;
import android.content.ContextWrapper;
import android.view.ContextThemeWrapper;
import android.view.HapticFeedbackConstants;

import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

import org.junit.After;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

/** The interface's own events follow ui_feedback_contract: one haptic each, rate-limited, silent over a stream. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34)
public class HapticsFeedbackTest {
    @After
    public void reset() {
        Haptics.resetForTests();
    }

    /** A screen that shows a stream, as Game is. */
    private static final class StreamScreen extends ContextWrapper implements StreamWindow {
        StreamScreen(Context base) {
            super(base);
        }

        @Override public boolean usesFullDisplayForVideo() {
            return true;
        }

        @Override public boolean isWindowedStream() {
            return false;
        }
    }

    @Test
    public void eachEventIsFeltAsThePolicySays() {
        for (Haptics.Event event : Haptics.Event.values()) {
            String pair = NightjarUiPolicyJni.feedback(event.key(), false, true, false);
            assertEquals(event.toString(), pair, Haptics.hapticOf(event) + "|");
            assertEquals("|", NightjarUiPolicyJni.feedback(event.key(), true, true, true));
            Haptics.resetForTests();
            assertEquals(Haptics.hapticOf(event), Haptics.haptic(event, false, 10_000));
            assertNull("a stream on screen silences " + event, Haptics.haptic(event, true, 20_000));
        }
    }

    @Test
    public void aHeldDpadTicksNoFasterThanAMotorCan() {
        assertEquals("tick", Haptics.haptic(Haptics.Event.FOCUS_TICK, false, 1000));
        assertNull(Haptics.haptic(Haptics.Event.FOCUS_TICK, false, 1020));
        assertEquals("press", Haptics.haptic(Haptics.Event.CONFIRM, false, 1021));
        assertEquals("press", Haptics.haptic(Haptics.Event.CONFIRM, false, 1022));
        assertEquals("tick", Haptics.haptic(Haptics.Event.FOCUS_TICK, false, 1045));
    }

    @Test
    public void theMirrorLimitsAsThePolicyDoes() {
        long limiter = NightjarUiPolicyJni.feedbackLimiterCreate();
        try {
            long now = 5000;
            java.util.Random random = new java.util.Random(7);
            for (int i = 0; i < 2000; i++) {
                Haptics.Event event = Haptics.Event.values()[random.nextInt(Haptics.Event.values().length)];
                boolean stream = random.nextInt(10) == 0;
                now += random.nextInt(80);
                String pair = NightjarUiPolicyJni.feedbackLimited(limiter, event.key(), stream, true, false, now);
                String policy = pair.substring(0, pair.indexOf('|'));
                String mirror = Haptics.hapticMirror(event, stream, now);
                assertEquals(i + " " + event + " " + stream + " " + now, policy.isEmpty() ? null : policy, mirror);
            }
        } finally {
            NightjarUiPolicyJni.feedbackLimiterDestroy(limiter);
        }
    }

    @Test
    public void theMirrorAnswersWithoutTheLibrary() {
        NightjarJni.forceUnavailableForTest(new UnsatisfiedLinkError("test"));
        try {
            assertEquals("double", Haptics.haptic(Haptics.Event.REJECT, false, 100));
            assertNull(Haptics.haptic(Haptics.Event.REJECT, false, 200));
            assertNull(Haptics.haptic(Haptics.Event.CONFIRM, true, 300));
        } finally {
            NightjarJni.forceUnavailableForTest(null);
        }
    }

    @Test
    public void aStreamIsFoundBehindAnyWrapper() {
        Context app = RuntimeEnvironment.getApplication();
        assertFalse(Haptics.streamOnScreen(app));
        assertTrue(Haptics.streamOnScreen(new StreamScreen(app)));
        assertTrue("a dialog over the stream", Haptics.streamOnScreen(new ContextThemeWrapper(new StreamScreen(app), 0)));
    }

    @Test
    public void eachHapticTakesTheClosestConstant() {
        assertEquals(HapticFeedbackConstants.CLOCK_TICK, Haptics.constant("tick", 33));
        assertEquals(HapticFeedbackConstants.SEGMENT_FREQUENT_TICK, Haptics.constant("tick", 34));
        assertEquals(HapticFeedbackConstants.CONTEXT_CLICK, Haptics.constant("click", 26));
        assertEquals(HapticFeedbackConstants.VIRTUAL_KEY, Haptics.constant("press", 29));
        assertEquals(HapticFeedbackConstants.CONFIRM, Haptics.constant("press", 30));
        assertEquals(HapticFeedbackConstants.REJECT, Haptics.constant("double", 30));
        assertEquals(HapticFeedbackConstants.GESTURE_START, Haptics.constant("rise", 34));
        assertNotEquals(Haptics.constant("press", 34), Haptics.constant("double", 34));
    }
}
