package com.limelight.files;

import android.app.Activity;
import android.net.Uri;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.FrameLayout;

import com.limelight.R;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.List;

/**
 * The Files page's transfers after the page closed with some unfinished, as on desktop: they go on while the app
 * runs, one PC at a time, and end when they finish or are cancelled on that PC's Files page opened again. One that
 * fails meanwhile keeps them until that page is opened again and shows it, with Try again. No
 * foreground service keeps them: in the background they may end with the process, as before. The screens that lead
 * to Files show how many run ({@link #showOn}). Main thread only.
 */
public final class RunningTransfers {
    private static FileTransferActivity.TransferWork held;
    private static final List<Runnable> listeners = new ArrayList<>();

    private RunningTransfers() {}

    static void hold(FileTransferActivity.TransferWork work) {
        held = work;
        changed();
    }

    static FileTransferActivity.TransferWork held() {
        return held;
    }

    /** The held work, which a Files page for its PC shows again; nothing is held after. */
    static FileTransferActivity.TransferWork take() {
        FileTransferActivity.TransferWork work = held;
        held = null;
        changed();
        return work;
    }

    static void release(FileTransferActivity.TransferWork work) {
        if (held != work) return;
        held = null;
        changed();
    }

    /** SAF document URIs currently retained by held transfers so openers do not revoke them early. */
    public static Collection<Uri> activeGrantUris() {
        FileTransferActivity.TransferWork work = held;
        if (work == null) return Collections.emptyList();
        synchronized (work.persistedPermissions) {
            return work.persistedPermissions.isEmpty()
                    ? Collections.emptyList()
                    : new ArrayList<>(work.persistedPermissions.keySet());
        }
    }

    /** Unfinished transfers a closed Files page left running; 0 for none. */
    public static int count() {
        FileTransferActivity.TransferWork work = held;
        if (work == null || work.controller == null) return 0;
        int count = 0;
        for (FileRow row : work.controller.rows()) {
            if (!row.isFinished()) count++;
        }
        return count;
    }

    /** Transfers that failed after the Files page closed and that it has not shown yet; 0 for none. */
    public static int failed() {
        FileTransferActivity.TransferWork work = held;
        return work == null ? 0 : work.unseenFailures();
    }

    /** The PC they run with, or null. */
    public static String pcUuid() {
        return held == null ? null : held.uuid;
    }

    static void changed() {
        for (Runnable listener : new ArrayList<>(listeners)) listener.run();
    }

    /**
     * Shows "File transfers running: n" over the activity's page while there are any, as desktop's button over the
     * launcher, or "File transfers failed: n" while failures wait to be shown, which wins since they need the user;
     * a press opens that PC's Files page, where each one can be cancelled or tried again. Call from onResume; the
     * returned Runnable, from onPause, takes the button away again.
     */
    public static Runnable showOn(Activity activity) {
        View root = activity.findViewById(android.R.id.content);
        if (!(root instanceof FrameLayout)) return () -> {};
        FrameLayout content = (FrameLayout) root;
        Button button = new com.google.android.material.button.MaterialButton(activity, null,
                com.google.android.material.R.attr.materialButtonOutlinedStyle);
        int margin = Math.round(24 * activity.getResources().getDisplayMetrics().density);
        FrameLayout.LayoutParams params = new FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT,
                FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM | Gravity.START);
        params.setMargins(margin, margin, margin, margin);
        button.setOnClickListener(v -> {
            String pc = pcUuid();
            if (pc != null) activity.startActivity(FileTransferActivity.forPc(activity, pc));
        });
        content.addView(button, params);
        Runnable update = () -> {
            int failed = failed();
            int count = count();
            button.setVisibility(failed > 0 || count > 0 ? View.VISIBLE : View.GONE);
            if (failed > 0 || count > 0) {
                String text = failed > 0 ? activity.getString(R.string.files_failed, failed)
                        : activity.getString(R.string.files_running, count);
                if (!text.contentEquals(button.getText())) button.setText(text);
            }
        };
        update.run();
        listeners.add(update);
        return () -> {
            listeners.remove(update);
            content.removeView(button);
        };
    }
}
