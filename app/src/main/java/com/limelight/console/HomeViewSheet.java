package com.limelight.console;

import android.app.Activity;
import android.preference.PreferenceManager;
import android.view.View;
import android.view.ViewGroup;
import android.widget.LinearLayout;

import com.google.android.material.button.MaterialButton;
import com.limelight.R;
import com.limelight.ui.ActionSheet;
import com.limelight.ui.GlassForm;
import com.limelight.ui.ShellBar;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.function.Consumer;
import java.util.function.IntConsumer;
import java.util.function.Supplier;

/**
 * The View button and sheet on Home: grid, list or rows, grouping, sort order and card size. Changes apply once the
 * sheet closes so trying several choices costs a single rebuild.
 */
final class HomeViewSheet {
    private final Activity activity;
    private ActionSheet viewSheet;

    HomeViewSheet(Activity activity) {
        this.activity = activity;
    }

    /** Wires the View button in the top row or phone bottom bar, including D-pad links beside it. */
    static void setUpButton(Activity activity, Runnable onOpen) {
        View button = activity.findViewById(R.id.console_view);
        button.setOnClickListener(v -> onOpen.run());
        GlassForm form = GlassForm.of(activity);
        if (form != GlassForm.PHONE) {
            // A TV's layout has one search; the other forms' top row shows its own and hides the foot's.
            View top = activity.findViewById(R.id.home_search_top);
            ShellBar.linkTopRow(activity, R.id.home_pc, R.id.console_view,
                    top != null && top.getVisibility() == View.VISIBLE ? R.id.home_search_top : R.id.home_search);
            return;
        }
        // A phone's bar is at the foot: View is past its last tab.
        ViewGroup tabs = activity.findViewById(R.id.shell_bar_items);
        if (tabs != null && tabs.getChildCount() > 0) {
            View last = tabs.getChildAt(tabs.getChildCount() - 1);
            last.setNextFocusRightId(R.id.console_view);
            button.setNextFocusLeftId(last.getId());
        }
        button.setNextFocusRightId(R.id.console_view);
    }

    /** Opens the View sheet if not already showing; saves and recreates the activity on close when changed. */
    void show(Supplier<HomeView> current, Consumer<HomeView> setView, HomeModel model, boolean television) {
        if (viewSheet != null) {
            return;
        }
        final HomeView before = current.get();
        final Consumer<HomeView> persist = updated -> {
            setView.accept(updated);
            updated.save(PreferenceManager.getDefaultSharedPreferences(activity));
        };
        // Off a phone View sits in the top row: its panel hangs under it, as the PC sheet hangs under the capsule.
        ActionSheet sheet = GlassForm.of(activity) == GlassForm.PHONE
                ? new ActionSheet(activity, activity.getString(R.string.home_view))
                : ActionSheet.under(activity, activity.getString(R.string.home_view),
                        activity.findViewById(R.id.console_view));
        viewSheet = sheet;
        // A TV's panel hangs under View with no room below: its parts go in two columns so the whole sheet fits with
        // nothing to scroll. Phones and tablets keep one column.
        LinearLayout[] columns = television ? sheet.addColumns(2) : null;
        LinearLayout left = columns == null ? null : columns[0];
        LinearLayout right = columns == null ? null : columns[1];
        addSection(sheet, left, R.string.home_view_layout);
        // A list's lines have one size, so Size shows only for the covers' layouts.
        View[] sizeSection = new View[2];
        addChoices(sheet, left, new int[] {R.drawable.ic_library, R.drawable.ic_view_list, R.drawable.ic_view_rows},
                new int[] {R.string.home_view_layout_grid, R.string.home_view_layout_list, R.string.home_view_layout_rows},
                current.get().layout.ordinal(), at -> {
                    HomeView next = current.get().with(HomeView.Layout.values()[at]);
                    persist.accept(next);
                    for (View part : sizeSection) {
                        part.setVisibility(next.layout == HomeView.Layout.LIST ? View.GONE : View.VISIBLE);
                    }
                });
        addSection(sheet, left, R.string.home_view_group);
        HomeView.Group[] groups = HomeView.offeredGroups(model.hasCollections(), model.hasInstallationSplit());
        int[] groupIcons = new int[groups.length];
        int[] groupLabels = new int[groups.length];
        for (int i = 0; i < groups.length; i++) {
            groupIcons[i] = groupIcon(groups[i]);
            groupLabels[i] = groupLabel(groups[i]);
        }
        // Collection chosen while no PC names any shows by store, and says so.
        addChoices(sheet, left, groupIcons, groupLabels, Math.max(0, Arrays.asList(groups).indexOf(current.get().group)),
                at -> persist.accept(current.get().with(groups[at])));
        addSection(sheet, right, R.string.home_view_sort);
        addSortChoices(sheet, right, current, persist, model);
        sizeSection[0] = addSection(sheet, right, R.string.home_view_size);
        sizeSection[1] = addChoices(sheet, right,
                new int[] {R.drawable.ic_view_size_small, R.drawable.ic_view_size_medium, R.drawable.ic_view_size_large},
                new int[] {R.string.home_view_size_small, R.string.home_view_size_medium, R.string.home_view_size_large},
                current.get().size.ordinal(), at -> persist.accept(current.get().with(HomeView.Size.values()[at])));
        for (View part : sizeSection) {
            part.setVisibility(current.get().layout == HomeView.Layout.LIST ? View.GONE : View.VISIBLE);
        }
        sheet.setOnClosed(() -> {
            viewSheet = null;
            HomeView after = current.get();
            if (!after.equals(before) && !activity.isFinishing() && !activity.isDestroyed()) {
                after.save(PreferenceManager.getDefaultSharedPreferences(activity));
                activity.recreate();
            }
        });
        sheet.show();
    }

