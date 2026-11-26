package com.limelight.console;

import android.app.Activity;
import android.graphics.Rect;
import android.graphics.drawable.ColorDrawable;
import android.view.View;
import android.view.ViewGroup;
import android.widget.LinearLayout;

import androidx.annotation.NonNull;
import androidx.recyclerview.widget.GridLayoutManager;
import androidx.recyclerview.widget.RecyclerView;

import com.limelight.R;
import com.limelight.ui.AppTheme;
import com.limelight.ui.StageView;

/**
 * TV overscan framing, glow-room padding/decorations, safe-edge sizing and responsive column fitting for
 * {@link AllGamesActivity}. On phones and tablets it aligns the title, filters and grid to the screen gutter.
 */
final class AllGamesTvFrame {
    /** The narrowest a poster's card gets before the grid drops a column. */
    private static final int MIN_CARD_WIDTH_DP = 120;

    private final Activity activity;
    private final boolean television;
    private final RecyclerView list;
    private final GridLayoutManager grid;
    private final ShelfAdapter adapter;
    private final ShelfAdapter.Env env;

    /** On a TV, the room above the first row inside the grid's frame, which a focused row keeps for its glow; 0 elsewhere. */
    private int roomAbove;
    private int lastWidthPx;
    private int lastHeightPx;
    /** Each column's width; on a TV a poster shrunk to fit the frame's height is narrower, the slack spread between. */
    private int slotPx;

    AllGamesTvFrame(Activity activity, boolean television, RecyclerView list, GridLayoutManager grid,
                    ShelfAdapter adapter, ShelfAdapter.Env env) {
        this.activity = activity;
        this.television = television;
        this.list = list;
        this.grid = grid;
        this.adapter = adapter;
        this.env = env;
    }

    int roomAbove() {
        return roomAbove;
    }

