package com.limelight;

import com.limelight.binding.video.LinkNoticeStatus;
import com.limelight.binding.video.MediaCodecDecoderRenderer;
import com.limelight.nvstream.AdaptiveBitrateController;
import com.limelight.nvstream.AdaptiveQualityController;
import com.limelight.nvstream.NvConnection;
import com.limelight.nvstream.PyroWaveBitrateController;
import com.limelight.nvstream.RateChangeResult;
import com.limelight.nvstream.StreamConfiguration;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.preferences.AutoBitrateChoice;
import com.limelight.ui.BitrateSlider;
import com.limelight.ui.NavigationDialogBuilder;
import androidx.appcompat.app.AlertDialog;
import android.os.SystemClock;
import android.text.InputType;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.Toast;
import org.xmlpull.v1.XmlPullParserException;
import java.io.IOException;
import java.math.BigDecimal;

/** Adapting the stream to the link: auto bitrate and Foundation ABR, adaptive quality, stream health sampling, the bitrate and rate change dialogs. */
final class GameAdaptation {
    private final Game game;
    final GameRateChangeControl rateChange;
    final GameAdaptiveQuality quality;

    GameAdaptation(Game game) {
        this.game = game;
        this.rateChange = new GameRateChangeControl(game, this);
        this.quality = new GameAdaptiveQuality(game, this);
    }

    // Claims of the host's encoder bitrate that failed in a row. Some hosts report 0 for good.
    private static final int MAX_BITRATE_CLAIM_FAILURES = 6;
    private static final java.util.Map<Game, java.util.Set<Object>> activeHostActions = new java.util.WeakHashMap<>();

    static Object beginHostAction(Game game) {
        Object token = new Object();
        synchronized (activeHostActions) {
            activeHostActions.computeIfAbsent(game, ignored -> new java.util.HashSet<>()).add(token);
            game.hostActionInProgress = true;
        }
        return token;
    }

    static void endHostAction(Game game, Object token) {
        game.runOnUiThread(() -> endHostActionOnUi(game, token));
    }

    static void endHostActionOnUi(Game game, Object token) {
        synchronized (activeHostActions) {
            java.util.Set<Object> tokens = activeHostActions.get(game);
            boolean remaining = false;
            if (tokens != null) {
                tokens.remove(token);
                remaining = !tokens.isEmpty();
                if (!remaining) {
                    activeHostActions.remove(game);
                }
            }
            game.hostActionInProgress = remaining;
        }
    }

    private int adaptiveBitrateClaimFailures;
    // A Foundation Sunshine PC's own bitrate controller, on in place of this device's.
    private com.limelight.nvstream.http.FoundationAbr foundationAbr;

    final Runnable sampleStreamHealth = new Runnable() {
        @Override public void run() {
            if (!game.connected || game.decoderRenderer == null || game.app.getRole() == NvApp.Role.INPUT_ONLY) return;
            boolean bitrateFollowsHeat = isAdaptiveBitrateEnabled() || isRateChangeBitrateOn();
            game.decoderRenderer.setHeatEffects(heatHoldsBitrate(game.prefConfig.thermalProtection, bitrateFollowsHeat),
                    heatEasesLoad(game.prefConfig.thermalProtection, bitrateFollowsHeat, game.adaptiveQuality != null));
            game.decoderRenderer.setStreamBitrateKbps(game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate);
            if (game.foreground && !game.hostActionInProgress) {
                // A hanging decoder holds the locks this sample reads for seconds: never on the main thread.
                MediaCodecDecoderRenderer renderer = game.decoderRenderer;
                int fps = game.streamFps;
                if (!streamHealthSampler().sample(() -> {
                    long rtt = MoonBridge.getEstimatedRttInfo();
                    return renderer.healthSample(SystemClock.uptimeMillis(), fps,
                            rtt == -1 ? -1 : (float) (rtt & 0xffffffffL));
                }, game.streamHealth::add)) {
                    renderer.skipHealthSample();
                    LimeLog.info("Stream health: the last sample is still running; this one is skipped");
                }
            } else {
                game.decoderRenderer.skipHealthSample();
            }
            game.bitrateHandler.postDelayed(this, 1000);
        }
    };

