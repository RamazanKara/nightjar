package com.limelight.grid;

import android.content.Context;
import android.content.res.ColorStateList;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.widget.AbsListView;
import android.widget.AdapterView;
import android.widget.ImageButton;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.TextView;

import androidx.core.content.ContextCompat;
import androidx.core.view.ViewCompat;
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat.AccessibilityActionCompat;

import com.google.android.material.card.MaterialCardView;
import com.google.android.material.color.MaterialColors;
import com.limelight.PcView;
import com.limelight.R;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.PairingManager;
import com.limelight.nvstream.wol.WakeTargets;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.ui.AppTheme;
import com.limelight.ui.GlassButton;
import com.limelight.ui.GlassForm;
import com.limelight.ui.StageView;
import com.limelight.ui.PcRow;
import com.limelight.ui.PcScreen;
import com.limelight.ui.TouchTargets;
import com.limelight.update.HostUpdateChecker;

import java.util.Collections;
import java.util.Comparator;
import java.util.List;

public class PcGridAdapter extends GenericGridAdapter<PcView.ComputerObject> {
    /** What a PC row says, in the order rows are listed. */
    public enum RowState {
        RUNNING, READY, NOT_PAIRED, CHECKING, WAKING, OFFLINE;

        /** Rows sort in groups: running, online and paired, unpaired, then the PCs that don't answer. */
        int group() {
            return this == WAKING ? OFFLINE.ordinal() : ordinal();
        }
    }

    public PcGridAdapter(Context context, PreferenceConfiguration prefs) {
        super(context, layoutFor(context));
    }

    public void updateLayoutWithPreferences(Context context, PreferenceConfiguration prefs) {
        setLayoutId(layoutFor(context));
    }

    private static int layoutFor(Context context) {
        return GlassForm.of(context) == GlassForm.PHONE ? R.layout.pc_grid_item : R.layout.pc_grid_card;
    }

    public void addComputer(PcView.ComputerObject computer) {
        holdSelection();
        itemList.add(computer);
        sortList();
    }

    private void sortList() {
        Collections.sort(itemList, Comparator.comparingInt((PcView.ComputerObject computer) -> computer.rowState().group())
                .thenComparing(computer -> computer.details.displayName(), String.CASE_INSENSITIVE_ORDER));
    }

    public boolean removeComputer(PcView.ComputerObject computer) {
        holdSelection();
        return itemList.remove(computer);
    }

    @Override
    public void notifyDataSetChanged() {
        holdSelection();
        // A PC changes group when it starts a game, pairs or stops answering.
        sortList();
        // The ring stays on its PC, not on the slot the PC left. The grid is given the PC's new position before it hears
        // of the change, which it then keeps as it would have kept the old one. A PC that is gone leaves the grid's own.
        int position = held == null || list == null ? -1 : itemList.indexOf(held);
        if (position >= 0) list.setSelection(position);
        held = null;
        rowsMoved = false;
        super.notifyDataSetChanged();
    }

    /** The grid that shows these rows; a refresh keeps its selection on the same PC. */
    public void setList(AbsListView list) {
        this.list = list;
        held = null;
        rowsMoved = false;
    }

    /**
     * Notes the PC the D-pad's ring is on before the rows first change since the grid last heard of them; afterwards the
     * grid's selected position no longer names it. A grid without focus, or in touch mode, shows no ring and keeps none.
     */
    private void holdSelection() {
        if (rowsMoved) return;
        rowsMoved = true;
        held = null;
        if (list == null || list.isInTouchMode() || !list.hasFocus()) return;
        int position = list.getSelectedItemPosition();
        if (position >= 0 && position < itemList.size()) held = itemList.get(position);
    }

    public static RowState rowState(ComputerDetails details, boolean waking) {
        if (details.state == ComputerDetails.State.ONLINE) {
            if (details.pairState != PairingManager.PairState.PAIRED) {
                return RowState.NOT_PAIRED;
            }
            return details.hasRunningGame() ? RowState.RUNNING : RowState.READY;
        }
        if (waking) {
            return RowState.WAKING;
        }
        return details.state == ComputerDetails.State.OFFLINE ? RowState.OFFLINE : RowState.CHECKING;
    }

