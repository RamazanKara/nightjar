package com.limelight.binding.video;

import android.content.Context;
import android.os.Build;
import android.os.PerformanceHintManager;
import android.os.Process;

import com.limelight.LimeLog;

import java.util.concurrent.atomic.AtomicLong;

final class DecoderPerformanceHints implements AutoCloseable {
    /** debug.nightjar.submit_adpf: the submit thread joins the renderer's session; overrides the Debug setting (on by default). */
    static final String SUBMIT_SWITCH = "submit_adpf";
    /** debug.nightjar.adpf_cpu_time: the renderer reports thread CPU time, not wall time (on by default). */
    static final String CPU_TIME_SWITCH = "adpf_cpu_time";

    private PerformanceHintManager manager;
    private volatile PerformanceHintManager.Session session;
    private long targetDurationNs;
    private int ownThreadId;
    private int submitThreadId;
    private int submitThreadId2;
    private boolean submitThreadsPending;
    private long submitThreadsRetryNs;

    DecoderPerformanceHints(Context context, boolean enabled, int frameRate) {
        if (enabled && frameRate > 0 && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            manager = context.getSystemService(PerformanceHintManager.class);
            if (manager != null) {
                targetDurationNs = 1000000000L / frameRate;
                ownThreadId = Process.myTid();
                try {
                    session = manager.createHintSession(new int[] { ownThreadId }, targetDurationNs);
                } catch (RuntimeException e) {
                    LimeLog.warning("Performance hint session creation failed: " + e);
                }
            }
        }
    }

    /**
     * Puts the threads that submit decode units in the session beside the creating thread, so the
     * governor boosts the whole path and not just the hand-off: the receive thread for units it
     * submits in place and the InputSubmit worker for the ones handed off. A new pair replaces the
     * last one; the same pair is a no-op. 0 is no thread.
     */
    void setSubmitThreads(int first, int second) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S || session == null || (first == 0 && second == 0)) {
            return;
        }
        boolean samePair = first == submitThreadId && second == submitThreadId2;
        // A pair refused because the app was not in the foreground is tried again once a second.
        if (samePair && (!submitThreadsPending || System.nanoTime() < submitThreadsRetryNs)) {
            return;
        }
        if (!samePair) {
            // Remembered even when refused, so a lasting refusal is logged once and not retried every frame.
            submitThreadId = first;
            submitThreadId2 = second;
            // Once per new pair, not per frame: shows whether the second submit thread (the InputSubmit worker) was seen.
            LimeLog.info("Performance hint submit threads: " + first + ", " + second);
        }
        submitThreadsPending = false;
        int[] threads = threadList(ownThreadId, first, second);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            // This runs in the render loop's finally, so nothing may escape it.
            try {
                session.setThreads(threads);
            } catch (IllegalStateException e) {
                // The session is not in the foreground (picture-in-picture): try again later.
                submitThreadsPending = true;
                submitThreadsRetryNs = System.nanoTime() + 1_000_000_000L;
                if (!samePair) {
                    LimeLog.warning("Performance hint session refused submit threads " + first + ", " + second + " for now: " + e);
                }
            } catch (RuntimeException e) {
                LimeLog.warning("Performance hint session refused submit threads " + first + ", " + second + ": " + e);
            }
        } else {
            PerformanceHintManager.Session replacement;
            try {
                replacement = manager.createHintSession(threads, targetDurationNs);
            } catch (RuntimeException e) {
                LimeLog.warning("Performance hint session could not add submit threads " + first + ", " + second + ": " + e);
                return;
            }
            if (replacement == null) {
                LimeLog.warning("Performance hint session could not add submit threads " + first + ", " + second);
            } else {
                try {
                    session.close();
                } catch (RuntimeException e) {
                    LimeLog.warning("Performance hint session close failed: " + e);
                }
                session = replacement;
            }
        }
    }

    /** The creating thread, then each submit thread once (none is 0 or the creating thread). */
    static int[] threadList(int own, int first, int second) {
        int n = 1;
        int[] out = { own, 0, 0 };
        if (first != 0 && first != own) out[n++] = first;
        if (second != 0 && second != own && second != first) out[n++] = second;
        return java.util.Arrays.copyOf(out, n);
    }

    /** True while an ADPF hint session is open; callers skip threadCpuTimeNanos() when false. */
    boolean isActive() {
        return session != null;
    }

    void reportWorkDuration(long durationNs, int frameRate) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && session != null) {
            try {
                if (frameRate > 0) {
                    long frameIntervalNs = 1000000000L / frameRate;
                    if (frameIntervalNs != targetDurationNs) {
                        session.updateTargetWorkDuration(frameIntervalNs);
                        targetDurationNs = frameIntervalNs;
                    }
                }
                session.reportActualWorkDuration(Math.max(1, durationNs));
            } catch (RuntimeException e) {
                LimeLog.warning("Performance hint work duration report failed: " + e);
            }
        }
    }

    @Override
    public void close() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && session != null) {
            try {
                session.close();
            } catch (RuntimeException e) {
                LimeLog.warning("Performance hint session close failed: " + e);
            }
            session = null;
        }
    }

    /**
     * The submit thread's CPU time since the renderer last reported, so each frame's report covers
     * both threads. CPU time, not wall time: the submit thread blocks for input buffers and spacing,
     * and that waiting is no work for ADPF (wall time was why the first version was reverted).
     */
    static final class SubmitWork {
        private final AtomicLong unreportedNs = new AtomicLong();
        // The receive thread (in-place units) and the InputSubmit worker (handed-off ones) both submit.
        private volatile int threadId;
        private volatile int threadId2;

        /** On a submit thread, after a decode unit: its thread id and the CPU time it took. */
        void add(int submitThreadId, long cpuNs) {
            if (submitThreadId != threadId && submitThreadId != threadId2) {
                // Both submit threads can see a new id at once; the lock keeps either from overwriting the other.
                synchronized (this) {
                    int first = threadId;
                    if (submitThreadId != first && submitThreadId != threadId2) {
                        if (first == 0) {
                            threadId = submitThreadId;
                        } else {
                            threadId2 = submitThreadId;
                        }
                    }
                }
            }
            if (cpuNs > 0) {
                unreportedNs.addAndGet(cpuNs);
            }
        }

        /** On the renderer thread, with its report: the submit time since the last take. */
        long take() {
            return unreportedNs.getAndSet(0);
        }

        /** The first submit thread seen. */
        int threadId() {
            return threadId;
        }

        /** The second one (the other of receive thread and worker), or 0. */
        int secondThreadId() {
            return threadId2;
        }

        /** A codec recovery drops the frames whose submissions are not reported yet. */
        void reset() {
            unreportedNs.set(0);
        }
    }
}