    private com.limelight.utils.StreamHealthSampler streamHealthSampler;

    private com.limelight.utils.StreamHealthSampler streamHealthSampler() {
        if (streamHealthSampler == null) {
            game.streamHealthThread = java.util.concurrent.Executors.newSingleThreadExecutor(task -> {
                Thread thread = new Thread(task, "Stream health");
                thread.setDaemon(true);
                return thread;
            });
            streamHealthSampler = new com.limelight.utils.StreamHealthSampler(game.streamHealthThread, game.bitrateHandler::post);
        }
        return streamHealthSampler;
    }

    // Once a second: this second's loss and frame rate for a Foundation PC's controller, and the
    // report every three seconds (FoundationAbr).
    private final Runnable foundationAbrTick = new Runnable() {
        @Override
        public void run() {
            com.limelight.nvstream.http.FoundationAbr abr = foundationAbr;
            if (!game.connected || abr == null) return;
            float[] rates = game.decoderRenderer.getFrameRates();
            abr.add(game.decoderRenderer.getNetworkFrameLossPercent(), rates.length > 1 ? Math.max(0, rates[1]) : 0);
            long rttInfo = MoonBridge.getEstimatedRttInfo();
            org.json.JSONObject feedback = abr.due(SystemClock.uptimeMillis(), rttInfo == -1 ? 0 : rttInfo >>> 32);
            if (feedback != null) {
                NvConnection connection = game.conn;
                new Thread(() -> {
                    org.json.JSONObject action = null;
                    try {
                        action = connection.sendFoundationAbrFeedback(feedback);
                    } catch (IOException e) {
                        LimeLog.warning("The PC's automatic bitrate report failed: " + e.getMessage());
                    }
                    org.json.JSONObject answer = action;
                    game.runOnUiThread(() -> {
                        if (foundationAbr != abr) return;
                        int kbps = abr.answered(answer);
                        if (kbps > 0) {
                            LimeLog.info("The PC set " + kbps + " kbps: " + answer.optString("reason"));
                            game.currentBitrate = kbps;
                        }
                    });
                }, "FoundationAbr").start();
            }
            game.bitrateHandler.postDelayed(this, 1000);
        }
    };

    /** The heat the bitrate and quality controllers react to: none unless Thermal protection is on. */
    int adaptationThermalLevel() {
        return com.limelight.binding.video.ThermalMonitor.forAdaptation(game.prefConfig.thermalProtection,
                game.decoderRenderer.getThermalLevel());
    }

    /** Warm stops a bitrate controller from raising it. Automatic quality has no bitrate to hold. */
    static boolean heatHoldsBitrate(boolean thermalProtection, boolean adaptiveBitrate) {
        return thermalProtection && adaptiveBitrate;
    }

    /** Heat acts only through Thermal protection and a controller that follows it: automatic bitrate or quality. */
    static boolean heatEasesLoad(boolean thermalProtection, boolean adaptiveBitrate, boolean adaptiveQuality) {
        return thermalProtection && (adaptiveBitrate || adaptiveQuality);
    }

    /** Whether the PC took a takeover's bitrate and so stands back (it does for any bitrate it applied or clamped). */
    static boolean takeoverHeld(int bitrateStatus) {
        return bitrateStatus == RateChangeResult.STATUS_APPLIED || bitrateStatus == RateChangeResult.STATUS_CLAMPED;
    }

    /**
     * Whether a PC's report that it lowered the bitrate is told to the player now: the first time in the stream, with
     * the performance overlay (which has its own line) not showing, and not with "Never: keep my bitrate".
     */
    static boolean tellLinkLowered(boolean told, boolean overlayShowing, boolean keepBitrate) {
        return !told && !overlayShowing && !keepBitrate;
    }

    /**
     * Whether automatic quality turns on the busy-network controller for its session: not under Never or while the PC
     * keeps the bitrate (it then steps only the resolution and frame rate), nor on a PC that already steers and reports
     * link bitrate over 0x5538, nor after a bitrate the player set or with another controller already running.
     */
    static boolean qualityTakesBitrate(boolean held, boolean supported, boolean busyOn, boolean hintOn, int manualKbps) {
        return qualityTakesBitrate(held, supported, busyOn, hintOn, manualKbps, false);
    }

