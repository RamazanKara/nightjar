package com.limelight.nvstream.jni;

/**
 * Pure-JDK JNI declarations for {@code nightjar_policy::controller}
 * ({@code client/protocol/android/src/policy_input/}). Loads {@code nightjar_core} via
 * {@link NightjarJni#ensureLibraryLoaded()}.
 */
public final class NightjarInputJni {
    static {
        NightjarJni.ensureLibraryLoaded();
    }

    private NightjarInputJni() {
    }

    // --- 1. Deadzone & Normalization ---

    public static native void applyStickDeadzone(
            float x, float y, float deadzoneRadius, float[] out);

    public static native float applyTriggerDeadzone(float value, float deadzone);

    public static native byte maxByMagnitudeByte(byte a, byte b);

    public static native short maxByMagnitudeShort(short a, short b);

    public static native void normalizeAxisSet(
            float lsX, float lsY, float rsX, float rsY,
            float lt, float rt, float hatX, float hatY,
            boolean hasLeftStick, boolean hasRightStick, boolean hasTriggers, boolean hasHat,
            float leftDeadzone, float rightDeadzone, float triggerDeadzone,
            boolean triggersIdleNegative, boolean leftTriggerUsed, boolean rightTriggerUsed,
            boolean hatXUsed, boolean hatYUsed, int currentInputMap,
            int[] out);

    public static native void normalizeUsbAxes(
            float lsX, float lsY, float rsX, float rsY, float lt, float rt,
            float leftDeadzone, float rightDeadzone, float triggerDeadzone,
            int[] out);

    // --- 2. Remapping & Emulated Combos ---

    public static final int QUIRK_IGNORE_BACK = 1 << 0;
    public static final int QUIRK_HAS_SHARE = 1 << 1;
    public static final int QUIRK_DS4_STANDALONE_TOUCHPAD = 1 << 2;
    public static final int QUIRK_LINUX_STD_FACE = 1 << 3;
    public static final int QUIRK_NON_STD_DS4 = 1 << 4;
    public static final int QUIRK_SERVAL = 1 << 5;
    public static final int QUIRK_NON_STD_XBOX_BT = 1 << 6;
    public static final int QUIRK_NO_HAT = 1 << 7;
    public static final int QUIRK_BACK_IS_START = 1 << 8;
    public static final int QUIRK_MODE_IS_SELECT = 1 << 9;
    public static final int QUIRK_SEARCH_IS_MODE = 1 << 10;

    public static native int[] androidButtonKeycodes();

    public static native int remapKeyEvent(
            boolean backAsGuide, int keyCode, int scanCode, int flags, boolean hasNoModifiers,
            int vendorId, int productId, int sdkInt, int quirkFlags, int[] outStateUpdate);

    public static native int flipFaceButtonKeyCode(int keyCode);

    public static native int normalizeMappedTarget(int target);

    public static native boolean isHandledGamepadKey(int keyCode, int scanCode, boolean hasPaddles);

    public static native void buttonDownStep(
            int inputMap, byte leftTrigger, byte rightTrigger,
            boolean hasPaddles, boolean hatXUsed, boolean hatYUsed,
            boolean ltUsed, boolean rtUsed, int keyCode, int scanCode, int[] out);

    public static native void buttonUpStep(
            int inputMap, byte leftTrigger, byte rightTrigger,
            boolean hasPaddles, boolean hatXUsed, boolean hatYUsed,
            boolean ltUsed, boolean rtUsed, int keyCode, int scanCode, int[] out);

    public static native int heldButtons(
            int inputMap, int emulatingFlags,
            boolean hasTargetTouchpad, boolean hasSelect, boolean hasLb);

    public static native void applyEmulatedCombosOnDown(
            int inputMap, int emulatingFlags, boolean hasSelect, boolean hasMode,
            boolean needsClickpad, long eventTimeMs, long lastLbUpMs, long lastRbUpMs,
            int[] out);

    public static native void applyEmulatedCombosOnUp(
            int inputMap, int emulatingFlags,
            boolean hasTargetTouchpad, boolean hasSelect, boolean hasLb,
            int[] out);

