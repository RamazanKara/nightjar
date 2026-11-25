package com.limelight.binding.input;

import android.annotation.TargetApi;
import android.media.AudioAttributes;
import android.os.Build;
import android.os.CombinedVibration;
import android.os.VibrationAttributes;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.os.VibratorManager;

import com.limelight.binding.input.driver.AbstractController;
import com.limelight.nvstream.jni.NightjarInputJni;

import java.util.List;

/**
 * Routes motor and trigger rumble requests from the host to Android {@link VibratorManager} /
 * {@link Vibrator} motors, NVIDIA SHIELD controllers, USB drivers, and phone-vibrator fallback
 * with focus pause/resume handling for {@link ControllerHandler}.
 */
final class ControllerRumbleRouter {
    private final ControllerHandler handler;
    private final int[] rumblePauseOut = new int[81];
    private final int[] rumbleAmpOut = new int[4];
    private final long[] rumbleSingleOut = new long[3];
    // Under handler.rumbleLock: while the stream has no focus the motors stay off, and the host's latest
    // levels per controller number ({low, high, left trigger, right trigger}) come back with focus.
    private boolean rumblePaused;

    ControllerRumbleRouter(ControllerHandler handler) {
        this.handler = handler;
    }

    /**
     * Without focus the pads and the phone stop vibrating instead of running on until the game sends
     * a zero; with focus back the host's latest levels play again.
     */
    void pauseRumble(boolean paused) {
        synchronized (handler.rumbleLock) {
            int count = NightjarInputJni.rumbleSetPaused(handler.rumbleStateHandle, paused, handler.stopped, rumblePauseOut);
            int kind = rumblePauseOut[0];
            if (kind == 0) {
                return;
            }
            rumblePaused = paused;
            if (kind == 1) {
                for (ControllerHandler.InputDeviceContext deviceContext : handler.inputDeviceContexts.values()) {
                    deviceContext.cancelMotors();
                }
                for (int i = 0; i < count; i++) {
                    short controllerNumber = (short) rumblePauseOut[1 + i];
                    List<AbstractController> usbPads = handler.usb.usbPadsOf(controllerNumber);
                    if (usbPads == null || usbPads.isEmpty()) continue;
                    synchronized (handler.usbRumbleLock) {
                        if (handler.stopped) return;
                        for (AbstractController pad : usbPads) {
                            pad.rumble((short) 0, (short) 0);
                            pad.rumbleTriggers((short) 0, (short) 0);
                        }
                    }
                }
                handler.deviceVibrator.cancel();
            }
            else if (kind == 2) {
                for (int i = 0; i < count; i++) {
                    int base = 1 + i * 5;
                    short slot = (short) rumblePauseOut[base];
                    short low = (short) rumblePauseOut[base + 1];
                    short high = (short) rumblePauseOut[base + 2];
                    short lt = (short) rumblePauseOut[base + 3];
                    short rt = (short) rumblePauseOut[base + 4];
                    if (low != 0 || high != 0) rumble(slot, low, high);
                    if (lt != 0 || rt != 0) rumbleTriggers(slot, lt, rt);
                }
            }
        }
    }

    private short[] levelsOf(short controllerNumber) {
        short[] levels = handler.rumbleLevels.get(controllerNumber);
        if (levels == null) {
            levels = new short[4];
            handler.rumbleLevels.put(controllerNumber, levels);
        }
        return levels;
    }

    /** A motor level after the chosen rumble feel. */
    private short feel(short level) {
        return RumblePolicy.shapeMotor(level, handler.rumbleFeel);
    }

