package com.limelight.console;

import android.app.Activity;
import android.os.Handler;

import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.computers.HostActions;
import com.limelight.nativeapi.ConsoleRepository;
import com.limelight.nativeapi.ConsoleSnapshot;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.ui.NavigationDialogBuilder;
import com.limelight.utils.FrontendSync;
import com.limelight.utils.LaunchSource;
import com.limelight.utils.UiHelper;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.function.Consumer;

/**
 * Game actions invoked from Home: opening a game (with multi-PC chooser), hero Play/Quit, and the card menu
 * (favourite, Open &amp; play, hide/show).
 */
final class HomeGameActions {
    interface Host {
        Activity activity();
        HostFeed feed();
        HostActions hostActions();
        ConsoleRepository repository();
        ConsoleSnapshot shownSnapshot();
        HomeModel model();
        HomeView view();
        GameOpener.Screen opener();
        ExecutorService worker();
        ExecutorService starWorker();
        Handler handler();
        boolean isShown();
        void play(NvApp app, ComputerDetails computer, String source);
        void end(NvApp app, ComputerDetails computer, Runnable onComplete);
        void close(NvApp app, ComputerDetails computer, Consumer<Boolean> onDone);
        void openGameOn(String hostUuid, int appId, String title, String hostName);
        void invalidateAndReload();
        void applyModel(HomeModel model);
    }

    private final Host host;
    /** A "which PC?" question is pending; further taps on games wait for its answer. */
    private boolean choosing;

    HomeGameActions(Host host) {
        this.host = host;
    }

    static Set<String> onlineOf(List<ComputerDetails> hosts) {
        Set<String> online = new HashSet<>();
        for (ComputerDetails details : hosts) {
            if (details.state == ComputerDetails.State.ONLINE) {
                online.add(details.uuid);
            }
        }
        return online;
    }

    void openGame(HomeModel.GameCard card) {
        // A game in several PCs' libraries asks which one first; a found game that is not added opens as before.
        if (!card.notAdded() && card.hostCount > 1 && askHost(card)) {
            return;
        }
        GameOpener.open(host.opener(), host.worker(), card);
    }

    /** Starts or resumes the game from the hero; a game that needs its page first opens that. */
    void playHero(HomeModel.GameCard card) {
        Activity activity = host.activity();
        ComputerDetails pc = host.feed().host(card.hostUuid);
        if (HomeHero.opensPage(card, pc)) {
            openGame(card);
            return;
        }
        if (host.feed().binder() == null) {
            host.hostActions().showError(pc.displayName(), activity.getString(R.string.error_manager_not_running), null);
            return;
        }
        GameDetailModel page = HomeHero.modelOf(card, pc, host.shownSnapshot());
        if (!page.canStart()) {
            // The page says why not.
            openGame(card);
            return;
        }
        NvApp app = CoverLoaders.appOf(card.title, card.appId, card.source);
        PlayPlan plan = HomeHero.planOf(page, app, pc);
        if (plan.kind == PlayPlan.Kind.START) {
            host.play(app, pc, card.badge == HomeModel.Badge.NONE ? LaunchSource.CONSOLE_PLAY : LaunchSource.CONSOLE_RESUME);
            return;
        }
        // Starting closes what runs there, so it asks as the game's page does.
        final String hostUuid = card.hostUuid;
        UiHelper.showDialog(activity, ReplaceRunningPrompt.ask(activity, plan, page.title, app, new ReplaceRunningPrompt.Screen() {
            @Override public ComputerDetails currentHost() {
                return host.feed().host(hostUuid);
            }

            @Override public void start(NvApp starting, ComputerDetails on) {
                host.play(starting, on, LaunchSource.CONSOLE_PLAY);
            }

            @Override public void resume(NvApp running, ComputerDetails on) {
                host.play(running, on, LaunchSource.CONSOLE_RESUME);
            }

            @Override public void close(NvApp running, ComputerDetails on, Consumer<Boolean> onDone) {
                host.close(running, on, onDone);
            }

            @Override public void reload() {
                host.invalidateAndReload();
            }
        }));
    }

    /** Asks, then ends the game on its PC; Home reads the PC again once that answered. */
    void quitHero(HomeModel.GameCard card) {
        Activity activity = host.activity();
        ComputerDetails pc = host.feed().host(card.hostUuid);
        if (pc == null) return;
        if (host.feed().binder() == null) {
            host.hostActions().showError(pc.displayName(), activity.getString(R.string.error_manager_not_running), null);
            return;
        }
        UiHelper.displayQuitConfirmationDialog(activity, () -> host.end(
                CoverLoaders.appOf(card.title, card.appId, card.source), pc,
                () -> host.handler().post(host::invalidateAndReload)), null);
    }

