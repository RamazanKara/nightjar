/**
 * The launch curtain on Android: while a game starts on the PC, the connecting screen (cover, ring, one stage word)
 * stays over the stream that already runs behind it, and goes the moment the host says the game's window is in
 * front. The rules (the word of each phase, the attention card's line, when the watchdog lifts it, how it lifted) run
 * in the Rust engine, {@code nightjar_policy::launch_curtain} through {@link CurtainBridge}, as on desktop; this shows
 * its state, schedules the watchdog by the state's {@code due_ms} and wires Show PC screen. The curtain always lifts:
 * on the host's word, on the watchdog (even when no further message comes), or on Show PC screen. It never holds the
 * video; Cancel, Back, B and Esc end the stream as they do while connecting (the screen's own cancel).
 */
package com.limelight.ui;

import android.content.Context;
import android.os.Handler;
import android.os.Looper;

import androidx.annotation.VisibleForTesting;

import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.nativeapi.NativeApiClient;
import com.limelight.nativeapi.NativeApiException;
import com.limelight.nvstream.jni.CurtainBridge;
import com.limelight.nvstream.jni.Json;

import java.util.function.Consumer;

public final class LaunchCurtainController {
    /** The engine's curtain: {@link #NATIVE} in the app, a fake in tests. Each answer is the state JSON, or null without a curtain. */
    public interface Engine {
        String state();

        String tick();

        String dismiss();

        void end();
    }

    public static final Engine NATIVE = new Engine() {
        @Override public String state() { return CurtainBridge.state(); }
        @Override public String tick() { return CurtainBridge.tick(); }
        @Override public String dismiss() { return CurtainBridge.dismiss(); }
        @Override public void end() { CurtainBridge.end(); }
    };

    private final Context context;
    private final Engine engine;
    private final Consumer<ConnectingScreen> release;
    private final Consumer<CharSequence> notice;
    private final Runnable dismissOnHost;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Runnable watchdog = this::watchdogDue;
    private final Runnable refresh = this::refresh;
    private ConnectingScreen screen;

    /**
     * @param release the curtain lifted: the connecting screen goes, as it did when the connection stood
     * @param notice one short notice that does not stop the stream
     * @param dismissOnHost Show PC screen with a host that takes it: tell the host, off the main thread
     */
    public LaunchCurtainController(Context context, Engine engine, Consumer<ConnectingScreen> release,
                                   Consumer<CharSequence> notice, Runnable dismissOnHost) {
        this.context = context;
        this.engine = engine;
        this.release = release;
        this.notice = notice;
        this.dismissOnHost = dismissOnHost;
    }

    /**
     * The connection stands, on the main thread: {@code connecting} stays over the stream while the curtain is down,
     * and is released at once when this stream has none or it already lifted.
     */
    public void hold(ConnectingScreen connecting) {
        handler.removeCallbacks(watchdog);
        screen = connecting;
        apply(engine.state());
    }

    /** The engine's curtain changed; from any thread. */
    public void changed() {
        handler.post(refresh);
    }

    /** Whether the curtain holds the connecting screen over the stream. */
    public boolean isDown() {
        return screen != null;
    }

    /**
     * The stream ends: the watchdog stops, the engine forgets the curtain, and a connecting screen still held goes
     * with it, as it went when the connection stood, so no dead curtain stays over the stream's end. Main thread.
     */
    public void end() {
        handler.removeCallbacks(watchdog);
        handler.removeCallbacks(refresh);
        ConnectingScreen held = screen;
        screen = null;
        engine.end();
        if (held != null) {
            held.setOnMenuListener(null);
            release.accept(held);
        }
    }

    private void refresh() {
        if (screen != null) apply(engine.state());
    }

    /** The state's deadline came without a word from the host. */
    private void watchdogDue() {
        if (screen != null) apply(engine.tick());
    }

    /** Show PC screen: lifts here at once, and tells a host that takes it in the background. */
    private void showPcScreen() {
        if (screen == null) return;
        State state = State.parse(engine.dismiss());
        if (state != null && "dismissed".equals(state.how) && state.hostDismissible) dismissOnHost.run();
        apply(state);
    }

    private void apply(String json) {
        apply(State.parse(json));
    }