    public static native void applyUsbClickpad(
            int buttons, int clickpadSwallowed, byte deviceType, byte connectAs,
            boolean hasDeviceMotion, boolean needsClickpad, int[] out);

    // --- 3. Arrival, Capabilities & Steam Controls ---

    public static native boolean countsAsExternalController(
            boolean external, boolean gamepad, boolean remote,
            boolean keyboard, boolean handheldControls);

    public static native boolean onscreenOwnsPlayerOne(
            boolean onscreenOwnPlayer, boolean multiController,
            boolean onscreenController, boolean onlyL3R3);

    public static native boolean looksLikeHandheldControls(
            String name, int vendorId, boolean onlyGamepad, boolean attachedUsb);

    public static native boolean isBackAsGuide(boolean backAsGuide, int keyCode, int flags);

    public static native boolean isPadBackAsGuide(
            boolean backAsGuide, int keyCode, int flags, boolean backIsStart);

    public static native boolean isHandheldSystemBack(
            int keyCode, int flags, int source, boolean virtualDevice, boolean external,
            boolean alphabeticKeyboard, boolean gamepad, boolean backIsSpareButton);

    public static native boolean backIsSpareButton(
            boolean internalGamepad, boolean internalSelect, boolean anyGamepad);

    public static native int isExternalOverride(String buildModel, String devName);

    public static native boolean shouldIgnoreBack(
            String devName, boolean hasJoystickAxes, boolean hasGamepadButtons,
            boolean external, boolean internalGamepad, boolean internalSelect);

    public static native byte arrivalType(
            byte detectedType, byte connectAs, int targetButtonFlags);

    public static native byte inputDeviceArrivalType(
            byte detectedType, boolean hasMotion, byte connectAs, int targetButtonFlags);

    public static native boolean dropsMotionAndTouchpad(byte connectAs);

    public static native short arrivalCapabilities(short capabilities, byte connectAs);

    public static native boolean takesDeviceMotion(
            boolean fallbackToDevice, short relativeSlot, short padCapabilities, byte connectAs);

    public static native boolean motionOverridesType(
            byte detectedType, boolean hasMotion, byte connectAs);

    public static native boolean needsClickpadEmulation(int targetButtonFlags);

    public static native byte onscreenPadType(boolean touchpadButton);

    public static native int onscreenPadButtons(boolean touchpadButton, boolean steamControls);

    public static native short onscreenPadCapabilities(boolean steamControls);

    public static native boolean carriesSteamControls(
            boolean steamControls, boolean onscreenOwnPlayer, short controllerNumber);

    public static native short withSteamControlsCapabilities(
            boolean steamControls, boolean onscreenOwnPlayer, short controllerNumber,
            short capabilities, byte connectAs);

    public static native int withSteamControlsButtons(
            boolean steamControls, boolean onscreenOwnPlayer, short controllerNumber, int buttons);

    public static native void classifyInputDevice(
            int vendorId, int productId, String devName, boolean isKeyboardMouseOnly,
            boolean hasLTriggerRTrigger, boolean hasBrakeGas, boolean hasBrakeThrottle,
            boolean hasGasRange, boolean hasRxRy, boolean hasZRz,
            boolean hasButtonC, boolean hasHat, boolean hasStartOrMenu,
            boolean initialHasSelect, boolean initialHasMode, boolean ignoreBack,
            float baseTriggerDeadzone, int[] outInts, float[] outTriggerDeadzone);

    public static native int computeBaseSupportedButtons(
            boolean[] hasKeys, boolean hasPaddles, boolean hasShare,
            int targetButtonFlags, boolean hasHatX, boolean hasHatY, boolean hasClickpad);

    public static native void computeInputDeviceArrival(
            byte detectedType, int baseSupportedButtons, short baseCapabilities,
            boolean hasMotion, byte connectAs, int targetButtonFlags,
            boolean carriesTouchpadButton, boolean steamControls,
            boolean onscreenOwnPlayer, short controllerNumber, int[] out);

