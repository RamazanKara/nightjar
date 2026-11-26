package com.limelight.binding.input;

import android.annotation.SuppressLint;
import android.annotation.TargetApi;
import android.hardware.BatteryState;
import android.hardware.Sensor;
import android.hardware.lights.Light;
import android.hardware.lights.LightState;
import android.hardware.lights.LightsManager;
import android.hardware.lights.LightsRequest;
import android.os.Build;
import android.view.InputDevice;
import android.view.MotionEvent;
import android.widget.Toast;

import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarInputJni;

import org.cgutman.shieldcontrollerextensions.SceChargingState;
import org.cgutman.shieldcontrollerextensions.SceConnectionType;

import java.util.ArrayList;

/**
 * Handles {@link ControllerHandler.InputDeviceContext} and {@link ControllerHandler.UsbDeviceContext}
 * arrivals, battery polling, RGB LED sessions, context migration on device reconfiguration, and
 * teardown for {@link ControllerHandler}.
 */
final class ControllerContextLifecycle {
    private final ControllerHandler handler;
    private final int[] batteryOut = new int[2];
    private final float[] sceCapOut = new float[1];

    ControllerContextLifecycle(ControllerHandler handler) {
        this.handler = handler;
    }

    void initAttachedGamepadFlag() {
        for (int id : InputDevice.getDeviceIds()) {
            InputDevice dev = InputDevice.getDevice(id);
            if (dev == null) continue;
            if ((dev.getSources() & InputDevice.SOURCE_JOYSTICK) != 0 ||
                    (dev.getSources() & InputDevice.SOURCE_GAMEPAD) != 0) {
                if (ControllerDeviceClassifier.getMotionRangeForJoystickAxis(dev, MotionEvent.AXIS_X) != null &&
                        ControllerDeviceClassifier.getMotionRangeForJoystickAxis(dev, MotionEvent.AXIS_Y) != null) {
                    handler.hasGameController = true;
                }
            }
        }
    }

    void initNoDeviceContext(ControllerHandler.InputDeviceContext context, short controllerNumber) {
        context.leftStickXAxis = MotionEvent.AXIS_X;
        context.leftStickYAxis = MotionEvent.AXIS_Y;
        context.leftStickDeadzoneRadius = (float) handler.stickDeadzone;
        context.rightStickXAxis = MotionEvent.AXIS_Z;
        context.rightStickYAxis = MotionEvent.AXIS_RZ;
        context.rightStickDeadzoneRadius = (float) handler.stickDeadzone;
        context.triggerDeadzone = handler.triggerDeadzone;
        context.leftTriggerAxis = MotionEvent.AXIS_BRAKE;
        context.rightTriggerAxis = MotionEvent.AXIS_GAS;
        context.hatXAxis = MotionEvent.AXIS_HAT_X;
        context.hatYAxis = MotionEvent.AXIS_HAT_Y;
        context.controllerNumber = controllerNumber;
        context.assignedControllerNumber = true;
        context.external = false;
        // Some devices (GPD XD) send back with device ID == 0; ignore those on the default context.
        context.ignoreBack = true;
    }

    void stopHandler() {
        if (handler.stopped) return;
        handler.stopped = true;
        handler.presenceListener = null;
        handler.inputManager.unregisterInputDeviceListener(handler);
        for (ControllerHandler.InputDeviceContext deviceContext : handler.inputDeviceContexts.values()) {
            deviceContext.destroy();
        }
        // Waits for a USB rumble write under way; one that comes after sees the handler stopped.
        synchronized (handler.usbRumbleLock) {
            for (ControllerHandler.UsbDeviceContext deviceContext : handler.usbDeviceContexts.values()) {
                deviceContext.device.stop();
                deviceContext.destroy();
            }
        }
        handler.usbDeviceContexts.clear();
        handler.inputDeviceContexts.clear();
        handler.defaultContext.destroy();
        if (handler.handheldContext != handler.defaultContext) handler.handheldContext.destroy();
        handler.deviceVibratorForPlayerOne = false;
        handler.deviceVibrator.cancel();
    }

