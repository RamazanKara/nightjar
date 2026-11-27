package com.limelight;

import com.limelight.binding.video.LinkNoticeStatus;
import com.limelight.nvstream.RateChangeResult;
import com.limelight.nvstream.StreamConfiguration;
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
import java.math.BigDecimal;

/** Live 0x553C rate-change requests, bitrate-query handshake, Keep-my-bitrate takeover, link-lowered notices, and device-side rate-change automatic bitrate. */
final class GameRateChangeControl {
    private final Game game;
    private final GameAdaptation adaptation;

    // The stream whose start waits for the PC's answer to the query (StreamRateChanges.sendQuery); null once it went on.
    private StreamRateChanges askingStream;
    // Automatic bitrate on this device waits for that answer, to start from the stream's bitrate as the PC counts it.
    private boolean automaticAfterAnswer;
    // The stream's start in uptime: Keep my bitrate's change goes out no sooner than a second after it.
    private long streamStartMs;
    // The query went out only on its second try, or the PC said it lowered the bitrate before its answer: the answer
    // may hold that cut, so it is not used (onRateChange).
    private boolean queryRetried, cutBeforeAnswer;
    // An open bitrate dialog whose bitrate waits for the PC's answer: it shows the stream's number and is usable then.
    private java.util.function.IntConsumer rateDialogBitrateKnown;

    GameRateChangeControl(Game game, GameAdaptation adaptation) {
        this.game = game;
        this.adaptation = adaptation;
    }

    int applyBitrateFromMenu(int kbps) {
        int fallback = game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate;
        if (askingStream == game.rateChanges) {
            // Its control is disabled until the PC said what it counts the stream's bitrate as; a settle left
            // from before a reconnect snaps back.
            LimeLog.warning("Rate change: " + kbps + " kbps not sent: the PC has not said yet what it counts this stream's bitrate as");
            return fallback;
        }
        return sendRateChange(game.rateChanges, kbps, 0) ? kbps : fallback;
    }

