package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import android.app.Dialog;
import android.view.View;
import android.widget.TextView;

import androidx.appcompat.app.AppCompatActivity;

import com.limelight.R;
import com.limelight.nvstream.jni.CurtainBridge;
import com.limelight.nvstream.jni.NightjarJni;

import org.junit.After;
import org.junit.Assume;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowLooper;

import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

/** The launch curtain keeps the connecting screen over the stream, shows the host's word, and always lifts. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34)
public class LaunchCurtainControllerTest {
    public static class Stream extends AppCompatActivity {
        @Override protected void onCreate(android.os.Bundle state) {
            setTheme(R.style.StreamTheme);
            super.onCreate(state);
        }
    }

    /** The engine's answers, as CurtainBridge gives them. */
    static final class FakeEngine implements LaunchCurtainController.Engine {
        String state;
        String afterTick;
        String afterDismiss;
        int ticks;
        int dismissals;
        int ends;

        @Override public String state() { return state; }

        @Override public String tick() {
            ticks++;
            if (afterTick != null) state = afterTick;
            return state;
        }

        @Override public String dismiss() {
            dismissals++;
            if (afterDismiss != null) state = afterDismiss;
            return state;
        }

        @Override public void end() {
            ends++;
            state = null;
        }
    }

    private static String down(String stage, String attention, boolean dismissible, boolean hostDismissible, long dueMs) {
        return "{\"down\":true,\"stage\":" + quoted(stage) + ",\"attention\":" + (attention == null ? "null" : attention)
                + ",\"dismissible\":" + dismissible + ",\"host_dismissible\":" + hostDismissible
                + ",\"due_ms\":" + dueMs + ",\"lift\":null}";
    }

    private static String lifted(String how, String notice, String reason) {
        return lifted(how, notice, reason, false);
    }

    private static String lifted(String how, String notice, String reason, boolean hostDismissible) {
        return "{\"down\":false,\"stage\":null,\"attention\":null,\"dismissible\":true,\"host_dismissible\":" + hostDismissible
                + ",\"due_ms\":null,\"lift\":{\"how\":\"" + how + "\",\"notice\":" + quoted(notice)
                + ",\"reason\":" + quoted(reason) + "}}";
    }

    private static String quoted(String text) {
        return text == null ? "null" : "\"" + text + "\"";
    }

    private ActivityController<Stream> activity;
    private FakeEngine engine;
    private final List<ConnectingScreen> released = new ArrayList<>();
    private final List<CharSequence> notices = new ArrayList<>();
    private final AtomicInteger hostDismissals = new AtomicInteger();
    private final AtomicInteger cancelled = new AtomicInteger();
    private LaunchCurtainController curtain;
    private ConnectingScreen screen;

    @Before
    public void setUp() {
        activity = Robolectric.buildActivity(Stream.class).setup();
        engine = new FakeEngine();
        curtain = new LaunchCurtainController(activity.get(), engine, s -> {
            released.add(s);
            s.dismiss();
        }, notices::add, hostDismissals::incrementAndGet);
        screen = ConnectingScreen.show(activity.get(), "Racing Game", "PC", "Connecting", "Starting connection", cancelled::incrementAndGet);
        ShadowLooper.idleMainLooper();
    }

    @After
    public void tearDown() {
        curtain.end();
        activity.pause().stop().destroy();
    }

    private String text(int id) {
        return ((TextView) screen.dialog().findViewById(id)).getText().toString();
    }

    private int visibility(int id) {
        return screen.dialog().findViewById(id).getVisibility();
    }

    private String string(int id, Object... args) {
        return activity.get().getString(id, args);
    }

    @Test
    public void aStreamWithoutACurtainLosesTheConnectingScreenAsBefore() {
        engine.state = null;
        curtain.hold(screen);
        ShadowLooper.idleMainLooper();
        assertEquals(List.of(screen), released);
        assertFalse(screen.dialog().isShowing());
        assertFalse(curtain.isDown());
        assertTrue(notices.isEmpty());
    }

    @Test
    public void aCurtainTheDescribeDidNotOfferLiftsAtOnceWithoutANotice() {
        engine.state = lifted("not_offered", null, null);
        curtain.hold(screen);
        ShadowLooper.idleMainLooper();
        assertEquals(List.of(screen), released);
        assertTrue(notices.isEmpty());
    }

    @Test
    public void theScreenStaysWithTheHostsWordUntilTheGameIsInFront() {
        engine.state = down(null, null, false, false, 60_000);
        curtain.hold(screen);
        ShadowLooper.idleMainLooper();
        assertTrue("the connecting screen stays over the running stream", screen.dialog().isShowing());
        assertTrue(curtain.isDown());
        assertEquals("the app's own word until the host speaks", "Connecting", text(R.id.conn_word));
        assertEquals(View.GONE, visibility(R.id.conn_show_screen));

        engine.state = down("opening", null, true, false, 10_000);
        curtain.changed();
        ShadowLooper.idleMainLooper();
        assertEquals(string(R.string.launch_curtain_opening), text(R.id.conn_word));
        assertEquals(View.GONE, visibility(R.id.conn_line));
        assertEquals("Show PC screen once the picture runs", View.VISIBLE, visibility(R.id.conn_show_screen));
        assertEquals(View.GONE, visibility(R.id.conn_attention));

        engine.state = lifted("ready", null, null);
        curtain.changed();
        ShadowLooper.idleMainLooper();
        assertFalse("removed the moment the game is in front", screen.dialog().isShowing());
        assertEquals(List.of(screen), released);
        assertTrue(notices.isEmpty());
        assertEquals("the watchdog went with it", 0, engine.ticks);
        ShadowLooper.idleMainLooper(20, TimeUnit.SECONDS);
        assertEquals(0, engine.ticks);
    }

    @Test
    public void theWatchdogLiftsItWhenTheHostGoesQuiet() {
        engine.state = down("starting", null, true, false, 10_000);
        engine.afterTick = lifted("quiet", "still_starting", null);
        curtain.hold(screen);
        ShadowLooper.idleMainLooper(9_999, TimeUnit.MILLISECONDS);
        assertEquals(0, engine.ticks);
        assertTrue(screen.dialog().isShowing());
        ShadowLooper.idleMainLooper(1, TimeUnit.MILLISECONDS);
        assertEquals("due without another message from the host", 1, engine.ticks);
        assertFalse(screen.dialog().isShowing());
        assertEquals(List.of(string(R.string.launch_curtain_still_starting)), notices);
    }

    @Test
    public void aWordFromTheHostMovesTheWatchdog() {
        engine.state = down("starting", null, true, false, 10_000);
        curtain.hold(screen);
        ShadowLooper.idleMainLooper(6, TimeUnit.SECONDS);
        engine.state = down("opening", null, true, false, 10_000);
        curtain.changed();
        // The paused looper runs what is due only after it moved the clock: the word is read now.
        ShadowLooper.idleMainLooper();
        ShadowLooper.idleMainLooper(6, TimeUnit.SECONDS);
        assertEquals("the first deadline went", 0, engine.ticks);
        ShadowLooper.idleMainLooper(4, TimeUnit.SECONDS);
        assertEquals(1, engine.ticks);
        assertTrue("still down: the engine said so", screen.dialog().isShowing());
    }

    @Test
    public void theAttentionCardNamesTheLauncherAndShowPcScreenLiftsAtOnceAndTellsTheHost() {
        String card = "{\"key\":\"sign_in\",\"launcher\":\"Steam\",\"title\":\"Steam Guard\"}";
        engine.state = down("starting", card, true, true, 10_000);
        engine.afterDismiss = lifted("dismissed", null, null, true);
        curtain.hold(screen);
        ShadowLooper.idleMainLooper();
        assertEquals(View.VISIBLE, visibility(R.id.conn_attention));
        assertEquals(string(R.string.launch_curtain_sign_in, "Steam"), text(R.id.conn_attention_line));
        assertEquals("Steam Guard", text(R.id.conn_attention_title));
        assertEquals(string(R.string.launch_curtain_hint), text(R.id.conn_attention_hint));
        assertEquals(string(R.string.launch_curtain_show_screen), text(R.id.conn_show_screen));
        assertEquals(View.VISIBLE, visibility(R.id.conn_cancel));

        screen.dialog().findViewById(R.id.conn_show_screen).performClick();
        ShadowLooper.idleMainLooper();
        assertEquals(1, engine.dismissals);
        assertFalse("lifted here before the host answers", screen.dialog().isShowing());
        assertEquals(1, hostDismissals.get());
        assertTrue(notices.isEmpty());
        assertEquals(0, cancelled.get());
    }

    @Test
    public void showPcScreenDoesNotBotherAHostThatDoesNotTakeIt() {
        String card = "{\"key\":\"waiting_any\",\"launcher\":null,\"title\":\"\"}";
        engine.state = down("starting", card, true, false, 10_000);
        engine.afterDismiss = lifted("dismissed", null, null);
        curtain.hold(screen);
        ShadowLooper.idleMainLooper();
        assertEquals(string(R.string.launch_curtain_waiting_any), text(R.id.conn_attention_line));
        assertEquals(View.GONE, visibility(R.id.conn_attention_title));
        screen.dialog().findViewById(R.id.conn_show_screen).performClick();
        ShadowLooper.idleMainLooper();
        assertFalse(screen.dialog().isShowing());
        assertEquals(0, hostDismissals.get());
    }

    @Test
    public void aFailedStartSaysSoWithTheHostsReason() {
        engine.state = down("starting", null, true, false, 10_000);
        curtain.hold(screen);
        engine.state = lifted("failed", "failed", "The game closed right after it started.");
        curtain.changed();
        ShadowLooper.idleMainLooper();
        assertFalse(screen.dialog().isShowing());
        assertEquals(List.of(string(R.string.launch_curtain_failed) + " The game closed right after it started."), notices);
    }

    @Test
    public void cancelEndsTheStreamAsBackDoesWhileConnecting() {
        engine.state = down("opening", null, true, false, 10_000);
        curtain.hold(screen);
        ShadowLooper.idleMainLooper();
        screen.dialog().findViewById(R.id.conn_cancel).performClick();
        ShadowLooper.idleMainLooper();
        assertEquals(1, cancelled.get());
        curtain.end();
        assertEquals(1, engine.ends);
        assertFalse(curtain.isDown());
        ShadowLooper.idleMainLooper(20, TimeUnit.SECONDS);
        assertEquals("the stream's end stops the watchdog", 0, engine.ticks);
    }

    @Test
    public void aStateThatDoesNotReadLiftsTheCurtain() {
        engine.state = "{\"down\":true,\"due_ms\":null}";
        curtain.hold(screen);
        ShadowLooper.idleMainLooper();
        assertFalse("down without a deadline might never lift", screen.dialog().isShowing());
        assertNull(LaunchCurtainController.State.parse("not json"));
    }

    @Test
    public void everyCurtainStringHasItsKey() throws IllegalAccessException {
        int found = 0;
        for (Field field : R.string.class.getFields()) {
            if (!field.getName().startsWith("launch_curtain_")) continue;
            String key = field.getName().substring("launch_curtain_".length());
            assertEquals(field.getName(), field.getInt(null), LaunchCurtainController.word(key));
            found++;
        }
        assertEquals("the 15 words of nightjar_policy::launch_curtain::ALL", 15, found);
        assertEquals(0, LaunchCurtainController.word("from_a_newer_engine"));
        assertEquals(0, LaunchCurtainController.word(null));
    }

    /** The engine's own answers read as the controller expects (protocol/android/src/launch_curtain.rs). */
    @Test
    public void theEnginesStateReads() {
        Assume.assumeTrue("libnightjar_core for the host", NightjarJni.isAvailable());
        try {
            CurtainBridge.begin(false);
            assertNull("a launch that did not ask has no curtain", CurtainBridge.state());
            CurtainBridge.begin(true);
            LaunchCurtainController.State state = LaunchCurtainController.State.parse(CurtainBridge.state());
            assertTrue(state.down);
            assertNull(state.stage);
            assertFalse(state.dismissible);
            assertTrue("due at the launch's limit", state.dueMs > 50_000 && state.dueMs <= 60_000);
            assertTrue(LaunchCurtainController.State.parse(CurtainBridge.tick()).down);
            LaunchCurtainController.State dismissed = LaunchCurtainController.State.parse(CurtainBridge.dismiss());
            assertFalse(dismissed.down);
            assertEquals("dismissed", dismissed.how);
            assertNull(dismissed.notice);
            assertEquals(-1, dismissed.dueMs);
            assertNotNull("lifted, but still this stream's curtain", CurtainBridge.state());
        } finally {
            CurtainBridge.end();
        }
        assertNull(CurtainBridge.state());
    }
}