    // This must only be called if hasDualAmplitudeControlledRumbleVibrators() is true!
    @TargetApi(31)
    private void rumbleDualVibrators(VibratorManager vm, short lowFreqMotor, short highFreqMotor) {
        if (!NightjarInputJni.rumblePlanDual(lowFreqMotor, highFreqMotor, handler.rumbleFeel.ordinal(), rumbleAmpOut)) {
            vm.cancel();
            return;
        }

        int[] vibratorIds = vm.getVibratorIds();
        CombinedVibration.ParallelCombination combo = CombinedVibration.startParallel();
        for (int i = 0; i < vibratorIds.length; i++) {
            if (rumbleAmpOut[i] != 0) {
                combo.addVibrator(vibratorIds[i], VibrationEffect.createOneShot(60000, rumbleAmpOut[i]));
            }
        }

        VibrationAttributes.Builder vibrationAttributes = new VibrationAttributes.Builder();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            vibrationAttributes.setUsage(VibrationAttributes.USAGE_MEDIA);
        }
        vm.vibrate(combo.combine(), vibrationAttributes.build());
    }

    // This must only be called if hasQuadAmplitudeControlledRumbleVibrators() is true!
    @TargetApi(31)
    private void rumbleQuadVibrators(VibratorManager vm, short lowFreqMotor, short highFreqMotor, short leftTrigger, short rightTrigger) {
        if (!NightjarInputJni.rumblePlanQuad(lowFreqMotor, highFreqMotor, leftTrigger, rightTrigger, handler.rumbleFeel.ordinal(), rumbleAmpOut)) {
            vm.cancel();
            return;
        }

        int[] vibratorIds = vm.getVibratorIds();
        CombinedVibration.ParallelCombination combo = CombinedVibration.startParallel();
        for (int i = 0; i < vibratorIds.length; i++) {
            if (rumbleAmpOut[i] != 0) {
                combo.addVibrator(vibratorIds[i], VibrationEffect.createOneShot(60000, rumbleAmpOut[i]));
            }
        }

        VibrationAttributes.Builder vibrationAttributes = new VibrationAttributes.Builder();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            vibrationAttributes.setUsage(VibrationAttributes.USAGE_MEDIA);
        }
        vm.vibrate(combo.combine(), vibrationAttributes.build());
    }

    private void rumbleSingleVibrator(Vibrator vibrator, short lowFreqMotor, short highFreqMotor) {
        int simulatedAmplitude = NightjarInputJni.rumblePlanSingle(
                lowFreqMotor, highFreqMotor, handler.rumbleFeel.ordinal(), 100, rumbleSingleOut);
        vibrateSinglePlan(vibrator, simulatedAmplitude, rumbleSingleOut[0], rumbleSingleOut[1]);
    }

    private void vibrateSinglePlan(Vibrator vibrator, int simulatedAmplitude, long onTime, long offTime) {
        if (simulatedAmplitude == 0) {
            vibrator.cancel();
            return;
        }

        // Attempt to use amplitude-based control if we're on Oreo and the device
        // supports amplitude-based vibration control.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            if (vibrator.hasAmplitudeControl()) {
                VibrationEffect effect = VibrationEffect.createOneShot(60000, simulatedAmplitude);
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    VibrationAttributes vibrationAttributes = new VibrationAttributes.Builder()
                            .setUsage(VibrationAttributes.USAGE_MEDIA)
                            .build();
                    vibrator.vibrate(effect, vibrationAttributes);
                }
                else {
                    AudioAttributes audioAttributes = new AudioAttributes.Builder()
                            .setUsage(AudioAttributes.USAGE_GAME)
                            .build();
                    vibrator.vibrate(effect, audioAttributes);
                }
                return;
            }
        }

        // If we reach this point, we don't have amplitude controls available, so
        // we must emulate it by PWMing the vibration. Ick.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            VibrationAttributes vibrationAttributes = new VibrationAttributes.Builder()
                    .setUsage(VibrationAttributes.USAGE_MEDIA)
                    .build();
            vibrator.vibrate(VibrationEffect.createWaveform(new long[]{0, onTime, offTime}, 0), vibrationAttributes);
        }
        else {
            AudioAttributes audioAttributes = new AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_GAME)
                    .build();
            vibrator.vibrate(new long[]{0, onTime, offTime}, 0, audioAttributes);
        }
    }

    void handleRumble(short controllerNumber, short lowFreqMotor, short highFreqMotor) {
        synchronized (handler.rumbleLock) {
            rumble(controllerNumber, lowFreqMotor, highFreqMotor);
        }
    }

    private void rumble(short controllerNumber, short lowFreqMotor, short highFreqMotor) {
        boolean foundMatchingDevice = false;
        boolean vibrated = false;

        int state = NightjarInputJni.rumbleRecordMotor(
                handler.rumbleStateHandle, controllerNumber, lowFreqMotor, highFreqMotor, handler.stopped);
        if ((state & 1) == 0) {
            return;
        }
        boolean active = (state & 2) != 0;
        short[] levels = levelsOf(controllerNumber);
        levels[0] = lowFreqMotor;
        levels[1] = highFreqMotor;

        for (ControllerHandler.InputDeviceContext deviceContext : handler.inputDeviceContexts.values()) {

            if (deviceContext.assignedControllerNumber && deviceContext.controllerNumber == controllerNumber) {
                foundMatchingDevice = true;

                // Also while rumble is paused: a trigger update right after it resumes re-sends both
                // motor levels, and must not bring back the ones from before the pause.
                deviceContext.lowFreqMotor = lowFreqMotor;
                deviceContext.highFreqMotor = highFreqMotor;
                if (!active) continue;

                if (applyInputDeviceRumble(deviceContext)) {
                    vibrated = true;
                }
            }
        }
        if (!active) return;

        List<AbstractController> usbPads = handler.usb.usbPadsOf(controllerNumber);
        if (usbPads == null) {
            return;
        }
        if (!usbPads.isEmpty()) {
            foundMatchingDevice = vibrated = true;
            synchronized (handler.usbRumbleLock) {
                if (handler.stopped) {
                    return;
                }
                for (AbstractController pad : usbPads) {
                    pad.rumble(feel(lowFreqMotor), feel(highFreqMotor));
                }
            }
        }

        int route = NightjarInputJni.rumbleDeviceVibratorRoute(
                handler.rumbleStateHandle, controllerNumber, lowFreqMotor, highFreqMotor,
                foundMatchingDevice, vibrated, handler.prefConfig.onscreenController,
                handler.prefConfig.onlyL3R3, handler.prefConfig.vibrateOsc,
                handler.prefConfig.vibrateFallbackToDevice, handler.prefConfig.vibrateFallbackToDeviceStrength,
                rumbleSingleOut);
        if (route == 1) {
            vibrateSinglePlan(handler.deviceVibrator, (int) rumbleSingleOut[0], rumbleSingleOut[1], rumbleSingleOut[2]);
            if (handler.stopped) handler.deviceVibrator.cancel();
        }
        else if (route == 2) {
            handler.deviceVibratorForPlayerOne = true;
            vibrateSinglePlan(handler.deviceVibrator, (int) rumbleSingleOut[0], rumbleSingleOut[1], rumbleSingleOut[2]);
            if (handler.stopped) handler.deviceVibrator.cancel();
        }
    }

    private boolean applyInputDeviceRumble(ControllerHandler.InputDeviceContext deviceContext) {
        boolean applied = false;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && deviceContext.vibratorManager != null) {
            if (deviceContext.quadVibrators) {
                rumbleQuadVibrators(deviceContext.vibratorManager,
                        deviceContext.lowFreqMotor, deviceContext.highFreqMotor,
                        deviceContext.leftTriggerMotor, deviceContext.rightTriggerMotor);
            }
            else {
                rumbleDualVibrators(deviceContext.vibratorManager,
                        deviceContext.lowFreqMotor, deviceContext.highFreqMotor);
            }
            applied = true;
        }
        else if (handler.sceManager.rumble(deviceContext.inputDevice,
                feel(deviceContext.lowFreqMotor), feel(deviceContext.highFreqMotor))) {
            applied = true;
        }
        else if (deviceContext.vibrator != null) {
            rumbleSingleVibrator(deviceContext.vibrator,
                    deviceContext.lowFreqMotor, deviceContext.highFreqMotor);
            applied = true;
        }
        if (handler.stopped) deviceContext.cancelMotors();
        return applied;
    }

    void reapplyMotors(ControllerHandler.InputDeviceContext deviceContext) {
        if (deviceContext == null) return;
        synchronized (handler.rumbleLock) {
            if (rumblePaused || handler.stopped) return;
            if (deviceContext.lowFreqMotor != 0 || deviceContext.highFreqMotor != 0
                    || (deviceContext.quadVibrators && (deviceContext.leftTriggerMotor != 0 || deviceContext.rightTriggerMotor != 0))) {
                applyInputDeviceRumble(deviceContext);
            }
        }
    }

    void handleRumbleTriggers(short controllerNumber, short leftTrigger, short rightTrigger) {
        synchronized (handler.rumbleLock) {
            rumbleTriggers(controllerNumber, leftTrigger, rightTrigger);
        }
    }

    private void rumbleTriggers(short controllerNumber, short leftTrigger, short rightTrigger) {
        int state = NightjarInputJni.rumbleRecordTriggers(
                handler.rumbleStateHandle, controllerNumber, leftTrigger, rightTrigger, handler.stopped);
        if ((state & 1) == 0) {
            return;
        }
        boolean active = (state & 2) != 0;
        short[] levels = levelsOf(controllerNumber);
        levels[2] = leftTrigger;
        levels[3] = rightTrigger;

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            for (ControllerHandler.InputDeviceContext deviceContext : handler.inputDeviceContexts.values()) {

                if (deviceContext.assignedControllerNumber && deviceContext.controllerNumber == controllerNumber) {
                    deviceContext.leftTriggerMotor = leftTrigger;
                    deviceContext.rightTriggerMotor = rightTrigger;
                    if (!active) continue;

                    if (deviceContext.quadVibrators) {
                        rumbleQuadVibrators(deviceContext.vibratorManager,
                                deviceContext.lowFreqMotor, deviceContext.highFreqMotor,
                                deviceContext.leftTriggerMotor, deviceContext.rightTriggerMotor);
                        if (handler.stopped) deviceContext.cancelMotors();
                    }
                }
            }
        }
        if (!active) {
            return;
        }

        List<AbstractController> usbPads = handler.usb.usbPadsOf(controllerNumber);
        if (usbPads == null || usbPads.isEmpty()) {
            return;
        }
        synchronized (handler.usbRumbleLock) {
            if (handler.stopped) {
                return;
            }
            for (AbstractController pad : usbPads) {
                pad.rumbleTriggers(feel(leftTrigger), feel(rightTrigger));
            }
        }
    }
}