    public static native String formatLaunchController(
            short slot, byte kind, short capabilities, int buttons);

    // --- 4. Slot Reservation & Context Fusion ---

    public static native long slotStateCreate(
            short initialControllers, boolean onscreenOwnPlayerSession,
            boolean touchpadButtonSession, boolean steamControlsSession);

    public static native void slotStateDestroy(long handle);

    public static native short activeControllerMask(
            long handle, boolean sendControllers, boolean multiController,
            boolean onscreenController, boolean layoutGamepadUsed, boolean hasBuiltInPlayerOne);

    public static native int assignInputDeviceSlot(
            long handle, boolean external, boolean hasJoystickAxes,
            boolean hasGamepadButtons, boolean hasAdjacentAssociatedJoystick,
            boolean multiController, int builtInPadSlot);

    public static native int resolveAssociatedFallbackSlot(long handle);

    public static native int assignUsbDeviceSlot(long handle, boolean multiController);

    public static native int releaseReservedSlot(
            long handle, short slot, boolean wasReserved, boolean assigned,
            boolean sendControllers, boolean multiController, boolean onscreenController,
            boolean layoutGamepadUsed, boolean hasBuiltInPlayerOne, boolean pendingUsbArrival);

    public static native void markSlotHasInput(long handle, short slot);

    public static native boolean slotHasInput(long handle, short slot);

    public static native boolean announcesOnscreenPad(
            long handle, boolean sendControllers, boolean onscreenController,
            boolean layoutGamepadUsed, boolean hasSlotZeroDevice);

    public static native int launchPassForInputDevice(
            boolean external, boolean hasJoystickAxes);

    public static native void fuseControllerInputs(
            int[] packedContexts, int count, boolean steamControlsSession,
            short controllerNumber, float steamPadTouchX, boolean mouseEmulationActive,
            int[] out);

    // --- 5. State Machines (Combos, GuideHold, Mouse Mode, Steer/Capture) ---

    public static native int[] capturedButtonFlags();

    public static native long usbInputStateCreate();

    public static native void usbInputStateDestroy(long handle);

    public static native int usbSteerMenuStep(
            long handle, int controllerId, int buttons, int[] outKeys);

    public static native void usbCapturePadStep(
            long handle, int controllerId, int buttons,
            float stickX, float stickY, int generation, int[] out);

    public static native int usbFilterGameButtons(long handle, int controllerId, int buttons);

    public static native void usbRemovePadCapture(long handle, int controllerId, int[] out);

    public static native int quitComboMask(String combo);

    public static native int quitButtonFlag(int keyCode);

    public static native boolean holdQuitComboStep(boolean pending, int buttons, String combo);

    public static native int releaseQuitComboStep(boolean pending, int buttons);

    public static native boolean shouldArmMenuCombo(int heldButtons);

    public static native boolean shouldOpenMenuOnRelease(
            boolean pendingExit, int inputMap, boolean pressedEmpty);

    public static native long tapHoldRemainingMs(long downTimeMs, long eventTimeMs);

    public static native boolean startLongPressTogglesMouse(
            long startDownTimeMs, long eventTimeMs, boolean mouseEmulationEnabled);

    public static native int guideHoldStep(int stateBits, int op, long holdMs, boolean repeat);

    public static native void usbGuideHoldStep(
            boolean guideDown, int guideHoldBits, boolean guideTapping,
            int buttonFlags, long holdMs, boolean canHold, int[] out);

    public static native int fixedStepNext(int step);

    public static native int fixedStepDelta(double component, int step);

    public static native boolean mouseSilenced(
            boolean sendControllers, boolean isDefaultContext);

    public static native int pickMenuMouseTarget(
            boolean[] eligible, short[] numbers, boolean[] active, int count);

    public static native void mousePacketStep(
            int inputMap, int lastInputMap, boolean fixedStepEnabled,
            int currentStep, int[] out);

