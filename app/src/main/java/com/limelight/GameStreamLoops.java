package com.limelight;

import android.annotation.SuppressLint;
import android.content.BroadcastReceiver;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.hardware.display.DisplayManager;
import android.hardware.input.InputManager;
import android.os.Build;
import android.os.IBinder;
import android.os.SystemClock;
import android.view.Display;
import android.view.View;
import android.widget.Toast;

import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;

import com.limelight.binding.input.driver.UsbDriverService;
import com.limelight.nvstream.AdaptiveQualityController;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.ui.PictureInPicturePolicy;

import java.util.EnumSet;

/**
 * Periodic bitrate, rate-change, quality, recovery-timeout, and system-UI runnables plus display,
 * input-device, PiP, and USB-driver listeners extracted from {@link Game}.
 */
final class GameStreamLoops {
    private final Game game;

    GameStreamLoops(Game game) {
        this.game = game;
    }

    InputManager.InputDeviceListener pcKeyboardDeviceListener() {
        return new InputManager.InputDeviceListener() {
            // A keyboard plugged in or switched to another layout changes what the host should type.
            @Override public void onInputDeviceAdded(int deviceId) { game.keyboardInput.reportKeyboardLayout(); }
            @Override public void onInputDeviceChanged(int deviceId) { game.keyboardInput.reportKeyboardLayout(); }
            @Override public void onInputDeviceRemoved(int deviceId) { game.runOnUiThread(game.keyboardInput::releasePcKeyboardPads); }
        };
    }

    void runRecoveryTimeout() {
        if (game.connecting && game.foreground && !game.isFinishing() && game.streamRecovery.isRecoveryAttempt()) {
            game.displayedFailureDialog = true;
            game.streamConnection.stopConnection();
            if (game.spinner != null) { game.spinner.dismiss(); game.spinner = null; }
            game.streamRecovery.showReconnectDialog(R.string.conn_terminated_title,
                    game.getString(R.string.stream_recovery_exhausted));
        }
    }

    void runWindowFocusActions() {
        if (!game.foreground || !game.connected || game.isFinishing()) {
            game.windowFocusActions.clear();
        } else if (game.streamMenu == null) {
            game.windowFocusActions.runPending(game.hasWindowFocus(), SystemClock.uptimeMillis());
        }
    }

    void runUpdateRateChangeBitrate() {
        if (!game.connected || game.rateChanges == null || !game.rateChanges.automaticOn() || game.decoderRenderer == null) return;
        long now = SystemClock.uptimeMillis();
        game.rateChanges.setThermalLevel(game.adaptation.adaptationThermalLevel());
        if (!game.hostActionInProgress && game.streamMenu == null && !game.rateChanges.sampleAutomatic(now,
                game.decoderRenderer.hasRecentVideoFrames(now), game.decoderRenderer.getNetworkFrameLossPercent(),
                game.decoderRenderer.getQueueDelayMs())) {
            LimeLog.warning("Automatic bitrate: a rate change could not be sent; off for this stream");
            game.messages.alert(R.string.stream_rate_change_auto_not_sent, Toast.LENGTH_LONG);
            return;
        }
        game.adaptation.scheduleRateChangeExpiry();
        game.bitrateHandler.postDelayed(game.updateRateChangeBitrate, 1000);
    }

    void runExpireRateChanges() {
        if (!game.connected || game.rateChanges == null) return;
        EnumSet<StreamRateChanges.Answer> expired = game.rateChanges.expire(SystemClock.uptimeMillis());
        if (expired.contains(StreamRateChanges.Answer.AUTOMATIC_NO_ANSWER)) {
            game.bitrateHandler.removeCallbacks(game.updateRateChangeBitrate);
            LimeLog.warning("Automatic bitrate: the host did not answer a rate change; off for this stream");
            game.messages.alert(R.string.stream_rate_change_auto_no_answer, Toast.LENGTH_LONG);
        }
        if (expired.contains(StreamRateChanges.Answer.TAKEOVER_NO_ANSWER)) {
            // The PC may still lower the bitrate: the switch shows off and the notice may say so.
            game.bitrateKept = false;
            LimeLog.warning("Keep my bitrate: the PC did not answer the bitrate change; it may still lower the bitrate");
        }
        if (expired.contains(StreamRateChanges.Answer.MANUAL_NO_ANSWER)) {
            game.messages.alert(R.string.stream_rate_change_no_answer, Toast.LENGTH_LONG);
        }
        if (expired.contains(StreamRateChanges.Answer.QUERY_NO_ANSWER)) {
            // Quietly, as before the query: Keep my bitrate and automatic bitrate start from the stream's own.
            LimeLog.warning("Rate change: the PC did not answer the bitrate query in " + StreamRateChanges.ANSWER_MS +
                    " ms; the stream goes on from " + game.streamStartBitrateKbps + " kbps");
            game.adaptation.streamBitrateKnown(game.rateChanges, 0);
        }
        game.adaptation.scheduleRateChangeExpiry();
    }