    void destroyHandler() {
        if (!handler.stopped) {
            handler.stop();
        }
        handler.mainThreadHandler.removeCallbacksAndMessages(null);
        handler.backgroundThreadHandler.removeCallbacksAndMessages(null);
        handler.sceManager.stop();
        handler.quitBackgroundThread();
        handler.motionSensorThread.quit();
        if (handler.slotStateHandle != 0L) {
            NightjarInputJni.slotStateDestroy(handler.slotStateHandle);
            handler.slotStateHandle = 0L;
        }
        if (handler.usbInputStateHandle != 0L) {
            NightjarInputJni.usbInputStateDestroy(handler.usbInputStateHandle);
            handler.usbInputStateHandle = 0L;
        }
        if (handler.rumbleStateHandle != 0L) {
            NightjarInputJni.rumbleStateDestroy(handler.rumbleStateHandle);
            handler.rumbleStateHandle = 0L;
        }
    }

    // A Binder call that can block for seconds (long enough for an ANR on a OnePlus Pad 2), so
    // never on the main thread and never holding a lock the main thread takes. Null: no battery.
    @SuppressLint("InlinedApi")
    ControllerHandler.BatteryReading readControllerBattery(ControllerHandler.InputDeviceContext context) {
        // Use the BatteryState object introduced in Android S, if it's available and present.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            BatteryState batteryState = context.inputDevice.getBatteryState();
            if (batteryState.isPresent()) {
                return new ControllerHandler.BatteryReading(batteryState.getStatus(), batteryState.getCapacity());
            }
        }