    /** The game the host reports as running, from its app list, or null when the list doesn't have it. */
    public static NvApp runningApp(List<NvApp> apps, ComputerDetails details) {
        if (apps == null || !details.hasRunningGame()) {
            return null;
        }
        for (NvApp app : apps) {
            if (app.getControl() == NvApp.Control.NONE &&
                    app.matchesRunningApp(details.runningGameId, details.runningGameUuid)) {
                return app;
            }
        }
        return null;
    }

    /** The row's one status line, such as "Playing Racing Game" or "Offline". */
    public static String statusText(Context context, PcView.ComputerObject computer) {
        switch (computer.rowState()) {
            case RUNNING:
                return computer.runningGame != null ?
                        context.getString(R.string.pc_status_playing, computer.runningGame.getAppName()) :
                        context.getString(R.string.pc_status_playing_unknown);
            case READY: {
                // Away from home, the line names the internet path in use.
                String path = com.limelight.computers.InternetPlay.pathLabel(context, computer.details.internetPath);
                return path != null ? context.getString(R.string.internet_path_label, path) :
                        context.getString(R.string.pc_status_online);
            }
            case NOT_PAIRED:
                return context.getString(R.string.pc_status_not_paired);
            case WAKING:
                return context.getString(R.string.pc_status_waking);
            case OFFLINE:
                return offlineReason(context, computer.details, context.getString(R.string.pc_offline));
            default:
                return context.getString(R.string.pc_checking);
        }
    }

    /**
     * A running game's cover was not on disk when its row looked for it. Only a bind looks again, so the grid has to
     * keep refreshing until it is there.
     */
    public boolean awaitsCover() {
        if (tiles == null) {
            return false;
        }
        for (PcView.ComputerObject computer : itemList) {
            if (computer.runningGame != null && computer.rowState() == RowState.RUNNING
                    && tiles.missed(computer.details.uuid, computer.runningGame)) {
                return true;
            }
        }
        return false;
    }

    /** What tapping the row does, named for screen readers. */
    public static int mainActionLabel(Context context, PcView.ComputerObject computer) {
        switch (computer.rowState()) {
            case RUNNING:
            case READY:
                return R.string.library_open;
            case NOT_PAIRED:
                return R.string.pcview_menu_pair_pc;
            case WAKING:
            case OFFLINE:
                return WakeTargets.hasMac(context, computer.details) ? R.string.pcview_menu_send_wol : R.string.pc_accessibility_actions;
            default:
                return R.string.pc_accessibility_actions;
        }
    }

    /** What a PC's visible action does. */
    public interface Actions {
        /** The running PC's Resume. */
        void onResume(PcView.ComputerObject computer);

        /** What a tap on the row does: Open, Wake or Pair. */
        void onMain(PcView.ComputerObject computer);
    }

    public void setActions(Actions actions) {
        this.actions = actions;
    }

    /** The short line under a PC's name, real data only: the game it plays, or its state in a word. */
    /**
     * Why an away PC is away, or {@code otherwise}. Another certificate at its address comes first: its fix (remove
     * the PC and add it again, if it was set up again) is the user's to make. A PC that gave this device internet
     * play details says which paths failed and why.
     */
    public static String offlineReason(Context context, ComputerDetails details, String otherwise) {
        if (details.anotherCertificate) {
            return context.getString(R.string.pc_another_answers);
        }
        return details.unreachable != null ? details.unreachable : otherwise;
    }

    public static String shortStatus(Context context, PcView.ComputerObject computer) {
        switch (computer.rowState()) {
            case RUNNING:
                return computer.runningGame != null ? computer.runningGame.getAppName() :
                        context.getString(R.string.pc_status_playing_unknown);
            case READY:
                return context.getString(R.string.pc_status_online);
            case NOT_PAIRED:
                return context.getString(R.string.pc_status_not_paired);
            case WAKING:
                return context.getString(R.string.pc_status_waking);
            case OFFLINE:
                return context.getString(R.string.pc_offline);
            default:
                return context.getString(R.string.pc_checking);
        }
    }