    /**
     * Computes 100 Hz mouse movement and scroll deltas for one tick.
     *
     * @param out 6-float buffer filled with {@code [move1X, move1Y, move2X, move2Y, scrollV, scrollH]}.
     * @return bitmask {@code (hasMove1 ? 1 : 0) | (hasMove2 ? 2 : 0) | (hasScroll ? 4 : 0)}.
     */
    public static native int mouseTickStepFloat(
            short lx, short ly, short rx, short ry,
            int scrollStickMode, int fixedStep, int sensitivityPercent, float[] out);

    // --- 6. Rumble, Sensors, Gyro & Battery ---

    public static native boolean rumbleAllows(boolean rumbleDisabled);

    public static native int rumbleFeelFromKey(String key);

    public static native short rumbleShapeMotor(short motor, int feelOrdinal);

    public static native int rumbleShapeAmplitude(int amplitude, int feelOrdinal);

    public static native short rumbleScaleMotor(short motor, int strengthPercent);

    public static native int rumbleSingleMotorAmplitude(
            short lowFreqMotor, short highFreqMotor, int feelOrdinal, int strengthPercent);

    public static native void rumblePwmTimings(int amplitude, int feelOrdinal, long[] out);

    public static native long rumbleStateCreate(
            boolean allowed, int feelOrdinal, short deviceFallbackSlot);

    public static native void rumbleStateDestroy(long handle);

    public static native int rumbleRecordMotor(
            long handle, short slot, short low, short high, boolean stopped);

    public static native int rumbleRecordTriggers(
            long handle, short slot, short lt, short rt, boolean stopped);

    public static native int rumbleSetPaused(
            long handle, boolean paused, boolean stopped, int[] out);

    public static native boolean rumblePlanDual(
            short low, short high, int feelOrdinal, int[] out);

    public static native boolean rumblePlanQuad(
            short low, short high, short lt, short rt, int feelOrdinal, int[] out);

    public static native int rumblePlanSingle(
            short low, short high, int feelOrdinal, int strengthPercent, long[] out);

    public static native int rumbleDeviceVibratorRoute(
            long handle, short slot, short low, short high,
            boolean foundMatching, boolean vibrated, boolean osc, boolean onlyL3R3,
            boolean vibrateOsc, boolean fallbackToDevice, int fallbackStrength, long[] out);

    public static native int rumbleReleaseSlot(
            long handle, short slot, boolean assigned,
            boolean slotStillAssigned, boolean devVibForP1);

    public static native boolean gyroLivenessStep(
            long zeroNanos, float x, float y, float z, long timestampNanos, long[] state);

    public static native boolean checksGyroLiveness(
            short slot, short deviceFallbackSlot, boolean usingDeviceSensors,
            boolean fallbackToDevice, boolean hasDeviceGyro);

    public static native int clampMotionRateHz(int rateHz);

    public static native int motionSamplingPeriodUs(int rateHz);

    public static native int usbMotionRateHz(
            int reportRateHz, boolean gamepadMotionSensors,
            boolean dropsMotionAndTouchpad, boolean usesDeviceSensors);

    public static native boolean isValidGyroSensitivity(float value);

    public static native int validateGyroMapping(
            int xAxis, int yAxis, int zAxis,
            int xSign, int ySign, int zSign, float hSens, float vSens);

    public static native void transformGyro(
            float x, float y, float z,
            int xAxis, int yAxis, int zAxis,
            int xSign, int ySign, int zSign, float hSens, float vSens, float[] out);

    public static native long motionFilterCreate(
            byte motionType, boolean needsDeviceOrientation, boolean checkLiveness,
            boolean hasMapping, int xAxis, int yAxis, int zAxis,
            int xSign, int ySign, int zSign, float hSens, float vSens);

    public static native void motionFilterDestroy(long handle);

    public static native int motionSampleStep(
            long handle, float x, float y, float z,
            long timestampNanos, int displayRotation, boolean flydigiForced, float[] out);

    public static native int sceBatteryReading(
            int percentage, int connectionType, int chargingState, float[] out);

    public static native boolean evaluateBatteryUpdate(
            boolean sendControllers, int status, float capacity,
            int lastStatus, float lastCapacity, int[] out);
}