    // Bitrate and frame rate in one 0x553C request; a field left as it is keeps its value.
    void showRateChangeDialog() {
        final StreamRateChanges stream = game.rateChanges;
        if (stream == null) return;
        float density = game.getResources().getDisplayMetrics().density;
        int padding = Math.round(24 * density);
        LinearLayout fields = new LinearLayout(game);
        fields.setOrientation(LinearLayout.VERTICAL);
        fields.setPadding(padding, 0, padding, 0);
        int bitrateKbps = game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate;
        // The bitrate as it was when the dialog opened, or as the PC's answer set it: a field left at it is unchanged.
        String[] bitrateText = {BigDecimal.valueOf(bitrateKbps, 3).stripTrailingZeros().toPlainString()};
        com.google.android.material.textfield.TextInputLayout bitrateField = rateChangeField(
                game.getString(R.string.stream_bitrate), bitrateText[0], density);
        final AlertDialog[] opened = new AlertDialog[1];
        com.limelight.ui.StepSlider bitrateSlider = BitrateSlider.create(game, bitrateField.getEditText(), bitrateKbps,
                () -> GameAdaptation.applyFromSlider(opened[0]));
        fields.addView(bitrateSlider);
        fields.addView(bitrateField);
        if (askingStream == stream) {
            // Until the PC said what it counts the stream's bitrate as; the frame rate stays free.
            bitrateField.setEnabled(false);
            bitrateSlider.setEnabled(false);
            rateDialogBitrateKnown = kbps -> {
                if (opened[0] == null || !opened[0].isShowing() || game.rateChanges != stream) return;
                bitrateText[0] = BigDecimal.valueOf(kbps, 3).stripTrailingZeros().toPlainString();
                bitrateField.getEditText().setText(bitrateText[0]);
                BitrateSlider.moveTo(bitrateSlider, kbps);
                bitrateField.setEnabled(true);
                bitrateSlider.setEnabled(true);
            };
        }
        // Keep my bitrate under the bitrate it keeps; it acts at once, without OK.
        com.google.android.material.materialswitch.MaterialSwitch keep =
                new com.google.android.material.materialswitch.MaterialSwitch(game);
        keep.setText(R.string.stream_keep_bitrate);
        keep.setMinHeight(Math.round(48 * density));
        keep.setChecked(keepBitrateShown());
        keep.setOnCheckedChangeListener((view, on) -> {
            if (game.rateChanges == stream) keepBitrateFromMenu(on);
        });
        fields.addView(keep);
        // Automatic quality owns the frame rate while it runs.
        int fpsMillihz = game.reconfigureRequester.fpsMillihz() > 0 ? game.reconfigureRequester.fpsMillihz() : game.streamFps * 1000;
        String fpsText = BigDecimal.valueOf(fpsMillihz, 3).stripTrailingZeros().toPlainString();
        com.google.android.material.textfield.TextInputLayout fpsField = game.adaptiveQuality != null ? null :
                rateChangeField(game.getString(R.string.stream_rate_change_fps), fpsText, density);
        if (fpsField != null) fields.addView(fpsField);
        AlertDialog dialog = new NavigationDialogBuilder(game).setTitle(R.string.stream_rate_change)
                .setMessage(R.string.stream_rate_change_help).setView(fields)
                .setNegativeButton(android.R.string.cancel, null)
                .setPositiveButton(android.R.string.ok, null).create();
        opened[0] = dialog;
        game.menuActions.showStreamDialog(dialog);
        dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(view -> {
            // connected stays true until stopConnection, which closes this dialog: a termination
            // that has arrived but not yet stopped the stream counts as ended (streamEnding).
            if (!game.connected || game.streamConnection.streamEnding() || game.rateChanges != stream) {
                // The stream this dialog was opened for ended or restarted: nothing goes out.
                dialog.dismiss();
                LimeLog.warning("Rate change: the stream ended before the request was sent");
                game.messages.alert(R.string.stream_rate_change_stream_ended, Toast.LENGTH_LONG);
                return;
            }
            bitrateField.setError(null);
            if (fpsField != null) fpsField.setError(null);
            int kbps = parseRateChange(bitrateField, bitrateText[0], 500, StreamConfiguration.MAX_BITRATE_KBPS,
                    R.string.stream_bitrate_invalid);
            int millihz = fpsField == null ? 0 : parseRateChange(fpsField, fpsText, 10_000, 500_000,
                    R.string.stream_rate_change_fps_invalid);
            if (kbps < 0 || millihz < 0) return;
            if (kbps == 0 && millihz == 0) {
                bitrateField.setError(game.getString(R.string.stream_rate_change_nothing));
                return;
            }
            dialog.dismiss();
            sendRateChange(stream, kbps, millihz);
        });
    }

    private boolean sendRateChange(StreamRateChanges stream, int kbps, int millihz) {
        if (!game.connected || game.streamConnection.streamEnding() || game.rateChanges != stream) {
            LimeLog.warning("Rate change: the stream ended before the request was sent");
            game.messages.alert(R.string.stream_rate_change_stream_ended, Toast.LENGTH_LONG);
            return false;
        }
        if (askingStream == stream && kbps != 0) {
            // The bitrate cannot change before the PC said what it counts the stream's as (its controls are disabled
            // meanwhile); a frame rate is no matter of that unit and goes out at once.
            LimeLog.warning("Rate change: " + kbps + " kbps not sent: the PC has not said yet what it counts this stream's bitrate as");
            if (millihz == 0) return false;
            kbps = 0;
        }
        if (!stream.sendManual(kbps, millihz, SystemClock.uptimeMillis())) {
            LimeLog.warning("Rate change: request not sent");
            game.messages.alert(R.string.stream_rate_change_not_sent, Toast.LENGTH_LONG);
            return false;
        }
        scheduleRateChangeExpiry();
        LimeLog.info("Rate change: asking for " + kbps + " kbps, " + millihz + " mHz");
        if (kbps != 0) {
            // As with the bitrate dialog, a bitrate set by hand stops the host-side automatic
            // bitrate for this session; automatic bitrate on this device follows it instead.
            adaptation.setAdaptiveBitrateEnabled(false, false);
            game.qualityAutoBitrate = false;
            game.healthOriginalBitrate = game.healthTrialBitrate = 0;
        }
        return true;
    }