    /** The word on a PC's action, or 0 for none (a PC being checked, or away with nothing saved to wake it with). */
    public static int actionWord(Context context, PcView.ComputerObject computer) {
        switch (computer.rowState()) {
            case RUNNING:
                return R.string.applist_menu_resume;
            case READY:
                return R.string.console_hint_open;
            case NOT_PAIRED:
                return R.string.pcview_menu_pair_pc;
            case OFFLINE:
                return WakeTargets.hasMac(context, computer.details) ? R.string.pc_action_wake : 0;
            default:
                return 0;
        }
    }

    @Override
    public void populateView(View parentView, ImageView imgView, ProgressBar prgView, TextView txtView, PcView.ComputerObject obj) {
        RowState state = obj.rowState();
        boolean checking = state == RowState.CHECKING;
        boolean away = state == RowState.OFFLINE || state == RowState.WAKING;
        boolean card = !(parentView instanceof PcRow);
        boolean running = state == RowState.RUNNING;

        // The glyph on the screen: the standby mark while away, a lock until paired, a spinner while looking.
        int glyph = away ? R.drawable.ic_glass_power : state == RowState.NOT_PAIRED ? R.drawable.ic_glass_lock : 0;
        imgView.setImageResource(glyph);
        imgView.setImageTintList(ColorStateList.valueOf(MaterialColors.getColor(imgView, away
                ? com.google.android.material.R.attr.colorOnSurfaceVariant : com.google.android.material.R.attr.colorOnSurface)));
        imgView.setVisibility(glyph == 0 ? View.INVISIBLE : View.VISIBLE);
        prgView.setVisibility(checking ? View.VISIBLE : View.GONE);
        txtView.setText(obj.details.displayName());

        String status = statusText(context, obj);
        TextView statusView = parentView.findViewById(R.id.grid_status);
        statusView.setText(shortStatus(context, obj));
        statusView.setCompoundDrawablesRelativeWithIntrinsicBounds(statusDot(state), 0, 0, 0);

        PcScreen screen = parentView.findViewById(R.id.grid_screen);
        screen.setAway(away);
        if (tiles == null) tiles = new TileBackdrops(context);
        ImageView face = parentView.findViewById(R.id.grid_backdrop);
        if (card) {
            size(parentView.findViewById(R.id.grid_card), R.dimen.tile_height);
            MaterialCardView cardView = parentView.findViewById(R.id.grid_card);
            cardView.setCardBackgroundColor(StageView.groupFill(context));
            // The panel's lit edge; high contrast draws it solid.
            boolean solid = AppTheme.isHighContrast(context);
            cardView.setStrokeColor(solid ? ContextCompat.getColor(context, R.color.nj_hc_color_rim)
                    : MaterialColors.getColor(cardView, R.attr.glassGroupRim));
            cardView.setStrokeWidth(solid ? context.getResources().getDimensionPixelSize(R.dimen.nj_glass_chrome_solid_rim_width)
                    : Math.max(1, Math.round(context.getResources().getDisplayMetrics().density * .75f)));
            tiles.show(face, obj.details.uuid, obj.runningGame, state, TileBackdrops.Cut.SCREEN);
        } else {
            bindRow((PcRow) parentView, obj, running);
            ImageView cover = parentView.findViewById(R.id.grid_cover);
            if (running && obj.runningGame != null) {
                ((TextView) parentView.findViewById(R.id.grid_cover_text)).setText(obj.runningGame.getAppName());
                tiles.show(cover, obj.details.uuid, obj.runningGame, state, TileBackdrops.Cut.COVER);
                face.setImageDrawable(null);
            } else {
                cover.setImageDrawable(null);
                tiles.show(face, obj.details.uuid, obj.runningGame, state, TileBackdrops.Cut.SCREEN);
            }
        }
        bindActions(parentView, obj);

        String update = HostUpdateChecker.availableVersion(context, obj.details.uuid, obj.details.caps);
        TextView updateView = parentView.findViewById(R.id.grid_update);
        updateView.setVisibility(update == null ? View.GONE : View.VISIBLE);
        if (update != null) {
            updateView.setText(context.getString(R.string.host_update_available, update));
            status += ". " + updateView.getText();
        }

        parentView.setContentDescription(context.getString(R.string.library_item_description, obj.details.displayName(), status));
        // The grid owns taps and long presses; name both for screen readers and route them back to it.
        ViewCompat.replaceAccessibilityAction(parentView, AccessibilityActionCompat.ACTION_CLICK,
                context.getString(mainActionLabel(context, obj)), (view, arguments) -> {
                    AdapterView<?> grid = grid(view);
                    int position = grid == null ? AdapterView.INVALID_POSITION : grid.getPositionForView(view);
                    return position != AdapterView.INVALID_POSITION &&
                            grid.performItemClick(view, position, grid.getItemIdAtPosition(position));
                });
        ViewCompat.replaceAccessibilityAction(parentView, AccessibilityActionCompat.ACTION_LONG_CLICK,
                context.getString(R.string.pc_accessibility_actions), (view, arguments) -> openOptions(view));
    }

