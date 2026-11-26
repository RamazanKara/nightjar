package com.limelight;

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.DialogInterface;
import android.content.Intent;
import android.widget.Toast;

import androidx.annotation.VisibleForTesting;
import androidx.appcompat.app.AlertDialog;

import com.limelight.nativeapi.NativeApiClient;
import com.limelight.nativeapi.NativeApiException;
import com.limelight.nativeapi.NativeGuests;
import com.limelight.ui.NavigationDialogBuilder;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * Guests from other devices, on the device playing (feature session.guests): asks "X wants to join your game" with
 * Let in and Deny while the host waits for an answer, and lists the guests in the stream menu with what each may do,
 * the keyboard and mouse, Kick and Block. Reads the host every 2 s while it may manage guests, every 10 s otherwise.
 * Where the PC offers invites and this device may make them, the Guests menu also makes an invite to play over the
 * internet, to share with a friend.
 */
final class StreamGuests {
    /** How often a device that doesn't manage the guests looks again: it may become the device playing. */
    @VisibleForTesting
    static final long IDLE_POLL_MS = 10_000;

    private final Game game;
    private final NativeApiClient client;
    /** The requests already asked about, so each is asked once. */
    private final Set<Integer> asked = new HashSet<>();
    private volatile NativeGuests latest;
    private volatile Thread polling;
    /** Whether the PC offers invites and this device may make them, read once when polling starts. */
    private volatile boolean mayInvite;
    private AlertDialog prompt;
    private int promptFor = -1;

    StreamGuests(Game game, NativeApiClient client) {
        this.game = game;
        this.client = client;
    }

    void start() {
        Thread thread = new Thread(this::poll, "Stream guests");
        thread.setDaemon(true);
        polling = thread;
        thread.start();
    }

    void stop() {
        Thread thread = polling;
        polling = null;
        if (thread != null) thread.interrupt();
        if (prompt != null) {
            prompt.dismiss();
            prompt = null;
        }
    }

    /** Whether this device manages the guests of its game, as the host last said. */
    boolean manages() {
        NativeGuests guests = latest;
        return guests != null && guests.manage;
    }

    private void poll() {
        Thread self = Thread.currentThread();
        try {
            mayInvite = client.capabilities().mayInvite();
        } catch (NativeApiException e) {
            LimeLog.info("Invites not offered: " + e.getMessage());
        }
        while (polling == self) {
            NativeGuests read = null;
            try {
                read = client.guests();
            } catch (NativeApiException e) {
                LimeLog.info("Guests not read: " + e.getMessage());
            }
            if (read != null) show(read);
            try {
                Thread.sleep(nextPollMs(read));
            } catch (InterruptedException e) {
                return;
            }
        }
    }

    @VisibleForTesting
    static long nextPollMs(NativeGuests read) {
        return read != null && read.manage ? NativeGuests.POLL_MS : IDLE_POLL_MS;
    }

    /** The join requests to ask about now: only for the device that manages them, each once, oldest first. */
    @VisibleForTesting
    static List<NativeGuests.Request> toAsk(NativeGuests read, Set<Integer> asked) {
        List<NativeGuests.Request> ask = new ArrayList<>();
        if (read == null || !read.manage) return ask;
        for (NativeGuests.Request request : read.waiting) {
            if (!asked.contains(request.id)) ask.add(request);
        }
        return ask;
    }

    /** Whether the open prompt asks about a request the host no longer waits on (answered elsewhere, or given up). */
    @VisibleForTesting
    static boolean stale(NativeGuests read, int promptFor) {
        if (promptFor < 0) return false;
        if (read == null || !read.manage) return true;
        for (NativeGuests.Request request : read.waiting) {
            if (request.id == promptFor) return false;
        }
        return true;
    }

    private void show(NativeGuests read) {
        game.runOnUiThread(() -> {
            if (polling == null || game.isFinishing()) return;
            latest = read;
            if (prompt != null && stale(read, promptFor)) {
                prompt.dismiss();
                prompt = null;
                promptFor = -1;
            }
            if (prompt != null) return;
            List<NativeGuests.Request> ask = toAsk(read, asked);
            if (!ask.isEmpty()) askAbout(ask.get(0));
        });
    }

    static final long JOIN_PROMPT_INPUT_GUARD_MS = 1000L;

    interface Clock {
        long uptimeMs();
    }

    Clock clock = android.os.SystemClock::uptimeMillis;