    private com.google.android.material.textfield.TextInputLayout rateChangeField(String hint, String text, float density) {
        com.google.android.material.textfield.TextInputLayout field = new com.google.android.material.textfield.TextInputLayout(game);
        field.setBoxBackgroundMode(com.google.android.material.textfield.TextInputLayout.BOX_BACKGROUND_OUTLINE);
        field.setHint(hint);
        EditText input = new com.google.android.material.textfield.TextInputEditText(game);
        input.setMinHeight(Math.round(48 * density));
        input.setSingleLine(true);
        BitrateSlider.moveFocusOnUpDown(input);
        input.setInputType(InputType.TYPE_CLASS_NUMBER | InputType.TYPE_NUMBER_FLAG_DECIMAL);
        input.setText(text);
        field.addView(input);
        return field;
    }

    // The field's value in thousandths (kbps from Mbps, mHz from fps); 0 when it is empty or as it
    // was, -1 with the error shown when it is outside min-max.
    private int parseRateChange(com.google.android.material.textfield.TextInputLayout field, String unchanged,
                                int min, int max, int error) {
        String text = field.getEditText().getText().toString().trim().replace(',', '.');
        if (text.isEmpty() || text.equals(unchanged)) return 0;
        try {
            int value = new BigDecimal(text).movePointRight(3).intValueExact();
            if (value >= min && value <= max) return value;
        } catch (NumberFormatException | ArithmeticException e) {
            // Shown below.
        }
        field.setError(game.getString(error));
        return -1;
    }

    // The host's answer to a 0x553C request, on the UI thread.
    void onRateChange(RateChangeResult result) {
        if (game.rateChanges == null || !game.connected || game.isFinishing()) return;
        StreamRateChanges.Outcome outcome = game.rateChanges.onResult(result, SystemClock.uptimeMillis());
        StreamRateChanges.Answer answer = outcome.answer;
        if (answer == StreamRateChanges.Answer.UNKNOWN) {
            LimeLog.warning("Rate change: a result for no request of this stream, " + result);
            return;
        }
        LimeLog.info("Rate change: " + result + " (" + answer + (outcome.takesBitrate ? ")" : ", older bitrate)"));
        boolean tainted = answer == StreamRateChanges.Answer.QUERY && (queryRetried || cutBeforeAnswer);
        // An answer that came after a later request's keeps that one's bitrate.
        if (outcome.takesBitrate && !tainted) game.currentBitrate = result.bitrateKbps;
        if (tainted) {
            // Video ran before the PC handled it (a retried query), or the PC said it lowered the bitrate first: the
            // answer may be a cut, which would be kept for the whole stream.
            LimeLog.warning("Rate change: the PC's answer to the bitrate query (" + result.bitrateKbps + " kbps) may hold a cut " +
                    (cutBeforeAnswer ? "it made before it" : "made while the query waited to go out") +
                    "; the stream goes on from " + game.streamStartBitrateKbps + " kbps");
            streamBitrateKnown(game.rateChanges, 0);
        } else if (answer == StreamRateChanges.Answer.QUERY) {
            if (result.bitrateKbps > 0) {
                LimeLog.info("Rate change: the PC counts this stream's bitrate as " + result.bitrateKbps +
                        " kbps; Keep my bitrate and automatic bitrate start from it");
            } else {
                LimeLog.warning("Rate change: the PC's answer to the bitrate query named no bitrate; " +
                        "the stream goes on from " + game.streamStartBitrateKbps + " kbps");
            }
            streamBitrateKnown(game.rateChanges, result.bitrateKbps);
        }
        if (answer == StreamRateChanges.Answer.TAKEOVER && !GameAdaptation.takeoverHeld(result.bitrateStatus)) {
            // A refused takeover: the PC still lowers the bitrate, so the switch shows off and the notice may say so.
            game.bitrateKept = false;
            LimeLog.warning("Keep my bitrate: the PC refused the bitrate change (" + result + "); it may still lower the bitrate");
        }
        // The player's own rate, which an automatic reconnect keeps.
        if (answer == StreamRateChanges.Answer.MANUAL && outcome.takesBitrate && result.bitrateKbps > 0) {
            game.sessionManualBitrate = result.bitrateKbps;
        }
        // The PC stands back for a bitrate the player chose (Session::set_client_bitrate), as for Never's takeover;
        // Keep my bitrate keeps that one.
        if (answer == StreamRateChanges.Answer.MANUAL && outcome.takesBitrate && result.bitrateKbps > 0
                && (result.bitrateStatus == RateChangeResult.STATUS_APPLIED || result.bitrateStatus == RateChangeResult.STATUS_CLAMPED)) {
            game.playerBitrateKbps = result.bitrateKbps;
            game.bitrateKept = true;
        }
        if (result.fpsChanged() && result.fpsMillihz > 0 && result.fpsMillihz != game.reconfigureRequester.fpsMillihz()) {
            game.reconfigureRequester.rateChanged(result.fpsMillihz);
            game.videoOutput.setStreamFrameRate(result.fpsMillihz);
        }
        if (answer == StreamRateChanges.Answer.AUTOMATIC_STOPPED) {
            game.bitrateHandler.removeCallbacks(game.updateRateChangeBitrate);
            LimeLog.warning("Automatic bitrate: the host refused a rate change; off for this stream");
            game.messages.alert(game.getString(R.string.stream_rate_change_auto_stopped,
                    Game.rateChangeReason(game, result.bitrateReason)), Toast.LENGTH_LONG);
        } else if (answer == StreamRateChanges.Answer.MANUAL) {
            // A clamp, a refusal or a frame rate is news the open menu's slider cannot show; a plain applied bitrate is on it.
            boolean onSlider = result.bitrateStatus == RateChangeResult.STATUS_APPLIED
                    && result.fpsStatus == RateChangeResult.STATUS_UNCHANGED;
            if (!onSlider || !game.menuActions.streamMenuOpen()) {
                game.messages.alert(Game.rateChangeText(game, result), Toast.LENGTH_LONG);
            }
        }
    }