    static boolean qualityTakesBitrate(boolean held, boolean supported, boolean busyOn, boolean hintOn,
                                       int manualKbps, boolean hostLinkNotice) {
        return com.limelight.nvstream.jni.NightjarPolicyJni.qualityTakesBitrate(
                held, supported, busyOn, hintOn, manualKbps != 0, hostLinkNotice);
    }

    static boolean hostLinkHasRoom(LinkNoticeStatus status) {
        if (status == null) {
            return com.limelight.nvstream.jni.NightjarPolicyJni.qualityHostLinkHasRoom(0, 0, 0);
        }
        return com.limelight.nvstream.jni.NightjarPolicyJni.qualityHostLinkHasRoom(
                status.state, status.currentKbps, status.ceilingKbps);
    }

    boolean hostLinkHasRoomBeforeQualityReduction() {
        return hostLinkHasRoomBeforeQualityReduction(
                MoonBridge.hostSupportsControlMessage(MoonBridge.CONTROL_MESSAGE_LINK_NOTICE));
    }

    boolean hostLinkHasRoomBeforeQualityReduction(boolean hostLinkNotice) {
        String pcUuid = game.getIntent() == null ? null : game.getIntent().getStringExtra(Game.EXTRA_PC_UUID);
        boolean pcDecides = !bitrateHeld()
                && (game.prefConfig == null || (!game.prefConfig.autoBitrate && !game.prefConfig.rateChangeBitrate))
                && !AutoBitrateChoice.isOn(game, pcUuid);
        if (!com.limelight.nvstream.jni.NightjarPolicyJni.qualityHostSteersLinkBitrate(
                hostLinkNotice, pcDecides, game.bitrateKept, game.sessionManualBitrate != 0,
                isAdaptiveBitrateEnabled(), isRateChangeBitrateOn())) {
            return false;
        }
        return hostLinkHasRoom(game.linkNotice);
    }

    void toggleAdaptiveBitrateFromMenu() {
        // Automatic bitrate on this device turns on and off for this stream only; the setting stays.
        if (isRateChangeBitrateOn() || (!isAdaptiveBitrateEnabled() && game.rateChanges != null && game.prefConfig.rateChangeBitrate)) {
            boolean enable = !isRateChangeBitrateOn();
            rateChange.setRateChangeBitrateEnabled(enable);
            // The setting's own explanation, as the settings screen shows it.
            if (enable) game.messages.alert(R.string.summary_rate_change_bitrate, Toast.LENGTH_LONG);
            return;
        }
        boolean enable = !isAdaptiveBitrateEnabled();
        if (!enable || !game.hostActionInProgress) {
            setAdaptiveBitrateEnabled(enable, true);
            if (enable) {
                game.messages.alert(game.pyroWaveBitrate != null ? R.string.stream_pyrowave_bitrate_help :
                        R.string.stream_auto_bitrate_help, Toast.LENGTH_LONG);
            }
        } else {
            game.messages.alert(R.string.stream_bitrate_busy, Toast.LENGTH_SHORT);
        }
    }

