package com.limelight;

import android.graphics.Bitmap;
import android.graphics.drawable.BitmapDrawable;
import android.os.Build;
import android.view.View;
import android.widget.ImageView;
import android.widget.Toast;

import com.google.android.material.snackbar.Snackbar;
import com.limelight.computers.ComputerManagerService;
import com.limelight.grid.assets.CachedAppAssetLoader;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.PairingManager;
import com.limelight.nvstream.jni.NightjarPolicyJni;
import com.limelight.preferences.AppQuitOnLeave;
import com.limelight.preferences.AppStreamSettings;
import com.limelight.preferences.HostStreamSettings;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.ui.ActionSheet;
import com.limelight.utils.Dialog;
import com.limelight.utils.LaunchSource;
import com.limelight.utils.ServerHelper;
import com.limelight.utils.UiHelper;

/**
 * Per-game action sheet construction and menu action handling extracted from {@link AppView}:
 * launch/resume/quit, small-stream input-only, profile picker, quit-on-leave, per-game stream
 * settings, frame generation, controls import trigger, hide/unhide, shortcuts, and app details.
 */
final class AppViewMenu {
    private static final int START_OR_RESUME_ID = 1;
    private static final int QUIT_ID = 2;
    private static final int START_INSTEAD_ID = 4;
    private static final int VIEW_DETAILS_ID = 5;
    private static final int CREATE_SHORTCUT_ID = 6;
    private static final int HIDE_APP_ID = 7;
    private static final int OPEN_ON_START_ID = 8;

    private final AppView view;
    private ActionSheet appMenu;
    private com.limelight.computers.NeuralSheet neuralSheet;

    AppViewMenu(AppView view) {
        this.view = view;
    }

    void dismiss() {
        if (appMenu != null) appMenu.dismiss();
        if (neuralSheet != null) {
            neuralSheet.dismiss();
            neuralSheet = null;
        }
    }

    void showAppMenu(AppView.AppObject selectedApp, View selectedView) {
        if (appMenu != null) return;
        ActionSheet sheet = new ActionSheet(view, selectedApp.app.getAppName());
        appMenu = sheet;
        view.suspendGridUpdates = true;
        boolean canLaunch = view.computer.hasPermission(ComputerDetails.PERMISSION_LAUNCH);
        if (selectedApp.app.getControl() != NvApp.Control.NONE) {
            addAppAction(sheet, selectedApp, selectedView, START_OR_RESUME_ID,
                    R.drawable.ic_play, selectedApp.app.getAppName().trim());
        } else if (selectedApp.isRunning) {
            addAppAction(sheet, selectedApp, selectedView, START_OR_RESUME_ID,
                    R.drawable.ic_play, view.getString(R.string.applist_menu_resume));
            if (canLaunch) {
                addAppAction(sheet, selectedApp, selectedView, QUIT_ID,
                        R.drawable.ic_stop, view.getString(R.string.applist_menu_quit));
            }
        } else if (view.hostRunningGame && canLaunch) {
            AppView.AppObject running = view.runningApp();
            if (view.hostConfirmsReplacement()) {
                // The host asks before it closes the running game; nothing is quit from here.
                CharSequence note = running != null ? view.getString(R.string.library_host_asks_named, running.app.getAppName()) :
                        view.getString(R.string.library_host_asks);
                sheet.addAction(R.drawable.ic_play, view.getString(R.string.library_play), note).setOnClickListener(v -> {
                    sheet.dismiss();
                    onAppAction(selectedApp, selectedView, START_OR_RESUME_ID);
                });
            } else {
                addAppAction(sheet, selectedApp, selectedView, START_INSTEAD_ID, R.drawable.ic_play, running != null ?
                        view.getString(R.string.library_quit_named_and_play, running.app.getAppName()) :
                        view.getString(R.string.library_quit_and_play));
            }
        } else if (canLaunch) {
            addAppAction(sheet, selectedApp, selectedView, START_OR_RESUME_ID,
                    R.drawable.ic_play, view.getString(R.string.library_play));
        }
        addSmallStreamInputOnlyAction(sheet, selectedApp);
        addProfileAction(sheet);
        if (LaunchPolicy.offersQuitOnLeave(canLaunch, selectedApp.app.getControl(), selectedApp.app.getRole())) {
            addQuitOnLeaveAction(sheet, selectedApp);
        }
        if (selectedApp.app.getControl() == NvApp.Control.NONE) {
            addGameSettingsAction(sheet, selectedApp);
            addNeuralAction(sheet, selectedApp);
        }
        addControllerEditorAction(sheet);
        if (selectedApp.app.getRole() != NvApp.Role.REMOTE_MONITOR) {
            sheet.addAction(R.drawable.ic_touch_controls, view.getString(R.string.library_import_controls),
                    view.getString(R.string.library_import_controls_note)).setOnClickListener(v -> {
                sheet.dismiss();
                view.launchControlsPicker(
                        com.limelight.binding.input.TouchLayout.profileKey(view.uuidString,
                                selectedApp.app.getAppUuid(), selectedApp.app.getAppId()),
                        selectedApp.app.getAppName());
            });
        }
        if (!selectedApp.isRunning || selectedApp.isHidden) {
            addAppAction(sheet, selectedApp, selectedView, HIDE_APP_ID,
                    selectedApp.isHidden ? R.drawable.ic_visibility : R.drawable.ic_visibility_off,
                    view.getString(selectedApp.isHidden ? R.string.applist_menu_show_app : R.string.applist_menu_hide_app));
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            addAppAction(sheet, selectedApp, selectedView, CREATE_SHORTCUT_ID,
                    R.drawable.ic_add, view.getString(R.string.applist_menu_scut));
        }
        addAppAction(sheet, selectedApp, selectedView, OPEN_ON_START_ID, R.drawable.ic_play,
                com.limelight.console.OpenPlay.menuLabel(view, view.computer.uuid, selectedApp.app.getAppId(),
                        selectedApp.app.getAppUuid()));
        addAppAction(sheet, selectedApp, selectedView, VIEW_DETAILS_ID,
                R.drawable.ic_info, view.getString(R.string.applist_menu_details));
        sheet.setOnDismissListener(dialog -> {
            appMenu = null;
            view.suspendGridUpdates = false;
            if (view.inForeground) view.pollLibraryNow();
        });
        sheet.show();
    }