    private static int groupIcon(HomeView.Group group) {
        return group == HomeView.Group.STORE ? R.drawable.ic_view_group_store
                : group == HomeView.Group.TYPE ? R.drawable.ic_view_group_type
                : group == HomeView.Group.COLLECTION ? R.drawable.ic_folder
                : group == HomeView.Group.INSTALLATION ? R.drawable.ic_download : R.drawable.ic_view_group_none;
    }

    private static int groupLabel(HomeView.Group group) {
        return group == HomeView.Group.STORE ? R.string.home_view_group_store
                : group == HomeView.Group.TYPE ? R.string.home_view_group_type
                : group == HomeView.Group.COLLECTION ? R.string.home_view_group_collection
                : group == HomeView.Group.INSTALLATION ? R.string.home_view_group_installation : R.string.home_view_group_none;
    }

    /** A heading in one of the sheet's columns, or in the sheet itself for null. */
    private static View addSection(ActionSheet sheet, ViewGroup column, int title) {
        return column == null ? sheet.addSection(title) : sheet.addSection(column, title);
    }

    /**
     * Sort by: Title, Last played or Play time, one row. Choosing the one already chosen turns its order around, as a
     * column header does; the chosen one's arrow shows which way. One row, not two, so the sheet still fits a TV whole.
     */
    private void addSortChoices(ActionSheet sheet, ViewGroup column, Supplier<HomeView> current,
                                Consumer<HomeView> setView, HomeModel model) {
        // Play time only while some PC sends it, as Group by offers Collection only with collections.
        List<HomeView.Sort> sorts = new ArrayList<>(Arrays.asList(HomeView.Sort.TITLE, HomeView.Sort.LAST_PLAYED));
        if (model.hasPlaytime()) sorts.add(HomeView.Sort.PLAYTIME);
        LinearLayout row = sheet.addToggleRow(column);
        MaterialButton[] toggles = new MaterialButton[sorts.size()];
        Runnable mark = () -> {
            HomeView now = current.get();
            HomeView.Sort shown = sorts.contains(now.sort) ? now.sort : HomeView.Sort.TITLE;
            for (int i = 0; i < toggles.length; i++) {
                boolean chosen = sorts.get(i) == shown;
                toggles[i].setChecked(chosen);
                toggles[i].setIconResource(!chosen ? sortIcon(sorts.get(i))
                        : now.order == HomeView.Order.ASCENDING ? R.drawable.ic_expand_less : R.drawable.ic_expand_more);
                String label = activity.getString(sortLabel(sorts.get(i)));
                toggles[i].setContentDescription(!chosen ? label : label + ", " + activity.getString(
                        now.order == HomeView.Order.ASCENDING
                                ? R.string.home_view_order_ascending : R.string.home_view_order_descending));
            }
        };
        for (int i = 0; i < toggles.length; i++) {
            final HomeView.Sort sort = sorts.get(i);
            toggles[i] = sheet.addToggle(row, sortIcon(sort), activity.getString(sortLabel(sort)), false);
            toggles[i].setOnClickListener(v -> {
                HomeView now = current.get();
                HomeView.Sort shown = sorts.contains(now.sort) ? now.sort : HomeView.Sort.TITLE;
                HomeView next = sort == shown
                        ? now.with(sort).with(now.order == HomeView.Order.ASCENDING
                                ? HomeView.Order.DESCENDING : HomeView.Order.ASCENDING)
                        : now.with(sort);
                setView.accept(next);
                mark.run();
            });
        }
        mark.run();
    }

    private static int sortIcon(HomeView.Sort sort) {
        return sort == HomeView.Sort.TITLE ? R.drawable.ic_view_list
                : sort == HomeView.Sort.LAST_PLAYED ? R.drawable.ic_refresh : R.drawable.ic_timer;
    }

    private static int sortLabel(HomeView.Sort sort) {
        return sort == HomeView.Sort.TITLE ? R.string.home_view_sort_title
                : sort == HomeView.Sort.LAST_PLAYED ? R.string.home_view_sort_last_played : R.string.home_view_sort_playtime;
    }

    /** One choice of the View sheet: a row of toggles of which exactly one is on; the row is returned. */
    private static LinearLayout addChoices(ActionSheet sheet, ViewGroup column, int[] icons, int[] labels,
                                           int chosen, IntConsumer choose) {
        LinearLayout row = sheet.addToggleRow(column);
        MaterialButton[] toggles = new MaterialButton[labels.length];
        for (int i = 0; i < labels.length; i++) {
            final int at = i;
            toggles[i] = sheet.addToggle(row, icons[i], sheet.getContext().getString(labels[i]), i == chosen);
            toggles[i].setOnClickListener(v -> {
                for (int j = 0; j < toggles.length; j++) toggles[j].setChecked(j == at);
                choose.accept(at);
            });
        }
        return row;
    }
}