    // At the stream's start: rate changes when the host offered them, and the automatic bitrate
    // the settings ask for, on this device or else on the host.
    void startStreamBitrate() {
        startStreamBitrate(offersRateChange() ? new StreamRateChanges(MoonBridge::sendRateChange) : null);
    }

    void startStreamBitrate(StreamRateChanges changes) {
        game.rateChanges = changes;
        game.bitrateKept = false;
        game.linkNotice = null;
        // After an automatic reconnect the player's own rate is the one this stream started at.
        game.playerBitrateKbps = game.sessionManualBitrate;
        game.streamStartBitrateKbps = game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate;
        askingStream = null;
        automaticAfterAnswer = false;
        queryRetried = cutBeforeAnswer = false;
        rateDialogBitrateKnown = null;
        streamStartMs = SystemClock.uptimeMillis();
        // Before anything that sets a bitrate: the PC's own count of this stream's bitrate, which Keep my bitrate
        // and automatic bitrate start from (Rubylight 6.0.1 and older count in the encoder's unit, newer PCs in the
        // launch's).
        if (changes != null) askStreamBitrate(changes, true);
        // A menu left open over a reconnect waits too: its bitrate cannot change until the answer.
        if (bitrateWaiting()) game.menuActions.showBitrate(game.streamStartBitrateKbps, false);
        // A reconnect is a new session on the PC, whose controllers lower the bitrate again: Never, and a bitrate the
        // player chose before the reconnect, send it again.
        if (game.prefConfig.keepBitrate || (changes != null && game.sessionManualBitrate > 0)) {
            if (changes != null) {
                // The bitrate dialog's switch shows on from now; it sends nothing more while the takeover waits for
                // the PC's answer (streamBitrateKnown).
                game.bitrateKept = true;
            } else {
                LimeLog.info("Never: keep my bitrate is chosen, but this PC takes no bitrate changes from the device; " +
                        "the stream runs as The PC decides");
            }
        }
        if (game.sessionManualBitrate > 0) {
            // An automatic reconnect after the player set a bitrate: it started at that rate, and a manual
            // bitrate keeps automatic bitrate off for the rest of the session.
            LimeLog.info("Automatic bitrate stays off: the player set " + game.sessionManualBitrate + " kbps before the reconnect");
        } else if (bitrateHeld()) {
            // Never: no bitrate controller of Nightjar's runs, whatever a PC's own stored choice says.
            LimeLog.info("Never: keep my bitrate: Nightjar lowers no bitrate in this stream");
        } else if (game.rateChanges != null && game.prefConfig.rateChangeBitrate) {
            setRateChangeBitrateEnabled(true);
        } else if (adaptation.autoBitrateAtStart()) {
            adaptation.setAdaptiveBitrateEnabled(true, false);
        } else if (adaptation.foundationAbrAtStart()) {
            adaptation.startFoundationAbr();
        }
    }

