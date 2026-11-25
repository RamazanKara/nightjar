package com.limelight.preferences;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.widget.Toast;

import com.google.android.material.dialog.MaterialAlertDialogBuilder;
import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.computers.IdentityManager;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.ui.NavigationDialogBuilder;
import com.limelight.utils.FrontendExportStore;
import com.limelight.utils.FrontendExporter;
import com.limelight.utils.FrontendLauncher;
import com.limelight.utils.FrontendSync;

import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Frontend launcher export and folder-sync workflow controller for {@link HostStreamSettings}.
 */
final class HostFrontendExporter {
    static final int PICK_ROMS_REQUEST = 1;
    static final int PICK_ESDE_REQUEST = 2;

    private static final class PendingResult {
        final FrontendLauncher launcher;
        final String result;
        final boolean success;

        PendingResult(FrontendLauncher launcher, String result, boolean success) {
            this.launcher = launcher;
            this.result = result;
            this.success = success;
        }
    }

    private static final java.util.Map<String, PendingResult> PENDING_RESULTS =
            new java.util.concurrent.ConcurrentHashMap<>();

    private final HostStreamSettings activity;
    private final Runnable refreshActions;
    private Uri exportRomsTree;
    private boolean exporting;
    private boolean exportRunning;
    private FrontendLauncher exportLauncher = FrontendLauncher.ES_DE;

    HostFrontendExporter(HostStreamSettings activity, Runnable refreshActions) {
        this.activity = activity;
        this.refreshActions = refreshActions;
    }

    void restoreState(Bundle state) {
        exporting = state.getBoolean("exporting");
        String launcher = state.getString("export_launcher");
        if (launcher != null) exportLauncher = FrontendLauncher.valueOf(launcher);
        String romsTree = state.getString("export_roms_tree");
        exportRomsTree = romsTree != null ? Uri.parse(romsTree) : null;
        if (activity.computer != null && activity.computer.uuid != null) {
            PendingResult pending = PENDING_RESULTS.remove(activity.computer.uuid);
            if (pending != null) {
                exportLauncher = pending.launcher;
                new android.os.Handler(android.os.Looper.getMainLooper()).post(() -> {
                    if (!activity.isFinishing() && !activity.isDestroyed()) {
                        deliverResult(pending.launcher, pending.result, pending.success);
                    }
                });
            }
        }
    }

    void saveState(Bundle state) {
        state.putBoolean("exporting", exporting);
        state.putString("export_launcher", exportLauncher.name());
        if (exportRomsTree != null) state.putString("export_roms_tree", exportRomsTree.toString());
    }

    void start() {
        if (exportRunning) return;
        FrontendLauncher[] launchers = FrontendLauncher.values();
        String[] labels = new String[launchers.length];
        for (int i = 0; i < labels.length; i++) labels[i] = launchers[i].label;
        new NavigationDialogBuilder(activity)
                .setTitle(R.string.frontend_export_menu)
                .setItems(labels, (dialog, which) -> {
                    exportLauncher = launchers[which];
                    showFrontendActions();
                }).show();
    }

    private FrontendExportStore frontendStore() {
        return new FrontendExportStore(activity, activity.computer.uuid, exportLauncher);
    }

    private void showFrontendActions() {
        FrontendExportStore store = frontendStore();
        if (store.romsTree() == null) {
            chooseFrontendFolder();
            return;
        }
        if (store.failed()) {
            new NavigationDialogBuilder(activity)
                    .setTitle(exportLauncher.label)
                    .setMessage(R.string.frontend_sync_failed_message)
                    .setPositiveButton(R.string.frontend_export_pick, (dialog, which) -> chooseFrontendFolder())
                    .setNeutralButton(R.string.frontend_sync_stop, (dialog, which) -> stopFrontendSync(store))
                    .setNegativeButton(android.R.string.cancel, null).show();
            return;
        }
        new NavigationDialogBuilder(activity)
                .setTitle(activity.getString(R.string.frontend_sync_title, exportLauncher.label))
                .setItems(new String[] {activity.getString(R.string.frontend_export_update),
                        activity.getString(R.string.frontend_export_pick),
                        activity.getString(R.string.frontend_setup_title),
                        activity.getString(R.string.frontend_sync_stop)},
                        (dialog, which) -> {
                            if (which == 0) {
                                exportRomsTree = store.romsTree();
                                runFrontendExport(store.esdeTree());
                            } else if (which == 1) chooseFrontendFolder();
                            else if (which == 2) showFrontendSetup(null);
                            else stopFrontendSync(store);
                        }).show();
    }

