package com.limelight;

import android.content.res.Configuration;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.View;
import android.widget.AbsListView;

import com.limelight.computers.HostMenu;
import com.limelight.grid.PcGridAdapter;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.ui.GlassForm;
import com.limelight.ui.InputSource;
import com.limelight.ui.ShellBar;

/**
 * Glass chrome, the stage behind it, edge-to-edge inset layout, list footer placement,
 * selection/key-hint bar, empty-state switching, and post-resume focus recovery for {@link PcView}.
 */
final class PcViewChrome {
    private final PcView view;

    private View noPcFoundLayout;
    private AbsListView pcList;
    private boolean listWasEmpty = true;
    /** Resumed and not yet back in the window's focus: the first focus after a stream or another screen. */
    private boolean resumedFocusPending;
    // What the keys do on the selected PC shows once a key or button was the last input, not a touch; a TV has only keys.
    private boolean keysAreLastInput;
    private InputSource hintSource;
    private int hintVendor;
    private com.limelight.console.PageChrome chrome;
    /** What the list keeps free under its last row. */
    private int listFoot;
    private final androidx.activity.OnBackPressedCallback unmarkBack =
            new androidx.activity.OnBackPressedCallback(false) {
                @Override
                public void handleOnBackPressed() {
                    if (pcList instanceof com.limelight.ui.AdaptiveGridView) {
                        ((com.limelight.ui.AdaptiveGridView) pcList).clearMarkedAction();
                    }
                    setEnabled(false);
                }
            };

    PcViewChrome(PcView view) {
        this.view = view;
    }

    void bindViews() {
        noPcFoundLayout = view.findViewById(R.id.no_pc_found_layout);
    }

    void onResume() {
        resumedFocusPending = true;
    }

    void onDestroy() {
        if (chrome != null) chrome.stop();
    }