    private void apply(State state) {
        if (screen == null) return;
        if (state == null || !state.down) {
            lift(state);
            return;
        }
        int stage = word(state.stage);
        screen.showCurtain(stage == 0 ? null : context.getString(stage), attentionLine(state),
                state.attentionTitle, state.dismissible ? this::showPcScreen : null);
        handler.removeCallbacks(watchdog);
        if (state.dueMs >= 0) handler.postDelayed(watchdog, state.dueMs);
    }

    private void lift(State state) {
        handler.removeCallbacks(watchdog);
        ConnectingScreen held = screen;
        screen = null;
        held.setOnMenuListener(null);
        release.accept(held);
        CharSequence words = state == null ? null : noticeText(state);
        if (words != null) notice.accept(words);
    }

    private CharSequence attentionLine(State state) {
        int line = word(state.attentionKey);
        if (line == 0) return null;
        return state.attentionLauncher == null ? context.getString(line) : context.getString(line, state.attentionLauncher);
    }

    @VisibleForTesting
    CharSequence noticeText(State state) {
        int words = word(state.notice);
        if (words == 0) return null;
        String text = context.getString(words);
        return state.reason == null || state.reason.isEmpty() ? text : text + " " + state.reason;
    }

    /** Tells the host the player chose Show PC screen, on a thread of its own; a refusal or a failure changes nothing here. */
    public static void dismissOnHost(NativeApiClient client) {
        if (client == null) return;
        new Thread(() -> {
            try {
                client.dismissCurtain();
            } catch (NativeApiException | RuntimeException e) {
                LimeLog.info("Launch curtain: the PC did not take Show PC screen: " + e.getMessage());
            }
        }, "Launch curtain").start();
    }

    /** The {@code launch_curtain_<key>} string of a word of the shared rules, or 0 for none or one this build lacks. */
    @VisibleForTesting
    static int word(String key) {
        if (key == null) return 0;
        switch (key) {
            case "preparing": return R.string.launch_curtain_preparing;
            case "starting": return R.string.launch_curtain_starting;
            case "opening": return R.string.launch_curtain_opening;
            case "focusing": return R.string.launch_curtain_focusing;
            case "updating": return R.string.launch_curtain_updating;
            case "updating_any": return R.string.launch_curtain_updating_any;
            case "sign_in": return R.string.launch_curtain_sign_in;
            case "sign_in_any": return R.string.launch_curtain_sign_in_any;
            case "waiting": return R.string.launch_curtain_waiting;
            case "waiting_any": return R.string.launch_curtain_waiting_any;
            case "uac": return R.string.launch_curtain_uac;
            case "hint": return R.string.launch_curtain_hint;
            case "show_screen": return R.string.launch_curtain_show_screen;
            case "still_starting": return R.string.launch_curtain_still_starting;
            case "failed": return R.string.launch_curtain_failed;
            default: return 0;
        }
    }

    /** The engine's state ({@link CurtainBridge}); a state that does not parse counts as lifted, never as a curtain stuck down. */
    @VisibleForTesting
    static final class State {
        boolean down;
        String stage;
        String attentionKey;
        String attentionLauncher;
        String attentionTitle;
        boolean dismissible;
        boolean hostDismissible;
        long dueMs = -1;
        String how;
        String notice;
        String reason;

        static State parse(String json) {
            if (json == null) return null;
            try {
                Json.Obj o = Json.parseObject(json);
                State s = new State();
                // A curtain down without a deadline might never lift: it counts as lifted.
                s.down = o.optBoolean("down", false) && !o.isNull("due_ms");
                s.stage = o.optString("stage", null);
                Json.Obj attention = o.optObject("attention");
                if (attention != null) {
                    s.attentionKey = attention.optString("key", null);
                    s.attentionLauncher = attention.optString("launcher", null);
                    s.attentionTitle = attention.optString("title", "");
                }
                s.dismissible = o.optBoolean("dismissible", false);
                s.hostDismissible = o.optBoolean("host_dismissible", false);
                s.dueMs = o.isNull("due_ms") ? -1 : Math.max(0, o.optLong("due_ms", 0));
                Json.Obj lift = o.optObject("lift");
                if (lift != null) {
                    s.how = lift.optString("how", null);
                    s.notice = lift.optString("notice", null);
                    s.reason = lift.optString("reason", null);
                }
                return s;
            } catch (RuntimeException e) {
                LimeLog.warning("Launch curtain: unreadable state, lifting it: " + e.getMessage());
                return null;
            }
        }
    }
}