    void applyHealthBitrate(boolean undo) {
        if (!game.foreground || !game.connected || game.hostActionInProgress || !supportsAdaptiveBitrate() ||
                !Game.healthTrialAllowed(isAdaptiveBitrateEnabled(), isRateChangeBitrateOn(), game.adaptiveQuality != null)) return;
        if (undo && (game.healthOriginalBitrate <= 0 || game.currentBitrate != game.healthTrialBitrate)) return;
        final int original = game.healthOriginalBitrate;
        final int expectedTrial = game.healthTrialBitrate;
        final int generation = game.foregroundGeneration;
        final NvConnection connection = game.conn;
        final Object actionToken = beginHostAction(game);
        new Thread(() -> {
            try {
                // The saved preference/current display value may be a transport budget including
                // FEC/audio, or may predate host adaptation. Only the live encoder rate is a baseline.
                int previous = connection.refreshHostDetails().encoderBitrateKbps;
                if (previous <= 0) throw new IOException(game.getString(R.string.health_bitrate_unavailable));
                if ((undo && previous != expectedTrial) || (!undo && previous <= 500)) {
                    game.runOnUiThread(() -> {
                        if (!game.connected || game.conn != connection || game.isFinishing() || isAdaptiveBitrateEnabled() || game.adaptiveQuality != null) return;
                        game.currentBitrate = previous;
                        game.healthOriginalBitrate = game.healthTrialBitrate = 0;
                        if (game.foreground && generation == game.foregroundGeneration) {
                            game.messages.alert(undo ? R.string.health_trial_replaced : R.string.health_bitrate_minimum, Toast.LENGTH_LONG);
                        }
                    });
                    return;
                }
                if (!game.connected || game.conn != connection) return;
                int target = undo ? original : Math.max(500, (int) ((long) previous * 4 / 5));
                int applied = connection.setBitrate(target);
                game.runOnUiThread(() -> {
                    if (!game.connected || game.conn != connection || game.isFinishing() || isAdaptiveBitrateEnabled() || game.adaptiveQuality != null) return;
                    game.currentBitrate = applied;
                    game.healthOriginalBitrate = undo ? 0 : previous;
                    game.healthTrialBitrate = undo ? 0 : applied;
                    if (game.foreground && generation == game.foregroundGeneration) {
                        game.messages.alert(game.getString(R.string.stream_bitrate_applied, applied / 1000.0), Toast.LENGTH_LONG);
                    }
                });
            } catch (IOException | XmlPullParserException | RuntimeException e) {
                showHostActionError(e, generation);
            } finally {
                endHostAction(game, actionToken);
            }
        }, "Stream doctor bitrate trial").start();
    }

    void showBitrateDialog() {
        com.google.android.material.textfield.TextInputLayout field = new com.google.android.material.textfield.TextInputLayout(game);
        field.setBoxBackgroundMode(com.google.android.material.textfield.TextInputLayout.BOX_BACKGROUND_OUTLINE);
        field.setHint(game.getString(R.string.stream_bitrate));
        int padding = Math.round(24 * game.getResources().getDisplayMetrics().density);
        LinearLayout fields = new LinearLayout(game);
        fields.setOrientation(LinearLayout.VERTICAL);
        fields.setPadding(padding, 0, padding, 0);
        EditText input = new com.google.android.material.textfield.TextInputEditText(game);
        input.setMinHeight(Math.round(48 * game.getResources().getDisplayMetrics().density));
        input.setSingleLine(true);
        BitrateSlider.moveFocusOnUpDown(input);
        field.addView(input);
        input.setInputType(InputType.TYPE_CLASS_NUMBER | InputType.TYPE_NUMBER_FLAG_DECIMAL);
        int bitrateKbps = game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate;
        input.setText(BigDecimal.valueOf(bitrateKbps, 3).stripTrailingZeros().toPlainString());
        input.selectAll();
        AlertDialog bitrateDialog = new NavigationDialogBuilder(game).setTitle(R.string.stream_bitrate)
                .setMessage(R.string.stream_bitrate_help).setView(fields)
                .setNegativeButton(android.R.string.cancel, null)
                .setPositiveButton(android.R.string.ok, null).create();
        fields.addView(BitrateSlider.create(game, input, bitrateKbps, () -> applyFromSlider(bitrateDialog)));
        fields.addView(field);
        game.menuActions.showStreamDialog(bitrateDialog);
        bitrateDialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(view -> {
            final int kbps;
            try {
                kbps = new BigDecimal(input.getText().toString().trim().replace(',', '.'))
                        .movePointRight(3).intValueExact();
                if (kbps < 500 || kbps > StreamConfiguration.MAX_BITRATE_KBPS) {
                    throw new NumberFormatException();
                }
            } catch (NumberFormatException | ArithmeticException e) {
                field.setError(game.getString(R.string.stream_bitrate_invalid));
                return;
            }
            if (game.hostActionInProgress) {
                game.messages.alert(R.string.stream_bitrate_busy, Toast.LENGTH_SHORT);
                return;
            }
            if (!game.foreground || !game.connected || game.hostActionInProgress) {
                return;
            }
            bitrateDialog.dismiss();
            sendManualBitrate(kbps);
        });
    }

