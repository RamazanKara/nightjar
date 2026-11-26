package com.limelight.binding.input;

import com.limelight.nvstream.jni.NightjarInputJni;

/**
 * Delegates 100 Hz mouse-emulation pointer moves and high-res scroll deltas to
 * {@link NightjarInputJni#mouseTickStepFloat} ({@code nightjar_policy::controller::state_machine::mouse_tick_step})
 * and computes per-tick dt compensation.
 */
final class ControllerMouseTick {
    static final int REPORT_PERIOD_MS = 10;

    private ControllerMouseTick() {}

    static int compute(short lx, short ly, short rx, short ry,
                       int scrollStickMode, int fixedStep, int sensitivityPercent, float[] out) {
        return NightjarInputJni.mouseTickStepFloat(
                lx, ly, rx, ry, scrollStickMode, fixedStep, sensitivityPercent, out);
    }

    static float dtCompensation(long lastTickNs, long nowNs, int nominalPeriodMs) {
        if (lastTickNs <= 0L || nowNs <= lastTickNs) {
            return 1.0f;
        }
        float nominalNs = nominalPeriodMs * 1_000_000.0f;
        float ratio = (nowNs - lastTickNs) / nominalNs;
        return Math.max(0.1f, Math.min(5.0f, ratio));
    }
}
