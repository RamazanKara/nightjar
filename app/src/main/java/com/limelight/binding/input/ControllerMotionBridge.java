package com.limelight.binding.input;

import android.content.Context;
import android.hardware.Sensor;
import android.hardware.SensorEvent;
import android.hardware.SensorEventListener;
import android.hardware.SensorManager;
import android.hardware.display.DisplayManager;
import android.os.Looper;
import android.view.Display;

import com.limelight.LimeLog;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarInputJni;

/**
 * Manages controller and fallback-device motion sensors, display orientation tracking, gyro
 * liveness checks, and host motion state updates for {@link ControllerHandler}.
 */
final class ControllerMotionBridge {
    private final ControllerHandler handler;
    // The activity display's rotation for device-orientation motion, read per sample without a Display lookup.
    private final DeviceRotation displayRotation = new DeviceRotation();

    ControllerMotionBridge(ControllerHandler handler) {
        this.handler = handler;
    }

    /** Stops a pad's rotation on the host, ordered after any sensor sample being sent. */
    void sendGyroAtRest(short controllerNumber) {
        synchronized (handler.motionSendLock) {
            handler.conn.sendControllerMotionEvent((byte) controllerNumber, MoonBridge.LI_MOTION_TYPE_GYRO, 0, 0, 0);
        }
    }

    /** Unregisters a motion listener so that no sample of it reaches the host from here on. */
    void closeMotionListener(SensorManager sensors, MotionListener listener) {
        // Closed first: a sample already on the sensor thread finds the listener closed, since
        // unregistering alone does not hold back one that was delivered just before.
        listener.close();
        sensors.unregisterListener(listener);
        displayRotation.release(listener);
    }

    /** Registers a motion listener; one that maps device orientation keeps the rotation current while it is registered. */
    boolean registerMotionListener(SensorManager sensors, Sensor sensor, MotionListener listener,
                                   int samplingPeriodUs) {
        // Acquired before registering, so the first sample already finds the rotation read.
        if (listener.needsDeviceOrientationCorrection) displayRotation.acquire(listener);
        boolean registered = handler.motionSensorThread.register(sensors, sensor, listener, samplingPeriodUs);
        if (!registered) displayRotation.release(listener);
        return registered;
    }

    /** atRest: also tell the PC each gyro is still, unless the caller's release has just done so. */
    void stopSensors(boolean atRest) {
        handler.sensorsEnabled = false;
        for (ControllerHandler.InputDeviceContext deviceContext : handler.inputDeviceContexts.values()) {
            deviceContext.disableSensors(atRest);
        }
        for (ControllerHandler.UsbDeviceContext context : handler.usbDeviceContexts.values()) {
            unregisterUsbDeviceMotion(context);
            if (atRest && context.assignedControllerNumber && (context.capabilities() & MoonBridge.LI_CCAP_GYRO) != 0) {
                sendGyroAtRest(context.controllerNumber);
            }
        }
    }

    void enableSensors() {
        if (handler.stopped) {
            return;
        }

        handler.sensorsEnabled = true;

        for (ControllerHandler.InputDeviceContext deviceContext : handler.inputDeviceContexts.values()) {
            deviceContext.enableSensors();
        }
        for (ControllerHandler.UsbDeviceContext context : handler.usbDeviceContexts.values()) {
            registerUsbDeviceMotion(context, MoonBridge.LI_MOTION_TYPE_ACCEL);
            registerUsbDeviceMotion(context, MoonBridge.LI_MOTION_TYPE_GYRO);
        }
    }

    /** Whether a pad already streams this device's motion sensors, which only one pad may. */
    boolean deviceSensorsInUse() {
        if (handler.deviceSensorManager == null) return false;
        for (ControllerHandler.InputDeviceContext context : handler.inputDeviceContexts.values()) {
            if (context.sensorManager == handler.deviceSensorManager) return true;
        }
        for (ControllerHandler.UsbDeviceContext context : handler.usbDeviceContexts.values()) {
            if (context.deviceSensors != null) return true;
        }
        return false;
    }

    // Player 1's controller gyroscope gets a liveness check when its demand may move to the device
    // gyroscope, the way Moonlight V+ does it.
    boolean checksGyroLiveness(short controllerNumber, SensorManager controllerSensors) {
        return NightjarInputJni.checksGyroLiveness(
                controllerNumber, handler.deviceFallbackSlot, controllerSensors == handler.deviceSensorManager,
                handler.prefConfig.gamepadMotionSensorsFallbackToDevice,
                handler.deviceSensorManager != null && handler.deviceSensorManager.getDefaultSensor(Sensor.TYPE_GYROSCOPE) != null);
    }