    void runQualityTick() {
        if (!game.connected || game.adaptiveQuality == null || game.isFinishing()) return;
        long now = SystemClock.uptimeMillis();
        if (!game.foreground || game.streamMenu != null || game.hostActionInProgress ||
                (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && game.isInPictureInPictureMode())) {
            game.adaptiveQuality.suspend();
            game.decoderRenderer.resetDecodeTiming();
        } else if (game.pendingQualityTarget != null) {
            game.decoderRenderer.resetDecodeTiming();
            int[] shown = game.decoderRenderer.getStreamSize();
            if (shown[0] == game.pendingQualityTarget.width && shown[1] == game.pendingQualityTarget.height) {
                game.pendingQualityTarget = null;
            } else if (now - game.qualityRequestSentAt >= 10000) {
                game.adaptation.stopAdaptiveQualityWithError();
                return;
            }
        } else {
            long rtt = MoonBridge.getEstimatedRttInfo();
            AdaptiveQualityController.Target target = game.adaptiveQuality.sample(now,
                    game.decoderRenderer.hasRecentVideoFrames(now), game.poorConnection,
                    game.decoderRenderer.getNetworkFrameLossPercent(), game.decoderRenderer.getDecodeTimeMs(),
                    rtt == -1 ? -1 : (float) (rtt & 0xffffffffL), game.decoderRenderer.getQueueDelayMs(),
                    game.adaptation.adaptationThermalLevel(),
                    (game.adaptiveBitrate != null && game.adaptiveBitrate.hasRoomBeforeQualityReduction())
                            || (game.rateChanges != null && game.rateChanges.hasRoomBeforeQualityReduction())
                            || game.adaptation.hostLinkHasRoomBeforeQualityReduction(),
                    game.decoderRenderer.takeDecodeTiming(game.streamFps));
            if (target != null && !game.adaptation.applyAdaptiveQuality(target, now)) return;
        }
        game.bitrateHandler.postDelayed(game.updateQuality, 1000);
    }

    void runUpdateBitrate() {
        if (!game.connected || !game.adaptation.isAdaptiveBitrateEnabled()) {
            return;
        }
        if (!game.hostActionInProgress && game.streamMenu == null && SystemClock.uptimeMillis() >= game.adaptiveBitrateRetryAt) {
            if (game.adaptiveBitrateRetryAt != 0) {
                game.adaptation.applyAdaptiveBitrate(game.adaptiveBitrateRetryTarget);
            } else {
                long now = SystemClock.uptimeMillis();
                long rttInfo = MoonBridge.getEstimatedRttInfo();
                float jitterMs = rttInfo == -1 ? -1 : (float) (rttInfo & 0xffffffffL);
                int thermalLevel = game.adaptation.adaptationThermalLevel();
                if (game.adaptiveBitrate != null) game.adaptiveBitrate.setThermalLevel(thermalLevel);
                if (game.pyroWaveBitrate != null) game.pyroWaveBitrate.setThermalLevel(thermalLevel);
                int target = game.pyroWaveBitrate != null ?
                        game.pyroWaveBitrate.sample(now, game.decoderRenderer.hasRecentVideoFrames(now), game.poorConnection,
                                game.decoderRenderer.getPyroWaveLossPercent(), game.decoderRenderer.getPyroWaveQueueDelayMs(),
                                game.decoderRenderer.getDecodeTimeMs(), jitterMs) :
                        game.adaptiveBitrate.sample(now, game.decoderRenderer.hasRecentVideoFrames(now),
                                game.poorConnection, game.decoderRenderer.getNetworkFrameLossPercent(),
                                game.decoderRenderer.getDecodeTimeMs(), jitterMs, game.decoderRenderer.getQueueDelayMs());
                if (target != 0) {
                    if (game.pyroWaveBitrate == null) {
                        LimeLog.info("Device controls bitrate: " + game.currentBitrate + " -> " + target + " kbps (" +
                                game.adaptiveBitrate.lastReason() + ")");
                    }
                    game.adaptation.applyAdaptiveBitrate(target);
                }
            }
        } else {
            if (game.adaptiveBitrate != null) game.adaptiveBitrate.suspend();
            if (game.pyroWaveBitrate != null) game.pyroWaveBitrate.suspend();
        }
        game.bitrateHandler.postDelayed(game.updateBitrate, 1000);
    }