    /** The query waits: the stream's bitrate controls are disabled. */
    boolean bitrateWaiting() {
        return askingStream != null && askingStream == game.rateChanges;
    }

    // The PC said it lowered the bitrate for the network (0x5538, limited or floor).
    void pcLowered() {
        if (askingStream != null && askingStream == game.rateChanges) cutBeforeAnswer = true;
    }

    // The query, at once; one that cannot be sent yet goes once more a second later, and then the stream goes on
    // without it.
    private void askStreamBitrate(StreamRateChanges stream, boolean again) {
        askingStream = stream;
        if (stream.sendQuery(SystemClock.uptimeMillis())) {
            if (!again) queryRetried = true;
            LimeLog.info("Rate change: asking the PC what it counts this stream's bitrate as");
            scheduleRateChangeExpiry();
        } else if (again) {
            LimeLog.warning("Rate change: the bitrate query could not be sent yet; trying again in a second");
            game.bitrateHandler.postDelayed(() -> {
                if (game.connected && game.rateChanges == stream && askingStream == stream) askStreamBitrate(stream, false);
            }, 1000);
        } else {
            LimeLog.warning("Rate change: the bitrate query could not be sent; the stream goes on from " +
                    game.streamStartBitrateKbps + " kbps");
            streamBitrateKnown(stream, 0);
        }
    }

    // The PC answered the query with the stream's bitrate in its own unit (kbps > 0), or did not, or it could not be
    // sent (0: the stream goes on from the bitrate it started with). What waited for it goes on, once.
    void streamBitrateKnown(StreamRateChanges stream, int kbps) {
        if (stream == null || askingStream != stream) return;
        askingStream = null;
        if (!game.connected || game.rateChanges != stream) return;
        if (kbps > 0) game.streamStartBitrateKbps = kbps;
        // The bitrate controls are usable again, at the stream's number: the PC's after its answer, else the one the
        // stream started with.
        int shownKbps = game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate;
        game.menuActions.showBitrate(shownKbps, true);
        if (rateDialogBitrateKnown != null) {
            rateDialogBitrateKnown.accept(shownKbps);
            rateDialogBitrateKnown = null;
        }
        if (game.bitrateKept) {
            // Never, a bitrate the player chose before a reconnect, or the switch turned on while the PC was asked.
            long waitMs = Math.max(0, streamStartMs + 1000 - SystemClock.uptimeMillis());
            game.bitrateHandler.postDelayed(() -> sendKeepBitrate(stream), waitMs);
        }
        if (automaticAfterAnswer) {
            automaticAfterAnswer = false;
            if (!bitrateHeld()) setRateChangeBitrateEnabled(true);
        }
    }

    // "Never: keep my bitrate": once the stream runs, its own bitrate goes to the PC as a bitrate change, which makes
    // the PC's link-rate and congestion controllers stand back for this stream. Once the PC said what it counts that
    // bitrate as, and no sooner than a second after the start, as the automatic bitrate's first sample is.
    private void sendKeepBitrate(StreamRateChanges stream) {
        // Turned off in the stream menu before this ran: nothing goes out, and the switch stays off.
        if (!game.connected || game.rateChanges != stream || !game.bitrateKept) return;
        int kbps = keepKbps();
        if (stream.sendTakeover(kbps, SystemClock.uptimeMillis())) {
            LimeLog.info("Keep my bitrate: sent " + kbps + " kbps as a bitrate change so the PC stops lowering it");
            scheduleRateChangeExpiry();
        } else {
            game.bitrateKept = false;
            LimeLog.warning("Keep my bitrate: the bitrate change could not be sent; the PC may still lower the bitrate");
        }
    }