    private void stopFrontendSync(FrontendExportStore store) {
        store.forget();
        refreshActions.run();
    }

    private void chooseFrontendFolder() {
        if (new FrontendExporter(activity, activity.computer).loadApps().isEmpty()) {
            Toast.makeText(activity, R.string.frontend_export_no_apps, Toast.LENGTH_LONG).show();
            return;
        }
        exporting = true;
        exportRomsTree = null;
        new NavigationDialogBuilder(activity)
                .setTitle(R.string.frontend_export_roms_title)
                .setMessage(R.string.frontend_export_roms_message)
                .setPositiveButton(R.string.frontend_export_pick, (dialog, which) -> pickFolder(PICK_ROMS_REQUEST))
                .setNegativeButton(android.R.string.cancel, (dialog, which) -> exporting = false)
                .setOnCancelListener(dialog -> exporting = false)
                .show();
    }

    private void pickFolder(int request) {
        try {
            activity.startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION |
                            Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION), request);
        } catch (ActivityNotFoundException e) {
            exporting = false;
            Toast.makeText(activity, R.string.frontend_export_no_picker, Toast.LENGTH_LONG).show();
        }
    }

    void onActivityResult(int requestCode, int resultCode, Intent data) {
        Uri picked = resultCode == Activity.RESULT_OK && data != null ? data.getData() : null;
        if (!exporting) {
            return;
        }
        if (requestCode != PICK_ROMS_REQUEST && requestCode != PICK_ESDE_REQUEST) return;
        if (picked != null) {
            try {
                int access = Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION;
                if ((data.getFlags() & access) == access) {
                    activity.getContentResolver().takePersistableUriPermission(picked, access);
                }
            } catch (SecurityException e) {
                LimeLog.info("Frontend folder has no persistent access");
            }
        }
        if (requestCode == PICK_ROMS_REQUEST) {
            if (picked == null) {
                exporting = false;
                return;
            }
            exportRomsTree = picked;
            if (exportLauncher != FrontendLauncher.ES_DE) {
                runFrontendExport(null);
                return;
            }
            new NavigationDialogBuilder(activity)
                    .setTitle(R.string.frontend_export_esde_title)
                    .setMessage(activity.getString(R.string.frontend_export_esde_message, activity.getString(R.string.app_name)))
                    .setPositiveButton(R.string.frontend_export_pick, (dialog, which) -> pickFolder(PICK_ESDE_REQUEST))
                    .setNegativeButton(R.string.frontend_export_skip, (dialog, which) -> runFrontendExport(null))
                    .setOnCancelListener(dialog -> runFrontendExport(null))
                    .show();
        } else if (requestCode == PICK_ESDE_REQUEST && exportRomsTree != null) {
            runFrontendExport(picked);
        }
    }

    private void runFrontendExport(final Uri esdeTree) {
        if (exportRunning) return;
        final ComputerDetails target = activity.computer;
        final Uri romsTree = exportRomsTree;
        final FrontendLauncher launcher = exportLauncher;
        final FrontendExportStore store = frontendStore();
        exporting = false;
        exportRomsTree = null;
        if (romsTree == null) {
            return;
        }
        exportRunning = true;
        Toast.makeText(activity, R.string.frontend_export_working, Toast.LENGTH_SHORT).show();
        new Thread(() -> {
            String message;
            boolean succeeded = false;
            try {
                FrontendExporter exporter = new FrontendExporter(activity, target);
                exporter.downloadMissingCovers(new IdentityManager(activity).getUniqueId());
                List<com.limelight.nvstream.http.NvApp> apps = exporter.loadApps();
                if (apps.isEmpty()) message = activity.getString(R.string.frontend_export_no_apps);
                else {
                    int count;
                    boolean lasting = FrontendSync.hasAccess(activity, romsTree) &&
                            (esdeTree == null || FrontendSync.hasAccess(activity, esdeTree));
                    synchronized (FrontendSync.EXPORT_LOCK) {
                        Map<String, String> covers = store.covers(esdeTree);
                        count = exporter.export(apps, romsTree, esdeTree, covers);
                        store.save(romsTree, esdeTree, FrontendSync.fingerprint(target.name, apps), covers);
                        if (!lasting) store.markFailed(romsTree, esdeTree);
                    }
                    succeeded = true;
                    message = activity.getResources().getQuantityString(R.plurals.frontend_export_done, count, count) +
                            "\n" + activity.getString(lasting ? R.string.frontend_sync_note : R.string.frontend_sync_unavailable,
                            activity.getString(R.string.app_name));
                }
            } catch (IOException | RuntimeException e) {
                LimeLog.warning("Frontend export failed: " + e);
                message = activity.getString(R.string.frontend_export_failed);
            }
            final String result = message;
            final boolean success = succeeded;
            final FrontendLauncher usedLauncher = launcher;
            final String hostUuid = target.uuid;
            activity.runOnUiThread(() -> {
                exportRunning = false;
                if (!activity.isFinishing() && activity.isDestroyed() && hostUuid != null) {
                    PENDING_RESULTS.put(hostUuid, new PendingResult(usedLauncher, result, success));
                    return;
                }
                if (activity.isFinishing() || activity.isDestroyed()) return;
                deliverResult(usedLauncher, result, success);
            });
        }, "Frontend export").start();
    }

    private void deliverResult(FrontendLauncher launcher, String result, boolean success) {
        refreshActions.run();
        exportLauncher = launcher;
        if (success) showFrontendSetup(result);
        else new NavigationDialogBuilder(activity).setTitle(launcher.label).setMessage(result)
                .setPositiveButton(R.string.frontend_export_pick, (dialog, which) -> chooseFrontendFolder())
                .setNegativeButton(android.R.string.cancel, null).show();
    }

    private void showFrontendSetup(String result) {
        String command = exportLauncher.command(activity.getPackageName());
        String message = (result == null ? "" : result + "\n\n") +
                activity.getString(exportLauncher.instructions, activity.getString(R.string.app_name));
        if (command != null) message += "\n\n" + command;
        MaterialAlertDialogBuilder dialog = new NavigationDialogBuilder(activity)
                .setTitle(exportLauncher.label).setMessage(message)
                .setPositiveButton(android.R.string.ok, null);
        if (command != null) {
            dialog.setNeutralButton(R.string.frontend_setup_copy, (ignored, which) -> {
                ClipboardManager clipboard = (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
                clipboard.setPrimaryClip(ClipData.newPlainText(exportLauncher.label, command));
            });
        }
        dialog.show();
    }

    static String exportSummary(HostStreamSettings activity, ComputerDetails pc) {
        List<String> synced = new ArrayList<>();
        List<String> failed = new ArrayList<>();
        for (FrontendLauncher launcher : FrontendExportStore.exported(activity, pc.uuid)) {
            boolean lost = new FrontendExportStore(activity, pc.uuid, launcher).failed();
            (lost ? failed : synced).add(launcher.label);
        }
        if (!failed.isEmpty()) {
            return activity.getString(R.string.frontend_sync_failed_summary, android.text.TextUtils.join(", ", failed));
        }
        if (!synced.isEmpty()) {
            return activity.getString(R.string.frontend_sync_summary, android.text.TextUtils.join(", ", synced));
        }
        return activity.getString(R.string.pc_settings_export_summary);
    }
}
