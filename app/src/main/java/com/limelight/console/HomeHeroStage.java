package com.limelight.console;

import android.app.Activity;
import android.graphics.Bitmap;
import android.os.Handler;
import android.os.Looper;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewTreeObserver;
import android.widget.FrameLayout;

import androidx.core.content.ContextCompat;
import androidx.recyclerview.widget.RecyclerView;

import com.limelight.R;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.ui.BackdropView;
import com.limelight.ui.GlassForm;
import com.limelight.ui.HeroStageView;
import com.limelight.ui.Motion;
import com.limelight.ui.NoArtTile;
import com.limelight.ui.ViewportScaler;

import java.util.function.Function;
import java.util.function.Supplier;

/**
 * Home's hero art: lays a {@link HeroStageView} over the backdrop and tells it which cover to show.
 *
 * <p>On a TV the hero is the card that has focus, so the art changes with the backdrop, in the same step. On a phone and a
 * handheld the hero is the first item of the list and keeps its game while focus roams the shelves: the art is that game's,
 * moves with the item as the list scrolls and is gone with it. A phone held upright gives the art room over the words, which
 * stand at the foot of the window's top third as far as the hero and the first shelf's upper half still fit above the tab bar
 * (a usual phone has no such room: its cover takes it, see {@link HomeHero#coverWidth}). A game without art shows
 * its tile's gradient, as its backdrop does. The art is in the stage's light on every form (the view holds it there), so the
 * words need no scrim and this class does not have to know where they stand.
 */
final class HomeHeroStage {
    /** The tag a hero's views hang their game on, on the list's item. */
    static final int GAME = R.id.hero_title;
    /** A phone held upright: the hero's words end this far down the window. */
    static final float WORDS_FOOT = 1f / 3f;
    /**
     * What a phone keeps under its hero for the first shelf, in dp: its heading and the upper half of its covers, less the
     * strip the list's own padding already keeps free over the tab bar.
     */
    static final int SHELF_PEEK_DP = HomeHero.SHELF_HEADING_DP + HomeHero.SHELF_HALF_DP - 28;

    private final Activity activity;
    private final BackdropView backdrop;
    private final RecyclerView list;
    private final GlassForm form;
    private final HeroStageView stage;
    private final Function<String, ComputerDetails> hosts;
    private final Supplier<CoverLoaders> covers;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final int[] at = new int[2];
    private final int[] base = new int[2];
    private final Runnable apply = this::applyWanted;
    private final ViewTreeObserver.OnGlobalLayoutListener onLayout = () -> follow(true);
    /** The hero's game on a phone and a handheld, and the key of the one whose art is shown. */
    private HomeModel.GameCard wanted;
    private String shownKey;
    /** A change of game is waiting out its settle time. */
    private boolean settling;