    /**
     * What the bitrate dialog's Keep my bitrate shows: the PC keeps this stream's bitrate (after Never's takeover, the
     * switch or a bitrate the player chose here) and no controller of this device steers it. The dialog exists only
     * for a stream whose PC took its rate changes.
     */
    boolean keepBitrateShown() {
        return game.rateChanges != null && game.bitrateKept && !game.rateChanges.automaticOn() && !adaptation.isAdaptiveBitrateEnabled();
    }

    /**
     * The bitrate dialog's Keep my bitrate, stored as the performance overlay's switch is (GameOverlays.
     * togglePerformanceOverlay): only Lower bitrate's own keys go to the main settings through DualScreen.saveSettings,
     * as choosing Never or The PC decides in Settings writes it, and this PC's own Lower bitrate gives way to it; nothing
     * else stored is touched. On
     * sends the bitrate the player last chose in this stream, else the stream's own, as one bitrate change (nothing
     * when the PC already keeps it) and stops this device's own lowering. Off sends nothing: the PC has no message
     * that takes the bitrate back, so this stream keeps it and the notice says so.
     */
    void keepBitrateFromMenu(boolean on) {
        StreamRateChanges stream = game.rateChanges;
        if (stream == null) return;
        if (on) {
            boolean steering = stream.automaticOn() || adaptation.isAdaptiveBitrateEnabled();
            setRateChangeBitrateEnabled(false);
            if (adaptation.isAdaptiveBitrateEnabled()) adaptation.setAdaptiveBitrateEnabled(false, false);
            game.qualityAutoBitrate = false;
            if (askingStream == stream && !game.bitrateKept) {
                // Sent with the PC's answer, from the stream's bitrate as it counts it (streamBitrateKnown).
                game.bitrateKept = true;
                LimeLog.info("Keep my bitrate (stream menu): goes out once the PC answered the bitrate query");
            } else if (steering || !game.bitrateKept) {
                int kbps = keepKbps();
                if (game.connected && stream.sendTakeover(kbps, SystemClock.uptimeMillis())) {
                    LimeLog.info("Keep my bitrate (stream menu): sent " + kbps + " kbps as a bitrate change so the PC stops lowering it");
                    scheduleRateChangeExpiry();
                    game.bitrateKept = true;
                } else {
                    LimeLog.warning("Keep my bitrate (stream menu): the bitrate change could not be sent");
                    game.messages.alert(R.string.stream_rate_change_not_sent, Toast.LENGTH_LONG);
                }
            }
        } else {
            game.bitrateKept = false;
            LimeLog.info("Keep my bitrate turned off in the stream menu: The PC decides from the next stream; this stream keeps its bitrate");
            game.messages.alert(R.string.stream_keep_bitrate_off, Toast.LENGTH_LONG);
        }
        game.prefConfig.keepBitrate = on;
        if (!DualScreen.saveSettings(game, editor -> AutoBitrateChoice.storeKeepBitrate(editor, on))) {
            // The second screen keeps its settings as they were at its start; the change holds for its own stream.
            LimeLog.info("Keep my bitrate: the second screen's stream keeps the choice in memory only");
        } else {
            // Lower bitrate is one value: this PC's own choice gives way to the one just saved for every PC.
            AutoBitrateChoice.setForPc(game, game.getIntent().getStringExtra(Game.EXTRA_PC_UUID),
                    on ? AutoBitrateChoice.KEEP : AutoBitrateChoice.PC);
        }
    }

    /**
     * The bitrate Keep my bitrate keeps, at the stream's start and from the switch alike: the one the player last chose
     * in this stream, else the one the stream started with (negotiated; a link test before the launch may have lowered it).
     */
    int keepKbps() {
        if (game.playerBitrateKbps > 0) return game.playerBitrateKbps;
        return game.streamStartBitrateKbps > 0 ? game.streamStartBitrateKbps : game.prefConfig.bitrate;
    }