    // The stream menu's slider settled: as the matching dialog's OK, on whichever path this stream changes its bitrate.
    // Answers the rate the slider is to show: a rate not sent snaps it back to the one applied.
    int applyBitrateFromMenu(int kbps) {
        if (game.rateChanges != null) {
            return rateChange.applyBitrateFromMenu(kbps);
        }
        int applied = game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate;
        if (game.hostActionInProgress) {
            game.messages.alert(R.string.stream_bitrate_busy, Toast.LENGTH_SHORT);
            return applied;
        }
        if (!game.foreground || !game.connected) return applied;
        sendManualBitrate(kbps);
        return kbps;
    }

    private void sendManualBitrate(int kbps) {
        // A manual bitrate stops automatic bitrate for this session only; the saved choice stays.
        final boolean restoreAuto = isAdaptiveBitrateEnabled();
        final boolean prevQualityAuto = game.qualityAutoBitrate;
        final NvConnection connection = game.conn;
        setAdaptiveBitrateEnabled(false, false);
        game.qualityAutoBitrate = false;
        final Object actionToken = beginHostAction(game);
        final int generation = game.foregroundGeneration;
        new Thread(() -> {
            try {
                int applied = connection.setBitrate(kbps);
                game.runOnUiThread(() -> {
                    if (!game.connected || game.conn != connection || game.isFinishing()) {
                        return;
                    }
                    game.currentBitrate = applied;
                    game.sessionManualBitrate = applied;
                    game.healthOriginalBitrate = game.healthTrialBitrate = 0;
                    // The open menu's slider shows the rate it asked for; one the host changed is news.
                    boolean onSlider = applied == kbps && game.menuActions.streamMenuOpen();
                    if (game.foreground && generation == game.foregroundGeneration && !onSlider) {
                        game.messages.alert(game.getString(R.string.stream_bitrate_applied, applied / 1000.0), Toast.LENGTH_LONG);
                    }
                });
            } catch (IOException | XmlPullParserException | RuntimeException e) {
                game.runOnUiThread(() -> {
                    if (game.connected && game.conn == connection && !game.isFinishing()
                            && restoreAuto && !isAdaptiveBitrateEnabled() && game.sessionManualBitrate == 0) {
                        game.qualityAutoBitrate = prevQualityAuto;
                        setAdaptiveBitrateEnabled(true, false);
                    }
                });
                showHostActionError(e, generation);
            } finally {
                endHostAction(game, actionToken);
            }
        }, "Stream bitrate").start();
    }

    // A settled bitrate slider sends through its dialog's OK, with the same checks as a typed value.
    static void applyFromSlider(AlertDialog dialog) {
        if (dialog != null && dialog.isShowing()) dialog.getButton(AlertDialog.BUTTON_POSITIVE).performClick();
    }

    // Automatic bitrate on a PC without Rubylight's runtime bitrate: a Foundation Sunshine PC runs
    // its own controller (as Moonlight V+ asks it to); other PCs answer 404 and nothing changes.
    boolean foundationAbrAtStart() {
        ComputerDetails details = game.conn.getHostDetails();
        return game.connected && game.app != null && game.app.getRole() == NvApp.Role.STREAM &&
                details.caps.family() != HostCapabilities.Family.RUBYLIGHT &&
                !details.caps.has(HostCapabilities.Feature.RUNTIME_BITRATE) &&
                AutoBitrateChoice.isOn(game, game.getIntent().getStringExtra(Game.EXTRA_PC_UUID));
    }