    // A controller gyroscope that reports only zeros hands both motion sensors to the device's own.
    // The new context keeps the device sensors through a reconfiguration, like the plain fallback.
    void fallBackToDeviceSensors(ControllerHandler.InputDeviceContext context) {
        if (handler.stopped || context.destroyed || context.sensorManager == handler.deviceSensorManager) {
            return;
        }
        LimeLog.warning("Gyroscope of " + context.name + " reports only zeros; using the device's motion sensors");
        context.disableSensors();
        context.sensorManager = handler.deviceSensorManager;
        context.enableSensorRunnable.run();
    }

    MotionListener createSensorListener(final short controllerNumber, final byte motionType,
                                        final boolean needsDeviceOrientationCorrection,
                                        final ControllerHandler.InputDeviceContext livenessContext) {
        return createSensorListener(controllerNumber, motionType, needsDeviceOrientationCorrection, livenessContext, null);
    }

    /** {@code gyroMapping} is a controller's own gyroscope's mapping; null for accelerometers and this device's sensors. */
    MotionListener createSensorListener(final short controllerNumber, final byte motionType,
                                        final boolean needsDeviceOrientationCorrection,
                                        final ControllerHandler.InputDeviceContext livenessContext,
                                        final GyroAxisTransformer gyroMapping) {
        return new MotionListener(controllerNumber, motionType, needsDeviceOrientationCorrection, livenessContext,
                gyroMapping);
    }

    void handleSetMotionEventState(final short controllerNumber, final byte motionType, short reportRateHz) {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            final short rate = reportRateHz;
            handler.mainThreadHandler.post(() -> handleSetMotionEventState(controllerNumber, motionType, rate));
            return;
        }
        if (handler.stopped) {
            return;
        }

        for (ControllerHandler.UsbDeviceContext context : handler.usbDeviceContexts.values()) {
            if (context.assignedControllerNumber && context.controllerNumber == controllerNumber) {
                short rate = (short) NightjarInputJni.usbMotionRateHz(
                        reportRateHz, handler.prefConfig.gamepadMotionSensors,
                        ControllerHandler.dropsMotionAndTouchpad(context.buttonMap), context.deviceSensors != null);
                if (context.deviceSensors != null) {
                    setUsbDeviceMotion(context, motionType, rate);
                }
                else {
                    context.device.setMotionEventState(motionType, rate);
                }
            }
        }

        // Report rate is restricted to <= 200 Hz without the HIGH_SAMPLING_RATE_SENSORS permission
        reportRateHz = (short) NightjarInputJni.clampMotionRateHz(reportRateHz);

