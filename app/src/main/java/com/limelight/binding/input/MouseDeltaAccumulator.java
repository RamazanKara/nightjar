package com.limelight.binding.input;

public final class MouseDeltaAccumulator {
    private float remainder;

    public int scale(float delta, float factor) {
        float scaled = delta * factor + remainder;
        int whole = Math.round(scaled);
        remainder = scaled - whole;
        return whole;
    }

    public void reset() {
        remainder = 0f;
    }

    public void copyFrom(MouseDeltaAccumulator other) {
        this.remainder = other == null ? 0f : other.remainder;
    }
}
