package com.limelight;

import android.graphics.drawable.ColorDrawable;
import android.os.Bundle;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.widget.AbsListView;
import android.widget.TextView;

import androidx.appcompat.widget.SearchView;

import com.limelight.console.HomeHints;
import com.limelight.console.PageChrome;
import com.limelight.grid.AppGridAdapter;
import com.limelight.ui.ActionSheet;
import com.limelight.ui.AdaptiveGridView;
import com.limelight.ui.AppTheme;
import com.limelight.ui.BackdropView;
import com.limelight.ui.GlassForm;
import com.limelight.ui.GlassHost;
import com.limelight.ui.InputSource;
import com.limelight.ui.LibraryAmbience;
import com.limelight.ui.ShellBar;

/**
 * Glass chrome, backdrop ambience, inset layout, running-game foot capsule, search bar, library
 * options sheet, input-source hint updates, and library status panels extracted from {@link AppView}.
 */
final class AppViewChrome {
    private final AppView view;

    private LibraryAmbience ambience;
    private SearchView librarySearch;
    private ActionSheet libraryMenu;
    private PageChrome chrome;
    private InputSource hintSource;
    private int hintVendor;
    /** What the grid keeps free, as the chrome leaves it: start, end, foot. */
    private int gridStart, gridEnd, gridFoot;
    private int safeBottom, safeRight, safeLeft;
    private int lastTvFocusDownId;

    AppViewChrome(AppView view) {
        this.view = view;
    }

    void bindSearch() {
        librarySearch = view.findViewById(R.id.library_search);
        librarySearch.setOnQueryTextListener(new SearchView.OnQueryTextListener() {
            @Override public boolean onQueryTextSubmit(String query) {
                librarySearch.clearFocus();
                View grid = view.findViewById(R.id.fragmentView);
                if (grid != null) grid.requestFocus();
                return true;
            }
            @Override public boolean onQueryTextChange(String query) {
                view.searchQuery = query;
                if (view.appGridAdapter != null) view.appGridAdapter.setSearchQuery(query);
                view.updateLibraryState();
                return true;
            }
        });
    }

    void restoreSearchState(Bundle savedInstanceState) {
        librarySearch.setQuery(view.searchQuery, false);
        librarySearch.setVisibility(savedInstanceState.getBoolean("librarySearchVisible") ? View.VISIBLE : View.GONE);
        librarySearch.clearFocus();
    }

    void saveSearchState(Bundle state) {
        state.putBoolean("librarySearchVisible", librarySearch != null && librarySearch.getVisibility() == View.VISIBLE);
    }

    void onPause() {
        if (libraryMenu != null) libraryMenu.dismiss();
    }

    void onDestroy() {
        if (chrome != null) chrome.stop();
    }