        if (handler.sceManager.isRecognizedDevice(context.inputDevice)) {
            // On the SHIELD Android TV, we can use a proprietary API to access battery/charge state.
            int batteryPercentage = handler.sceManager.getBatteryPercentage(context.inputDevice);
            SceConnectionType connectionType = handler.sceManager.getConnectionType(context.inputDevice);
            SceChargingState chargingState = handler.sceManager.getChargingState(context.inputDevice);
            int connType = connectionType == SceConnectionType.WIRED ? 1
                    : connectionType == SceConnectionType.WIRELESS ? 2
                    : connectionType == SceConnectionType.BOTH ? 3 : 0;
            int chargeState = chargingState == SceChargingState.NOT_CHARGING ? 1
                    : chargingState == SceChargingState.CHARGING ? 2 : 0;
            int status = NightjarInputJni.sceBatteryReading(batteryPercentage, connType, chargeState, sceCapOut);
            return new ControllerHandler.BatteryReading(status, sceCapOut[0]);
        }
        return null;
    }

    @SuppressLint("InlinedApi")
    void sendControllerBatteryPacket(ControllerHandler.InputDeviceContext context, ControllerHandler.BatteryReading battery) {
        if (!NightjarInputJni.evaluateBatteryUpdate(
                handler.prefConfig.sendControllers, battery.status, battery.capacity,
                context.lastReportedBatteryStatus, context.lastReportedBatteryCapacity, batteryOut)) {
            return;
        }
        handler.conn.sendControllerBatteryEvent((byte) context.controllerNumber, (byte) batteryOut[0], (byte) batteryOut[1]);
        context.lastReportedBatteryStatus = battery.status;
        context.lastReportedBatteryCapacity = battery.capacity;
    }

    void runBatteryStateUpdate(ControllerHandler.InputDeviceContext context, Runnable self) {
        synchronized (context) {
            if (context.destroyed || handler.stopped || !context.assignedControllerNumber) {
                return;
            }
        }
        // Outside the lock: destroy() takes it on the main thread, and this read can block.
        ControllerHandler.BatteryReading battery = readControllerBattery(context);
        synchronized (context) {
            if (context.destroyed || handler.stopped) {
                return;
            }
            if (battery != null) {
                handler.sendControllerBatteryPacket(context, battery);
            }

            // Requeue the callback
            handler.backgroundThreadHandler.postDelayed(self, ControllerHandler.BATTERY_RECHECK_INTERVAL_MS);
        }
    }

    @TargetApi(Build.VERSION_CODES.S)
    void runLedStateUpdate(ControllerHandler.InputDeviceContext context) {
        LightsManager.LightsSession session;
        synchronized (context) {
            if (context.destroyed || handler.stopped) {
                return;
            }
            session = context.lightsSession;
        }
        // Binder calls like the battery read, so outside the lock destroy() takes.
        try {
            LightState lightState = new LightState.Builder().setColor(context.ledArgbValue).build();

            // Set the RGB value for each RGB-controllable LED on the device
            boolean hasRgbLight = false;
            LightsRequest.Builder lightsRequestBuilder = new LightsRequest.Builder();
            for (Light light : context.inputDevice.getLightsManager().getLights()) {
                if (light.hasRgbControl()) {
                    lightsRequestBuilder.addLight(light, lightState);
                    hasRgbLight = true;
                }
            }

            if (hasRgbLight) {
                // Create a new light session if one doesn't already exist
                if (session == null) {
                    session = context.inputDevice.getLightsManager().openSession();
                    boolean destroyedMeanwhile;
                    synchronized (context) {
                        destroyedMeanwhile = context.destroyed || handler.stopped;
                        if (!destroyedMeanwhile) {
                            context.lightsSession = session;
                        }
                    }
                    if (destroyedMeanwhile) {
                        session.close();
                        return;
                    }
                }

                // Apply the LED changes
                session.requestLights(lightsRequestBuilder.build());
            }
        } catch (RuntimeException e) {
            LimeLog.warning("Controller lights unavailable after detach");
        }
    }

    void destroyInputDeviceContext(ControllerHandler.InputDeviceContext context) {
        context.destroyed = true;
        for (Runnable r : context.pendingButtonReleases.values()) {
            handler.mainThreadHandler.removeCallbacks(r);
        }
        context.pendingButtonReleases.clear();
        context.destroyGenericBase();

        context.cancelMotors();

        handler.mainThreadHandler.removeCallbacks(context.enableSensorRunnable);
        handler.backgroundThreadHandler.removeCallbacks(context.setLedStateRunnable);

        if (context.gyroListener != null) {
            handler.motion.closeMotionListener(context.sensorManager, context.gyroListener);
        }
        if (context.accelListener != null) {
            handler.motion.closeMotionListener(context.sensorManager, context.accelListener);
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            if (context.lightsSession != null) {
                try {
                    context.lightsSession.close();
                } catch (RuntimeException e) {
                    LimeLog.warning("Error closing LightsSession: " + e.getMessage());
                }
            }
        }

        handler.backgroundThreadHandler.removeCallbacks(context.batteryStateUpdateRunnable);
    }

    void sendInputDeviceArrival(ControllerHandler.InputDeviceContext context) {
        byte type;
        switch (context.inputDevice.getVendorId()) {
            case 0x045e: // Microsoft
                type = MoonBridge.LI_CTYPE_XBOX;
                break;
            case 0x054c: // Sony
                type = MoonBridge.LI_CTYPE_PS;
                break;
            case 0x057e: // Nintendo
                type = MoonBridge.LI_CTYPE_NINTENDO;
                break;
            default:
                // Consult SDL's controller type list to see if it knows
                type = MoonBridge.guessControllerType(context.inputDevice.getVendorId(), context.inputDevice.getProductId());
                break;
        }

        int[] keyCodes = ControllerHandler.androidButtonKeycodes();
        boolean[] hasKeys = new boolean[keyCodes.length];
        for (int i = 0; i < keyCodes.length; i++) {
            hasKeys[i] = context.inputDevice.hasKeys(keyCodes[i])[0];
        }
        boolean hasHatX = ControllerDeviceClassifier.getMotionRangeForJoystickAxis(context.inputDevice, MotionEvent.AXIS_HAT_X) != null;
        boolean hasHatY = ControllerDeviceClassifier.getMotionRangeForJoystickAxis(context.inputDevice, MotionEvent.AXIS_HAT_Y) != null;
        boolean hasTouchpadSource =
                (context.inputDevice.getSources() & InputDevice.SOURCE_TOUCHPAD) == InputDevice.SOURCE_TOUCHPAD;
        boolean hasClickpad = hasTouchpadSource && ControllerDeviceClassifier.hasButtonUnderTouchpad(context.inputDevice, type);
        int baseSupportedButtons = NightjarInputJni.computeBaseSupportedButtons(
                hasKeys, context.hasPaddles, context.hasShare, context.buttonMap.targetButtonFlags(),
                hasHatX, hasHatY, hasClickpad);

        short capabilities = 0;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            if (context.quadVibrators) {
                capabilities |= MoonBridge.LI_CCAP_RUMBLE | MoonBridge.LI_CCAP_TRIGGER_RUMBLE;
            }
            else if (context.vibratorManager != null || context.vibrator != null) {
                capabilities |= MoonBridge.LI_CCAP_RUMBLE;
            }
            if (context.external) {
                capabilities |= MoonBridge.LI_CCAP_BATTERY_STATE;
            }
            if (context.hasRgbLed) {
                capabilities |= MoonBridge.LI_CCAP_RGB_LED;
            }
        }
        if (context.leftTriggerAxis != -1 || context.rightTriggerAxis != -1) {
            capabilities |= MoonBridge.LI_CCAP_ANALOG_TRIGGERS;
        }
        if (context.sensorManager != null && context.sensorManager.getDefaultSensor(Sensor.TYPE_ACCELEROMETER) != null) {
            capabilities |= MoonBridge.LI_CCAP_ACCEL;
        }
        if (context.sensorManager != null && context.sensorManager.getDefaultSensor(Sensor.TYPE_GYROSCOPE) != null) {
            capabilities |= MoonBridge.LI_CCAP_GYRO;
        }
        if (context.vibrator != null) {
            capabilities |= MoonBridge.LI_CCAP_RUMBLE;
        }
        if (handler.sceManager.isRecognizedDevice(context.inputDevice)) {
            capabilities |= MoonBridge.LI_CCAP_RUMBLE | MoonBridge.LI_CCAP_BATTERY_STATE;
        }
        if (hasTouchpadSource) {
            capabilities |= MoonBridge.LI_CCAP_TOUCHPAD;
        }

        int[] arrivalOut = new int[4];
        NightjarInputJni.computeInputDeviceArrival(
                type, baseSupportedButtons, capabilities, context.sensorManager != null,
                context.buttonMap.connectAs(), context.buttonMap.targetButtonFlags(),
                handler.slots.carriesTouchpadButton(context), handler.steamControlsSession, handler.onscreenOwnPlayerSession,
                context.controllerNumber, arrivalOut);
        context.reportedType = (byte) arrivalOut[0];
        context.reportedCapabilities = (short) arrivalOut[1];
        context.supportedButtonFlags = arrivalOut[2];
        int arrivalFlags = arrivalOut[3];
        if ((arrivalFlags & 1) != 0) {
            handler.mainThreadHandler.post(() -> Toast.makeText(handler.activityContext,
                    R.string.toast_controller_type_changed, Toast.LENGTH_LONG).show());
        }
        if ((arrivalFlags & 2) != 0) {
            context.needsClickpadEmulation = (arrivalFlags & 4) != 0;
        }
        capabilities = context.reportedCapabilities;
        // A built-in device sharing player 1 with an external pad stays quiet, so the host
        // keeps the external pad's type, touchpad and motion.
        if (!context.external && handler.slots.sharesSlotWithExternalPad(context)) return;
        if (!handler.prefConfig.sendControllers) return;
        // Which controller it is goes first, so the host can choose its pad on arrival.
        handler.conn.sendControllerIdentityEvent((byte) context.controllerNumber, context.vendorId, context.productId);
        handler.conn.sendControllerArrivalEvent((byte) context.controllerNumber, handler.getActiveControllerMask(),
                context.reportedType, context.supportedButtonFlags, capabilities);

        // After reporting arrival to the host, send initial battery state and begin monitoring
        handler.backgroundThreadHandler.removeCallbacks(context.batteryStateUpdateRunnable);
        handler.backgroundThreadHandler.post(context.batteryStateUpdateRunnable);
    }

    void migrateInputDeviceContext(ControllerHandler.InputDeviceContext target,
                                   ControllerHandler.InputDeviceContext oldContext) {
        for (Runnable r : new ArrayList<>(oldContext.pendingButtonReleases.values())) {
            handler.mainThreadHandler.removeCallbacks(r);
            r.run();
        }
        target.reportedType = oldContext.reportedType;
        target.reportedCapabilities = oldContext.reportedCapabilities;
        target.supportedButtonFlags = oldContext.supportedButtonFlags;
        target.handheldControls = oldContext.handheldControls;
        target.inputMap = oldContext.inputMap;
        target.leftTrigger = oldContext.leftTrigger;
        target.rightTrigger = oldContext.rightTrigger;
        target.leftStickX = oldContext.leftStickX;
        target.leftStickY = oldContext.leftStickY;
        target.rightStickX = oldContext.rightStickX;
        target.rightStickY = oldContext.rightStickY;
        target.pressedButtons.putAll(oldContext.pressedButtons);
        target.startDownTime = oldContext.startDownTime;
        target.lastLbUpTime = oldContext.lastLbUpTime;
        target.lastRbUpTime = oldContext.lastRbUpTime;
        target.emulatingButtonFlags = oldContext.emulatingButtonFlags;
        target.pendingExit = oldContext.pendingExit;
        target.hasSelect |= oldContext.hasSelect;
        target.hasMode |= oldContext.hasMode;
        target.leftTriggerAxisUsed = oldContext.leftTriggerAxisUsed && target.leftTriggerAxis == oldContext.leftTriggerAxis;
        target.rightTriggerAxisUsed = oldContext.rightTriggerAxisUsed && target.rightTriggerAxis == oldContext.rightTriggerAxis;
        target.hatXAxisUsed = oldContext.hatXAxisUsed && target.hatXAxis == oldContext.hatXAxis;
        target.hatYAxisUsed = oldContext.hatYAxisUsed && target.hatYAxis == oldContext.hatYAxis;
        target.mouseEmulationActive = oldContext.mouseEmulationActive;
        int oldMouseLastInputMap;
        boolean oldMouseFixedStepHeld;
        int oldMouseStep;
        long oldMouseLastTickNs;
        final MouseDeltaAccumulator oldMoveX = new MouseDeltaAccumulator();
        final MouseDeltaAccumulator oldMoveY = new MouseDeltaAccumulator();
        final MouseDeltaAccumulator oldScrollV = new MouseDeltaAccumulator();
        final MouseDeltaAccumulator oldScrollH = new MouseDeltaAccumulator();
        synchronized (oldContext.mousePacketLock) {
            oldMouseLastInputMap = oldContext.mouseEmulationLastInputMap;
            oldMouseFixedStepHeld = oldContext.mouseEmulationFixedStepHeld;
            oldMouseStep = oldContext.mouseEmulationStep;
            oldMouseLastTickNs = oldContext.mouseEmulationLastTickNs;
            oldMoveX.copyFrom(oldContext.mouseMoveAccumX);
            oldMoveY.copyFrom(oldContext.mouseMoveAccumY);
            oldScrollV.copyFrom(oldContext.mouseScrollVAccum);
            oldScrollH.copyFrom(oldContext.mouseScrollHAccum);
            oldContext.mouseEmulationLastInputMap = 0;
            oldContext.mouseEmulationFixedStepHeld = false;
        }
        synchronized (target.mousePacketLock) {
            target.mouseEmulationLastInputMap = oldMouseLastInputMap;
            target.mouseEmulationFixedStepHeld = oldMouseFixedStepHeld;
            target.mouseEmulationStep = oldMouseStep;
            target.mouseEmulationLastTickNs = oldMouseLastTickNs;
            target.mouseMoveAccumX.copyFrom(oldMoveX);
            target.mouseMoveAccumY.copyFrom(oldMoveY);
            target.mouseScrollVAccum.copyFrom(oldScrollV);
            target.mouseScrollHAccum.copyFrom(oldScrollH);
        }
        int oldGuideBits = oldContext.guideHold.stateBits();
        android.view.KeyEvent oldGuideHeldDown = oldContext.guideHeldDown;
        boolean oldGuideTapping = oldContext.guideTapping;
        boolean oldQuitComboPending = oldContext.quitComboPending;
        int oldQuitButtons = oldContext.quitButtons;
        boolean oldGuideTapPending = oldContext.guideTapEnd != null;
        oldContext.guideHeldDown = null;

        target.lowFreqMotor = oldContext.lowFreqMotor;
        target.highFreqMotor = oldContext.highFreqMotor;
        target.leftTriggerMotor = oldContext.leftTriggerMotor;
        target.rightTriggerMotor = oldContext.rightTriggerMotor;

        synchronized (oldContext) {
            // Take ownership of the sensor and light sessions
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                target.lightsSession = oldContext.lightsSession;
                oldContext.lightsSession = null;
            }
            target.ledArgbValue = oldContext.ledArgbValue;
            target.gyroReportRateHz = oldContext.gyroReportRateHz;
            target.accelReportRateHz = oldContext.accelReportRateHz;

            // Don't release the controller number, because we will carry it over if it is present.
            // We also want to make sure the change is invisible to the host PC to avoid an add/remove
            // cycle for the gamepad which may break some games.
            oldContext.destroy();
        }

        // Copy over existing controller number state
        target.assignedControllerNumber = oldContext.assignedControllerNumber;
        target.reservedControllerNumber = oldContext.reservedControllerNumber;
        target.controllerNumber = oldContext.controllerNumber;

        target.guideHold.setStateBits(oldGuideBits);
        target.guideHeldDown = oldGuideHeldDown;
        target.guideTapping = oldGuideTapping;
        target.quitComboPending = oldQuitComboPending;
        target.quitButtons = oldQuitButtons;
        if (oldGuideBits == 1 && handler.prefConfig.guideMenuHoldMs > 0 && !handler.stopped) {
            long elapsed = oldGuideHeldDown != null
                    ? Math.max(0L, android.os.SystemClock.uptimeMillis() - oldGuideHeldDown.getEventTime()) : 0L;
            long remaining = Math.max(0L, handler.prefConfig.guideMenuHoldMs - elapsed);
            handler.mainThreadHandler.postDelayed(target.guideHoldRunnable, remaining);
        }
        if (oldGuideTapPending && !handler.stopped) {
            Runnable end = new Runnable() {
                @Override public void run() {
                    if (target.guideTapEnd != this) return;
                    target.guideTapEnd = null;
                    target.guideTapping = false;
                    if (!handler.stopped) {
                        target.inputMap &= ~com.limelight.nvstream.input.ControllerPacket.SPECIAL_BUTTON_FLAG;
                        handler.sendControllerInputPacket(target);
                    }
                }
            };
            target.guideTapEnd = end;
            handler.mainThreadHandler.postDelayed(end, ControllerHandler.MINIMUM_BUTTON_DOWN_TIME_MS);
        }

        if (target.mouseEmulationActive) {
            handler.mainThreadHandler.postDelayed(target.mouseEmulationRunnable, target.mouseEmulationReportPeriod);
        }

        // We may have set this device to use the built-in sensor manager. If so, do that again.
        if (oldContext.sensorManager == handler.deviceSensorManager) {
            target.sensorManager = handler.deviceSensorManager;
        }

        // Copy state initialized in reportControllerArrival()
        target.needsClickpadEmulation = oldContext.needsClickpadEmulation;

        // Re-enable sensors on the new context
        target.enableSensors();

        // Refresh the LED state if it's set. We don't know if there was a pending LED state
        // callback when the context was migrated, so we'll reset it just to be safe. The old
        // context may still have been opening its session, which it closes when it finds itself
        // destroyed, so a colour without a session counts too.
        if (target.lightsSession != null || target.ledArgbValue != 0) {
            handler.backgroundThreadHandler.post(target.setLedStateRunnable);
        }

        handler.rumble.reapplyMotors(target);

        // Refresh battery state and start the battery state polling again
        handler.backgroundThreadHandler.post(target.batteryStateUpdateRunnable);
    }

    short usbCapabilities(ControllerHandler.UsbDeviceContext context) {
        short capabilities = ControllerHandler.arrivalCapabilities(context.device.getCapabilities(), context.buttonMap);
        if (context.deviceSensors != null && context.deviceSensors.getDefaultSensor(Sensor.TYPE_ACCELEROMETER) != null) {
            capabilities |= MoonBridge.LI_CCAP_ACCEL;
        }
        if (context.deviceSensors != null && context.deviceSensors.getDefaultSensor(Sensor.TYPE_GYROSCOPE) != null) {
            capabilities |= MoonBridge.LI_CCAP_GYRO;
        }
        return capabilities;
    }

    void sendUsbDeviceArrival(ControllerHandler.UsbDeviceContext context) {
        if (!handler.conn.canSendControllerInput()) {
            // The slot is already reserved. Keep its capabilities ahead of any
            // input if permission is restored, with the same bounded retry rate.
            context.arrivalPending = true;
            context.arrivalRetryAt = android.os.SystemClock.uptimeMillis() + 1000;
            return;
        }
        short capabilities = context.capabilities();
        if ((capabilities & MoonBridge.LI_CCAP_HAPTICS_PCM) != 0) {
            handler.waveformControllers.put((short) context.controllerNumber, context.device);
        }
        if (!handler.prefConfig.gamepadMotionSensors) {
            capabilities &= ~(MoonBridge.LI_CCAP_ACCEL | MoonBridge.LI_CCAP_GYRO);
        }
        byte type = context.arrivalType();
        if (!handler.prefConfig.sendControllers) return;
        handler.conn.sendControllerIdentityEvent((byte) context.controllerNumber, context.vendorId, context.productId);
        int result = handler.conn.sendControllerArrivalEvent((byte) context.controllerNumber, handler.getActiveControllerMask(),
                type, handler.slots.withSteamControlButtons((short) context.controllerNumber,
                        context.device.getSupportedButtonFlags() | context.buttonMap.targetButtonFlags()),
                handler.slots.withSteamControls((short) context.controllerNumber, capabilities, context.buttonMap));
        if (result != 0) {
            if (!context.arrivalPending) {
                LimeLog.warning("USB controller announcement deferred: slot=" + context.controllerNumber + ", rc=" + result);
            }
            context.arrivalPending = true;
            context.arrivalRetryAt = android.os.SystemClock.uptimeMillis() + 1000;
            return;
        }
        context.arrivalPending = false;
        LimeLog.info("USB controller announcement queued: slot=" + context.controllerNumber +
                ", type=" + type + ", capabilities=0x" + Integer.toHexString(capabilities & 0xffff));
        context.device.setPlayerNumber(context.controllerNumber);
        context.device.bindAudio(handler.conn, context.controllerNumber);
    }
}
