package com.limelight.binding.input;

import com.limelight.nvstream.jni.NightjarInputJni;

/**
 * Computes fractional 100 Hz mouse-emulation pointer moves and high-res scroll deltas matching
 * {@code nightjar_policy::controller::state_machine::mouse_tick_step}.
 */
final class ControllerMouseTick {
    static final int REPORT_PERIOD_MS = 10;
    static final float TIME_COMPENSATION = 0.2f;
    static final int MIN_SENSITIVITY_PERCENT = 25;
    static final int MAX_SENSITIVITY_PERCENT = 200;
    static final int DEFAULT_SENSITIVITY_PERCENT = 100;

    private static final float STICK_SCALE = 32766.0f;

    private ControllerMouseTick() {}

    static int clampSensitivityPercent(int percent) {
        return Math.max(MIN_SENSITIVITY_PERCENT, Math.min(MAX_SENSITIVITY_PERCENT, percent));
    }

    // TODO(integrator-642): switch compute() to NightjarInputJni.mouseTickStepFloat(...) once
    // wip/v642-fix-client-core and wip/v642-fix-android-stream merge into main.
    static int compute(short lx, short ly, short rx, short ry,
                       int scrollStickMode, int fixedStep, int sensitivityPercent, float[] out) {
        out[0] = out[1] = out[2] = out[3] = out[4] = out[5] = 0f;
        double scale = (double) TIME_COMPENSATION * (clampSensitivityPercent(sensitivityPercent) / 100.0);
        int flags = 0;

        if (scrollStickMode == 1) {
            if (stickMove(lx, ly, fixedStep, scale, out, 0)) flags |= 1;
            if (stickScroll(rx, ry, out, 4)) flags |= 4;
        } else if (scrollStickMode == 2) {
            if (stickMove(rx, ry, fixedStep, scale, out, 0)) flags |= 1;
            if (stickScroll(lx, ly, out, 4)) flags |= 4;
        } else {
            if (stickMove(lx, ly, fixedStep, scale, out, 0)) flags |= 1;
            if (stickMove(rx, ry, fixedStep, scale, out, 2)) flags |= 2;
        }
        return flags;
    }

    private static void mouseStickCurve(short stickX, short stickY, float[] vxVy) {
        double invScale = 1.0f / STICK_SCALE;
        float x = (float) (stickX * invScale);
        float y = (float) (stickY * invScale);
        x = (float) (x * 4.0);
        y = (float) (y * 4.0);
        double mag = Math.hypot(x, y);
        if (mag > 0.0) {
            double factor = mag * mag;
            vxVy[0] = (float) (x * factor);
            vxVy[1] = (float) (y * factor);
        } else {
            vxVy[0] = x;
            vxVy[1] = y;
        }
    }

    private static boolean stickMove(short x, short y, int fixedStep, double scale, float[] out, int offset) {
        float[] v = new float[2];
        mouseStickCurve(x, y, v);
        float vx = v[0];
        float vy = v[1];
        if (vx == 0.0f && vy == 0.0f) {
            return false;
        }
        float dx;
        float dy;
        if (fixedStep > 0) {
            int fx = fixedStepDelta(vx, fixedStep);
            int fy = fixedStepDelta(-vy, fixedStep);
            dx = (float) (fx * scale);
            dy = (float) (fy * scale);
        } else {
            dx = (float) ((double) vx * scale);
            dy = (float) ((double) (-vy) * scale);
        }
        out[offset] = dx;
        out[offset + 1] = dy;
        return dx != 0.0f || dy != 0.0f;
    }

    private static boolean stickScroll(short x, short y, float[] out, int offset) {
        float[] v = new float[2];
        mouseStickCurve(x, y, v);
        float vx = v[0];
        float vy = v[1];
        double mag = Math.hypot(vx, vy);
        if (mag < 1.0) {
            return false;
        }
        out[offset] = (float) ((double) vy * TIME_COMPENSATION);
        out[offset + 1] = (float) ((double) vx * TIME_COMPENSATION);
        return true;
    }

    private static int fixedStepDelta(double component, int step) {
        int s = Integer.signum((int) component);
        long prod = (long) s * (long) step;
        if (prod > Integer.MAX_VALUE) return Integer.MAX_VALUE;
        if (prod < Integer.MIN_VALUE) return Integer.MIN_VALUE;
        return (int) prod;
    }

    static float dtCompensation(long lastTickNs, long nowNs, int nominalPeriodMs) {
        if (lastTickNs <= 0L || nowNs <= lastTickNs) {
            return 1.0f;
        }
        float nominalNs = nominalPeriodMs * 1_000_000.0f;
        float ratio = (nowNs - lastTickNs) / nominalNs;
        return Math.max(0.1f, Math.min(5.0f, ratio));
    }

    static void unusedSilenceCheckKeep() {
        // Ensures unused import warnings don't arise before integrator swaps to NightjarInputJni.
        NightjarInputJni.mouseSilenced(true, true);
    }
}