    /** The stage, the glass and where everything goes on this form factor: the list stands under the chrome and over the tab bar. */
    void setUpGlass() {
        com.limelight.ui.GlassHost host = view.findViewById(R.id.glass_host);
        GlassForm form = GlassForm.of(view);
        View title = view.findViewById(R.id.pc_title);
        View add = view.findViewById(R.id.manuallyAddPc);
        android.widget.TextView focusTitle = view.findViewById(R.id.pc_focus_title);
        if (form == GlassForm.PHONE) {
            // The keys' line floats over the tab bar.
            View hint = view.findViewById(R.id.pc_options_hint);
            ((android.view.ViewGroup) hint.getParent()).removeView(hint);
            host.addView(hint, new android.widget.FrameLayout.LayoutParams(android.view.ViewGroup.LayoutParams.WRAP_CONTENT,
                    android.view.ViewGroup.LayoutParams.WRAP_CONTENT, android.view.Gravity.BOTTOM | android.view.Gravity.CENTER_HORIZONTAL));
        } else {
            // The tabs lead the row and the + closes it; the title below follows the selection.
            title.setVisibility(View.INVISIBLE);
            ((android.widget.LinearLayout.LayoutParams) title.getLayoutParams()).height = 0;
            focusTitle.setVisibility(View.VISIBLE);
            boolean television = form == GlassForm.TELEVISION;
            focusTitle.setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, view.getResources().getDimension(R.dimen.pcs_hero_title));
            ((android.widget.LinearLayout.LayoutParams) focusTitle.getLayoutParams()).topMargin =
                    Math.round(view.getResources().getDisplayMetrics().density * (television ? 26 : 10));
        }
        ((android.widget.LinearLayout.LayoutParams) add.getLayoutParams()).setMarginStart(view.getResources().getDimensionPixelSize(R.dimen.space_s));
        hintSource = InputSource.last() != null ? InputSource.last() : com.limelight.ui.StreamMenuHint.hasGamepad() ? InputSource.PAD : InputSource.REMOTE;
        keysAreLastInput = form == GlassForm.TELEVISION;
        com.limelight.ui.StageView stage = view.findViewById(R.id.pc_stage);
        host.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> {
            if (r - l != or - ol || b - t != ob - ot) host.setAmbient(stage.bake(r - l, b - t), false, 0);
        });
        chrome = new com.limelight.console.PageChrome(view, host, R.id.pc_tabs, R.id.pc_top_row, R.id.pc_bottom_row, R.id.pc_foot_fade, this::layOutUnderChrome);
    }

    /** Lays the content out edge to edge, inside what the chrome and the window leave. */
    void layOutUnderChrome(int left, int top, int right, int bottom) {
        GlassForm form = chrome != null ? chrome.form() : GlassForm.of(view);
        android.content.res.Resources res = view.getResources();
        float density = res.getDisplayMetrics().density;
        View row = view.findViewById(R.id.pc_top_row);
        View frame = view.findViewById(R.id.pc_list_frame);
        android.widget.FrameLayout.LayoutParams params = (android.widget.FrameLayout.LayoutParams) frame.getLayoutParams();
        // First run's last action keeps its ring clear of the screen's edge, the gesture bar and a phone's tabs.
        view.findViewById(R.id.first_run).setPadding(0, 0, 0, bottom + res.getDimensionPixelSize(R.dimen.space_l)
                + (form == GlassForm.PHONE ? res.getDimensionPixelSize(R.dimen.nj_size_tab_bar_height) : 0));
        if (form == GlassForm.PHONE) {
            int edge = res.getDimensionPixelSize(R.dimen.glass_edge);
            int rowTop = top + Math.round(39 * density);
            row.setPaddingRelative(left + Math.round(20 * density), rowTop, right + edge, 0);
            params.topMargin = rowTop + res.getDimensionPixelSize(R.dimen.glass_capsule) + Math.round(12 * density);
            params.leftMargin = left;
            params.rightMargin = right;
            View bar = view.findViewById(R.id.pc_bottom_row);
            bar.setPadding(left + res.getDimensionPixelSize(R.dimen.space_m), 0, right + res.getDimensionPixelSize(R.dimen.space_m),
                    bottom + res.getDimensionPixelSize(R.dimen.space_s));
            listFoot = bottom + res.getDimensionPixelSize(R.dimen.nj_size_tab_bar_height) + 3 * res.getDimensionPixelSize(R.dimen.space_m);
            View fade = view.findViewById(R.id.pc_foot_fade);
            android.view.ViewGroup.LayoutParams fadeSize = fade.getLayoutParams();
            fadeSize.height = res.getDimensionPixelSize(R.dimen.home_foot_fade) + bottom;
            fade.setLayoutParams(fadeSize);
            View hint = view.findViewById(R.id.pc_options_hint);
            ((android.widget.FrameLayout.LayoutParams) hint.getLayoutParams()).bottomMargin = bottom
                    + res.getDimensionPixelSize(R.dimen.nj_size_tab_bar_height) + res.getDimensionPixelSize(R.dimen.space_m);
        } else {
            boolean television = form == GlassForm.TELEVISION;
            int side = res.getDimensionPixelSize(R.dimen.shell_overscan_horizontal);
            int inset = television ? 0 : Math.round(24 * density);
            // The capsule stands where it stands on Home; only a handheld keeps a gap above the safe area.
            int rowTop = television ? top : top + res.getDimensionPixelSize(R.dimen.space_m);
            row.setPaddingRelative(left + inset, rowTop, right + inset, 0);
            View focusTitle = view.findViewById(R.id.pc_focus_title);
            focusTitle.setPaddingRelative(left + inset, 0, right + inset + Math.round(density * (television ? 300 : 200)), 0);
            View hint = view.findViewById(R.id.pc_options_hint);
            hint.setPaddingRelative(left + inset, hint.getPaddingTop(), 0, hint.getPaddingBottom());
            params.topMargin = top + res.getDimensionPixelSize(R.dimen.pcs_list_top);
            // A card pads itself with its glow: the card itself starts under the title, the glow past it.
            int glow = res.getDimensionPixelSize(R.dimen.tile_glow);
            params.leftMargin = left + side + inset - glow;
            params.rightMargin = right + side + inset - glow;
            ((com.limelight.ui.ContentFrame) frame).setMaxContentWidth(Integer.MAX_VALUE);
            listFoot = bottom;
        }
        frame.setLayoutParams(params);
        applyListFoot();
    }

    void applyListFoot() {
        if (pcList != null && listFoot != pcList.getPaddingBottom()) {
            pcList.setPadding(pcList.getPaddingLeft(), pcList.getPaddingTop(), pcList.getPaddingRight(), listFoot);
        }
    }

    /** The selected PC's name and what the keys do on it. */
    void showSelection() {
        if (pcList == null) return;
        Object item = pcList.getSelectedItem();
        PcView.ComputerObject computer = item instanceof PcView.ComputerObject ? (PcView.ComputerObject) item : null;
        android.widget.TextView title = view.findViewById(R.id.pc_focus_title);
        if (title.getVisibility() == View.VISIBLE) {
            CharSequence name = computer != null ? computer.details.displayName() : view.getString(R.string.library_computers);
            if (!name.toString().contentEquals(title.getText())) title.setText(name);
        }
        android.widget.TextView hint = view.findViewById(R.id.pc_options_hint);
        if (hintSource == null) return;
        CharSequence line = computer == null ? null : com.limelight.console.HomeHints.line(view, hintSource, hintVendor,
                goWord(computer), view.getString(R.string.console_hint_options));
        hint.setText(line != null ? line : "");
    }

    /** What OK does on this PC, which is what a tap does: the word must not promise anything else. */
    private CharSequence goWord(PcView.ComputerObject computer) {
        PcGridAdapter.RowState state = computer.rowState();
        if (LaunchPolicy.resumesOnPcTap(PreferenceConfiguration.readPreferences(view).quickResume, state == PcGridAdapter.RowState.RUNNING)) {
            return view.getString(R.string.applist_menu_resume);
        }
        if (state == PcGridAdapter.RowState.RUNNING) return view.getString(R.string.console_hint_open);
        int word = PcGridAdapter.actionWord(view, computer);
        return word == 0 ? null : view.getString(word);
    }

    boolean onKeyEvent(KeyEvent event) {
        if (event.getAction() == KeyEvent.ACTION_DOWN) {
            InputSource source = InputSource.of(event);
            if (source != null) {
                InputSource.remember(source);
                hintSource = source;
                hintVendor = InputSource.vendorOf(event);
            }
            setKeysAreLastInput(true);
        }
        // Up from the grid's first row goes to the current tab of the shell bar, as on Home; on a phone the tabs are at the foot.
        return GlassForm.of(view) != GlassForm.PHONE && com.limelight.ui.ShellFocus.upToCurrentTab(view, event,
                view.findViewById(R.id.fragmentView), view.findViewById(R.id.manuallyAddPc));
    }

    void onTouchEvent(MotionEvent event) {
        if (event.getActionMasked() == MotionEvent.ACTION_DOWN) {
            setKeysAreLastInput(false);
        }
    }

    private void setKeysAreLastInput(boolean keys) {
        keysAreLastInput = keys;
        updateOptionsHint();
        showSelection();
    }

    private void updateOptionsHint() {
        View hint = view.findViewById(R.id.pc_options_hint);
        if (hint != null && view.pcGridAdapter != null) {
            // Invisible, not gone, while a pointer is in use: the list must not resize on every input switch.
            hint.setVisibility(view.pcGridAdapter.getCount() == 0 ? View.GONE
                    : keysAreLastInput ? View.VISIBLE : View.INVISIBLE);
        }
    }

    void updateEmptyState() {
        boolean empty = view.pcGridAdapter.getCount() == 0;
        GlassForm form = chrome != null ? chrome.form() : GlassForm.of(view);
        updateOptionsHint();
        noPcFoundLayout.setVisibility(empty ? View.VISIBLE : View.GONE);
        // With no PC there is no selection to name, and the selected tab already names the page.
        if (form != GlassForm.PHONE) {
            view.findViewById(R.id.pc_focus_title).setVisibility(empty ? View.GONE : View.VISIBLE);
            showSelection();
        }
        view.findViewById(R.id.pcFragmentContainer).setVisibility(empty ? View.GONE : View.VISIBLE);
        view.findViewById(R.id.manuallyAddPc).setVisibility(empty ? View.GONE : View.VISIBLE);
        if (form != GlassForm.PHONE) ShellBar.linkTopRow(view, R.id.manuallyAddPc);
        placeListFooter();
        boolean filled = listWasEmpty && !empty;
        listWasEmpty = empty;
        if (filled && pcList != null && !pcList.isInTouchMode()) {
            // The first PC replaced the empty state, which held focus; give it to the list, not the shell bar.
            claimListFocus();
        }
    }

    /**
     * Puts the footer just under the last PC, as long as every PC shows and the footer fits above the tab bar; otherwise it
     * waits, invisible. It fills the screen a few PCs leave empty with what a new user needs next: the list is still
     * looking, and how to connect a PC that does not appear. A TV has none: its tiles sit centred on the screen.
     * <p>It is placed by its top margin, not a translation, which focus search ignores; while it shows, Down from the
     * last row goes through its link.
     */
    void placeListFooter() {
        View footer = view.findViewById(R.id.pc_list_footer);
        View container = view.findViewById(R.id.pcFragmentContainer);
        if (footer == null || pcList == null || view.pcGridAdapter == null) return;
        boolean television = (view.getResources().getConfiguration().uiMode & Configuration.UI_MODE_TYPE_MASK)
                == Configuration.UI_MODE_TYPE_TELEVISION;
        int count = view.pcGridAdapter.getCount();
        boolean show = false;
        if (!television && count > 0 && container.getVisibility() == View.VISIBLE && pcList.getFirstVisiblePosition() == 0
                && pcList.getChildCount() == count) {
            View last = pcList.getChildAt(count - 1);
            int top = container.getTop() + pcList.getTop() + last.getBottom()
                    + view.getResources().getDimensionPixelSize(R.dimen.space_l);
            int room = ((View) footer.getParent()).getHeight() - listFoot;
            android.view.ViewGroup.MarginLayoutParams params = (android.view.ViewGroup.MarginLayoutParams) footer.getLayoutParams();
            if (params.topMargin != top) {
                params.topMargin = top;
                footer.setLayoutParams(params);
            }
            show = top + footer.getHeight() <= room;
        }
        int visibility = show ? View.VISIBLE : View.INVISIBLE;
        if (footer.getVisibility() != visibility) footer.setVisibility(visibility);
        linkListFooter(show);
    }

    /** With the footer shown its link sits under the PCs, for the D-pad and for a screen reader. */
    private void linkListFooter(boolean shown) {
        View help = view.findViewById(R.id.pc_list_help);
        pcList.setNextFocusDownId(shown ? R.id.pc_list_help : View.NO_ID);
        help.setNextFocusUpId(R.id.fragmentView);
        // A screen reader reads the views in layout order, footer last, unless told where the footer comes.
        help.setAccessibilityTraversalAfter(R.id.fragmentView);
    }

    private boolean focusIsLoose() {
        View focus = view.getCurrentFocus();
        return focus == null || isInside(focus, R.id.shell_bar) || isInside(focus, R.id.no_pc_found_layout);
    }

    void onWindowFocusChanged(boolean hasFocus) {
        if (!hasFocus || !resumedFocusPending) return;
        resumedFocusPending = false;
        // Back from a stream a pad or a remote drove (its menu's Disconnect), the PC that runs the game takes the ring, so A
        // acts at once. This window can still be in the touch mode its last tap left: the keys of the screen before end it.
        if (pcList == null || view.pcGridAdapter == null || view.pcGridAdapter.getCount() == 0) return;
        boolean touch = pcList.isInTouchMode();
        InputSource last = InputSource.last();
        if (touch && (last == null || last == InputSource.NONE)) return;
        boolean ringShown = !touch && pcList.hasFocus() && pcList.getSelectedItemPosition() != android.widget.AdapterView.INVALID_POSITION;
        if (ringShown || (!pcList.hasFocus() && !focusIsLoose())) return;
        // Out of touch mode first: a list in touch mode keeps no selection.
        if (touch) pcList.requestFocusFromTouch();
        for (int i = 0; i < view.pcGridAdapter.getCount(); i++) {
            Object item = view.pcGridAdapter.getItem(i);
            if (item instanceof PcView.ComputerObject && ((PcView.ComputerObject) item).rowState() == PcGridAdapter.RowState.RUNNING) {
                pcList.setSelection(i);
                break;
            }
        }
        if (!touch) claimListFocus();
    }

    private void claimListFocus() {
        if (!focusIsLoose() || pcList.requestFocus()) return;
        // The list cannot take focus yet; it tries again after its next layout. Not from inside that layout: the grid asks
        // for a layout of its own to mark the selected PC, and a request made while its layout runs is lost.
        pcList.addOnLayoutChangeListener(new View.OnLayoutChangeListener() {
            @Override
            public void onLayoutChange(View v, int l, int t, int r, int b, int ol, int ot, int or, int ob) {
                v.removeOnLayoutChangeListener(this);
                v.post(() -> {
                    if (!v.isInTouchMode() && focusIsLoose()) v.requestFocus();
                });
            }
        });
    }

    private boolean isInside(View target, int containerId) {
        if (target.getId() == containerId) return true;
        android.view.ViewParent p = target.getParent();
        while (p != null) {
            if (p instanceof View && ((View) p).getId() == containerId) return true;
            p = p.getParent();
        }
        return false;
    }

    void bindListView(AbsListView listView, PcViewActions actions) {
        pcList = listView;
        listView.setAdapter(view.pcGridAdapter);
        view.pcGridAdapter.setList(listView);
        applyListFoot();
        view.pcGridAdapter.setActions(new PcGridAdapter.Actions() {
            @Override public void onResume(PcView.ComputerObject computer) { actions.onComputerAction(computer, HostMenu.RESUME); }
            @Override public void onMain(PcView.ComputerObject computer) { actions.onComputerTapped(computer); }
        });
        listView.setOnItemSelectedListener(new android.widget.AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(android.widget.AdapterView<?> parent, View v, int position, long id) { showSelection(); }
            @Override public void onNothingSelected(android.widget.AdapterView<?> parent) { showSelection(); }
        });
        listView.setOnItemClickListener((parent, v, position, id) ->
                actions.onComputerTapped((PcView.ComputerObject) view.pcGridAdapter.getItem(position)));
        listView.setOnItemLongClickListener((parent, v, position, id) -> {
            view.showComputerMenu((PcView.ComputerObject) view.pcGridAdapter.getItem(position));
            return true;
        });
        if (listView instanceof com.limelight.ui.AdaptiveGridView) {
            com.limelight.ui.AdaptiveGridView grid = (com.limelight.ui.AdaptiveGridView) listView;
            grid.setEndFocusTarget(view.findViewById(R.id.manuallyAddPc));
            unmarkBack.remove();
            unmarkBack.setEnabled(grid.isActionMarked());
            grid.setOnMarkChanged(() -> unmarkBack.setEnabled(pcList instanceof com.limelight.ui.AdaptiveGridView
                    && ((com.limelight.ui.AdaptiveGridView) pcList).isActionMarked()));
            view.getOnBackPressedDispatcher().addCallback(view, unmarkBack);
        } else {
            unmarkBack.setEnabled(false);
        }
        listView.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> placeListFooter());
        listView.setOnScrollListener(new AbsListView.OnScrollListener() {
            @Override public void onScrollStateChanged(AbsListView v, int state) { }
            @Override public void onScroll(AbsListView v, int first, int visible, int total) { placeListFooter(); }
        });
        if (view.pcGridAdapter.getCount() == 0) view.findViewById(R.id.discovery_add).requestFocus();
        else if (!listView.isInTouchMode()) listView.requestFocus(); // content first, not the shell bar
    }
}