    void askAbout(NativeGuests.Request request) {
        asked.add(request.id);
        promptFor = request.id;
        prompt = new NavigationDialogBuilder(game)
                .setMessage(game.getString(R.string.guests_wants_to_join, request.name))
                .setPositiveButton(R.string.guests_let_in, null)
                .setNegativeButton(R.string.guests_deny, null)
                .create();
        prompt.setOnDismissListener(dialog -> {
            if (prompt == dialog) {
                prompt = null;
                promptFor = -1;
            }
        });
        prompt.setOnShowListener(shown -> {
            long shownAtMs = clock.uptimeMs();
            android.widget.Button approve = prompt.getButton(DialogInterface.BUTTON_POSITIVE);
            android.widget.Button deny = prompt.getButton(DialogInterface.BUTTON_NEGATIVE);
            if (approve != null) {
                approve.setOnClickListener(v -> {
                    if (clock.uptimeMs() - shownAtMs < JOIN_PROMPT_INPUT_GUARD_MS) return;
                    prompt.dismiss();
                    act("approve", request.id, null);
                });
            }
            if (deny != null) {
                deny.setOnClickListener(v -> {
                    if (clock.uptimeMs() - shownAtMs < JOIN_PROMPT_INPUT_GUARD_MS) return;
                    prompt.dismiss();
                    act("deny", request.id, null);
                });
                if (!deny.requestFocus()) deny.requestFocusFromTouch();
            }
        });
        prompt.show();
    }

    /**
     * The stream menu's Guests: first who waits to join, until the host stops waiting (a prompt closed with Back
     * leaves it answerable here), then each guest with what it may do; selecting one offers Let in and Deny, or the
     * changes.
     */
    void showGuests() {
        NativeGuests read = latest;
        if (read == null || game.isFinishing()) return;
        NavigationDialogBuilder builder = new NavigationDialogBuilder(game);
        builder.setTitle(R.string.guests_title);
        List<NativeGuests.Request> waiting = answerable(read);
        if (mayInvite) builder.setNeutralButton(R.string.invite_title, (dialog, which) -> invite());
        if (read.guests.isEmpty() && waiting.isEmpty()) {
            builder.setMessage(R.string.guests_none).setPositiveButton(android.R.string.ok, null).show();
            return;
        }
        String[] lines = new String[waiting.size() + read.guests.size()];
        for (int i = 0; i < waiting.size(); i++) lines[i] = game.getString(R.string.guests_wants_to_join, waiting.get(i).name);
        for (int i = 0; i < read.guests.size(); i++) lines[waiting.size() + i] = line(read, read.guests.get(i));
        builder.setItems(lines, (dialog, which) -> {
                    if (which < waiting.size()) {
                        if (prompt == null) askAbout(waiting.get(which));
                    } else {
                        showGuest(read, read.guests.get(which - waiting.size()));
                    }
                })
                .setNegativeButton(android.R.string.cancel, null)
                .show();
    }

    /** The join requests the Guests menu offers to answer: the manager's, oldest first. */
    @VisibleForTesting
    static List<NativeGuests.Request> answerable(NativeGuests read) {
        return read != null && read.manage ? read.waiting : new ArrayList<>();
    }

    private String line(NativeGuests read, NativeGuests.Guest guest) {
        StringBuilder line = new StringBuilder(guest.name).append(" · ").append(permissionLabel(guest.permission));
        if (read.keyboardMouse == guest.id) line.append(" · ").append(game.getString(R.string.guests_has_keyboard));
        if (!guest.streaming) line.append(" · ").append(game.getString(R.string.guests_away));
        return line.toString();
    }

    private String permissionLabel(String permission) {
        if (NativeGuests.VIEW.equals(permission)) return game.getString(R.string.guests_view);
        if (NativeGuests.KEYBOARD_MOUSE.equals(permission)) return game.getString(R.string.guests_keyboard_mouse);
        return game.getString(R.string.guests_controller);
    }

    /** The changes offered for one guest, in the order shown. */
    @VisibleForTesting
    enum Change { VIEW, CONTROLLER, KEYBOARD_MOUSE, GIVE_KEYBOARD, TAKE_KEYBOARD, KICK, BLOCK }

    @VisibleForTesting
    static List<Change> changes(NativeGuests read, NativeGuests.Guest guest) {
        List<Change> changes = new ArrayList<>();
        if (!NativeGuests.VIEW.equals(guest.permission)) changes.add(Change.VIEW);
        if (!NativeGuests.CONTROLLER.equals(guest.permission)) changes.add(Change.CONTROLLER);
        if (!NativeGuests.KEYBOARD_MOUSE.equals(guest.permission)) changes.add(Change.KEYBOARD_MOUSE);
        if (NativeGuests.usesKeyboardMouse(guest.permission) && guest.streaming) {
            changes.add(read.keyboardMouse == guest.id ? Change.TAKE_KEYBOARD : Change.GIVE_KEYBOARD);
        }
        changes.add(Change.KICK);
        changes.add(Change.BLOCK);
        return changes;
    }