    /**
     * Input-only on a PC whose host has no Input-only mode of its own, when the setting is on: a
     * small stream this device does not show. It joins the running game or starts one on an idle
     * PC, and never replaces a game that runs.
     */
    private void addSmallStreamInputOnlyAction(ActionSheet sheet, AppView.AppObject app) {
        if (app.app.getControl() != NvApp.Control.NONE ||
                !NightjarPolicyJni.inputOnlyKeepsRunningGame(view.hostRunningGame, app.isRunning)) {
            return;
        }
        boolean hostListsInputOnly = false;
        for (AppView.AppObject listed : view.appGridAdapter.getAllApps()) {
            if (listed.app.getRole() == NvApp.Role.INPUT_ONLY) {
                hostListsInputOnly = true;
                break;
            }
        }
        if (NightjarPolicyJni.inputOnlyMode(PreferenceConfiguration.readPreferences(view).inputOnlyAnyHost,
                view.computer.caps.has(HostCapabilities.Feature.REMOTE_SESSION_TILES),
                hostListsInputOnly) != NightjarPolicyJni.INPUT_ONLY_MODE_SMALL_STREAM) {
            return;
        }
        int blockerCode = NightjarPolicyJni.inputOnlySmallStreamBlocker(
                view.computer.pairState == PairingManager.PairState.PAIRED, view.computer.permission);
        int blocker = blockerCode == NightjarPolicyJni.INPUT_ONLY_BLOCKER_PAIR ? HostCapabilities.Reason.PAIR.text
                : blockerCode == NightjarPolicyJni.INPUT_ONLY_BLOCKER_VIEW_PERMISSION ? HostCapabilities.Reason.VIEW_PERMISSION.text
                : blockerCode == NightjarPolicyJni.INPUT_ONLY_BLOCKER_INPUT_PERMISSION ? R.string.input_only_reason_input_permission : 0;
        com.google.android.material.button.MaterialButton action = sheet.addAction(R.drawable.ic_input_only,
                view.getString(R.string.input_only_start), blocker != 0 ? view.getString(blocker) :
                        view.getString(R.string.input_only_small_stream_note,
                                NightjarPolicyJni.inputOnlyEstimatedKbps() / 1000f,
                                NightjarPolicyJni.inputOnlyEstimatedMegabytesPerHour()));
        action.setEnabled(blocker == 0);
        action.setOnClickListener(v -> {
            sheet.dismiss();
            if (view.managerBinder == null || view.computer.state == ComputerDetails.State.OFFLINE) {
                view.showAppListStatus(R.string.library_offline);
                return;
            }
            ServerHelper.doStart(view, app.app, view.computer, view.managerBinder, true, LaunchSource.LIBRARY_INPUT_ONLY);
        });
    }

