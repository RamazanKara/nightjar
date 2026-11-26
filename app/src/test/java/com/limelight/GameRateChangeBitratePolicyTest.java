package com.limelight;

import android.app.Application;
import android.content.Intent;
import android.widget.EditText;

import androidx.appcompat.app.AlertDialog;

import com.limelight.GameRateChangeTest.Harness;
import com.limelight.binding.video.MediaCodecDecoderRenderer;
import com.limelight.binding.video.MediaCodecHelper;
import com.limelight.nvstream.RateChangeResult;
import com.limelight.preferences.PreferenceConfiguration;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowDialog;
import org.robolectric.shadows.ShadowLooper;
import org.robolectric.shadows.ShadowToast;
import org.robolectric.util.ReflectionHelpers;

import java.util.List;

import static com.limelight.GameRateChangeTest.Harness.*;
import static com.limelight.nvstream.RateChangeResult.*;
import static org.junit.Assert.*;

/**
 * Tests "Keep my bitrate" takeover persistence, reconnect behaviour, and startup bitrate-unit
 * query handling against older and newer hosts.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class,
        shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class GameRateChangeBitratePolicyTest {
    // A Rubylight 6.0.1 PC counts a stream launched at the setting's 50 Mbps as its encoder's 37488 kbps.
    private static final int OLDER_PC_KBPS = 37_488;

    private final Harness h = new Harness();

    @Before
    public void streamingGame() {
        h.streamingGame();
    }

    @Test
    public void neverKeepMyBitrateSendsTheStreamsBitrateOnceAfterTheStartAndOnlyOnAPcThatTakesRateChanges() {
        h.prefs.keepBitrate = true;
        StreamRateChanges changes = h.recording();
        h.startStream(changes);
        assertTrue("nothing at the very start", h.sent.isEmpty());
        runFor(1);
        assertEquals(1, h.sent.size());
        assertEquals(50_000, h.sent.get(0)[1]);
        assertEquals(0, h.sent.get(0)[2]);
        assertFalse("no controller of this device runs", changes.automaticOn());
        runFor(20);
        assertEquals("once", 1, h.sent.size());

        // A PC that took no rate changes: nothing is sent and the stream runs as The PC decides.
        h.sent.clear();
        h.startStream(null);
        runFor(5);
        assertTrue(h.sent.isEmpty());

        // Another choice sends nothing.
        h.prefs.keepBitrate = false;
        h.startStream(h.recording());
        runFor(1);
        assertTrue(h.sent.isEmpty());
    }

    @Test
    public void keepMyBitrateIsAbsentOnAPcThatTookNoRateChanges() {
        h.startStream(null);
        ReflectionHelpers.callInstanceMethod(h.game.adaptation, "showRateChangeDialog");
        assertNull("no bitrate dialog, so no switch", ShadowDialog.getLatestDialog());
        h.game.adaptation.keepBitrateFromMenu(true);
        assertTrue(h.sent.isEmpty());
        assertFalse(h.stored().contains(com.limelight.preferences.AutoBitrateChoice.SETTINGS_KEY));
        assertFalse(h.game.adaptation.keepBitrateShown());
    }

    @Test
    public void turningKeepMyBitrateOnSendsOneChangeAtTheStreamsBitrateAndStoresNever() {
        h.startStream(h.recording());
        AlertDialog dialog = h.openDialog();
        com.google.android.material.materialswitch.MaterialSwitch keep = keepSwitch(dialog);
        assertFalse(keep.isChecked());
        keep.setChecked(true);
        ShadowLooper.idleMainLooper();
        assertEquals(1, h.sent.size());
        assertEquals("the stream's bitrate", 50_000, h.sent.get(0)[1]);
        assertEquals("only the bitrate", 0, h.sent.get(0)[2]);
        assertTrue(dialog.isShowing());
        // Stored as choosing Never in Settings.
        assertEquals(com.limelight.preferences.AutoBitrateChoice.KEEP, h.stored().getString(com.limelight.preferences.AutoBitrateChoice.SETTINGS_KEY, null));
        assertTrue(h.prefs.keepBitrate);
        assertTrue(PreferenceConfiguration.readPreferences(h.game).keepBitrate);
        // Its answer shows nothing, and the switch stays on when the dialog opens again; nothing more goes out.
        int toasts = ShadowToast.shownToastCount();
        h.game.rateChange(new RateChangeResult(h.sent.get(0)[0], 50_000, 0, STATUS_APPLIED, REASON_NONE, STATUS_UNCHANGED, REASON_NONE));
        ShadowLooper.idleMainLooper();
        assertEquals(toasts, ShadowToast.shownToastCount());
        dialog.dismiss();
        assertTrue(keepSwitch(h.openDialog()).isChecked());
        assertEquals(1, h.sent.size());
    }

    @Test
    public void keepMyBitrateKeepsTheBitrateThePlayerChoseAndShowsOnOnceTheyChoseOne() {
        h.startStream(h.recording());
        AlertDialog dialog = h.openDialog();
        fields(dialog).get(0).setText("30");
        send(dialog);
        h.game.rateChange(new RateChangeResult(h.sent.get(0)[0], 30_000, 60_000, STATUS_APPLIED, REASON_NONE, STATUS_UNCHANGED, REASON_NONE));
        ShadowLooper.idleMainLooper();
        // The chosen bitrate already makes the PC stand back: on, and nothing to send.
        com.google.android.material.materialswitch.MaterialSwitch keep = keepSwitch(h.openDialog());
        assertTrue(keep.isChecked());
        keep.setChecked(false);
        keep.setChecked(true);
        ShadowLooper.idleMainLooper();
        assertEquals(2, h.sent.size());
        assertEquals("the player's bitrate, not the stream's", 30_000, h.sent.get(1)[1]);
    }

    @Test
    public void aStreamStartedWithNeverShowsTheSwitchOnAndSendsNothingMore() {
        h.prefs.keepBitrate = true;
        h.startStream(h.recording());
        assertTrue("on before the takeover", keepSwitch(h.openDialog()).isChecked());
        runFor(1);
        assertEquals(1, h.sent.size());
        assertTrue(keepSwitch(h.openDialog()).isChecked());
        runFor(5);
        assertEquals("nothing more", 1, h.sent.size());
    }

    @Test
    public void turnedOffBeforeTheStartsTakeoverWentOutNothingIsSent() {
        h.prefs.keepBitrate = true;
        h.startStream(h.recording());
        // Within the first second, before the stream start's delayed takeover runs.
        keepSwitch(h.openDialog()).setChecked(false);
        ShadowLooper.idleMainLooper();
        runFor(5);
        assertEquals("the player turned it off first", 0, h.sent.size());
        assertFalse(h.game.adaptation.keepBitrateShown());
        assertFalse(h.prefs.keepBitrate);
    }

    @Test
    public void turningKeepMyBitrateOffStoresThePcDecidesAndSendsNothing() {
        h.stored().edit().putString(com.limelight.preferences.AutoBitrateChoice.SETTINGS_KEY, com.limelight.preferences.AutoBitrateChoice.KEEP).commit();
        h.prefs.keepBitrate = true;
        h.startStream(h.recording());
        runFor(1);
        assertEquals(1, h.sent.size());
        com.google.android.material.materialswitch.MaterialSwitch keep = keepSwitch(h.openDialog());
        keep.setChecked(false);
        ShadowLooper.idleMainLooper();
        assertEquals("nothing goes out", 1, h.sent.size());
        assertEquals(com.limelight.preferences.AutoBitrateChoice.PC, h.stored().getString(com.limelight.preferences.AutoBitrateChoice.SETTINGS_KEY, null));
        assertFalse(h.prefs.keepBitrate);
        assertEquals(h.game.getString(R.string.stream_keep_bitrate_off), ShadowToast.getTextOfLatestToast());
        runFor(5);
        assertEquals(1, h.sent.size());
    }

    @Test
    public void neverRunsNoBitrateControllerOfNightjarsEvenWhereAPcsStoredChoiceIsBusy() {
        com.limelight.preferences.AutoBitrateChoice.setForPc(h.game, "rate-change-pc", com.limelight.preferences.AutoBitrateChoice.SOONER);
        h.prefs.keepBitrate = true;
        h.startStream(h.recording());
        assertFalse(h.game.adaptation.isAdaptiveBitrateEnabled());
        assertFalse((boolean) ReflectionHelpers.callInstanceMethod(h.game.adaptation, "isRateChangeBitrateOn"));
        assertTrue(keepSwitch(h.openDialog()).isChecked());
        // The control case: without Never the PC's stored choice starts the busy-network controller.
        h.prefs.keepBitrate = false;
        h.startStream(h.recording());
        assertTrue(h.game.adaptation.isAdaptiveBitrateEnabled());
        // Automatic quality then steps only the resolution and frame rate.
        assertFalse(GameAdaptation.qualityTakesBitrate(true, true, false, false, 0));
        assertTrue(GameAdaptation.qualityTakesBitrate(false, true, false, false, 0));
    }

    @Test
    public void aTakeoverThePcRefusedOrNeverAnsweredIsNotKept() {
        h.prefs.keepBitrate = true;
        h.prefs.enablePerfOverlay = false;
        h.startStream(h.recording());
        runFor(1);
        assertTrue(h.game.adaptation.keepBitrateShown());
        h.game.rateChange(new RateChangeResult(h.sent.get(0)[0], 50_000, 0, STATUS_REFUSED, REASON_DISABLED, STATUS_UNCHANGED, REASON_NONE));
        ShadowLooper.idleMainLooper();
        assertFalse("refused: the switch shows off", h.game.adaptation.keepBitrateShown());
        h.game.adaptation.noteLinkLowered(com.limelight.binding.video.LinkNoticeStatus.parse(1, 50_000, 18_400));
        assertNotNull("and the notice may say the PC lowered it", ShadowToast.getLatestToast());

        // No answer in the usual time.
        h.sent.clear();
        h.startStream(h.recording());
        runFor(1);
        assertTrue(h.game.adaptation.keepBitrateShown());
        runFor((int) (StreamRateChanges.ANSWER_MS / 1000) + 1);
        assertFalse(h.game.adaptation.keepBitrateShown());
    }

    @Test
    public void aReconnectSendsTheTakeoverAgainOnTheNewSession() {
        // The switch turned on, then the stream reconnects: exactly one takeover on the new session.
        h.startStream(h.recording());
        keepSwitch(h.openDialog()).setChecked(true);
        ShadowLooper.idleMainLooper();
        assertEquals(1, h.sent.size());
        ShadowDialog.reset();
        h.sent.clear();
        h.startStream(h.recording());
        runFor(5);
        assertEquals(1, h.sent.size());
        assertEquals(50_000, h.sent.get(0)[1]);

        // A bitrate the player chose before the reconnect: sent again at that bitrate.
        h.prefs.keepBitrate = false;
        ReflectionHelpers.setField(h.game, "sessionManualBitrate", 30_000);
        ReflectionHelpers.setField(h.game, "currentBitrate", 30_000);
        h.sent.clear();
        h.startStream(h.recording());
        runFor(5);
        assertEquals(1, h.sent.size());
        assertEquals(30_000, h.sent.get(0)[1]);
    }

    @Test
    public void theSwitchSendsTheBitrateTheStreamStartedWithNotTheSetting() {
        // A link test before the launch started the stream below the setting.
        ReflectionHelpers.setField(h.game, "currentBitrate", 40_000);
        h.startStream(h.recording());
        keepSwitch(h.openDialog()).setChecked(true);
        ShadowLooper.idleMainLooper();
        assertEquals(40_000, h.sent.get(0)[1]);
    }

    @Test
    public void theSwitchStoresOnlyLowerBitratesKeysAsTheOverlaysSwitchDoes() {
        com.limelight.preferences.AutoBitrateChoice.setForPc(h.game, "another-pc", com.limelight.preferences.AutoBitrateChoice.SOONER);
        h.startStream(h.recording());
        keepSwitch(h.openDialog()).setChecked(true);
        ShadowLooper.idleMainLooper();
        assertEquals("another PC's own choice stays", com.limelight.preferences.AutoBitrateChoice.SOONER, h.game.getSharedPreferences(
                com.limelight.preferences.AutoBitrateChoice.PC_STORE, android.content.Context.MODE_PRIVATE).getString("another-pc", null));
        assertEquals(com.limelight.preferences.AutoBitrateChoice.KEEP, h.stored().getString(com.limelight.preferences.AutoBitrateChoice.SETTINGS_KEY, null));
    }

    @Test
    public void onTheSecondScreenTheSwitchHoldsForItsStreamWithoutStoring() {
        h.game.setIntent(new Intent().putExtra(Game.EXTRA_PC_UUID, "rate-change-pc").putExtra(DualScreen.EXTRA_SECOND_SCREEN, true));
        h.startStream(h.recording());
        keepSwitch(h.openDialog()).setChecked(true);
        ShadowLooper.idleMainLooper();
        assertEquals(1, h.sent.size());
        assertTrue(h.prefs.keepBitrate);
        assertFalse(h.stored().contains(com.limelight.preferences.AutoBitrateChoice.SETTINGS_KEY));
    }

    @Test
    public void onAnOlderPcKeepMyBitrateSendsWhatThePcAnsweredAndTheSliderShowsIt() {
        h.prefs.keepBitrate = true;
        h.startStreamAsking(h.recording());
        assertEquals("the query goes out at once", 1, h.queries.size());
        runFor(3);
        assertTrue("nothing sets a bitrate before the answer", h.sent.isEmpty());
        int toasts = ShadowToast.shownToastCount();
        h.answerQuery(OLDER_PC_KBPS);
        assertEquals(1, h.sent.size());
        assertEquals("not the setting's 50000, which would raise the encoder", OLDER_PC_KBPS, h.sent.get(0)[1]);
        assertEquals(0, h.sent.get(0)[2]);
        assertEquals(java.util.Arrays.asList("query", "change"), h.order);
        assertEquals("the answer says nothing", toasts, ShadowToast.shownToastCount());
        assertEquals(OLDER_PC_KBPS, (int) ReflectionHelpers.getField(h.game, "currentBitrate"));
        assertEquals("37.488", fields(h.openDialog()).get(0).getText().toString());
        // Its answer is the takeover's, and the switch stays on.
        h.game.rateChange(new RateChangeResult(h.sent.get(0)[0], OLDER_PC_KBPS, 60_000, STATUS_APPLIED, REASON_NONE,
                STATUS_UNCHANGED, REASON_NONE));
        ShadowLooper.idleMainLooper();
        assertTrue(h.game.adaptation.keepBitrateShown());
        runFor(10);
        assertEquals("once", 1, h.sent.size());
    }

    @Test
    public void onAnOlderPcAutomaticBitrateStartsFromWhatThePcAnswered() {
        h.prefs.rateChangeBitrate = true;
        ReflectionHelpers.setStaticField(MediaCodecHelper.class, "initialized", true);
        ReflectionHelpers.setField(h.game, "decoderRenderer", new MediaCodecDecoderRenderer(h.game, h.prefs, null, 0,
                false, false, "", null) {
            @Override public boolean hasRecentVideoFrames(long nowMs) { return true; }
            @Override public float getNetworkFrameLossPercent() { return 10; }
        });
        h.startStreamAsking(h.recording());
        assertNull("not from the setting's bitrate before the answer", h.game.rateChanges.automatic());
        assertTrue("it owns the bitrate meanwhile, so automatic quality starts no other controller",
                (boolean) ReflectionHelpers.callInstanceMethod(h.game.adaptation, "isRateChangeBitrateOn"));
        assertFalse(GameAdaptation.qualityTakesBitrate(false, true, false,
                (boolean) ReflectionHelpers.callInstanceMethod(h.game.adaptation, "isRateChangeBitrateOn"), 0));
        runFor(3);
        assertTrue("nothing before the answer", h.sent.isEmpty());
        h.answerQuery(OLDER_PC_KBPS);
        assertTrue((boolean) ReflectionHelpers.callInstanceMethod(h.game.adaptation, "isRateChangeBitrateOn"));
        assertEquals(OLDER_PC_KBPS, h.game.rateChanges.automatic().currentKbps());
        for (int i = 0; i < 10 && h.sent.isEmpty(); i++) runFor(1);
        assertFalse("loss cuts the bitrate", h.sent.isEmpty());
        assertTrue("the first cut is below the stream's " + OLDER_PC_KBPS + ": " + h.sent.get(0)[1], h.sent.get(0)[1] < OLDER_PC_KBPS);
    }

    @Test
    public void onANewerPcTheAnswerIsTheStreamsOwnBitrateAndEverythingIsAsBefore() {
        h.prefs.keepBitrate = true;
        h.startStreamAsking(h.recording());
        h.answerQuery(50_000);
        assertTrue("a second into the stream, as before", h.sent.isEmpty());
        runFor(1);
        assertEquals(1, h.sent.size());
        assertEquals(50_000, h.sent.get(0)[1]);
        assertEquals(50_000, (int) ReflectionHelpers.getField(h.game, "currentBitrate"));
    }

    @Test
    public void withNoAnswerTheStreamGoesOnAsBeforeOnceAndALateAnswerChangesNothing() {
        h.prefs.keepBitrate = true;
        h.startStreamAsking(h.recording());
        runFor((int) (StreamRateChanges.ANSWER_MS / 1000) - 1);
        assertTrue("waits for the answer", h.sent.isEmpty());
        int toasts = ShadowToast.shownToastCount();
        runFor(1);
        assertEquals(1, h.sent.size());
        assertEquals("the stream's own, as before the query", 50_000, h.sent.get(0)[1]);
        assertEquals("quietly", toasts, ShadowToast.shownToastCount());
        h.answerQuery(OLDER_PC_KBPS);
        runFor(5);
        assertEquals("once", 1, h.sent.size());
        assertEquals(50_000, (int) ReflectionHelpers.getField(h.game, "currentBitrate"));
        assertEquals(toasts, ShadowToast.shownToastCount());

        // Automatic bitrate starts then too, from the stream's own.
        h.prefs.keepBitrate = false;
        h.prefs.rateChangeBitrate = true;
        h.startStreamAsking(h.recording());
        runFor((int) (StreamRateChanges.ANSWER_MS / 1000));
        assertTrue((boolean) ReflectionHelpers.callInstanceMethod(h.game.adaptation, "isRateChangeBitrateOn"));
        assertEquals(50_000, h.game.rateChanges.automatic().currentKbps());
    }

    @Test
    public void aQueryThatCannotBeSentYetGoesOnceMoreASecondLater() {
        h.prefs.keepBitrate = true;
        h.unsentQueries = 1;
        h.startStreamAsking(h.recording());
        assertTrue(h.queries.isEmpty());
        runFor(1);
        assertEquals("again a second later", 1, h.queries.size());
        assertTrue(h.sent.isEmpty());
        // Video ran meanwhile, so a cut by the PC may be in the answer: not kept, the setting's number goes out.
        h.answerQuery(OLDER_PC_KBPS);
        assertEquals(50_000, h.sent.get(0)[1]);
        assertEquals(50_000, (int) ReflectionHelpers.getField(h.game, "currentBitrate"));

        // Twice unsent: the stream goes on from its own bitrate, as before the query.
        h.sent.clear();
        h.queries.clear();
        ReflectionHelpers.setField(h.game, "currentBitrate", 50_000);
        h.unsentQueries = 2;
        h.startStreamAsking(h.recording());
        runFor(1);
        assertTrue(h.queries.isEmpty());
        assertEquals(1, h.sent.size());
        assertEquals(50_000, h.sent.get(0)[1]);
    }

    @Test
    public void aReconnectAfterThePlayerChoseABitrateSendsThatBitrateWhateverThePcAnswers() {
        // The player's 30 Mbps came from this PC's own answers, so it is in the PC's unit already.
        ReflectionHelpers.setField(h.game, "sessionManualBitrate", 30_000);
        ReflectionHelpers.setField(h.game, "currentBitrate", 30_000);
        h.startStreamAsking(h.recording());
        h.answerQuery(22_488);
        runFor(1);
        assertEquals(1, h.sent.size());
        assertEquals(30_000, h.sent.get(0)[1]);
    }

    @Test
    public void aCutThePcMadeBeforeTheAnswerIsNotKept() {
        h.prefs.keepBitrate = true;
        h.startStreamAsking(h.recording());
        // The PC's link-rate controller lowered the bitrate before it handled the query: its answer is the cut.
        h.game.linkNotice(1, 50_000, 30_000);
        ShadowLooper.idleMainLooper();
        h.answerQuery(30_000);
        runFor(1);
        assertEquals(1, h.sent.size());
        assertEquals("the setting's number, as before the query", 50_000, h.sent.get(0)[1]);
        assertEquals(50_000, (int) ReflectionHelpers.getField(h.game, "currentBitrate"));

        // Automatic bitrate starts from the setting's number too.
        h.prefs.keepBitrate = false;
        h.prefs.rateChangeBitrate = true;
        h.startStreamAsking(h.recording());
        h.game.linkNotice(2, 50_000, 5_000);
        ShadowLooper.idleMainLooper();
        h.answerQuery(5_000);
        assertEquals(50_000, h.game.rateChanges.automatic().currentKbps());

        // A notice from an earlier stream does not count against this one.
        h.prefs.rateChangeBitrate = false;
        h.prefs.keepBitrate = true;
        h.sent.clear();
        h.startStreamAsking(h.recording());
        h.answerQuery(OLDER_PC_KBPS);
        runFor(1);
        assertEquals(OLDER_PC_KBPS, h.sent.get(0)[1]);
    }

    /** A frame rate is no matter of the bitrate's unit: picked before the answer it goes out at once, the bitrate unchanged. */
    @Test
    public void aFrameRatePickedBeforeTheAnswerGoesOutAtOnceAndLeavesTheBitrateAlone() {
        h.startStreamAsking(h.recording());
        AlertDialog dialog = h.openDialog();
        List<EditText> fields = fields(dialog);
        assertFalse("the bitrate waits for the PC's answer", fields.get(0).isEnabled());
        // A bitrate typed anyway is refused by the state, not only by the disabled field.
        fields.get(0).setText("30");
        fields.get(1).setText("120");
        send(dialog);
        assertEquals(1, h.sent.size());
        assertEquals("the bitrate unchanged", 0, h.sent.get(0)[1]);
        assertEquals(120_000, h.sent.get(0)[2]);
        h.answerQuery(OLDER_PC_KBPS);
        assertEquals(OLDER_PC_KBPS, (int) ReflectionHelpers.getField(h.game, "currentBitrate"));
        assertEquals("nothing more", 1, h.sent.size());
    }

    @Test
    public void whileTheQueryWaitsTheBitrateCannotChange() {
        h.startStreamAsking(h.recording());
        assertEquals("the slider snaps back", 50_000, h.game.adaptation.applyBitrateFromMenu(30_000));
        assertTrue(h.sent.isEmpty());
        // The dialog opened before the answer is usable once it comes, at the PC's number.
        AlertDialog dialog = h.openDialog();
        assertFalse(fields(dialog).get(0).isEnabled());
        h.answerQuery(OLDER_PC_KBPS);
        assertTrue(fields(dialog).get(0).isEnabled());
        assertEquals("37.488", fields(dialog).get(0).getText().toString());
        send(dialog);
        assertTrue("left at the PC's number it is unchanged", h.sent.isEmpty());
        assertEquals(30_000, h.game.adaptation.applyBitrateFromMenu(30_000));
        assertEquals(30_000, h.sent.get(0)[1]);
    }

    // The open Control Center on Picture, where the bitrate's slider stands.
    private Object pictureModule() {
        com.limelight.ui.StreamMenu menu = ReflectionHelpers.getField(h.game.menuActions, "streamMenuSheet");
        if (menu.reveal(com.limelight.ui.StreamMenu.Item.BITRATE) == null) {
            assertNotNull(menu.reveal(com.limelight.ui.StreamMenu.Item.RATE_CHANGE));
        }
        assertEquals(com.limelight.ui.ControlCenterModel.PICTURE, menu.module());
        return menu;
    }

    @Test
    public void anOpenMenusSliderWaitsForTheAnswerAndShowsTheStreamsNumber() {
        // A clean answer: usable at the PC's number.
        h.startStreamAsking(h.recording());
        h.game.showStreamMenu();
        Object sheet = pictureModule();
        com.limelight.ui.StepSlider slider = ReflectionHelpers.callInstanceMethod(sheet, "bitrateSlider");
        assertFalse("disabled while the query waits", slider.isEnabled());
        assertEquals(com.limelight.ui.BitrateSlider.progress(50_000), slider.getPosition());
        h.answerQuery(OLDER_PC_KBPS);
        assertTrue(slider.isEnabled());
        assertEquals(com.limelight.ui.BitrateSlider.progress(OLDER_PC_KBPS), slider.getPosition());

        // A reconnect with the menu open: a new wait, disabled again, and no pick left over.
        ReflectionHelpers.setField(h.game, "currentBitrate", 50_000);
        h.startStreamAsking(h.recording());
        assertFalse(slider.isEnabled());
        assertEquals(com.limelight.ui.BitrateSlider.progress(50_000), slider.getPosition());
        assertEquals(50_000, h.game.adaptation.applyBitrateFromMenu(30_000));
        assertTrue(h.sent.isEmpty());
        // An answer that is not used: usable at the stream's own number.
        h.game.linkNotice(1, 50_000, 30_000);
        ShadowLooper.idleMainLooper();
        h.answerQuery(30_000);
        assertTrue(slider.isEnabled());
        assertEquals(com.limelight.ui.BitrateSlider.progress(50_000), slider.getPosition());

        // No answer: usable after the timeout, at the stream's own number.
        h.startStreamAsking(h.recording());
        assertFalse(slider.isEnabled());
        runFor((int) (StreamRateChanges.ANSWER_MS / 1000));
        assertTrue(slider.isEnabled());
        assertEquals(com.limelight.ui.BitrateSlider.progress(50_000), slider.getPosition());
        assertTrue("no pick went out", h.sent.isEmpty());
    }

    /**
     * A slider the player let go of just before a reconnect's wait began: the PC's answer moves it, and the settle
     * left from before must not send that number as the player's own.
     */
    @Test
    public void aSliderLetGoJustBeforeTheWaitSendsNothingAfterTheAnswer() {
        h.prefs.rateChangeBitrate = true;
        h.startStream(h.recording());
        h.game.showStreamMenu();
        Object sheet = pictureModule();
        com.limelight.ui.StepSlider slider = ReflectionHelpers.callInstanceMethod(sheet, "bitrateSlider");
        // A D-pad step: the settle goes out SETTLE_MS later.
        slider.setPosition(com.limelight.ui.BitrateSlider.progress(30_000));
        com.limelight.ui.StepSlider.OnPositionChangeListener listener = ReflectionHelpers.getField(slider, "listener");
        listener.onPositionChanged(slider, slider.getPosition());
        // A reconnect starts the stream's bitrate wait within that time, and the PC answers at once.
        h.sent.clear();
        h.startStreamAsking(h.recording());
        h.answerQuery(OLDER_PC_KBPS);
        runFor(1);
        assertTrue("nothing goes out as the player's bitrate: " + h.sent.size(), h.sent.isEmpty());
        assertTrue((boolean) ReflectionHelpers.callInstanceMethod(h.game.adaptation, "isRateChangeBitrateOn"));
        assertEquals(com.limelight.ui.BitrateSlider.progress(OLDER_PC_KBPS), slider.getPosition());
    }

    @Test
    public void automaticBitrateTurnedOffBeforeTheAnswerStaysOff() {
        h.prefs.rateChangeBitrate = true;
        h.startStreamAsking(h.recording());
        ReflectionHelpers.callInstanceMethod(h.game.adaptation, "toggleAdaptiveBitrateFromMenu");
        assertFalse((boolean) ReflectionHelpers.callInstanceMethod(h.game.adaptation, "isRateChangeBitrateOn"));
        h.answerQuery(OLDER_PC_KBPS);
        assertFalse((boolean) ReflectionHelpers.callInstanceMethod(h.game.adaptation, "isRateChangeBitrateOn"));
        runFor(15);
        assertTrue(h.sent.isEmpty());
    }

    @Test
    public void keepMyBitrateTurnedOnBeforeTheAnswerSendsWhatThePcAnswered() {
        h.startStreamAsking(h.recording());
        keepSwitch(h.openDialog()).setChecked(true);
        ShadowLooper.idleMainLooper();
        assertTrue("nothing before the answer", h.sent.isEmpty());
        assertTrue(h.game.adaptation.keepBitrateShown());
        h.answerQuery(OLDER_PC_KBPS);
        runFor(1);
        assertEquals(1, h.sent.size());
        assertEquals(OLDER_PC_KBPS, h.sent.get(0)[1]);
    }
}
