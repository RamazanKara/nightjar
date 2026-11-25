package com.limelight.console;

import android.app.Activity;
import android.os.Handler;
import android.os.SystemClock;
import android.widget.Toast;

import androidx.appcompat.app.AlertDialog;

import com.limelight.R;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.ui.NavigationDialogBuilder;

import java.io.IOException;
import java.util.function.Function;

/**
 * The Wake PC action on Home: tapping a card of a PC that is away sends the wake packets and waits for it, telling
 * the player how it went. One PC at a time (tapping another moves the wait to it); the wait ends when the screen is
 * left, since the feed stops polling then. While it lasts a dialog says which PC is waited for, with Cancel.
 */
final class HomeWake {
    private final Activity activity;
    private final Function<String, ComputerDetails> hosts;
    private final Runnable onWoke;
    private final Runnable onSettled;
    private final HostWaker waker;
    private String uuid;
    private String name;
    /** The wait on screen, null when none shows. */
    private AlertDialog progress;

    HomeWake(Activity activity, Handler handler, Function<String, ComputerDetails> hosts, HostWaker.Sender sender,
             Runnable onWoke) {
        this(activity, handler, hosts, sender, onWoke, null);
    }

    /**
     * @param hosts  the PC for a UUID, as the feed knows it now
     * @param onWoke called when the PC answers, so Home refreshes it
     * @param onSettled called when a wake wait ends (cancelled, timed out, or failed) so form changes can proceed
     */
    HomeWake(Activity activity, Handler handler, Function<String, ComputerDetails> hosts, HostWaker.Sender sender,
             Runnable onWoke, Runnable onSettled) {
        this.activity = activity;
        this.hosts = hosts;
        this.onWoke = onWoke;
        this.onSettled = onSettled;
        this.waker = new HostWaker(handler, r -> new Thread(r, "console-wake").start(), sender,
                () -> hosts.apply(uuid), SystemClock::elapsedRealtime, new HostWaker.Listener() {
            @Override
            public void onOnline() {
                hideProgress();
                Toast.makeText(activity, activity.getString(R.string.wol_pc_online), Toast.LENGTH_SHORT).show();
                HomeWake.this.onWoke.run();
            }

            @Override
            public void onTimedOut() {
                hideProgress();
                notifySettledAfter(WakeFeedback.timedOut(activity, name, () -> wake(uuid)));
            }

            @Override
            public void onSendFailed(IOException error) {
                hideProgress();
                notifySettledAfter(WakeFeedback.failed(activity, name, R.string.wol_fail, () -> wake(uuid)));
            }
        });
    }

    private void notifySettledAfter(AlertDialog dialog) {
        if (onSettled == null) return;
        if (dialog == null) onSettled.run();
        else dialog.setOnDismissListener(ignored -> onSettled.run());
    }

    /**
     * Wakes the PC and waits for it.
     *
     * @return false when this PC cannot be woken from here (unknown, online, or no MAC address); the caller then
     *         falls back to the list of PCs, which explains how to set Wake-on-LAN up
     */
    boolean wake(String hostUuid) {
        ComputerDetails host = hosts.apply(hostUuid);
        if (!HostWaker.canWake(activity, host)) {
            return false;
        }
        if (waker.isWaiting()) {
            if (hostUuid.equals(uuid)) {
                // Tapped again: the same wait shows, nothing is sent twice.
                showProgress();
                return true;
            }
            // Another PC was tapped: the wait moves to it, the first one is no longer waited for.
            cancel();
        }
        uuid = hostUuid;
        name = host.displayName();
        waker.start();
        showProgress();
        return true;
    }

    /** A wake started here still waits for its PC. */
    boolean isWaiting() {
        return waker.isWaiting();
    }

    void cancel() {
        boolean wasWaiting = waker.isWaiting();
        waker.cancel();
        hideProgress();
        if (wasWaiting && onSettled != null) {
            onSettled.run();
        }
    }

    private void showProgress() {
        if (!waker.isWaiting() || (progress != null && progress.isShowing())
                || activity.isFinishing() || activity.isDestroyed()) {
            return;
        }
        // Cancel and Back end the wait; the PC may still wake, Home then shows it once a poll finds it.
        progress = new NavigationDialogBuilder(activity)
                .setTitle(R.string.wol_waking_pc)
                .setMessage(activity.getString(R.string.console_wake_waiting, name))
                .setNegativeButton(android.R.string.cancel, (dialog, which) -> cancel())
                .setOnCancelListener(dialog -> cancel())
                .show();
    }

    private void hideProgress() {
        AlertDialog shown = progress;
        progress = null;
        if (shown != null && shown.isShowing()) {
            shown.dismiss();
        }
    }
}