    /** Turns quit on leave on or off for this game in place; the sheet stays open and says what leaving now does. */
    private void addQuitOnLeaveAction(ActionSheet sheet, AppView.AppObject app) {
        String appUuid = app.app.getAppUuid();
        int appId = app.app.getAppId();
        com.google.android.material.button.MaterialButton action = sheet.addAction(R.drawable.ic_power,
                view.getString(R.string.library_quit_on_leave),
                quitOnLeaveNote(AppQuitOnLeave.isEnabled(view, view.uuidString, appUuid, appId)));
        action.setOnClickListener(v -> {
            boolean enabled = !AppQuitOnLeave.isEnabled(view, view.uuidString, appUuid, appId);
            AppQuitOnLeave.setEnabled(view, view.uuidString, appUuid, appId, enabled);
            sheet.setActionText(action, view.getString(R.string.library_quit_on_leave), quitOnLeaveNote(enabled));
            action.announceForAccessibility(quitOnLeaveNote(enabled));
        });
    }

    /** Opens the game's frame generation on the PC, while the PC offers it to this device. */
    private void addNeuralAction(ActionSheet sheet, AppView.AppObject app) {
        String appUuid = app.app.getAppUuid();
        if (appUuid == null || appUuid.isEmpty() || view.managerBinder == null || !offersNeural()) {
            return;
        }
        sheet.addAction(R.drawable.ic_speed, view.getString(R.string.neural_menu), view.getString(R.string.neural_menu_note))
                .setOnClickListener(v -> {
                    sheet.dismiss();
                    ComputerManagerService.ComputerManagerBinder binder = view.managerBinder;
                    if (binder == null) {
                        view.showAppListStatus(R.string.library_offline);
                        return;
                    }
                    if (neuralSheet != null) {
                        neuralSheet.dismiss();
                    }
                    neuralSheet = com.limelight.computers.NeuralSheet.show(view, view.computer, binder.getUniqueId(), appUuid,
                            app.app.getAppName());
                });
    }

    /** The PC lists frame generation in its native capabilities and lets this device view streams. */
    private boolean offersNeural() {
        String known = com.limelight.nativeapi.KnownCapabilities.of(view.computer.uuid);
        if (known == null) return false;
        try {
            return view.computer.caps.withNativeCapabilities(known)
                    .has(HostCapabilities.Feature.NEURAL_FRAME_GENERATION);
        } catch (org.json.JSONException e) {
            LimeLog.warning("Unreadable native capabilities of " + view.computer.uuid + ": " + e.getMessage());
            return false;
        }
    }

    /** Opens this game's own stream values; the note says whether it has any. */
    private void addGameSettingsAction(ActionSheet sheet, AppView.AppObject app) {
        String appUuid = app.app.getAppUuid();
        int appId = app.app.getAppId();
        int own = AppStreamSettings.load(view, view.uuidString, appUuid, appId).count();
        sheet.addAction(R.drawable.ic_tune, view.getString(R.string.game_settings_action), own == 0 ?
                view.getString(R.string.game_settings_note_none) :
                view.getResources().getQuantityString(R.plurals.game_settings_note_own, own, own)).setOnClickListener(v -> {
            sheet.dismiss();
            HostStreamSettings.showGame(view, view.uuidString, view.getIntent().getStringExtra(AppView.NAME_EXTRA), appUuid, appId,
                    app.app.getAppName());
        });
    }

    /** Settings' Controller section, the controller editor: its settings hold for every game. */
    private void addControllerEditorAction(ActionSheet sheet) {
        sheet.addAction(R.drawable.ic_gamepad, view.getString(R.string.settings_section_controller),
                view.getString(R.string.controller_editor_summary)).setOnClickListener(v -> {
            sheet.dismiss();
            com.limelight.preferences.StreamSettings.open(view,
                    com.limelight.preferences.StreamSettings.CONTROLLER_SECTION, null);
        });
    }

