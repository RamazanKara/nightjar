package com.limelight;

import com.limelight.computers.HostNotices;
import com.limelight.nvstream.AdaptiveQualityController;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.ui.NavigationDialogBuilder;
import android.content.Context;
import android.content.SharedPreferences;
import android.os.SystemClock;
import android.widget.Toast;

/** Automatic quality controller wiring: availability checks, quality-mode persistence, stream reconfigure steps, and quality dialog. */
final class GameAdaptiveQuality {
    private final Game game;
    private final GameAdaptation adaptation;

    // Automatic quality chosen in the second screen's stream menu, which its process does not save
    // (DualScreen.saveSettings): it holds for that stream only. Null choice: the settings' mode.
    private boolean qualityChosenHere;
    private String qualityChoice;
    // What supportsAdaptiveQuality last answered, with no decoder rebuild in progress then; null until
    // it was first asked (at stream start and after each rebuild, inside the decoder's door).
    private Boolean adaptiveQualityShown;

    GameAdaptiveQuality(Game game, GameAdaptation adaptation) {
        this.game = game;
        this.adaptation = adaptation;
    }

    AdaptiveQualityController.Mode qualityMode() {
        String chosen = qualityChosenHere ? qualityChoice : game.getSharedPreferences("AdaptiveQuality", Context.MODE_PRIVATE)
                .getString(game.getIntent().getStringExtra(Game.EXTRA_PC_UUID), null);
        return AdaptiveQualityController.Mode.parse(chosen != null ? chosen : game.prefConfig.adaptiveQualityMode.preference());
    }

    // For a menu or help text, which cannot wait: during a decoder rebuild, whose limits are cleared
    // for a moment, what the decoder last answered. A rebuild under way before it ever answered: what
    // the stream and the PC allow; a choice then waits for the decoder, which says whether it can.
    boolean showsAdaptiveQuality() {
        if (game.decoderRenderer == null) return supportsAdaptiveQuality();
        if (!game.decoderRenderer.runUnlessRebuilding(this::supportsAdaptiveQuality) && adaptiveQualityShown == null) {
            return streamAllowsAdaptiveQuality();
        }
        return game.connected && adaptiveQualityShown;
    }

    // Asked only with no rebuild in progress: inside a whenDecoderReady action or showsAdaptiveQuality.
    boolean supportsAdaptiveQuality() {
        boolean supported = streamAllowsAdaptiveQuality() && game.decoderRenderer.getReconfigureLimits() != null;
        adaptiveQualityShown = supported;
        return supported;
    }

    private boolean streamAllowsAdaptiveQuality() {
        return game.connected && game.decoderRenderer != null && game.app != null && game.app.getRole() == NvApp.Role.STREAM &&
                (game.decoderRenderer.getActiveVideoFormat() & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) == 0 &&
                MoonBridge.hostSupportsControlMessage(MoonBridge.CONTROL_MESSAGE_RECONFIGURE);
    }

    void startAdaptiveQuality() {
        game.bitrateHandler.removeCallbacks(game.updateQuality);
        game.videoOutput.dropDecoderAction("tick");
        if (game.adaptiveQuality != null) game.adaptiveQuality.close(); game.adaptiveQuality = null;
        game.pendingQualityTarget = null;
        game.qualityStatus = null;
        if (game.decoderRenderer != null) game.decoderRenderer.resetDecodeTiming();
        AdaptiveQualityController.Mode mode = qualityMode();
        if (mode == AdaptiveQualityController.Mode.OFF) {
            releaseQualityAutoBitrate();
            return;
        }
        String pcUuid = game.getIntent().getStringExtra(Game.EXTRA_PC_UUID);
        if (!supportsAdaptiveQuality()) {
            releaseQualityAutoBitrate();
            game.qualityStatus = game.getString(R.string.quality_unavailable);
            // Said once per PC; the performance overlay keeps showing it.
            if (game.foreground) game.messages.notice(pcUuid, HostNotices.Notice.AUTOMATIC_QUALITY_UNAVAILABLE, game.qualityStatus);
            return;
        }
        HostNotices.reset(game, pcUuid, HostNotices.Notice.AUTOMATIC_QUALITY_UNAVAILABLE);
        game.healthOriginalBitrate = game.healthTrialBitrate = 0;
        int[] limits = game.decoderRenderer.getReconfigureLimits();
        game.adaptiveQuality = new AdaptiveQualityController(mode, game.qualityMaxWidth, game.qualityMaxHeight, game.qualityMaxFps,
                game.prefConfig.qualityMinShortEdge, game.prefConfig.qualityMinFps, limits[2], limits[3], SystemClock.uptimeMillis());
        // After an automatic reconnect it returns to the step it had reached; the ladder may climb back.
        int[] step = game.streamRecovery.takeQualityStep();
        if (step != null) game.adaptiveQuality.resumeAt(step[0], step[1], step[2]);
        // A user changing the mode returns to the session's ceiling before the new policy starts.
        int[] shown = game.decoderRenderer.getStreamSize();
        if ((!game.adaptiveQuality.current().matches(shown[0], shown[1], game.streamFps) ||
                game.reconfigureRequester.fpsMillihz() != qualityCeilingFrameRateMilliHz()) &&
                !applyAdaptiveQuality(game.adaptiveQuality.current(), SystemClock.uptimeMillis())) return;
        // Automatic quality needs it for this session only; the saved choice stays the user's own. Never keeps the
        // bitrate: automatic quality then steps only the resolution and frame rate.
        if (GameAdaptation.qualityTakesBitrate(adaptation.bitrateHeld(), adaptation.supportsAdaptiveBitrate(),
                adaptation.isAdaptiveBitrateEnabled(), adaptation.isRateChangeBitrateOn(), game.sessionManualBitrate,
                !adaptation.clientTookBitrate && MoonBridge.hostSupportsControlMessage(MoonBridge.CONTROL_MESSAGE_LINK_NOTICE))) {
            adaptation.setAdaptiveBitrateEnabled(true, false);
            game.qualityAutoBitrate = true;
        }
        game.bitrateHandler.postDelayed(game.updateQuality, 1000);
    }

