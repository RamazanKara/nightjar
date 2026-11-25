package com.limelight.binding.video;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

/** What the decoder threads tell ADPF is work: CPU time, never the time they wait. */
@RunWith(RobolectricTestRunner.class)
public class DecoderPerformanceHintsTest {
    private static String renderer() throws Exception {
        return new String(Files.readAllBytes(
                Paths.get("src/main/java/com/limelight/binding/video/MediaCodecDecoderRenderer.java")), StandardCharsets.UTF_8);
    }

    private static String rendererLoop(String source) {
        int thread = source.indexOf("private void startRendererThread()");
        assertTrue(thread > 0);
        return source.substring(thread, source.indexOf("rendererThread.start();", thread));
    }

    // The renderer loop needs a running codec and a display, so this pins its wiring: the debt wait
    // (up to DecoderQueuePolicy.Debt.WAIT_US) and blocking releases sit inside the reported window,
    // so the window must be measured in thread CPU time, not wall time.
    @Test
    public void theRendererThreadReportsItsCpuTimeNotTheWaitsInItsHandOff() throws Exception {
        String loop = rendererLoop(renderer());
        int report = loop.indexOf("performanceHints.reportWorkDuration(");
        assertTrue(report > 0);
        assertEquals("performanceHints.reportWorkDuration(workNs, refreshRate)",
                loop.substring(report, loop.indexOf(';', report)));
        assertTrue("reports thread CPU time unless the switch restores wall time", loop.contains(
                "long workNs = cpuTime ? Debug.threadCpuTimeNanos() - workCpuStartNs :\n" +
                "                                        System.nanoTime() - workStartNs;"));
        assertTrue("CPU time by default", loop.contains(
                "MediaCodecHelper.debugSwitch(DecoderPerformanceHints.CPU_TIME_SWITCH, true)"));
        assertEquals("adpf_cpu_time", DecoderPerformanceHints.CPU_TIME_SWITCH);
        assertTrue("off, no thread clock is read", loop.contains("if (cpuTime) workCpuStartNs = Debug.threadCpuTimeNanos();"));
        int start = loop.indexOf("workCpuStartNs = Debug.threadCpuTimeNanos();");
        int dequeue = loop.indexOf("videoDecoder.dequeueOutputBuffer(info, 50000)");
        assertTrue("the window starts after the idle dequeue", start > dequeue && dequeue > 0);
    }

    @Test
    public void submitWorkHandsEachSubmissionToTheNextReportOnce() {
        DecoderPerformanceHints.SubmitWork work = new DecoderPerformanceHints.SubmitWork();
        assertEquals(0, work.take());
        assertEquals(0, work.threadId());
        work.add(42, 300_000);
        work.add(42, 200_000);
        assertEquals(42, work.threadId());
        assertEquals(500_000, work.take());
        // Reported once: the next frame starts from nothing.
        assertEquals(0, work.take());
        // A clock that steps back is no negative work.
        work.add(43, -5);
        assertEquals(0, work.take());
        assertEquals("a second submit thread joins the first", 43, work.secondThreadId());
        assertEquals(42, work.threadId());
        work.add(42, 1);
        work.add(43, 1);
        assertEquals("only two distinct threads are kept", 42, work.threadId());
        // A recovery drops what its flushed frames took.
        work.add(43, 1_000);
        work.reset();
        assertEquals(0, work.take());
    }

    @Test
    public void theSessionListsTheCreatingThreadAndEachSubmitThreadOnce() {
        assertArrayEquals(new int[] { 7 }, DecoderPerformanceHints.threadList(7, 0, 0));
        assertArrayEquals(new int[] { 7, 8 }, DecoderPerformanceHints.threadList(7, 7, 8));
        assertArrayEquals(new int[] { 7, 8, 9 }, DecoderPerformanceHints.threadList(7, 8, 9));
        assertArrayEquals(new int[] { 7, 8 }, DecoderPerformanceHints.threadList(7, 8, 8));
    }

    // The submit side must measure CPU time and only when the switch is on: off, a frame costs
    // no clock reads (no added latency by default).
    @Test
    public void submitThreadTimingIsCpuTimeAndBehindItsSwitch() throws Exception {
        String source = renderer();
        int method = source.indexOf("public int submitDecodeUnit(");
        assertTrue(method > 0);
        String wrapper = source.substring(method, source.indexOf("private int queueDecodeUnit(", method));
        int off = wrapper.indexOf("if (!submitHints) {");
        int firstClock = wrapper.indexOf("Debug.threadCpuTimeNanos()");
        assertTrue("off returns before any clock read", off > 0 && firstClock > off &&
                wrapper.indexOf("return queueDecodeUnit(", off) < firstClock);
        assertTrue(wrapper.contains("submitWork.add(Process.myTid(), Debug.threadCpuTimeNanos() - cpuStartNs);"));
        assertFalse(wrapper.contains("System.nanoTime()"));
        assertTrue("the setting (off by default, SettingsKeysTest), which the property overrides",
                source.contains("MediaCodecHelper.debugSwitch(DecoderPerformanceHints.SUBMIT_SWITCH, prefs.submitHint)"));
        assertEquals("submit_adpf", DecoderPerformanceHints.SUBMIT_SWITCH);
    }

    @Test
    @org.robolectric.annotation.Config(sdk = 34, shadows = ThrowingHintSessionShadow.class)
    public void adpfGuardsZeroFrameRateAndPowerHalRuntimeExceptions() throws Exception {
        String hints = new String(Files.readAllBytes(
                Paths.get("src/main/java/com/limelight/binding/video/DecoderPerformanceHints.java")), StandardCharsets.UTF_8);
        assertTrue(hints.contains("enabled && frameRate > 0"));
        assertTrue(hints.contains("if (frameRate > 0)"));

        android.content.Context context = org.robolectric.RuntimeEnvironment.getApplication();
        // Zero frame rate must not throw ArithmeticException in ctor.
        DecoderPerformanceHints zeroFps = new DecoderPerformanceHints(context, true, 0);
        assertFalse(zeroFps.isActive());
        zeroFps.reportWorkDuration(1_000_000L, 0);
        zeroFps.close();

        // A throwing Power HAL session must not crash reportWorkDuration or close.
        DecoderPerformanceHints throwingHints = new DecoderPerformanceHints(context, false, 60);
        android.os.PerformanceHintManager.Session badSession =
                org.robolectric.shadow.api.Shadow.newInstanceOf(android.os.PerformanceHintManager.Session.class);
        org.robolectric.util.ReflectionHelpers.setField(throwingHints, "session", badSession);
        org.robolectric.util.ReflectionHelpers.setField(throwingHints, "targetDurationNs", 16_666_666L);
        throwingHints.reportWorkDuration(500_000L, 120);
        throwingHints.reportWorkDuration(500_000L, 0);
        throwingHints.close();
        assertFalse(throwingHints.isActive());
    }

    @org.robolectric.annotation.Implements(android.os.PerformanceHintManager.Session.class)
    public static class ThrowingHintSessionShadow {
        @org.robolectric.annotation.Implementation
        public void updateTargetWorkDuration(long targetDurationNanos) {
            throw new IllegalStateException("HAL dead");
        }

        @org.robolectric.annotation.Implementation
        public void reportActualWorkDuration(long actualDurationNanos) {
            throw new IllegalStateException("HAL dead");
        }

        @org.robolectric.annotation.Implementation
        public void close() {
            throw new IllegalStateException("HAL dead");
        }
    }
}