    DisplayManager.DisplayListener inputDisplayListener() {
        return new DisplayManager.DisplayListener() {
            @Override public void onDisplayAdded(int displayId) {}
            @Override public void onDisplayRemoved(int displayId) {
                Display video = game.videoOutput.getVideoDisplay();
                if (video.getDisplayId() == displayId) game.streamConnection.noteVideoDisplay(video, true);
                if (game.externalPresentation != null && game.externalPresentation.getDisplay().getDisplayId() == displayId) {
                    game.videoOutput.returnVideoToDevice();
                }
            }
            @Override public void onDisplayChanged(int displayId) {
                Display display = game.videoOutput.getVideoDisplay();
                if (display.getDisplayId() == displayId) game.streamConnection.noteVideoDisplay(display, false);
                if (game.conn != null && display.getDisplayId() == displayId) {
                    game.videoOutput.updateInputPollingRate();
                    // Also fires when a foldable moves the stream to its other screen.
                    game.videoOutput.reportDisplayCaps();
                    game.videoOutput.onDisplayModesChanged();
                }
            }
        };
    }

    BroadcastReceiver pipReceiver() {
        return new BroadcastReceiver() {
            @Override public void onReceive(Context context, Intent intent) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
                        PictureInPicturePolicy.canDisconnect(game.connected, game.isInPictureInPictureMode(), game.app.getRole())) {
                    game.menuActions.leaveStream();
                }
            }
        };
    }

    ServiceConnection usbDriverServiceConnection() {
        return new ServiceConnection() {
            @Override
            public void onServiceConnected(ComponentName componentName, IBinder iBinder) {
                if (!game.usbDriverServiceBound || game.usbDriverReleased ||
                        game.isFinishing() || game.isDestroyed() || game.connectionStopThread != null) {
                    return;
                }
                UsbDriverService.UsbDriverBinder binder = (UsbDriverService.UsbDriverBinder) iBinder;
                game.usbDriverBinder = binder;
                binder.setListener(game.controllerHandler);
                binder.setStateListener(game);
                binder.start();
            }

            @Override
            public void onServiceDisconnected(ComponentName componentName) {
                if (game.usbDriverBinder != null) {
                    game.usbDriverBinder.setListener(null);
                    game.usbDriverBinder.setStateListener(null);
                    game.usbDriverBinder = null;
                }
            }
        };
    }

    @SuppressLint("InlinedApi")
    void runHideSystemUi() {
        View decor = game.getWindow().getDecorView();
        WindowInsetsControllerCompat controller = WindowCompat.getInsetsController(game.getWindow(), decor);

        // Keep one edge-to-edge coordinate space. AdaptiveLayout applies the
        // visible system-bar insets while windowed; fitting here as well would
        // subtract them twice, particularly after leaving fullscreen on API 29.
        if (game.isWindowedStream()) {
            decor.setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE |
                    View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION |
                    View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
            controller.show(WindowInsetsCompat.Type.systemBars());
        } else {
            // Use immersive mode
            decor.setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_LAYOUT_STABLE |
                    View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION |
                    View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN |
                    View.SYSTEM_UI_FLAG_HIDE_NAVIGATION |
                    View.SYSTEM_UI_FLAG_FULLSCREEN |
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY);
            controller.setSystemBarsBehavior(WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            controller.hide(WindowInsetsCompat.Type.systemBars());
        }
    }
}