    /** Configures either the TV stage and glow frame or the non-TV gutter alignment, plus column sizing on layout. */
    void install() {
        if (television) {
            installTelevisionFrame();
        } else {
            alignToGutter();
        }
        list.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> fitColumns(r - l, b - t));
    }

    private void installTelevisionFrame() {
        activity.getWindow().setBackgroundDrawable(new ColorDrawable(AppTheme.tvCanvas(activity)));
        // The stage of PCs and Settings, behind the posters.
        StageView stage = activity.findViewById(R.id.all_games_stage);
        stage.setVisibility(View.VISIBLE);
        // Edge to edge, not inset by the TV margin as a lighter frame.
        stage.bleedPastPadding();
        // Rows scrolling up fade out under the heading and filters instead of stopping on a hard edge.
        list.setVerticalFadingEdgeEnabled(true);
        // The frame clips the grid. A focused poster's glow reaches past its 24 dp bleed and the row's top padding, and
        // was cut square there: the frame stands further out at the top and sides, and the list pads by as much. The
        // room above goes to the first row, not the padding: the fade starts at the padding's edge, and rows scrolled
        // into an unfaded strip would stand under the focused title.
        int room = Math.round(AllGamesActivity.GLOW_ROOM_DP * activity.getResources().getDisplayMetrics().density);
        View gridFrame = (View) list.getParent();
        ViewGroup.MarginLayoutParams frame = (ViewGroup.MarginLayoutParams) gridFrame.getLayoutParams();
        frame.topMargin -= room;
        // That room lies over the focus line and filters: a touch there is theirs, not the grid's.
        ((GridFrame) gridFrame).passTouchesAbove(room);
        frame.setMarginStart(frame.getMarginStart() - room);
        frame.setMarginEnd(frame.getMarginEnd() - room);
        int above = list.getPaddingTop() + room;
        roomAbove = above;
        // The last row can come up to the place every focused row takes: as much room below it as above (fitColumns).
        int fitRoom = Math.round(AllGamesActivity.FIT_ROOM_DP * activity.getResources().getDisplayMetrics().density);
        list.setPaddingRelative(list.getPaddingStart() + room, 0, list.getPaddingEnd() + room, above + fitRoom);
        list.addItemDecoration(new RecyclerView.ItemDecoration() {
            @Override
            public void getItemOffsets(@NonNull Rect out, @NonNull View card, @NonNull RecyclerView parent,
                                       @NonNull RecyclerView.State state) {
                int at = parent.getChildAdapterPosition(card);
                int columns = grid.getSpanCount();
                out.top = at != RecyclerView.NO_POSITION && at < columns ? above : 0;
                // Posters narrower than their columns keep the first at the start and the last at the end, the gaps even.
                int slack = Math.max(0, slotPx - env.cardWidthPx);
                if (at == RecyclerView.NO_POSITION || slack == 0 || columns < 2) return;
                int start = at % columns * slack / (columns - 1);
                boolean rtl = parent.getLayoutDirection() == View.LAYOUT_DIRECTION_RTL;
                out.left = rtl ? slack - start : start;
                out.right = rtl ? start : slack - start;
            }
        });
        list.setFadingEdgeLength(activity.getResources().getDimensionPixelSize(R.dimen.all_games_fade) + room);
        // The frame ends at the TV's safe edge, so a focused row's ring and glow fit whole. As the page's weighted last
        // child it ended the glow room short of the page's margin: a LinearLayout gives a negative margin no space.
        gridFrame.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> {
            if (t != ot || b != ob) v.post(() -> reachSafeEdge(v));
        });
        // A handheld in the forced TV layout rotates without being made again: the screen's height changes, the
        // frame's own place may not.
        activity.findViewById(R.id.all_games_page).addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> {
            if (r - l != or - ol || b - t != ob - ot) v.post(() -> reachSafeEdge(gridFrame));
        });
    }

    /**
     * The layout's insets are the TV's, inside its overscan margin. Elsewhere the title, the filters and the poster grid
     * line up with the shell bar on the screen gutter, and the focus line goes: a phone's posters carry their own titles.
     */
    private void alignToGutter() {
        int gutter = activity.getResources().getDimensionPixelSize(R.dimen.screen_gutter);
        for (int id : new int[] {R.id.all_games_title, R.id.all_games_filters_scroll}) {
            View view = activity.findViewById(id);
            view.setPaddingRelative(gutter, view.getPaddingTop(), gutter, view.getPaddingBottom());
        }
        activity.findViewById(R.id.all_games_focus).setVisibility(View.GONE);
        // The grid's frame reaches 24 dp past the content on both sides; each phone card keeps its cover inset as margin.
        int bleed = Math.round(24 * activity.getResources().getDisplayMetrics().density);
        int inset = activity.getResources().getDimensionPixelSize(R.dimen.cover_inset);
        int side = bleed + gutter - inset;
        list.setPaddingRelative(side, list.getPaddingTop(), side, list.getPaddingBottom());
    }

    /**
     * Each poster fills its column, so the grid spans the screen's width whatever it is. On a TV a poster is at most as
     * tall as lets a focused row's ring and glow fit the frame; one shrunk for that is narrower than its column.
     */
    private void fitColumns(int widthPx, int heightPx) {
        if (widthPx <= 0 || (widthPx == lastWidthPx && heightPx == lastHeightPx)) return;
        lastWidthPx = widthPx;
        lastHeightPx = heightPx;
        int content = widthPx - list.getPaddingLeft() - list.getPaddingRight();
        int count = AllGamesActivity.columnsFor(content,
                Math.round(MIN_CARD_WIDTH_DP * activity.getResources().getDisplayMetrics().density));
        int slot = content / count;
        int cover = ShelfAdapter.coverHeight(activity, television, slot);
        int card = slot;
        int tallest = roomAbove > 0 ? tallestCover(heightPx) : 0;
        if (tallest > 0 && cover > tallest) {
            int poster = tallest * 2 / 3;
            cover = poster * 3 / 2;
            card = poster + 2 * activity.getResources().getDimensionPixelSize(R.dimen.cover_inset);
        }
        if (count == grid.getSpanCount() && slot == slotPx && card == env.cardWidthPx && cover == env.coverHeightPx) return;
        int width = card;
        int height = cover;
        list.post(() -> {
            slotPx = slot;
            env.cardWidthPx = width;
            env.coverHeightPx = height;
            grid.setSpanCount(count);
            adapter.notifyDataSetChanged();
            list.invalidateItemDecorations();
        });
    }

    /**
     * The tallest TV poster whose focused ring and glow fit the frame of the given height. A focused poster grows about its
     * middle, so they reach as far below it as above; above, the first row's room holds them, so the frame keeps as much
     * below the poster, and a little room.
     */
    private int tallestCover(int frameHeightPx) {
        int inset = activity.getResources().getDimensionPixelSize(R.dimen.cover_inset);
        int fitRoom = Math.round(AllGamesActivity.FIT_ROOM_DP * activity.getResources().getDisplayMetrics().density);
        return frameHeightPx - 2 * (roomAbove + inset) - fitRoom;
    }

    /** Gives the grid's frame the height that ends it at the TV's safe edge. */
    private static void reachSafeEdge(View frame) {
        int screen = frame.getRootView().getHeight();
        if (screen <= 0) return;
        int[] at = new int[2];
        frame.getLocationInWindow(at);
        int height = screen - Math.round(screen * AllGamesActivity.SAFE_EDGE) - at[1];
        LinearLayout.LayoutParams params = (LinearLayout.LayoutParams) frame.getLayoutParams();
        if (height <= 0 || (params.height == height && params.weight == 0)) return;
        params.height = height;
        params.weight = 0;
        frame.setLayoutParams(params);
    }
}