    /**
     * Switches the profile in use before a stream, as the Profiles page does. Not offered without profiles. When "Choose by
     * network" has a rule for this network the rule decides, so the entry shows what the next stream uses and opens the page.
     */
    private void addProfileAction(ActionSheet sheet) {
        String shown = com.limelight.preferences.StreamProfilePicker.inUse(view);
        if (shown == null) return;
        sheet.addAction(R.drawable.ic_folder, view.getString(R.string.profiles_subtitle), shown).setOnClickListener(v -> {
            sheet.dismiss();
            showProfilePicker();
        });
    }

    private void showProfilePicker() {
        ActionSheet picker = com.limelight.preferences.StreamProfilePicker.chooser(view, null);
        if (picker == null) return;
        view.suspendGridUpdates = true;
        picker.setOnDismissListener(dialog -> {
            view.suspendGridUpdates = false;
            if (view.inForeground) view.pollLibraryNow();
        });
        picker.show();
    }

    private String quitOnLeaveNote(boolean enabled) {
        return view.getString(enabled ? R.string.library_quit_on_leave_on : R.string.library_quit_on_leave_off);
    }

    private void addAppAction(ActionSheet sheet, AppView.AppObject app, View itemView, int action, int icon, String label) {
        sheet.addAction(icon, label).setOnClickListener(v -> {
            sheet.dismiss();
            onAppAction(app, itemView, action);
        });
    }

    private boolean onAppAction(AppView.AppObject app, View itemView, int action) {
        if ((action == START_OR_RESUME_ID || action == START_INSTEAD_ID || action == QUIT_ID) &&
                (view.computer == null || view.managerBinder == null || view.computer.state == ComputerDetails.State.OFFLINE)) {
            view.showAppListStatus(R.string.library_offline);
            return true;
        }
        switch (action) {
            case START_INSTEAD_ID:
                view.startInsteadOfRunning(app);
                return true;

            case START_OR_RESUME_ID:
                // Resume is the same as start for us
                ServerHelper.doStart(view, app.app, view.computer, view.managerBinder, LaunchSource.LIBRARY_MENU);
                return true;

            case QUIT_ID:
                // Display a confirmation dialog first
                UiHelper.displayQuitConfirmationDialog(view, () -> {
                    view.suspendGridUpdates = true;
                    ServerHelper.doQuit(view, view.computer, app.app, view.managerBinder, () -> {
                        // Trigger a poll immediately
                        view.suspendGridUpdates = false;
                        view.pollLibraryIfReady();
                    });
                }, null);
                return true;

            case VIEW_DETAILS_ID:
                Dialog.displayDialog(view, view.getString(R.string.title_details),
                        view.getString(R.string.app_details, app.app.getAppName(),
                                view.getString(app.app.isHdrSupported() ? R.string.yes : R.string.app_hdr_unknown),
                                app.app.getAppId()), false);
                return true;

            case HIDE_APP_ID:
                if (app.isHidden) {
                    // Transitioning hidden to shown
                    view.hiddenAppIds.remove(app.app.getAppId());
                } else {
                    // Transitioning shown to hidden
                    view.hiddenAppIds.add(app.app.getAppId());
                }
                boolean wasHidden = app.isHidden;
                view.updateHiddenApps(true);
                if (!wasHidden) {
                    Snackbar.make(view.findViewById(R.id.appFragmentContainer), R.string.library_hidden_notice, Snackbar.LENGTH_LONG)
                            .setAction(R.string.library_undo, v -> {
                                view.hiddenAppIds.remove(app.app.getAppId());
                                view.updateHiddenApps(true);
                            }).show();
                }
                return true;

            case OPEN_ON_START_ID:
                com.limelight.console.OpenPlay.toggle(view, view.computer.uuid, view.computer.displayName(), app.app.getAppId(), app.app.getAppUuid(), app.app.getAppName());
                return true;

            case CREATE_SHORTCUT_ID:
                // The game's artwork when it has some; the PC icon otherwise.
                ImageView appImageView = itemView == null ? null : itemView.findViewById(R.id.grid_image);
                Bitmap appBits = appImageView != null && CachedAppAssetLoader.hasArtwork(appImageView) ?
                        ((BitmapDrawable) appImageView.getDrawable()).getBitmap() : null;
                if (!view.shortcutHelper.createPinnedGameShortcut(view.computer, app.app, appBits)) {
                    Toast.makeText(view, view.getResources().getString(R.string.unable_to_pin_shortcut), Toast.LENGTH_LONG).show();
                }
                return true;

            default:
                return false;
        }
    }
}