    /** The glass and where everything goes on this form factor: the grid stands under the chrome and over the tabs. */
    void setUpChrome() {
        GlassHost host = view.findViewById(R.id.glass_host);
        GlassForm form = GlassForm.of(view);
        View back = view.findViewById(R.id.library_back);
        View search = view.findViewById(R.id.library_search_button);
        View options = view.findViewById(R.id.library_options);
        back.setOnClickListener(v -> view.finish());
        search.setOnClickListener(v -> openLibrarySearch());
        options.setOnClickListener(v -> showLibraryOptions());
        ((TextView) view.findViewById(R.id.library_pc_name)).setText(view.getIntent().getStringExtra(AppView.NAME_EXTRA));
        showDot(view.findViewById(R.id.library_resume_dot), true);
        showPcState(true);
        view.findViewById(R.id.library_resume_button).setOnClickListener(v -> {
            AppView.AppObject running = view.runningApp();
            if (running != null) view.play(running, v);
        });
        TextView title = view.findViewById(R.id.library_focus_title);
        TextView line = view.findViewById(R.id.library_focus_status);
        if (form == GlassForm.PHONE) {
            // The capsule follows the back button, and one search sits beside the tabs, as on Home.
            view.findViewById(R.id.library_space_before).setVisibility(View.GONE);
            view.findViewById(R.id.library_space_after).setVisibility(View.VISIBLE);
            ((android.view.ViewGroup) search.getParent()).removeView(search);
            int size = view.getResources().getDimensionPixelSize(R.dimen.nj_size_tab_bar_height);
            android.widget.LinearLayout.LayoutParams place = new android.widget.LinearLayout.LayoutParams(size, size);
            place.setMarginStart(Math.round(10 * view.getResources().getDisplayMetrics().density));
            ((android.view.ViewGroup) view.findViewById(R.id.library_bottom_row)).addView(search, place);
            // The keys' line floats over the foot, for a phone with a pad in hand.
            ((android.view.ViewGroup) line.getParent()).removeView(line);
            host.addView(line, new android.widget.FrameLayout.LayoutParams(android.view.ViewGroup.LayoutParams.WRAP_CONTENT,
                    view.getResources().getDimensionPixelSize(R.dimen.library_hint_height), android.view.Gravity.BOTTOM | android.view.Gravity.CENTER_HORIZONTAL));
        } else {
            // The tabs lead the row and name the page; there is no way back that they do not offer.
            back.setVisibility(View.GONE);
            title.setVisibility(View.VISIBLE);
            line.setVisibility(View.VISIBLE);
            ((android.widget.LinearLayout.LayoutParams) title.getLayoutParams()).topMargin =
                    Math.round(view.getResources().getDisplayMetrics().density * (form == GlassForm.TELEVISION ? 16 : 10));
            title.forceHasOverlappingRendering(false);
            if (form == GlassForm.HANDHELD) {
                // Alone at the foot, it is as wide as what it says.
                android.widget.FrameLayout.LayoutParams place = (android.widget.FrameLayout.LayoutParams) view.findViewById(R.id.library_resume).getLayoutParams();
                place.gravity = android.view.Gravity.BOTTOM | android.view.Gravity.END;
                place.width = Math.round(view.getResources().getDisplayMetrics().density * 320);
            }
        }
        // Before the chrome, which styles the tabs: installing them again would undo that.
        ShellBar.install(view, ShellBar.Destination.PCS);
        // Last: the chrome reports the safe area at once, and that needs everything in its place.
        chrome = new PageChrome(view, host, R.id.library_tabs, R.id.library_top_row, R.id.library_bottom_row, R.id.library_foot_fade, this::layOutUnderChrome);
        if (form != GlassForm.PHONE) ShellBar.linkTopRow(view, R.id.library_options, R.id.library_search_button);
        // This page is inside PCs: its tab goes back to the list.
        view.findViewById(R.id.shell_pcs).setOnClickListener(v -> view.finish());
        view.findViewById(R.id.library_chrome).addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> placeGrid(b));
        hintSource = InputSource.last() != null ? InputSource.last() : form == GlassForm.TELEVISION
                ? (com.limelight.ui.StreamMenuHint.hasGamepad() ? InputSource.PAD : InputSource.REMOTE) : InputSource.NONE;
        host.getViewTreeObserver().addOnTouchModeChangeListener(touch -> {
            if (touch || hintSource != InputSource.NONE) return;
            onInput(InputSource.leavingTouch(com.limelight.ui.StreamMenuHint.hasGamepad(),
                    view.getResources().getConfiguration().keyboard == android.content.res.Configuration.KEYBOARD_QWERTY), 0);
        });
    }

    private void showDot(View target, boolean ok) {
        android.graphics.drawable.GradientDrawable dot = new android.graphics.drawable.GradientDrawable();
        dot.setShape(android.graphics.drawable.GradientDrawable.OVAL);
        dot.setColor(ok ? com.google.android.material.color.MaterialColors.getColor(target, R.attr.glassOk)
                : androidx.core.content.ContextCompat.getColor(view, R.color.nj_color_label_secondary));
        target.setBackground(dot);
    }

    /** Whether the capsule's dot says the PC is there. */
    void showPcState(boolean online) {
        showDot(view.findViewById(R.id.library_pc_dot), online);
    }

    /** Lays the content out edge to edge, inside what the chrome and the window leave. */
    private void layOutUnderChrome(int left, int top, int right, int bottom) {
        GlassForm form = chrome != null ? chrome.form() : GlassForm.of(view);
        android.content.res.Resources res = view.getResources();
        float density = res.getDisplayMetrics().density;
        safeLeft = left;
        safeRight = right;
        safeBottom = bottom;
        View row = view.findViewById(R.id.library_top_row);
        if (form == GlassForm.PHONE) {
            int edge = res.getDimensionPixelSize(R.dimen.glass_edge);
            row.setPaddingRelative(left + edge, top + Math.round(8 * density), right + edge, 0);
            ((android.view.ViewGroup.MarginLayoutParams) librarySearch.getLayoutParams()).setMarginStart(left + edge);
            ((android.view.ViewGroup.MarginLayoutParams) librarySearch.getLayoutParams()).setMarginEnd(right + edge);
            placeSummary(left + edge, right + edge);
            gridStart = left + edge;
            gridEnd = right + edge;
            view.findViewById(R.id.library_bottom_row).setPadding(left + res.getDimensionPixelSize(R.dimen.space_m), 0,
                    right + res.getDimensionPixelSize(R.dimen.space_m), bottom + res.getDimensionPixelSize(R.dimen.space_s));
            View fade = view.findViewById(R.id.library_foot_fade);
            android.view.ViewGroup.LayoutParams fadeSize = fade.getLayoutParams();
            fadeSize.height = res.getDimensionPixelSize(R.dimen.home_foot_fade) + bottom;
            fade.setLayoutParams(fadeSize);
        } else {
            boolean television = form == GlassForm.TELEVISION;
            int side = res.getDimensionPixelSize(R.dimen.shell_overscan_horizontal) + (television ? 0 : Math.round(24 * density));
            row.setPaddingRelative(left + side, television ? top : top + res.getDimensionPixelSize(R.dimen.space_m), right + side, 0);
            android.view.ViewGroup.MarginLayoutParams title = (android.view.ViewGroup.MarginLayoutParams) view.findViewById(R.id.library_focus_title).getLayoutParams();
            title.setMarginStart(left + side);
            title.setMarginEnd(right + side + Math.round(density * (television ? 300 : 200)));
            android.view.ViewGroup.MarginLayoutParams line = (android.view.ViewGroup.MarginLayoutParams) view.findViewById(R.id.library_focus_status).getLayoutParams();
            line.setMarginStart(left + side);
            line.setMarginEnd(right + side);
            ((android.view.ViewGroup.MarginLayoutParams) librarySearch.getLayoutParams()).setMarginStart(left + side);
            ((android.view.ViewGroup.MarginLayoutParams) librarySearch.getLayoutParams()).setMarginEnd(right + side);
            placeSummary(left + side, right + side);
            android.view.ViewGroup.MarginLayoutParams statusBanner = (android.view.ViewGroup.MarginLayoutParams) view.findViewById(R.id.library_status_banner).getLayoutParams();
            statusBanner.setMarginStart(left + side);
            statusBanner.setMarginEnd(right + side);
            gridStart = left + side;
            gridEnd = right + side;
        }
        android.widget.FrameLayout.LayoutParams loading = (android.widget.FrameLayout.LayoutParams) view.findViewById(R.id.library_loading).getLayoutParams();
        loading.topMargin = top;
        placeFoot();
    }

    private void placeSummary(int start, int end) {
        android.view.ViewGroup.MarginLayoutParams at = (android.view.ViewGroup.MarginLayoutParams) view.findViewById(R.id.library_summary).getLayoutParams();
        at.setMarginStart(start);
        at.setMarginEnd(end);
    }

    /** The capsule of the running game and the keys' line above the tabs, and what the grid keeps free under its last row. */
    private void placeFoot() {
        GlassForm form = chrome != null ? chrome.form() : GlassForm.of(view);
        android.content.res.Resources res = view.getResources();
        View resume = view.findViewById(R.id.library_resume);
        AppView.AppObject running = form == GlassForm.TELEVISION ? null : view.runningApp();
        int shown = running == null ? View.GONE : View.VISIBLE;
        if (resume.getVisibility() != shown) resume.setVisibility(shown);
        if (running != null) {
            TextView name = view.findViewById(R.id.library_resume_name);
            if (!running.app.getAppName().contentEquals(name.getText())) name.setText(running.app.getAppName());
        }
        int gap = res.getDimensionPixelSize(R.dimen.space_s);
        int side = res.getDimensionPixelSize(R.dimen.space_m);
        int capsule = running == null ? 0 : res.getDimensionPixelSize(R.dimen.library_resume_height) + gap;
        android.widget.FrameLayout.LayoutParams place = (android.widget.FrameLayout.LayoutParams) resume.getLayoutParams();
        int foot;
        if (form == GlassForm.PHONE) {
            int bar = safeBottom + gap + res.getDimensionPixelSize(R.dimen.nj_size_tab_bar_height);
            place.leftMargin = safeLeft + side;
            place.rightMargin = safeRight + side;
            place.bottomMargin = bar + gap;
            foot = bar + capsule + 2 * side;
            View line = view.findViewById(R.id.library_focus_status);
            android.widget.FrameLayout.LayoutParams at = (android.widget.FrameLayout.LayoutParams) line.getLayoutParams();
            at.bottomMargin = bar + capsule + gap;
            line.setLayoutParams(at);
        } else {
            place.rightMargin = gridEnd;
            place.bottomMargin = safeBottom + side;
            foot = safeBottom + (running == null ? side : capsule + 2 * side);
            // A TV's focused poster grows and wears its ring: the grid keeps room for that and the overscan under its last
            // row, so it can scroll the card that far clear of the screen's edge.
            if (form == GlassForm.TELEVISION) {
                foot = Math.max(foot, safeBottom + res.getDimensionPixelSize(R.dimen.shell_overscan_vertical)
                        + Math.round(24 * res.getDisplayMetrics().density));
            }
        }
        resume.setLayoutParams(place);
        gridFoot = foot;
        applyGridPadding();
    }

    void applyGridPadding() {
        View grid = view.findViewById(R.id.fragmentView);
        if (grid == null) return;
        int top = view.getResources().getDimensionPixelSize(R.dimen.app_grid_top);
        if (grid.getPaddingStart() != gridStart || grid.getPaddingEnd() != gridEnd || grid.getPaddingTop() != top || grid.getPaddingBottom() != gridFoot) {
            grid.setPaddingRelative(gridStart, top, gridEnd, gridFoot);
        }
    }

    /** The grid starts where the chrome ends, which a search field opening moves. */
    private void placeGrid(int chromeBottom) {
        View frame = view.findViewById(R.id.appFragmentContainer);
        android.widget.FrameLayout.LayoutParams params = (android.widget.FrameLayout.LayoutParams) frame.getLayoutParams();
        if (params.topMargin == chromeBottom) return;
        params.topMargin = chromeBottom;
        // From a layout listener: on the next turn, or the request is lost in this pass.
        frame.post(() -> frame.setLayoutParams(params));
    }

    /** The last input names the keys on the line: on the next input, with no animation. */
    private void onInput(InputSource source, int vendor) {
        if (source == null) return;
        InputSource.remember(source);
        if (source == hintSource && vendor == hintVendor) return;
        hintSource = source;
        hintVendor = vendor;
        if (ambience != null) ambience.refresh();
    }

    void onTouchEvent(MotionEvent event) {
        if (event.getActionMasked() == MotionEvent.ACTION_DOWN) onInput(InputSource.of(event), 0);
    }

    void onGenericMotionEvent(MotionEvent event) {
        onInput(InputSource.of(event), event.getDevice() == null ? 0 : event.getDevice().getVendorId());
    }

    boolean onKeyEvent(KeyEvent event) {
        if (event.getAction() == KeyEvent.ACTION_DOWN) onInput(InputSource.of(event), InputSource.vendorOf(event));
        // Up from the grid's first row goes to the current tab of the shell bar, as on Home; on a phone the tabs are at the foot.
        return GlassForm.of(view) != GlassForm.PHONE && com.limelight.ui.ShellFocus.upToCurrentTab(view, event,
                view.findViewById(R.id.fragmentView), view.findViewById(R.id.library_options),
                view.findViewById(R.id.library_search_button));
    }

    /** What the game in focus is, then what the keys do on it. What OK does is what a tap does: the word promises nothing else. */
    private CharSequence focusLine(AppView.AppObject app) {
        int state = AppGridAdapter.stateWord(app);
        CharSequence keys = HomeHints.line(view, hintSource, hintVendor,
                view.getString(app.isRunning ? R.string.applist_menu_resume : R.string.library_play), view.getString(R.string.console_hint_options));
        android.text.SpannableStringBuilder line = new android.text.SpannableStringBuilder();
        if (state != 0 && GlassForm.of(view) != GlassForm.PHONE) {
            line.append(view.getString(state));
            line.setSpan(new android.text.style.ForegroundColorSpan(com.google.android.material.color.MaterialColors.getColor(view.findViewById(R.id.library_focus_status), com.google.android.material.R.attr.colorOnSurfaceVariant)),
                    0, line.length(), android.text.Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
        }
        if (keys != null) {
            if (line.length() > 0) line.append("    ");
            line.append(keys);
        }
        return line;
    }

    /** The ambient of the game in focus behind everything; on a handheld and a TV its title and keys over the grid. */
    void setUpAmbience() {
        BackdropView backdrop = view.findViewById(R.id.library_backdrop);
        GlassForm form = GlassForm.of(view);
        int canvas = form == GlassForm.TELEVISION ? AppTheme.tvCanvas(view)
                : com.google.android.material.color.MaterialColors.getColor(backdrop, R.attr.glassCanvas);
        chrome.setUpBackdrop(backdrop, canvas);
        // The window shows the canvas until the first backdrop covers it, then stops painting under it.
        view.getWindow().setBackgroundDrawable(new ColorDrawable(canvas));
        // The TV's safe-area padding sits on the content view, which would otherwise clip the backdrop that reaches past it.
        ((android.view.ViewGroup) view.findViewById(android.R.id.content)).setClipChildren(false);
        backdrop.setCanvasColor(canvas);
        backdrop.setDim(AppTheme.isHighContrast(view) ? 0.85f : 1f);
        backdrop.setOnCovered(() -> view.getWindow().setBackgroundDrawable(null));
        ambience = new LibraryAmbience(backdrop, form == GlassForm.PHONE ? null : view.findViewById(R.id.library_focus_title), view.findViewById(R.id.library_focus_status));
        ambience.setLine(this::focusLine);
    }

    /** The first time the grid holds games: a PC's cached list reaches the adapter before the grid exists, a fresh one after. */
    void playEnterOnce(View grid) {
        if (view.enterPlayed || !(grid instanceof AdaptiveGridView)) return;
        view.enterPlayed = true;
        ((AdaptiveGridView) grid).playEnter();
    }

    void updateLibraryState() {
        if (librarySearch == null) return;
        int visible = view.appGridAdapter == null ? 0 : view.appGridAdapter.getCount();
        if (ambience != null) {
            if (visible == 0) {
                ambience.clear();
            } else {
                ambience.refresh();
                playEnterOnce(view.findViewById(R.id.fragmentView));
            }
        }
        placeFoot();
        view.findViewById(R.id.library_loading).setVisibility(view.loadingApps ? View.VISIBLE : View.GONE);
        String summary = view.appGridAdapter == null ? null
                : AppView.librarySummary(view.getResources(), view.searchQuery, view.showHiddenApps, visible, view.appGridAdapter.getLibraryCount());
        TextView summaryLine = view.findViewById(R.id.library_summary);
        if (summary == null) {
            summaryLine.setVisibility(View.GONE);
        } else {
            if (!summary.contentEquals(summaryLine.getText())) summaryLine.setText(summary);
            summaryLine.setVisibility(View.VISIBLE);
        }

        int message = view.libraryStatus;
        int action = R.string.library_retry;
        View.OnClickListener retry = v -> view.refreshLibrary();
        if (view.loadingApps && visible == 0) {
            message = R.string.library_loading;
        } else if (message == 0 && visible == 0 && view.appGridAdapter != null) {
            if (!view.searchQuery.trim().isEmpty()) {
                message = R.string.library_no_matches;
                action = R.string.library_clear_search;
                retry = v -> librarySearch.setQuery("", false);
            } else if (!view.appGridAdapter.getAllApps().isEmpty()) {
                message = R.string.library_all_hidden;
                action = R.string.library_show_hidden;
                retry = v -> view.setShowHiddenApps(true);
            }
        }
        if (message == R.string.library_no_apps) action = R.string.library_refresh;
        TextView status = view.findViewById(R.id.app_list_status);
        status.setVisibility(message == 0 ? View.GONE : View.VISIBLE);
        if (message != 0) status.setText(message);
        // A TV that still shows its saved games gets the line above the grid; the panel is for a grid with nothing in it,
        // where nothing would be covered (nightjar#7).
        boolean inline = message != 0 && visible > 0 && GlassForm.of(view) == GlassForm.TELEVISION;
        view.findViewById(R.id.library_status_banner).setVisibility(inline ? View.VISIBLE : View.GONE);
        TextView bannerText = view.findViewById(R.id.app_list_status_banner);
        if (inline) bannerText.setText(message);
        view.findViewById(R.id.library_status_panel).setVisibility(message == 0 || inline ? View.GONE : View.VISIBLE);
        TextView retryButton = view.findViewById(R.id.library_retry);
        retryButton.setText(action);
        retryButton.setOnClickListener(retry);
        boolean offered = message != 0 && !view.loadingApps;
        retryButton.setVisibility(offered ? View.VISIBLE : View.GONE);
        TextView bannerRetry = view.findViewById(R.id.library_retry_banner);
        bannerRetry.setText(action);
        bannerRetry.setOnClickListener(retry);
        bannerRetry.setVisibility(inline && offered ? View.VISIBLE : View.GONE);
        // The line stands between the top row and the grid: Down reaches its Retry before the games.
        if (GlassForm.of(view) == GlassForm.TELEVISION) {
            int down = inline && offered ? R.id.library_retry_banner : R.id.fragmentView;
            if (down != lastTvFocusDownId) {
                lastTvFocusDownId = down;
                for (int id : new int[] {R.id.library_back, R.id.library_options, R.id.library_search_button}) {
                    View top = view.findViewById(id);
                    if (top != null) top.setNextFocusDownId(down);
                }
                ViewGroup items = view.findViewById(R.id.shell_bar_items);
                if (items != null) {
                    for (int i = 0; i < items.getChildCount(); i++) {
                        items.getChildAt(i).setNextFocusDownId(down);
                    }
                }
                bannerRetry.setNextFocusDownId(R.id.fragmentView);
            }
        }
        // An empty grid has nothing for a remote to land on: what can be done about it takes the focus.
        View focused = view.getCurrentFocus();
        if (offered && visible == 0 && !retryButton.isInTouchMode() && (focused == null || focused.getId() == R.id.fragmentView)) {
            retryButton.requestFocus();
        }
    }

    void openLibrarySearch() {
        librarySearch.setVisibility(View.VISIBLE);
        librarySearch.setIconified(false);
        librarySearch.requestFocus();
    }

    private void showLibraryOptions() {
        if (libraryMenu != null) return;
        ActionSheet sheet = new ActionSheet(view, view.getString(R.string.library_options));
        libraryMenu = sheet;
        sheet.addAction(R.drawable.ic_refresh, view.getString(R.string.library_refresh)).setOnClickListener(v -> {
            sheet.dismiss();
            view.refreshLibrary();
        });
        sheet.addAction(view.showHiddenApps ? R.drawable.ic_visibility_off : R.drawable.ic_visibility,
                view.getString(view.showHiddenApps ? R.string.library_hide_hidden : R.string.library_show_hidden)).setOnClickListener(v -> {
            sheet.dismiss();
            view.setShowHiddenApps(!view.showHiddenApps);
        });
        sheet.setOnDismissListener(dialog -> libraryMenu = null);
        sheet.show();
    }

    void navigateBack() {
        View grid = view.findViewById(R.id.fragmentView);
        if (grid instanceof com.limelight.ui.AdaptiveGridView && ((com.limelight.ui.AdaptiveGridView) grid).clearMarkedAction()) {
            return;
        }
        if (librarySearch != null && librarySearch.getVisibility() == View.VISIBLE) {
            librarySearch.setQuery("", false);
            librarySearch.clearFocus();
            librarySearch.setVisibility(View.GONE);
            view.findViewById(R.id.library_search_button).requestFocus();
        } else {
            view.finish();
        }
    }

    void bindAmbience(AbsListView listView) {
        if (ambience != null) ambience.bind(listView, view.appGridAdapter);
    }
}