    /** Never is chosen or the PC keeps this stream's bitrate: no bitrate controller of Nightjar's runs. */
    boolean bitrateHeld() {
        return game.bitrateKept || (game.prefConfig != null && game.prefConfig.keepBitrate);
    }

    // The PC reports it holds the bitrate down for the network (a limited or floor notice).
    void noteLinkLowered(LinkNoticeStatus status) {
        // The overlay shows when its view is visible (GameOverlays.togglePerformanceOverlay).
        boolean overlayShowing = game.performanceOverlayScroll != null
                && game.performanceOverlayScroll.getVisibility() == android.view.View.VISIBLE;
        // On a PC that takes rate changes, whether it keeps the bitrate (a refused or unanswered takeover does not); elsewhere
        // Never alone keeps the notice quiet.
        boolean keep = game.rateChanges != null ? game.bitrateKept : game.prefConfig != null && game.prefConfig.keepBitrate;
        if (status == null || !GameAdaptation.tellLinkLowered(game.linkLoweredTold, overlayShowing, keep)) return;
        game.linkLoweredTold = true;
        java.util.Locale locale = game.getResources().getConfiguration().locale;
        // The stream menu's switch only where it works: on a PC that took this stream's rate changes.
        int text = game.rateChanges != null ? R.string.link_notice_keep : R.string.link_notice_lowered_only;
        game.messages.alert(game.getString(text, status.currentMbps(locale)), Toast.LENGTH_LONG);
    }

    // The host offered this stream 0x553C (rlRateChange).
    private boolean offersRateChange() {
        return game.connected && game.app != null && game.app.getRole() == NvApp.Role.STREAM &&
                MoonBridge.hostSupportsControlMessage(MoonBridge.CONTROL_MESSAGE_RATE_CHANGE);
    }

    // The next check for an unanswered 0x553C request, when one is outstanding.
    void scheduleRateChangeExpiry() {
        game.bitrateHandler.removeCallbacks(game.expireRateChanges);
        long at = game.rateChanges == null ? 0 : game.rateChanges.nextExpiryMs();
        if (at > 0) game.bitrateHandler.postAtTime(game.expireRateChanges, at);
    }

    // Also while it waits for the PC's answer to the query: no other controller takes the bitrate meanwhile, and the
    // menu's switch turns it off.
    boolean isRateChangeBitrateOn() {
        return game.rateChanges != null && (game.rateChanges.automaticOn() ||
                (automaticAfterAnswer && askingStream == game.rateChanges));
    }

    // Automatic bitrate on this device, for this stream; the host-side automatic bitrate stops,
    // since both would set the same encoder.
    void setRateChangeBitrateEnabled(boolean enabled) {
        game.bitrateHandler.removeCallbacks(game.updateRateChangeBitrate);
        if (game.rateChanges == null) return;
        if (!enabled) {
            automaticAfterAnswer = false;
            game.rateChanges.stopAutomatic();
            return;
        }
        game.sessionManualBitrate = 0;
        if (adaptation.isAdaptiveBitrateEnabled()) adaptation.setAdaptiveBitrateEnabled(false, false);
        game.qualityAutoBitrate = false;
        game.healthOriginalBitrate = game.healthTrialBitrate = 0;
        if (askingStream == game.rateChanges) {
            // Not from the setting's bitrate: it starts once the PC said what it counts this stream's as.
            automaticAfterAnswer = true;
            LimeLog.info("Automatic bitrate: starts once the PC answered the bitrate query");
            return;
        }
        game.rateChanges.startAutomatic(game.currentBitrate == 0 ? game.prefConfig.bitrate : game.currentBitrate,
                StreamRateChanges.clientLimitKbps(game.prefConfig.layers == null ? null : game.prefConfig.layers.network,
                        game.prefConfig.mobileBitrateLimit),
                SystemClock.uptimeMillis());
        // After an automatic reconnect it returns to the rate it had cut to, below the same ceiling.
        int resumeKbps = game.streamRecovery.takeAutomaticBitrateStart();
        if (resumeKbps > 0) game.rateChanges.automatic().resumeFrom(resumeKbps);
        game.bitrateHandler.postDelayed(game.updateRateChangeBitrate, 1000);
    }
}