    HomeHeroStage(Activity activity, BackdropView backdrop, RecyclerView list, GlassForm form, int canvas,
                  Function<String, ComputerDetails> hosts, Supplier<CoverLoaders> covers) {
        this.activity = activity;
        this.backdrop = backdrop;
        this.list = list;
        this.form = form;
        this.hosts = hosts;
        this.covers = covers;
        stage = new HeroStageView(activity);
        stage.setCanvasColor(canvas);
        stage.setHeroShare(ViewportScaler.of(activity, false).heroHeight);
        backdrop.addView(stage, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        if (form == GlassForm.TELEVISION) {
            // The TV's hero is written in the label colours whatever the theme.
            stage.setInk(ContextCompat.getColor(activity, R.color.nj_color_label));
            backdrop.setCoverListener((key, cover, animate) -> stage.show(cover, animate));
            return;
        }
        stage.setVisibility(View.INVISIBLE);
        list.addOnScrollListener(new RecyclerView.OnScrollListener() {
            @Override public void onScrolled(RecyclerView view, int dx, int dy) {
                follow(false);
            }
        });
        list.getViewTreeObserver().addOnGlobalLayoutListener(onLayout);
    }

    /** The view that draws the art. */
    HeroStageView view() {
        return stage;
    }

    /**
     * The room a phone's hero leaves over its words: enough to end them at {@link #WORDS_FOOT} of the window, never so much
     * that the rest of the hero and what the first shelf shows under it no longer fit the list, and none where the words
     * already reach that far.
     *
     * @param window   the window's height
     * @param restTop  where the hero's item starts in the window while the list rests
     * @param words    the words' own height
     * @param rest     the height of what follows the words in the hero: the carousel and Play
     * @param listRoom the list's height between its paddings, less what the first shelf keeps under the hero
     */
    static int room(int window, int restTop, int words, int rest, int listRoom) {
        int wanted = Math.round(window * WORDS_FOOT) - restTop - words;
        return Math.max(0, Math.min(wanted, listRoom - words - rest));
    }

    /** Where a view stands in the stage while it rests, whatever is moving it for a moment. */
    private void place(View view) {
        view.getLocationInWindow(at);
        backdrop.getLocationInWindow(base);
        at[0] -= base[0] + Math.round(view.getTranslationX());
        at[1] -= base[1] + Math.round(view.getTranslationY());
    }

    /**
     * The list was laid out, scrolled, shown or hidden: the art stays with the hero's item, and goes when that is not on screen.
     * {@code laidOut} is false for a scrolled frame, which only moves it.
     */
    private void follow(boolean laidOut) {
        RecyclerView.LayoutManager rows = list.getLayoutManager();
        View item = rows == null || !list.isShown() ? null : rows.findViewByPosition(0);
        Object tag = item == null ? null : item.getTag(GAME);
        if (!(tag instanceof HomeModel.GameCard)) {
            stage.setVisibility(View.INVISIBLE);
            return;
        }
        // How far the list has carried the item from where it rests.
        int carried = item.getTop() - list.getPaddingTop();
        place(item);
        int restTop = at[1] - carried;
        View words = item.findViewById(R.id.hero_words);
        if (form == GlassForm.PHONE && words != null) {
            int own = words.getHeight() - words.getPaddingTop();
            int peek = Math.round(SHELF_PEEK_DP * activity.getResources().getDisplayMetrics().density);
            int room = room(stage.getHeight(), restTop, own, item.getHeight() - words.getHeight(),
                    list.getHeight() - list.getPaddingTop() - list.getPaddingBottom() - peek);
            if (room != words.getPaddingTop() && words.getHeight() > 0) {
                words.post(() -> words.setPadding(words.getPaddingLeft(), room, words.getPaddingRight(), words.getPaddingBottom()));
            }
        }
        stage.setTranslationY(carried);
        stage.setVisibility(View.VISIBLE);
        want((HomeModel.GameCard) tag, laidOut);
    }

    /**
     * The hero of a phone or a handheld is about this game now: its carousel came to rest on it, or a line of its panel was
     * pressed. Neither lays the list out, so the hero says so itself.
     */
    void hero(HomeModel.GameCard card) {
        if (form != GlassForm.TELEVISION && card != null) want(card, true);
    }

    private void want(HomeModel.GameCard card, boolean lookAgain) {
        if (wanted != null && wanted.key.equals(card.key)) {
            wanted = card;
            // A cover that was not there may have come to the cache since, with no one to say so: a lookup finds out.
            if (lookAgain && !settling && !card.key.equals(shownKey)) applyWanted();
            return;
        }
        boolean first = wanted == null;
        wanted = card;
        handler.removeCallbacks(apply);
        // The art waits for the carousel as the backdrop does; the first game is simply there.
        if (first) {
            applyWanted();
        } else {
            settling = true;
            handler.postDelayed(apply, BackdropView.SETTLE_MS);
        }
    }

    /** A cover that was still loading has arrived: if it is the hero's, the art can have it. */
    void onArt(NvApp app) {
        HomeModel.GameCard card = wanted;
        if (card != null && card.appId == app.getAppId()) applyWanted();
    }

    private void applyWanted() {
        settling = false;
        handler.removeCallbacks(apply);
        HomeModel.GameCard card = wanted;
        if (card == null) return;
        CoverLoaders loaders = covers.get();
        ComputerDetails host = CoverLoaders.coverHost(hosts.apply(card.hostUuid), card.source);
        NvApp app = CoverLoaders.appOf(card.title, card.appId, card.source);
        Bitmap cover = loaders == null ? null : loaders.peek(host, app);
        boolean animate = stage.isShown() && !Motion.reduced(activity);
        if (cover == null) {
            if (loaders == null || !loaders.lacksArt(host, app)) {
                // Still loading: another game's picture does not stay behind this one's name.
                if (!card.key.equals(shownKey)) {
                    shownKey = null;
                    stage.show(null, animate);
                }
                return;
            }
            cover = NoArtTile.ambientSource(activity, card.title);
        }
        shownKey = card.key;
        stage.show(cover, animate);
    }

    /** The screen is going away. */
    void destroy() {
        handler.removeCallbacks(apply);
        if (form != GlassForm.TELEVISION) list.getViewTreeObserver().removeOnGlobalLayoutListener(onLayout);
        backdrop.setCoverListener(null);
    }
}