    /**
     * A phone's card, one for each PC: a running PC shows its game's cover with the action row under its name, the others
     * keep their action beside the name.
     */
    private void bindRow(PcRow row, PcView.ComputerObject obj, boolean running) {
        android.content.res.Resources res = context.getResources();
        float density = res.getDisplayMetrics().density;
        row.setPlace(true, true, itemList.indexOf(obj) + 1 < itemList.size(), 0f);
        span(row.findViewById(R.id.grid_resume), running, density);
        span(row.findViewById(R.id.grid_action), running, density);

        boolean withCover = running && obj.runningGame != null;
        row.findViewById(R.id.grid_cover_frame).setVisibility(withCover ? View.VISIBLE : View.GONE);
        row.findViewById(R.id.grid_screen).setVisibility(withCover ? View.GONE : View.VISIBLE);
        LinearLayout rowLine = row.findViewById(R.id.grid_row);
        LinearLayout body = row.findViewById(R.id.grid_body);
        LinearLayout actions = row.findViewById(R.id.grid_actions);
        rowLine.setGravity(running ? Gravity.TOP : Gravity.CENTER_VERTICAL);
        int pad = Math.round(density * (running ? 14 : 11));
        rowLine.setPaddingRelative(Math.round(14 * density), pad, Math.round((running ? 14 : 8) * density), pad);
        ViewGroup wanted = running ? body : rowLine;
        if (actions.getParent() != wanted) {
            ((ViewGroup) actions.getParent()).removeView(actions);
            LinearLayout.LayoutParams params = running
                    ? new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT)
                    : new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
            if (running) params.topMargin = Math.round(12 * density);
            else params.setMarginStart(Math.round(8 * density));
            wanted.addView(actions, params);
        }
        ImageButton more = row.findViewById(R.id.grid_more);
        LinearLayout.LayoutParams moreSize = (LinearLayout.LayoutParams) more.getLayoutParams();
        int moreWidth = Math.round((running ? 48 : 40) * density);
        if (moreSize.width != moreWidth) {
            moreSize.width = moreWidth;
            moreSize.setMarginStart(Math.round((running ? 10 : 2) * density));
            more.setLayoutParams(moreSize);
        }
        more.setBackgroundResource(running ? R.drawable.pc_more_filled : R.drawable.pc_more_quiet);
        TextView name = row.findViewById(R.id.grid_text);
        name.setTextSize(TypedValue.COMPLEX_UNIT_SP, running ? 19 : 17);
    }

    /** In the action row under a running PC's name an action is as wide as the row leaves; beside a name it is as wide as its word. */
    private static void span(View button, boolean wide, float density) {
        LinearLayout.LayoutParams params = (LinearLayout.LayoutParams) button.getLayoutParams();
        int width = wide ? 0 : ViewGroup.LayoutParams.WRAP_CONTENT;
        int height = Math.round((wide ? 48 : 44) * density);
        float weight = wide ? 1f : 0f;
        if (params.width == width && params.height == height && params.weight == weight) return;
        params.width = width;
        params.height = height;
        params.weight = weight;
        button.setLayoutParams(params);
    }

    /**
     * Whether a PC's action is the page's one amber control. The list is sorted with the PCs that are up first, so it is the
     * first PC's while that PC is up: Resume, Open or Pair. Every other action is quiet, and so is Wake.
     */
    static boolean leads(int position, RowState state) {
        return position == 0 && (state == RowState.RUNNING || state == RowState.READY || state == RowState.NOT_PAIRED);
    }

    /**
     * One action on a PC: the amber fill on the PC that leads the page, the quiet control on the rest. Resume resumes the
     * running game; every other word does what a tap on the PC does. A TV's keys say it instead.
     */
    private void bindActions(View parent, PcView.ComputerObject obj) {
        boolean television = GlassForm.of(context) == GlassForm.TELEVISION;
        RowState state = obj.rowState();
        GlassButton primary = parent.findViewById(R.id.grid_resume);
        GlassButton quiet = parent.findViewById(R.id.grid_action);
        View more = parent.findViewById(R.id.grid_more);
        // Touch is the only way into these controls; under 48dp they get a 48dp touch area, and keep their look and place.
        TouchTargets.apply(parent instanceof PcRow ? (ViewGroup) parent : (ViewGroup) more.getParent(), primary, quiet, more);
        int word = television ? 0 : actionWord(context, obj);
        boolean amber = word != 0 && leads(itemList.indexOf(obj), state);
        primary.setVisibility(amber ? View.VISIBLE : View.GONE);
        quiet.setVisibility(!amber && word != 0 ? View.VISIBLE : View.GONE);
        if (word == 0) return;
        boolean resumes = state == RowState.RUNNING;
        GlassButton shown = amber ? primary : quiet;
        shown.setText(word);
        shown.setIconResource(resumes ? R.drawable.ic_glass_play : state == RowState.OFFLINE ? R.drawable.ic_glass_power : 0);
        shown.setOnClickListener(v -> {
            if (actions == null) return;
            if (resumes) actions.onResume(obj);
            else actions.onMain(obj);
        });
    }

    /** The dot beside a PC's state; a PC that is not paired has none, its screen shows a lock. */
    static int statusDot(RowState state) {
        return state == RowState.NOT_PAIRED ? 0 : state == RowState.RUNNING || state == RowState.READY ? R.drawable.pc_dot_on : R.drawable.pc_dot_off;
    }

    private Actions actions;
    private TileBackdrops tiles;
    private AbsListView list;
    /** The PC the ring was on when the rows began to change, or null. */
    private PcView.ComputerObject held;
    /** The rows changed since the grid last heard of them. */
    private boolean rowsMoved;

    /** A TV tile part's height at the current font scale: its height at scale 1, plus what the text stack grows by. */
    static int tileHeight(Context context, int dimen) {
        android.content.res.Resources resources = context.getResources();
        float scale = Math.max(1f, resources.getConfiguration().fontScale);
        return resources.getDimensionPixelSize(dimen)
                + Math.round((scale - 1f) * resources.getDimension(R.dimen.tile_text_stack));
    }

    private void size(View view, int dimen) {
        if (view == null) return;
        ViewGroup.LayoutParams params = view.getLayoutParams();
        int height = tileHeight(context, dimen);
        if (params.height != height) {
            params.height = height;
            view.setLayoutParams(params);
        }
    }

    /** The PC sheet's status dot: the theme's ok colour for a PC that is up, the neutral dots otherwise. */
    public static int phoneDot(RowState state) {
        int dot = dot(state);
        return dot == R.drawable.tv_dot_live ? R.drawable.status_dot_ok : dot;
    }

    /** Green for a PC that is up, a filled neutral dot for one waiting on something, an outlined one for a PC that is offline. */
    public static int dot(RowState state) {
        switch (state) {
            case RUNNING:
            case READY:
                return R.drawable.tv_dot_live;
            case OFFLINE:
                return R.drawable.tv_dot_idle;
            default:
                return R.drawable.tv_dot_suspended;
        }
    }

    /** Opens the options of the PC on this card, as a long press does. */
    private static boolean openOptions(View card) {
        AdapterView<?> grid = grid(card);
        int position = grid == null ? AdapterView.INVALID_POSITION : grid.getPositionForView(card);
        AdapterView.OnItemLongClickListener listener = grid == null ? null : grid.getOnItemLongClickListener();
        return position != AdapterView.INVALID_POSITION && listener != null &&
                listener.onItemLongClick(grid, card, position, grid.getItemIdAtPosition(position));
    }

    private static AdapterView<?> grid(View card) {
        return card.getParent() instanceof AdapterView ? (AdapterView<?>) card.getParent() : null;
    }
}