    private void showGuest(NativeGuests read, NativeGuests.Guest guest) {
        List<Change> changes = changes(read, guest);
        String[] labels = new String[changes.size()];
        for (int i = 0; i < labels.length; i++) labels[i] = label(changes.get(i));
        new NavigationDialogBuilder(game)
                .setTitle(guest.name)
                .setItems(labels, (dialog, which) -> apply(changes.get(which), guest))
                .setNegativeButton(android.R.string.cancel, null)
                .show();
    }

    private String label(Change change) {
        switch (change) {
            case VIEW: return game.getString(R.string.guests_view);
            case CONTROLLER: return game.getString(R.string.guests_controller);
            case KEYBOARD_MOUSE: return game.getString(R.string.guests_keyboard_mouse);
            case GIVE_KEYBOARD: return game.getString(R.string.guests_give_keyboard);
            case TAKE_KEYBOARD: return game.getString(R.string.guests_take_keyboard);
            case KICK: return game.getString(R.string.guests_kick);
            default: return game.getString(R.string.guests_block);
        }
    }

    private void apply(Change change, NativeGuests.Guest guest) {
        switch (change) {
            case VIEW: act("permission", guest.id, NativeGuests.VIEW); break;
            case CONTROLLER: act("permission", guest.id, NativeGuests.CONTROLLER); break;
            case KEYBOARD_MOUSE: act("permission", guest.id, NativeGuests.KEYBOARD_MOUSE); break;
            case GIVE_KEYBOARD: act("keyboard_mouse", guest.id, null); break;
            case TAKE_KEYBOARD: act("keyboard_mouse", -1, null); break;
            case KICK: act("kick", guest.id, null); break;
            default: act("revoke", guest.id, null); break;
        }
    }

    /** Sends one action off the UI thread; a refusal is shown in the host's words. */
    private void act(String action, int id, String permission) {
        new Thread(() -> {
            try {
                NativeGuests after = client.guestAction(action, id, permission);
                game.runOnUiThread(() -> latest = after);
            } catch (NativeApiException e) {
                LimeLog.warning("Guest " + action + " failed: " + e.getMessage());
                game.runOnUiThread(() -> {
                    // An answer that did not reach the host asks again while the host still waits.
                    if (answers(action)) asked.remove(id);
                    if (game.isFinishing()) return;
                    Toast.makeText(game, game.getString(R.string.guests_failed) + "\n" + e.getMessage(), Toast.LENGTH_LONG).show();
                });
            }
        }, "Guest action").start();
    }

    /** Makes an invite off the UI thread and offers it to share or copy; a refusal is shown in the host's words. */
    private void invite() {
        Toast.makeText(game, R.string.invite_making, Toast.LENGTH_SHORT).show();
        new Thread(() -> {
            try {
                NativeGuests.Invite made = client.createInvite();
                game.runOnUiThread(() -> {
                    if (!game.isFinishing()) showInvite(made);
                });
            } catch (NativeApiException e) {
                LimeLog.warning("Invite failed: " + e.getMessage());
                game.runOnUiThread(() -> {
                    if (game.isFinishing()) return;
                    Toast.makeText(game, game.getString(R.string.invite_make_failed, e.getMessage()), Toast.LENGTH_LONG).show();
                });
            }
        }, "Guest invite").start();
    }

    /** The message a friend gets: what to do with the link, then the link. */
    @VisibleForTesting
    static String inviteMessage(Context context, String link) {
        return context.getString(R.string.invite_share_text, link);
    }

    private void showInvite(NativeGuests.Invite made) {
        String message = inviteMessage(game, made.link);
        new NavigationDialogBuilder(game)
                .setTitle(R.string.invite_title)
                .setMessage(R.string.invite_made)
                .setPositiveButton(R.string.invite_share, (dialog, which) -> game.startActivity(Intent.createChooser(
                        new Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, message),
                        game.getString(R.string.invite_title))))
                .setNeutralButton(R.string.invite_copy, (dialog, which) -> {
                    ClipboardManager clipboard = (ClipboardManager) game.getSystemService(Context.CLIPBOARD_SERVICE);
                    if (clipboard == null) return;
                    clipboard.setPrimaryClip(ClipData.newPlainText(game.getString(R.string.invite_title), message));
                    Toast.makeText(game, R.string.invite_copied, Toast.LENGTH_SHORT).show();
                })
                .setNegativeButton(android.R.string.cancel, null)
                .show();
    }

    /** Whether {@code action} answers a join request. */
    @VisibleForTesting
    static boolean answers(String action) {
        return "approve".equals(action) || "deny".equals(action);
    }
}
