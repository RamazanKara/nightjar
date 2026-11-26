package com.limelight.computers;

import android.app.Activity;
import android.os.Handler;
import android.os.Looper;
import android.widget.Toast;

import androidx.appcompat.app.AlertDialog;

import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.wol.WakeRelayKeys;
import com.limelight.ui.NavigationDialogBuilder;

import java.util.concurrent.Executor;

/**
 * One wake from away through the PC's wake relays ({@link AwayWake}): its steps in a dialog whose Cancel ends it, then
 * its outcome in words, with Try again where that may help. The request blocks for a minute or more on the worker;
 * everything on screen happens on the main thread, and nothing after {@link #cancel}.
 */
final class AwayWakeFlow {
    /** Runs the request: {@link NightjarJni} on a device, a fake in tests. */
    interface Relay {
        long create();

        void cancel(long id);

        String request(long id, String bundle, NightjarJni.WakeRelayListener listener);
    }

    static final Relay JNI = new Relay() {
        @Override public long create() { return NightjarJni.wakeRelayCreate(); }
        @Override public void cancel(long id) { NightjarJni.wakeRelayCancel(id); }
        @Override public String request(long id, String bundle, NightjarJni.WakeRelayListener listener) {
            return NightjarJni.wakeRelayRequest(id, bundle, listener);
        }
    };

    private static final String UNAVAILABLE = "{\"outcome\":\"network\"}";

    private final HostActions screen;
    private final HostActions.Callbacks callbacks;
    private final ComputerDetails computer;
    private final WakeRelayKeys.Kept kept;
    private final Relay relay;
    private final Executor worker;
    private final Runnable retry;
    private final Handler main = new Handler(Looper.getMainLooper());
    private AlertDialog progress;
    private long id;
    private boolean done;

    AwayWakeFlow(HostActions screen, HostActions.Callbacks callbacks, ComputerDetails computer, WakeRelayKeys.Kept kept,
                 Relay relay, Executor worker, Runnable retry) {
        this.screen = screen;
        this.callbacks = callbacks;
        this.computer = computer;
        this.kept = kept;
        this.relay = relay;
        this.worker = worker;
        this.retry = retry;
    }

    void start() {
        LimeLog.info("Wake: asking a wake relay for " + computer.uuid);
        callbacks.onWakeSent(computer);
        showProgress();
        try {
            id = relay.create();
        } catch (LinkageError e) {
            finish(UNAVAILABLE);
            return;
        }
        final long mine = id;
        final String bundle = kept.bundle;
        worker.execute(() -> {
            String outcome;
            try {
                outcome = relay.request(mine, bundle, step -> main.post(() -> onStep(step)));
            } catch (LinkageError | RuntimeException e) {
                LimeLog.warning("Wake: the wake relay request failed: " + e);
                outcome = UNAVAILABLE;
            }
            final String ended = outcome;
            main.post(() -> finish(ended));
        });
    }

    boolean isWaiting() {
        return !done;
    }

    /** Ends the request and its dialog; the outcome, when it comes, shows nothing. */
    void cancel() {
        if (done) {
            return;
        }
        done = true;
        try {
            relay.cancel(id);
        } catch (LinkageError ignored) {
        }
        dismissProgress();
        callbacks.onWakeEnded(computer);
    }

    private void onStep(String json) {
        AwayWake.Words words = AwayWake.step(json, computer.displayName());
        if (!done && words != null && progress != null && !screen.gone()) {
            progress.setMessage(words.in(screen.activity()));
        }
    }

    private void finish(String json) {
        if (done) {
            return;
        }
        done = true;
        dismissProgress();
        String key = AwayWake.outcomeKey(json);
        LimeLog.info("Wake: the wake relay request for " + computer.uuid + " ended: " + key);
        callbacks.onWakeEnded(computer);
        AwayWake.Words words = AwayWake.outcome(json, computer.displayName());
        if (words == null || screen.gone()) {
            return;
        }
        Activity activity = screen.activity();
        if ("awake".equals(key)) {
            Toast.makeText(activity, words.in(activity), Toast.LENGTH_LONG).show();
            return;
        }
        screen.showError(computer.displayName(), words.in(activity), AwayWake.offersRetry(key) ? retry : null);
    }

    private void showProgress() {
        if (screen.gone()) {
            return;
        }
        progress = new NavigationDialogBuilder(screen.activity()).setTitle(R.string.wol_waking_pc)
                .setMessage(AwayWake.starting(kept).in(screen.activity()))
                .setNegativeButton(android.R.string.cancel, (dialog, which) -> cancel())
                .setOnCancelListener(dialog -> cancel())
                .show();
    }

    private void dismissProgress() {
        if (progress != null) {
            progress.dismiss();
            progress = null;
        }
    }
}