    void startFoundationAbr() {
        NvConnection connection = game.conn;
        int maxKbps = game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate;
        new Thread(() -> {
            try {
                int initial = connection.startFoundationAbr(maxKbps);
                game.runOnUiThread(() -> {
                    if (initial < 0) {
                        LimeLog.info("The PC has no automatic bitrate");
                        return;
                    }
                    if (!game.connected || game.isFinishing() || game.conn != connection) {
                        // The stream ended while the PC turned it on: stopFoundationAbr had nothing to stop.
                        sendFoundationAbrStop(connection);
                        return;
                    }
                    foundationAbr = new com.limelight.nvstream.http.FoundationAbr(initial, maxKbps, SystemClock.uptimeMillis());
                    game.currentBitrate = foundationAbr.currentKbps();
                    LimeLog.info("The PC's automatic bitrate is on: " + game.currentBitrate + " kbps, at most " + maxKbps + " kbps");
                    game.bitrateHandler.postDelayed(foundationAbrTick, 1000);
                });
            } catch (IOException e) {
                LimeLog.warning("The PC did not start its automatic bitrate: " + e.getMessage());
                // Loud, as on desktop: the stream keeps its bitrate and the user should know why.
                game.runOnUiThread(() -> {
                    if (game.connected && !game.isFinishing() && game.conn == connection) {
                        game.messages.alert(game.getString(R.string.stream_host_abr_failed, e.getMessage()), Toast.LENGTH_LONG);
                    }
                });
            }
        }, "FoundationAbr").start();
    }

    // Turns a Foundation PC's controller off at the stream's end (V+ PC's stopSunshineAbr).
    void stopFoundationAbr() {
        if (foundationAbr == null) return;
        foundationAbr.close(); foundationAbr = null;
        game.bitrateHandler.removeCallbacks(foundationAbrTick);
        sendFoundationAbrStop(game.conn);
    }

    private static void sendFoundationAbrStop(NvConnection connection) {
        new Thread(() -> {
            try {
                connection.stopFoundationAbr();
            } catch (IOException e) {
                LimeLog.warning("The PC's automatic bitrate did not stop: " + e.getMessage());
            }
        }, "FoundationAbr").start();
    }

    boolean supportsAdaptiveBitrate() {
        ComputerDetails details = game.conn.getHostDetails();
        return game.app.getRole() != NvApp.Role.INPUT_ONLY && details.caps.has(HostCapabilities.Feature.RUNTIME_BITRATE) &&
                details.encoderBitrateSupported &&
                details.hasPermission(ComputerDetails.PERMISSION_VIEW | ComputerDetails.PERMISSION_LAUNCH);
    }

    boolean isAdaptiveBitrateEnabled() {
        return game.adaptiveBitrate != null || game.pyroWaveBitrate != null;
    }

    /** Whether the stream starts with automatic bitrate: Lower it sooner, this PC's choice or else the Settings value. */
    boolean autoBitrateAtStart() {
        return supportsAdaptiveBitrate() &&
                AutoBitrateChoice.isOn(game, game.getIntent().getStringExtra(Game.EXTRA_PC_UUID));
    }