        for (ControllerHandler.InputDeviceContext deviceContext : handler.inputDeviceContexts.values()) {

            if (deviceContext.assignedControllerNumber && deviceContext.controllerNumber == controllerNumber) {
                // Store the desired report rate even if we don't have sensors. In some cases,
                // input devices can be reconfigured at runtime which results in a change where
                // sensors disappear and reappear. By storing the desired report rate, we can
                // reapply the desired motion sensor configuration after they reappear.
                switch (motionType) {
                    case MoonBridge.LI_MOTION_TYPE_ACCEL:
                        deviceContext.accelReportRateHz = reportRateHz;
                        break;
                    case MoonBridge.LI_MOTION_TYPE_GYRO:
                        deviceContext.gyroReportRateHz = reportRateHz;
                        break;
                }

                handler.mainThreadHandler.removeCallbacks(deviceContext.enableSensorRunnable);

                SensorManager sm = deviceContext.sensorManager;
                if (sm == null) {
                    continue;
                }

                switch (motionType) {
                    case MoonBridge.LI_MOTION_TYPE_ACCEL:
                        if (deviceContext.accelListener != null) {
                            closeMotionListener(sm, deviceContext.accelListener);
                            deviceContext.accelListener = null;
                        }

                        // Enable the accelerometer if requested
                        Sensor accelSensor = sm.getDefaultSensor(Sensor.TYPE_ACCELEROMETER);
                        if (handler.sensorsEnabled && reportRateHz != 0 && accelSensor != null) {
                            deviceContext.accelListener = createSensorListener(controllerNumber, motionType, sm == handler.deviceSensorManager, null);
                            registerMotionListener(sm, accelSensor, deviceContext.accelListener, NightjarInputJni.motionSamplingPeriodUs(reportRateHz));
                        }
                        break;
                    case MoonBridge.LI_MOTION_TYPE_GYRO:
                        if (deviceContext.gyroListener != null) {
                            closeMotionListener(sm, deviceContext.gyroListener);
                            deviceContext.gyroListener = null;
                        }

                        // Enable the gyroscope if requested
                        Sensor gyroSensor = sm.getDefaultSensor(Sensor.TYPE_GYROSCOPE);
                        if (handler.sensorsEnabled && reportRateHz != 0 && gyroSensor != null) {
                            deviceContext.gyroListener = createSensorListener(controllerNumber, motionType, sm == handler.deviceSensorManager,
                                    checksGyroLiveness(controllerNumber, sm) ? deviceContext : null,
                                    sm == handler.deviceSensorManager ? null
                                            : GyroMappingPreferences.transformer(handler.activityContext, deviceContext.inputDevice));
                            registerMotionListener(sm, gyroSensor, deviceContext.gyroListener, NightjarInputJni.motionSamplingPeriodUs(reportRateHz));
                        }
                        break;
                }
            }
        }
    }

    void disableInputContextSensors(ControllerHandler.InputDeviceContext context, boolean atRest) {
        // Stop any pending enablement
        handler.mainThreadHandler.removeCallbacks(context.enableSensorRunnable);

        // Unregister all sensor listeners
        if (context.gyroListener != null) {
            closeMotionListener(context.sensorManager, context.gyroListener);
            context.gyroListener = null;

            // Send a gyro event to ensure the virtual controller is stationary
            if (atRest) {
                sendGyroAtRest(context.controllerNumber);
            }
        }
        if (context.accelListener != null) {
            closeMotionListener(context.sensorManager, context.accelListener);
            context.accelListener = null;

            // We leave the acceleration as-is to preserve the attitude of the controller
        }
    }

    void setUsbDeviceMotion(ControllerHandler.UsbDeviceContext context, byte motionType, short reportRateHz) {
        if (motionType == MoonBridge.LI_MOTION_TYPE_GYRO) context.gyroReportRateHz = reportRateHz;
        else if (motionType == MoonBridge.LI_MOTION_TYPE_ACCEL) context.accelReportRateHz = reportRateHz;
        else return;
        registerUsbDeviceMotion(context, motionType);
    }

    /** Registers the device sensor for the motion type at the rate the host asked for, if any. */
    void registerUsbDeviceMotion(ControllerHandler.UsbDeviceContext context, byte motionType) {
        if (context.deviceSensors == null) return;
        boolean gyro = motionType == MoonBridge.LI_MOTION_TYPE_GYRO;
        MotionListener old = gyro ? context.gyroListener : context.accelListener;
        if (old != null) closeMotionListener(context.deviceSensors, old);
        MotionListener listener = null;
        short rate = gyro ? context.gyroReportRateHz : context.accelReportRateHz;
        Sensor sensor = context.deviceSensors.getDefaultSensor(gyro ? Sensor.TYPE_GYROSCOPE : Sensor.TYPE_ACCELEROMETER);
        if (!handler.stopped && handler.sensorsEnabled && rate != 0 && sensor != null) {
            listener = createSensorListener(context.controllerNumber, motionType, true, null);
            registerMotionListener(context.deviceSensors, sensor, listener, NightjarInputJni.motionSamplingPeriodUs(rate));
        }
        if (gyro) context.gyroListener = listener;
        else context.accelListener = listener;
    }

    void unregisterUsbDeviceMotion(ControllerHandler.UsbDeviceContext context) {
        if (context.deviceSensors == null) return;
        if (context.gyroListener != null) closeMotionListener(context.deviceSensors, context.gyroListener);
        if (context.accelListener != null) closeMotionListener(context.deviceSensors, context.accelListener);
        context.gyroListener = null;
        context.accelListener = null;
    }

    /**
     * The activity display's rotation. Held by the motion listeners that map device orientation:
     * read when the first one registers, kept current by a display listener that lives only while
     * one is registered. Display callbacks run on the main thread; the cached value is volatile.
     */
    private final class DeviceRotation implements DisplayManager.DisplayListener {
        private volatile int rotation;
        private volatile int displayId;
        private int holders; // guarded by this

        synchronized void acquire(MotionListener listener) {
            if (listener.holdsRotation) return;
            listener.holdsRotation = true;
            if (holders++ == 0) {
                Display display = handler.activityContext.getWindowManager().getDefaultDisplay();
                displayId = display.getDisplayId();
                rotation = display.getRotation();
                displayManager().registerDisplayListener(this, handler.mainThreadHandler);
            }
        }

        synchronized void release(MotionListener listener) {
            if (!listener.holdsRotation) return;
            listener.holdsRotation = false;
            if (--holders == 0) displayManager().unregisterDisplayListener(this);
        }

        int rotation() {
            return rotation;
        }

        private DisplayManager displayManager() {
            return (DisplayManager) handler.activityContext.getSystemService(Context.DISPLAY_SERVICE);
        }

        @Override
        public void onDisplayAdded(int addedDisplayId) {
        }

        @Override
        public void onDisplayRemoved(int removedDisplayId) {
        }

        @Override
        public void onDisplayChanged(int changedDisplayId) {
            // Read the activity's display again, not only the one cached: the window may have
            // moved to another display (Dual screen, a foldable's cover screen).
            Display display = handler.activityContext.getWindowManager().getDefaultDisplay();
            if (changedDisplayId == displayId || display.getDisplayId() != displayId) {
                displayId = display.getDisplayId();
                rotation = display.getRotation();
            }
        }
    }

    /**
     * One motion sensor of one pad. Its events arrive on {@link ControllerHandler#motionSensorThread},
     * in the order the sensor produced them, so everything here that the main thread also touches
     * is either volatile or under {@link ControllerHandler#motionSendLock}.
     */
    final class MotionListener implements SensorEventListener {
        private final short controllerNumber;
        private final byte motionType;
        private final boolean needsDeviceOrientationCorrection;
        private final ControllerHandler.InputDeviceContext livenessContext;
        private final float[] motionOut = new float[3];
        private volatile long filterHandle;
        // Set by closeMotionListener() on the thread that unregisters this listener.
        volatile boolean closed;
        // Whether this listener holds the display rotation; guarded by the DeviceRotation monitor.
        boolean holdsRotation;

        MotionListener(short controllerNumber, byte motionType, boolean needsDeviceOrientationCorrection,
                       ControllerHandler.InputDeviceContext livenessContext, GyroAxisTransformer gyroMapping) {
            this.controllerNumber = controllerNumber;
            this.motionType = motionType;
            this.needsDeviceOrientationCorrection = needsDeviceOrientationCorrection;
            this.livenessContext = livenessContext;
            GyroAxisTransformer map = gyroMapping != null ? gyroMapping : GyroAxisTransformer.identity();
            this.filterHandle = NightjarInputJni.motionFilterCreate(
                    motionType, needsDeviceOrientationCorrection, livenessContext != null,
                    gyroMapping != null, map.sourceAxes[0], map.sourceAxes[1], map.sourceAxes[2],
                    map.signs[0], map.signs[1], map.signs[2],
                    map.horizontalSensitivity, map.verticalSensitivity);
        }

        void close() {
            closed = true;
            long h = filterHandle;
            filterHandle = 0L;
            if (h != 0L) {
                NightjarInputJni.motionFilterDestroy(h);
            }
        }

        @Override
        public void onSensorChanged(SensorEvent sensorEvent) {
            if (closed || handler.stopped || !handler.sensorsEnabled || !handler.inputEnabled || !handler.prefConfig.sendControllers) {
                return;
            }
            int rotation = needsDeviceOrientationCorrection ? displayRotation.rotation() : 0;
            int step = NightjarInputJni.motionSampleStep(
                    filterHandle, sensorEvent.values[0], sensorEvent.values[1], sensorEvent.values[2],
                    sensorEvent.timestamp, rotation, GyroDiagnosticOverlay.isFlydigiForced(), motionOut);
            if ((step & 1) != 0) {
                handler.mainThreadHandler.post(() -> fallBackToDeviceSensors(livenessContext));
            }
            if ((step & 2) == 0) {
                return;
            }

            synchronized (handler.motionSendLock) {
                // Asked again under the lock: the main thread may have ended this pad's motion and
                // sent its stationary report since the check above, and this sample must not follow it.
                if (closed || handler.stopped || !handler.sensorsEnabled || !handler.inputEnabled || !handler.prefConfig.sendControllers) {
                    return;
                }
                float mx = motionOut[0];
                float my = motionOut[1];
                float mz = motionOut[2];
                if (GyroDiagnosticOverlay.isEnabled()) {
                    GyroDiagnosticOverlay.report("sensor", controllerNumber, motionType, mx, my, mz);
                }
                handler.conn.sendControllerMotionEvent((byte) controllerNumber, motionType, mx, my, mz);
            }
        }

        @Override
        public void onAccuracyChanged(Sensor sensor, int accuracy) {}
    }
}
