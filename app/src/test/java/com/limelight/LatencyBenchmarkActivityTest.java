package com.limelight;

import android.app.Application;
import android.widget.TextView;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.util.ReflectionHelpers;
import static org.junit.Assert.*;

@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class, shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class LatencyBenchmarkActivityTest {
    @Test public void startsWithoutHostAndStopsOnPause() {
        try (ActivityController<LatencyBenchmarkActivity> controller =
                     Robolectric.buildActivity(LatencyBenchmarkActivity.class).setup()) {
            LatencyBenchmarkActivity activity = controller.get();
            assertTrue(((TextView) activity.findViewById(R.id.benchmark_results)).getText().toString()
                    .contains(activity.getString(R.string.latency_unavailable)));
            activity.findViewById(R.id.benchmark_start).performClick();
            assertTrue(ReflectionHelpers.<Boolean>getField(activity, "running"));
            controller.pause();
            assertFalse(ReflectionHelpers.<Boolean>getField(activity, "running"));
        }
    }

    /** Review, TV preview: a device without audio properties read "no data yet Hz · no data yet frames per buffer". */
    @Test public void unreportedAudioIsSaidInWordsNotAsANumber() {
        try (ActivityController<LatencyBenchmarkActivity> controller =
                     Robolectric.buildActivity(LatencyBenchmarkActivity.class).setup()) {
            LatencyBenchmarkActivity activity = controller.get();
            String unavailable = activity.getString(R.string.latency_benchmark_audio_unavailable);
            assertEquals(unavailable, LatencyBenchmarkActivity.audioLine(activity, null, null));
            assertEquals("half a report is no report", unavailable, LatencyBenchmarkActivity.audioLine(activity, "48000", null));
            assertEquals(activity.getString(R.string.latency_benchmark_audio, "48000", "192"),
                    LatencyBenchmarkActivity.audioLine(activity, "48000", "192"));
            String shown = ((TextView) activity.findViewById(R.id.benchmark_results)).getText().toString();
            String unknown = activity.getString(R.string.latency_unavailable);
            assertFalse(shown, shown.contains(unknown + " Hz"));
            assertFalse(shown, shown.contains(unknown + " " + "frames"));
        }
    }

    @Test public void finishesAfterTenSecondsWithoutInput() {
        try (ActivityController<LatencyBenchmarkActivity> controller =
                     Robolectric.buildActivity(LatencyBenchmarkActivity.class).setup()) {
            LatencyBenchmarkActivity activity = controller.get();
            activity.findViewById(R.id.benchmark_start).performClick();
            org.robolectric.Shadows.shadowOf(android.os.Looper.getMainLooper())
                    .idleFor(java.time.Duration.ofSeconds(10));
            assertFalse(ReflectionHelpers.<Boolean>getField(activity, "running"));
        }
    }

    @Test public void manualStopAndRestartDiscardPreviousSamples() {
        try (ActivityController<LatencyBenchmarkActivity> controller =
                     Robolectric.buildActivity(LatencyBenchmarkActivity.class).setup()) {
            LatencyBenchmarkActivity activity = controller.get();
            activity.findViewById(R.id.benchmark_start).performClick();
            Object first = ReflectionHelpers.getField(activity, "stats");
            activity.findViewById(R.id.benchmark_start).performClick();
            assertFalse(ReflectionHelpers.<Boolean>getField(activity, "running"));
            activity.findViewById(R.id.benchmark_start).performClick();
            assertNotSame(first, ReflectionHelpers.getField(activity, "stats"));
        }
    }

    @Test public void exportAddsLatencyResultsOnlyAfterATestRan() {
        try (ActivityController<LatencyBenchmarkActivity> controller =
                     Robolectric.buildActivity(LatencyBenchmarkActivity.class).setup()) {
            LatencyBenchmarkActivity activity = controller.get();
            String before = ReflectionHelpers.callInstanceMethod(activity, "exportResults");
            assertTrue(before.contains("\"format\": \"nightjar-device-results\""));
            assertTrue(before.contains("\"measured\": false"));
            assertTrue(before.contains("\"latencyTest\": null"));
            activity.findViewById(R.id.benchmark_start).performClick();
            controller.pause();
            String after = ReflectionHelpers.callInstanceMethod(activity, "exportResults");
            assertTrue(after.contains("\"panelHz\": "));
            assertFalse(after.contains("\"latencyTest\": null"));
        }
    }
}