    void setAdaptiveBitrateEnabled(boolean enabled, boolean remember) {
        // Automatic bitrate on again: a reconnect starts from the saved bitrate with it.
        if (enabled) game.sessionManualBitrate = 0;
        if (remember) {
            // This PC's Lower bitrate: Lower it sooner while on, The PC decides when turned off.
            AutoBitrateChoice.setForPc(game, game.getIntent().getStringExtra(Game.EXTRA_PC_UUID),
                    enabled ? AutoBitrateChoice.SOONER : AutoBitrateChoice.PC);
            // The person chose it themselves; Automatic quality no longer owns it.
            game.qualityAutoBitrate = false;
        }
        game.bitrateHandler.removeCallbacks(game.updateBitrate);
        if (game.adaptiveBitrate != null) game.adaptiveBitrate.close(); game.adaptiveBitrate = null;
        if (game.pyroWaveBitrate != null) game.pyroWaveBitrate.close(); game.pyroWaveBitrate = null;
        game.adaptiveBitrateRetryAt = 0;
        adaptiveBitrateClaimFailures = 0;
        if (enabled) {
            game.healthOriginalBitrate = game.healthTrialBitrate = 0;
            if ((game.decoderRenderer.getActiveVideoFormat() & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0) {
                game.pyroWaveBitrate = new PyroWaveBitrateController(0, 0, game.streamFps,
                        SystemClock.uptimeMillis());
            } else {
                game.adaptiveBitrate = new AdaptiveBitrateController(0, 0, game.streamFps, SystemClock.uptimeMillis(),
                        game.prefConfig.width, game.prefConfig.height, adaptiveBitrateCodec());
            }
            // The preference includes FEC/audio, while runtime changes use encoder Kbps.
            // Read the actual encoder budget, then claim ownership at that same bitrate.
            applyAdaptiveBitrate(0);
            game.bitrateHandler.postDelayed(game.updateBitrate, 1000);
        }
    }

    // The codec sets how many bits a picture of this size needs, which the built-in floor follows.
    private AdaptiveBitrateController.Codec adaptiveBitrateCodec() {
        if (game.decoderRenderer == null) return null;
        int format = game.decoderRenderer.getActiveVideoFormat();
        if ((format & MoonBridge.VIDEO_FORMAT_MASK_AV1) != 0) return AdaptiveBitrateController.Codec.AV1;
        if ((format & MoonBridge.VIDEO_FORMAT_MASK_H265) != 0) return AdaptiveBitrateController.Codec.HEVC;
        return AdaptiveBitrateController.Codec.H264;
    }

    void applyAdaptiveBitrate(int target) {
        AdaptiveBitrateController controller = game.adaptiveBitrate;
        PyroWaveBitrateController pyroController = game.pyroWaveBitrate;
        // After an automatic reconnect, the first claim starts from the rate automatic bitrate had cut to,
        // and the stream's start bitrate stays the ceiling it may climb back to.
        final int resumeKbps = target == 0 ? game.streamRecovery.takeAutomaticBitrateStart() : 0;
        final NvConnection connection = game.conn;
        final Object actionToken = beginHostAction(game);
        new Thread(() -> {
            try {
                ComputerDetails details = target == 0 ? connection.refreshHostDetails() : null;
                if (details != null && (details.encoderBitrateKbps <= 0 || details.initialEncoderBitrateKbps <= 0)) {
                    // A host without the bitrate fields will not start sending them on a retry.
                    throw details.encoderBitrateSupported ?
                            new IOException("Host did not report the stream's encoder bitrate") :
                            new EncoderBitrateUnsupported();
                }
                // Claim bitrate ownership on hosts with their own controller, without probing a new rate.
                int applied = connection.setBitrate(details == null ? target : com.limelight.nvstream.jni.NightjarPolicyJni.recoveryAutomaticBitrateStart(details.encoderBitrateKbps, resumeKbps));
                game.runOnUiThread(() -> {
                    if (game.connected && game.conn == connection && !game.isFinishing()) {
                        if (game.adaptiveBitrate == controller && game.pyroWaveBitrate == pyroController) {
                            game.currentBitrate = applied;
                            game.adaptiveBitrateRetryAt = 0;
                            adaptiveBitrateClaimFailures = 0;
                            long now = SystemClock.uptimeMillis();
                            if (details != null) {
                                // The bitrate the stream started at (the bitrate setting, after profiles) is the
                                // ceiling; the built-in floor applies.
                                int ceiling = Math.max(details.initialEncoderBitrateKbps, applied);
                                if (controller != null) game.adaptiveBitrate = new AdaptiveBitrateController(ceiling, applied, game.streamFps, now,
                                        game.prefConfig.width, game.prefConfig.height, adaptiveBitrateCodec());
                                if (pyroController != null) game.pyroWaveBitrate = new PyroWaveBitrateController(ceiling, applied, game.streamFps, now);
                            } else {
                                if (controller != null) controller.applied(target, applied, now);
                                if (pyroController != null) pyroController.applied(target, applied, now);
                            }
                        }
                    }
                });
            } catch (IOException | XmlPullParserException | RuntimeException e) {
                LimeLog.warning("Automatic bitrate update failed: " + e);
                game.runOnUiThread(() -> {
                    if (game.conn != connection || game.isFinishing() || game.isDestroyed()) return;
                    if (game.adaptiveBitrate == controller && game.pyroWaveBitrate == pyroController) {
                        if (target == 0 && (e instanceof EncoderBitrateUnsupported ||
                                ++adaptiveBitrateClaimFailures >= MAX_BITRATE_CLAIM_FAILURES)) {
                            // Without the encoder's bitrate there is nothing to adapt from: stop
                            // retrying for this stream and leave the PC's saved choice alone.
                            LimeLog.warning("Automatic bitrate: the host reports no encoder bitrate; off for this stream");
                            game.qualityAutoBitrate = false;
                            setAdaptiveBitrateEnabled(false, false);
                            if (game.foreground && game.connected && !game.isFinishing()) {
                                game.messages.alert(R.string.stream_auto_bitrate_unavailable, Toast.LENGTH_LONG);
                            }
                            return;
                        }
                        boolean firstFailure = game.adaptiveBitrateRetryAt == 0;
                        game.adaptiveBitrateRetryTarget = target;
                        game.adaptiveBitrateRetryAt = SystemClock.uptimeMillis() + 5000;
                        if (controller != null) controller.suspend();
                        if (pyroController != null) pyroController.suspend();
                        if (firstFailure && game.foreground && game.connected && !game.isFinishing()) {
                            game.messages.alert(R.string.stream_auto_bitrate_failed, Toast.LENGTH_LONG);
                        }
                    }
                });
            } finally {
                endHostAction(game, actionToken);
            }
        }, "Automatic bitrate").start();
    }

    private static final class EncoderBitrateUnsupported extends IOException {
        EncoderBitrateUnsupported() {
            super("Host does not support runtime bitrate changes");
        }
    }

    void showHostActionError(Exception error, int generation) {
        game.runOnUiThread(() -> {
            if (game.foreground && generation == game.foregroundGeneration && game.connected && !game.isFinishing()) {
                game.messages.alert(game.getString(R.string.stream_host_action_failed, error.getMessage()), Toast.LENGTH_LONG);
            }
        });
    }

    void showRateChangeDialog() { rateChange.showRateChangeDialog(); }
    void onRateChange(RateChangeResult result) { rateChange.onRateChange(result); }
    void startStreamBitrate() { rateChange.startStreamBitrate(); }
    void startStreamBitrate(StreamRateChanges changes) { rateChange.startStreamBitrate(changes); }
    boolean bitrateWaiting() { return rateChange.bitrateWaiting(); }
    void pcLowered() { rateChange.pcLowered(); }
    void streamBitrateKnown(StreamRateChanges stream, int kbps) { rateChange.streamBitrateKnown(stream, kbps); }
    boolean keepBitrateShown() { return rateChange.keepBitrateShown(); }
    void keepBitrateFromMenu(boolean on) { rateChange.keepBitrateFromMenu(on); }
    int keepKbps() { return rateChange.keepKbps(); }
    boolean bitrateHeld() { return rateChange.bitrateHeld(); }
    void noteLinkLowered(LinkNoticeStatus status) { rateChange.noteLinkLowered(status); }
    void scheduleRateChangeExpiry() { rateChange.scheduleRateChangeExpiry(); }
    boolean isRateChangeBitrateOn() { return rateChange.isRateChangeBitrateOn(); }

    AdaptiveQualityController.Mode qualityMode() { return quality.qualityMode(); }
    boolean showsAdaptiveQuality() { return quality.showsAdaptiveQuality(); }
    boolean supportsAdaptiveQuality() { return quality.supportsAdaptiveQuality(); }
    void startAdaptiveQuality() { quality.startAdaptiveQuality(); }
    boolean applyAdaptiveQuality(AdaptiveQualityController.Target target, long now) { return quality.applyAdaptiveQuality(target, now); }
    int qualityCeilingFrameRateMilliHz() { return quality.qualityCeilingFrameRateMilliHz(); }
    boolean applyAdaptiveQuality(AdaptiveQualityController.Target target, long now, int ceilingMilliHz) { return quality.applyAdaptiveQuality(target, now, ceilingMilliHz); }
    void applyQualityChoice() { quality.applyQualityChoice(); }
    void stopAdaptiveQualityWithError() { quality.stopAdaptiveQualityWithError(); }
    void showAdaptiveQualityDialog() { quality.showAdaptiveQualityDialog(); }
}