    /**
     * A game that several PCs have asks which one to open it on, from the saved libraries.
     *
     * @return false when there is nothing to ask (no library read yet): the card's own PC opens instead
     */
    private boolean askHost(HomeModel.GameCard card) {
        final ConsoleRepository repo = host.repository();
        final String libraryKey = card.libraryKey;
        if (repo == null || libraryKey == null) {
            return false;
        }
        if (choosing) {
            return true;
        }
        choosing = true;
        final List<ComputerDetails> hosts = host.feed().hosts();
        final Set<String> online = onlineOf(hosts);
        Activity activity = host.activity();
        // The saved libraries are on disk: read them on the star thread, not behind a refresh from the hosts.
        host.starWorker().execute(() -> {
            List<HostChoice> choices;
            try {
                choices = HostChoice.of(repo.cached(hosts), libraryKey, online);
            } catch (RuntimeException e) {
                LimeLog.warning("Home could not read the saved libraries: " + e);
                choices = Collections.emptyList();
            }
            final List<HostChoice> found = choices;
            activity.runOnUiThread(() -> {
                if (!host.isShown() || activity.isFinishing() || activity.isDestroyed()) {
                    choosing = false;
                } else if (found.size() > 1) {
                    HostChooser.show(activity, found, chosen ->
                            host.openGameOn(chosen.hostUuid, chosen.appId, card.title, chosen.hostName),
                            () -> choosing = false);
                } else {
                    choosing = false;
                    host.openGameOn(card.hostUuid, card.appId, card.title, card.hostName);
                }
            });
        });
        return true;
    }

    /** The long-press menu of a game card: star or unstar it, hide it. A game not added has no app id for either. */
    void askFavourite(HomeModel.GameCard card) {
        if (host.repository() == null || card.notAdded()) {
            return;
        }
        Activity activity = host.activity();
        boolean remove = card.favourite;
        List<ConsoleSnapshot.Source> copies = host.model().copiesOf(card);
        CharSequence star = activity.getString(remove ? R.string.console_favourite_remove : R.string.console_favourite_add);
        // A running game stays under Continue playing while hidden: its menu shows it again.
        boolean hide = !card.sourceHidden();
        CharSequence hiding = activity.getString(hide ? R.string.applist_menu_hide_app : R.string.applist_menu_show_app);
        CharSequence openPlay = OpenPlay.menuLabel(activity, card.hostUuid, card.appId,
                card.source != null ? card.source.stableId() : "");
        List<CharSequence> labels = new ArrayList<>();
        List<Runnable> actions = new ArrayList<>();
        // A phone's hero has room for one button, Play: Quit game is here, on Hold OK and Y too.
        if (card.badge == HomeModel.Badge.RUNNING && !OfflineLook.allAway(card.hostUuids, host.feed()::host)) {
            ComputerDetails pc = host.feed().host(card.hostUuid);
            if (pc != null && pc.hasPermission(ComputerDetails.PERMISSION_LAUNCH)) {
                labels.add(activity.getString(R.string.applist_menu_quit));
                actions.add(() -> quitHero(card));
            }
        }
        labels.add(star);
        actions.add(() -> setFavourite(card, !remove));
        labels.add(openPlay);
        actions.add(() -> OpenPlay.toggle(activity, card.hostUuid, card.hostName, card.appId,
                card.source != null ? card.source.stableId() : "", card.title));
        if (!copies.isEmpty()) {
            labels.add(hiding);
            actions.add(() -> setHidden(copies, hide));
        }
        UiHelper.showDialog(activity, new NavigationDialogBuilder(activity)
                .setTitle(card.title)
                .setItems(labels.toArray(new CharSequence[0]), (dialog, which) -> actions.get(which).run())
                .setNegativeButton(android.R.string.cancel, null).create());
    }

    /** Hides or shows on the star thread and paints the saved result right away, like a star. */
    private void setHidden(List<ConsoleSnapshot.Source> copies, boolean hide) {
        final ConsoleRepository repo = host.repository();
        if (repo == null) {
            return;
        }
        Activity activity = host.activity();
        final List<ComputerDetails> hosts = host.feed().hosts();
        final Set<String> online = onlineOf(hosts);
        final String hostFilter = HomeHostFilter.chosen(activity);
        final HomeView view = host.view();
        host.starWorker().execute(() -> {
            try {
                repo.setHidden(copies, hide);
                HomeModel next = HomeModel.of(repo.cached(hosts), online, HomeModel.launchDenied(hosts), hostFilter).sorted(view);
                activity.runOnUiThread(() -> {
                    if (activity.isFinishing() || activity.isDestroyed()) {
                        return;
                    }
                    host.applyModel(next);
                    for (ConsoleSnapshot.Source copy : copies) {
                        FrontendSync.appListChanged(activity, host.feed().host(copy.hostUuid));
                    }
                    host.invalidateAndReload();
                });
            } catch (RuntimeException e) {
                LimeLog.warning("Home could not hide a game: " + e);
            }
        });
    }

    /**
     * Stars or unstars on the star thread and paints the saved result right away, from the stored library;
     * the normal reload follows and takes over.
     */
    void setFavourite(HomeModel.GameCard card, boolean starred) {
        final ConsoleRepository repo = host.repository();
        if (repo == null || card.notAdded()) {
            return;
        }
        Activity activity = host.activity();
        final List<ComputerDetails> hosts = host.feed().hosts();
        final Set<String> online = onlineOf(hosts);
        final String key = card.source != null ? card.source.stableId() : "";
        final String hostFilter = HomeHostFilter.chosen(activity);
        final HomeView view = host.view();
        host.starWorker().execute(() -> {
            try {
                repo.setFavourite(card.hostUuid, card.hostName, key, card.appId, card.title, starred);
                HomeModel next = HomeModel.of(repo.cached(hosts), online, HomeModel.launchDenied(hosts), hostFilter).sorted(view);
                activity.runOnUiThread(() -> {
                    if (activity.isFinishing() || activity.isDestroyed()) {
                        return;
                    }
                    host.applyModel(next);
                    host.invalidateAndReload();
                });
            } catch (RuntimeException e) {
                LimeLog.warning("Home could not update a favourite: " + e);
            }
        });
    }
}