    boolean applyAdaptiveQuality(AdaptiveQualityController.Target target, long now) {
        return applyAdaptiveQuality(target, now, qualityCeilingFrameRateMilliHz());
    }

    int qualityCeilingFrameRateMilliHz() {
        return game.qualityMaxFrameRateMilliHz > 0 ? game.qualityMaxFrameRateMilliHz : game.qualityMaxFps * 1000;
    }

    boolean applyAdaptiveQuality(AdaptiveQualityController.Target target, long now, int ceilingMilliHz) {
        int[] request = target.request();
        if (target.fps == Math.round(ceilingMilliHz / 1000f)) request[2] = ceilingMilliHz;
        if (!game.videoOutput.requestStreamReconfigure(request)) {
            stopAdaptiveQualityWithError();
            return false;
        }
        game.healthOriginalBitrate = game.healthTrialBitrate = 0;
        game.adaptiveQuality.applied(target, now);
        game.pendingQualityTarget = target;
        game.qualityRequestSentAt = now;
        return true;
    }

    // The player's choice in the quality dialog, now or once a decoder rebuild is done: Off returns the
    // stream to its ceiling.
    void applyQualityChoice() {
        game.videoOutput.whenDecoderReady("choice", this::takeQualityChoice);
    }

    private void takeQualityChoice() {
        startAdaptiveQuality();
        if (qualityMode() == AdaptiveQualityController.Mode.OFF && supportsAdaptiveQuality()) {
            game.videoOutput.requestStreamReconfigure(new int[] {game.qualityMaxWidth, game.qualityMaxHeight, qualityCeilingFrameRateMilliHz()});
        }
    }

    void stopAdaptiveQualityWithError() {
        if (game.adaptiveQuality != null) game.adaptiveQuality.close(); game.adaptiveQuality = null;
        game.pendingQualityTarget = null;
        game.bitrateHandler.removeCallbacks(game.updateQuality);
        releaseQualityAutoBitrate();
        game.qualityStatus = game.getString(R.string.quality_request_failed);
        LimeLog.warning("Automatic quality: requested size was not confirmed; adaptation stopped");
        if (game.foreground) game.messages.alert(game.qualityStatus, Toast.LENGTH_LONG);
    }

    void showAdaptiveQualityDialog() {
        String[] modes = game.getResources().getStringArray(R.array.adaptive_quality_names);
        String[] choices = new String[] {game.getString(R.string.quality_use_settings), modes[0], modes[1], modes[2], modes[3]};
        SharedPreferences saved = game.getSharedPreferences("AdaptiveQuality", Context.MODE_PRIVATE);
        String key = game.getIntent().getStringExtra(Game.EXTRA_PC_UUID);
        int selected = (qualityChosenHere ? qualityChoice != null : saved.contains(key)) ? qualityMode().ordinal() + 1 : 0;
        game.menuActions.showStreamDialog(new NavigationDialogBuilder(game).setTitle(R.string.title_adaptive_quality)
                .setSingleChoiceItems(choices, selected, (dialog, which) -> {
                    if (!game.foreground || !game.connected || game.hostActionInProgress) return;
                    String choice = which == 0 ? null : AdaptiveQualityController.Mode.values()[which - 1].preference();
                    if (!DualScreen.saveSettings(saved, game.getIntent(), editor -> {
                        if (choice == null) editor.remove(key);
                        else editor.putString(key, choice);
                    })) {
                        // The second screen's choice holds for its own stream only.
                        qualityChosenHere = true;
                        qualityChoice = choice;
                    }
                    dialog.dismiss();
                    applyQualityChoice();
                })
                .setNeutralButton(R.string.quality_about, (dialog, which) ->
                        game.menuActions.showStreamDialog(new NavigationDialogBuilder(game).setTitle(R.string.title_adaptive_quality)
                                .setMessage(game.getString(R.string.quality_stream_help) + "\n\n" +
                                        game.getString(showsAdaptiveQuality() ? R.string.summary_adaptive_quality : R.string.quality_unavailable))
                                .setPositiveButton(android.R.string.ok, null).create()))
                .setNegativeButton(android.R.string.cancel, null).create());
    }

    // Automatic quality is off now: auto bitrate goes back to the person's choice, which was off.
    private void releaseQualityAutoBitrate() {
        if (!game.qualityAutoBitrate) return;
        game.qualityAutoBitrate = false;
        if (adaptation.isAdaptiveBitrateEnabled()) adaptation.setAdaptiveBitrateEnabled(false, false);
    }
}
