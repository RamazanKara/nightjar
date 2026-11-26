package com.limelight.console;

import static com.limelight.nativeapi.NativeApiFixtures.game;
import static com.limelight.nativeapi.NativeApiFixtures.library;
import static com.limelight.nativeapi.NativeApiFixtures.nativeHost;
import static com.limelight.nativeapi.NativeApiFixtures.ok;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;
import static org.robolectric.Shadows.shadowOf;

import android.app.Application;
import android.content.Intent;
import android.content.ServiceConnection;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.LinearGradient;
import android.graphics.Paint;
import android.graphics.Shader;
import android.os.Looper;
import android.preference.PreferenceManager;
import android.view.View;
import android.view.ViewGroup;
import android.widget.ImageView;
import android.widget.TextView;

import androidx.recyclerview.widget.RecyclerView;

import com.limelight.PcViewLifecycleTest;
import com.limelight.R;
import com.limelight.computers.ComputerManagerListener;
import com.limelight.computers.ComputerManagerService;
import com.limelight.grid.assets.CachedAppAssetLoader;
import com.limelight.grid.assets.MemoryAssetLoader;
import com.limelight.grid.assets.ScaledBitmap;
import com.limelight.nativeapi.ConsoleSnapshot;
import com.limelight.nativeapi.NativeApiFixtures;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.nvstream.http.NvApp;
import com.limelight.ui.BackdropView;
import com.limelight.ui.HeroStageView;
import com.limelight.ui.ScreenShots;

import org.junit.After;
import org.junit.Before;
import org.junit.Rule;
import org.junit.Test;
import org.junit.rules.TemporaryFolder;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;
import org.robolectric.util.ReflectionHelpers;

import java.io.File;
import java.time.Duration;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.concurrent.Executor;

/**
 * Home's hero art on each form: on a phone the hero's words stand under the art at the foot of the window's top third, on a
 * handheld the art lies behind the whole hero, on a TV it is the focused card's; the art moves with the list and goes with
 * the hero, and is in the stage's light on all three. With {@code NIGHTJAR_PREVIEW_OUTPUT} set, each form is also drawn to
 * a file.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni", qualifiers = "w412dp-h915dp-port-night-xhdpi")
public class HomeHeroStageTest {
    private static final String[] TITLES = {"Hollow Knight", "Outer Wilds", "Celeste", "Hades", "Disco Elysium", "Tunic",
            "Return of the Obra Dinn", "Stardew Valley", "Slay the Spire", "Dead Cells", "Inside", "Journey"};
    /** A host's cover as the loaders keep it in memory. */
    private static final int COVER_W = 300;
    private static final int COVER_H = 450;

    /** Home without the computer manager service. */
    public static class Home extends ConsoleHomeActivity {
        /** The real check would ask GitHub. */
        @Override protected void checkForUpdates() { }

        @Override protected void play(NvApp app, ComputerDetails computer, String source) { }

        @Override public boolean bindService(Intent intent, ServiceConnection connection, int flags) {
            return false;
        }
    }

    @Rule public TemporaryFolder temporary = new TemporaryFolder();

    private ConsoleRig rig;
    private CoverLoaders covers;

    @Before
    public void setUp() throws Exception {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit().clear().commit();
        rig = new ConsoleRig(temporary.newFolder());
    }

    @After
    public void tearDown() {
        if (covers != null) covers.release();
        // The covers in memory are every screen's: the next test starts without this one's.
        new MemoryAssetLoader().clearCache();
        rig.close();
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit().clear().commit();
    }

    private static void layOut(Home home) {
        View root = home.getWindow().getDecorView();
        int w = home.getResources().getDisplayMetrics().widthPixels;
        int h = home.getResources().getDisplayMetrics().heightPixels;
        root.measure(View.MeasureSpec.makeMeasureSpec(w, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(h, View.MeasureSpec.EXACTLY));
        root.layout(0, 0, w, h);
        root.getViewTreeObserver().dispatchOnGlobalLayout();
    }

    /** Lays the screen out until what one pass asks of the next has been done. */
    private static void settle(Home home) {
        for (int i = 0; i < 3; i++) {
            layOut(home);
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofMillis(300));
        }
        layOut(home);
    }

    /** One PC with these games in its library, nothing running. */
    private RecyclerView show(Home home) throws Exception {
        // The art is baked in the test's thread, so what a step asks for is there when the step is over.
        stage(home).setWorker(Runnable::run);
        ComputerDetails tower = nativeHost("tower", "Tower", true);
        tower.caps = HostCapabilities.UNKNOWN;
        HostFeed feed = ReflectionHelpers.getField(home, "feed");
        ReflectionHelpers.setField(feed, "binder", new ComputerManagerService().new ComputerManagerBinder() {
            @Override public boolean isPersistent() { return false; }
            @Override public void startPolling(ComputerManagerListener listener) { }
            @Override public void stopPolling() { }
            @Override public void invalidateStateForComputer(String uuid) { }
            @Override public String getUniqueId() { return null; }
            @Override public void waitForPollingStopped() { }
        });
        feed.update(tower);
        String[] listed = new String[TITLES.length];
        for (int i = 0; i < listed.length; i++) listed[i] = game("tower-g" + i, i + 1, TITLES[i], null, false);
        rig.transport.reply("tower", "library", ok(library("r", listed)));
        rig.transport.reply("tower", "session", ok(NativeApiFixtures.IDLE));
        ConsoleSnapshot snapshot = rig.repository.refresh(Collections.singletonList(nativeHost("tower", "Tower", true)));
        ReflectionHelpers.setField(home, "shownSnapshot", snapshot);
        HomeModel model = HomeModel.of(snapshot, new HashSet<>(Collections.singletonList("tower")));
        // Every game's cover is in memory, as after a load: the hero's cover, the shelves' covers, the backdrop and the art on
        // the stage are then what the screen itself makes of them, and one game's are one picture.
        MemoryAssetLoader memory = new MemoryAssetLoader();
        for (java.util.List<HomeModel.GameCard> cards : Arrays.asList(model.allGames, model.library, model.continuePlaying, model.favourites)) {
            for (HomeModel.GameCard card : cards) {
                memory.populateCache(new CachedAppAssetLoader.LoaderTuple(tower, CoverLoaders.appOf(card.title, card.appId, card.source)),
                        new ScaledBitmap(COVER_W, COVER_H, cover(card.title)));
            }
        }
        covers = new CoverLoaders(home, "unique", 120);
        ReflectionHelpers.setField(home, "covers", covers);
        // The backdrop's blur runs in the test's thread too.
        View backdrop = home.findViewById(R.id.home_backdrop);
        ReflectionHelpers.setField(backdrop, "executor", (Executor) Runnable::run);
        ConsoleHomeAdapter shelves = ReflectionHelpers.getField(home, "shelves");
        shelves.setCovers(covers);
        shelves.submit(model);
        home.findViewById(R.id.console_loading).setVisibility(View.GONE);
        settle(home);
        return home.findViewById(R.id.console_list);
    }

    private static HeroStageView stage(Home home) {
        BackdropView backdrop = home.findViewById(R.id.home_backdrop);
        View last = backdrop.getChildAt(backdrop.getChildCount() - 1);
        assertTrue("the art lies over the backdrop's own layers", last instanceof HeroStageView);
        return (HeroStageView) last;
    }

    /** Where the tab bar starts in the window: nothing of the page may be needed under it. */
    private static int dockTop(Home home, RecyclerView list) {
        View bar = home.findViewById(R.id.shell_bar);
        if (bar != null && bar.isShown() && bar.getHeight() > 0) return topOf(bar);
        return topOf(list) + list.getHeight() - list.getPaddingBottom();
    }

    /** The hero, and under it the first shelf's heading and the upper half of its covers, all above the tab bar. */
    private static void assertTheHeroAndHalfAShelfFit(Home home, RecyclerView list, boolean halfACover) {
        View item = list.getLayoutManager().findViewByPosition(0);
        View cards = item.findViewById(R.id.shelf_cards);
        View play = item.findViewById(R.id.hero_play);
        int dock = dockTop(home, list);
        assertTrue("the name starts under the top row", topOf(item.findViewById(R.id.hero_title)) >= topOf(list) + list.getPaddingTop());
        assertEquals("Play stands under the cover with the standard gap, not on it",
                home.getResources().getDimensionPixelSize(R.dimen.space_l), topOf(play) - (topOf(cards) + cards.getHeight()));
        assertTrue("Play is above the tab bar: " + (topOf(play) + play.getHeight()) + " of " + dock, topOf(play) + play.getHeight() <= dock);
        View shelf = list.getLayoutManager().findViewByPosition(1);
        assertNotNull("the first shelf is on screen", shelf);
        View heading = shelf.findViewById(R.id.shelf_title);
        View covers = shelf.findViewById(R.id.shelf_cards);
        assertNotNull(heading);
        assertNotNull(covers);
        assertTrue("its heading is above the tab bar: " + (topOf(heading) + heading.getHeight()) + " of " + dock,
                topOf(heading) + heading.getHeight() <= dock);
        if (!halfACover) return;
        assertTrue("and so is the upper half of its covers: " + (topOf(covers) + covers.getHeight() / 2) + " of " + dock,
                topOf(covers) + covers.getHeight() / 2 <= dock);
    }

    private static int topOf(View view) {
        int[] at = new int[2];
        view.getLocationInWindow(at);
        return at[1];
    }

    /**
     * A game's cover, the size a host's cover is kept in memory: a picture in its upper part, so a preview shows what the
     * crop and the mask keep of it, in hues of the game's own.
     */
    private static Bitmap cover(String title) {
        // Celeste, which the previews' hero is about, keeps the hues the picture was drawn in.
        int game = Math.max(0, Arrays.asList(TITLES).indexOf(title));
        float turn = (game + TITLES.length - 2) % TITLES.length * (360f / TITLES.length);
        Bitmap cover = Bitmap.createBitmap(COVER_W, COVER_H, Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(cover);
        canvas.scale(COVER_W / 400f, COVER_H / 600f);
        Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        paint.setShader(new LinearGradient(0, 0, 400, 600,
                new int[] {turned(0xFFF2A65A, turn), turned(0xFFB5446E, turn), turned(0xFF2B2D6E, turn)}, null, Shader.TileMode.CLAMP));
        canvas.drawRect(0, 0, 400, 600, paint);
        paint.setShader(null);
        paint.setColor(turned(0xFFFFF3D6, turn));
        canvas.drawCircle(250 - game % 3 * 50, 190, 70, paint);
        paint.setColor(turned(0xFF1B1B3A, turn));
        for (int i = 0; i < 6; i++) canvas.drawRect(i * 80 - 20, 330 - i * 14, i * 80 + 40, 600, paint);
        return cover;
    }

    private static int turned(int color, float degrees) {
        float[] hsv = new float[3];
        Color.colorToHSV(color, hsv);
        hsv[0] = (hsv[0] + degrees) % 360f;
        return Color.HSVToColor(hsv);
    }

    /** The game the hero of a phone or a handheld is about. */
    private static HomeModel.GameCard heroGame(RecyclerView list) {
        return (HomeModel.GameCard) list.getLayoutManager().findViewByPosition(0).getTag(HomeHeroStage.GAME);
    }

    /**
     * The stage has the hero's art by the screen's own path; for the preview it is shown once more without the cross-fade,
     * so the picture does not depend on how far a fade has come.
     */
    private static void assertTheStageHasTheArtOf(Home home, String title) {
        HeroStageView stage = stage(home);
        assertTrue("the stage was given the art of " + title, stage.hasArt());
        stage.show(cover(title), false);
    }

    private static void capture(Home home, String name) throws Exception {
        File directory = ScreenShots.directory("hero");
        if (directory == null) return;
        ScreenShots.capture(home, home.getResources().getDisplayMetrics().widthPixels,
                home.getResources().getDisplayMetrics().heightPixels, new File(directory, name + ".png"));
    }

    @Test
    public void thePhonesRoomEndsTheWordsAtTheWindowsThirdWhileTheHeroStillFits() {
        assertEquals("a third of 900 is 300: from 100, over words of 80", 120, HomeHeroStage.room(900, 100, 80, 470, 760));
        assertEquals("no more than leaves the carousel and Play in the list", 30, HomeHeroStage.room(640, 100, 80, 340, 450));
        assertEquals("none in a window that is already full", 0, HomeHeroStage.room(640, 100, 80, 400, 450));
        assertEquals("none where the words already reach the third", 0, HomeHeroStage.room(600, 150, 80, 100, 500));
    }

    @Test
    public void onAPhoneTheHerosNameStandsUnderItsArtAndAboveTheCovers() throws Exception {
        ActivityController<Home> controller = Robolectric.buildActivity(Home.class).setup().visible();
        try {
            Home home = controller.get();
            RecyclerView list = show(home);
            View item = list.getLayoutManager().findViewByPosition(0);
            assertNotNull("the hero is the list's first item", item);
            Object game = item.getTag(HomeHeroStage.GAME);
            assertTrue("it says whose art the stage shows", game instanceof HomeModel.GameCard);
            TextView title = item.findViewById(R.id.hero_title);
            assertEquals(((HomeModel.GameCard) game).title, title.getText().toString());
            View words = item.findViewById(R.id.hero_words);
            int window = home.getWindow().getDecorView().getHeight();
            int foot = topOf(words) + words.getHeight();
            assertTrue("the words end by the window's third: " + foot + " of " + window, foot <= window / 3 + 2);
            assertTheHeroAndHalfAShelfFit(home, list, true);
            View cards = item.findViewById(R.id.shelf_cards);
            assertTrue("the covers follow the words", topOf(cards) >= foot);
            View play = item.findViewById(R.id.hero_play);
            assertTrue("Play is still above the tab bar's room",
                    topOf(play) + play.getHeight() <= topOf(list) + list.getHeight() - list.getPaddingBottom());

            HeroStageView stage = stage(home);
            assertEquals(View.VISIBLE, stage.getVisibility());
            assertEquals("the backdrop lies in the window's corner", 0, topOf(home.findViewById(R.id.home_backdrop)));
            assertFalse("under the night's light words the art is the night's", stage.isDay());

            // The art is part of the hero: it goes up with it, and is gone when the hero is.
            list.scrollBy(0, 120);
            assertEquals(-120f, stage.getTranslationY(), 0f);
            // Search hides the list: the art goes with it, and is back with it.
            list.setVisibility(View.INVISIBLE);
            settle(home);
            assertEquals(View.INVISIBLE, stage.getVisibility());
            list.setVisibility(View.VISIBLE);
            list.scrollToPosition(0);
            settle(home);
            assertEquals(View.VISIBLE, stage.getVisibility());
            assertEquals(0f, stage.getTranslationY(), 0f);

            assertTheStageHasTheArtOf(home, ((HomeModel.GameCard) game).title);
            capture(home, "home-phone-night");
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = "w412dp-h915dp-port-xhdpi")
    public void aPhoneByDay() throws Exception {
        ActivityController<Home> controller = Robolectric.buildActivity(Home.class).setup().visible();
        try {
            Home home = controller.get();
            RecyclerView list = show(home);
            HeroStageView stage = stage(home);
            assertEquals(View.VISIBLE, stage.getVisibility());
            assertTrue("under the day's dark words the art is lifted, not held down", stage.isDay());
            assertTheStageHasTheArtOf(home, heroGame(list).title);
            capture(home, "home-phone-day");
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-port-night-xhdpi")
    public void onAUsualPhoneTheHeroAndTheFirstShelfsUpperHalfStandAboveTheTabBar() throws Exception {
        ActivityController<Home> controller = Robolectric.buildActivity(Home.class).setup().visible();
        try {
            Home home = controller.get();
            RecyclerView list = show(home);
            assertTheHeroAndHalfAShelfFit(home, list, true);
            View item = list.getLayoutManager().findViewByPosition(0);
            ViewGroup cards = item.findViewById(R.id.shelf_cards);
            ViewGroup shelf = list.getLayoutManager().findViewByPosition(1).findViewById(R.id.shelf_cards);
            int heroCover = cards.getChildAt(0).getWidth();
            int shelfCover = shelf.getChildAt(0).getWidth();
            assertTrue("the hero's cover is the larger one: " + heroCover + " over " + shelfCover, heroCover > shelfCover);
            assertTheStageHasTheArtOf(home, heroGame(list).title);
            capture(home, "home-phone-360x800-night");
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = "w360dp-h640dp-port-night-xhdpi")
    public void aSmallPhoneKeepsItsHeroInTheList() throws Exception {
        ActivityController<Home> controller = Robolectric.buildActivity(Home.class).setup().visible();
        try {
            Home home = controller.get();
            RecyclerView list = show(home);
            View item = list.getLayoutManager().findViewByPosition(0);
            View words = item.findViewById(R.id.hero_words);
            View play = item.findViewById(R.id.hero_play);
            int room = list.getHeight() - list.getPaddingTop() - list.getPaddingBottom();
            assertTrue("the room over the words never pushes Play out: " + words.getPaddingTop(),
                    words.getPaddingTop() == 0 || item.getHeight() <= room);
            assertTrue(play.getHeight() > 0);
            // Too low for the shelf's covers: the hero's cover stays as high as theirs, and the heading still shows.
            assertTheHeroAndHalfAShelfFit(home, list, false);
            assertTheStageHasTheArtOf(home, heroGame(list).title);
            capture(home, "home-small-phone-night");
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = "w891dp-h411dp-land-night-xxhdpi")
    public void onAHandheldTheArtLiesBehindTheWholeHero() throws Exception {
        ActivityController<Home> controller = Robolectric.buildActivity(Home.class).setup().visible();
        try {
            Home home = controller.get();
            RecyclerView list = show(home);
            View item = list.getLayoutManager().findViewByPosition(0);
            assertNotNull(item);
            assertTrue(item.getTag(HomeHeroStage.GAME) instanceof HomeModel.GameCard);
            assertNotNull("the handheld's hero has the line too", item.findViewById(R.id.hero_line));
            HeroStageView stage = stage(home);
            assertEquals(View.VISIBLE, stage.getVisibility());
            View play = item.findViewById(R.id.hero_play);
            assertTrue("the art's view lies behind the whole hero, down to its buttons", stage.getHeight() >= topOf(play) + play.getHeight());
            assertFalse(stage.isDay());
            // The hero's own cover is the game's art as the shelves' covers are theirs, not an empty frame beside them.
            ImageView art = item.findViewById(R.id.hero_cover_image);
            assertEquals(View.VISIBLE, art.getVisibility());
            assertNotNull("the hero's cover shows the game's art", art.getDrawable());
            assertEquals("and no tile of a game without art", View.GONE, item.findViewById(R.id.hero_cover_text).getVisibility());
            ViewGroup shelf = list.getLayoutManager().findViewByPosition(1).findViewById(R.id.shelf_cards);
            ImageView shelfArt = shelf.getChildAt(0).findViewById(R.id.console_cover);
            assertNotNull("a shelf's cover shows its game's art too", shelfArt.getDrawable());
            assertTheStageHasTheArtOf(home, heroGame(list).title);
            capture(home, "home-handheld-night");
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = "w960dp-h540dp-land-television-night-xhdpi")
    public void onATvTheArtIsTheFocusedCardsAndTheNightsWhateverTheTheme() throws Exception {
        ActivityController<Home> controller = Robolectric.buildActivity(Home.class).setup().visible();
        try {
            Home home = controller.get();
            RecyclerView list = show(home);
            HeroStageView stage = stage(home);
            assertEquals("a TV's art is the focused card's, wherever the list is", View.VISIBLE, stage.getVisibility());

            // A key was used: the first card takes focus, and the hero says its name.
            Object root = ReflectionHelpers.callInstanceMethod(home.getWindow().getDecorView(), "getViewRootImpl");
            ReflectionHelpers.callInstanceMethod(root, "ensureTouchMode", ReflectionHelpers.ClassParameter.from(boolean.class, false));
            ViewGroup row = (ViewGroup) list.getChildAt(0);
            RecyclerView cards = row.findViewById(R.id.shelf_cards);
            assertNotNull("a shelf of cards", cards);
            assertTrue(cards.getChildAt(0).requestFocus());
            settle(home);
            TextView title = home.findViewById(R.id.home_hero_title);
            assertTrue("the hero names the card", title.getText().length() > 0);
            assertFalse("the TV's hero is written in the light label colours: its art is the night's", stage.isDay());

            // The row's first cover rests on the page margin: its lift grows away from it, so the focus ring stays on the
            // margin instead of standing closer to the screen's edge than the page's own content.
            View first = cards.getChildAt(0);
            int[] lifted = new int[2];
            first.getLocationInWindow(lifted);
            int[] row = new int[2];
            cards.getLocationInWindow(row);
            int rest = row[0] + first.getLeft();
            assertTrue("the lifted cover has not moved toward the screen's edge: " + lifted[0] + " from " + rest, lifted[0] >= rest);
            if (first.getScaleX() > 1f) {
                assertTrue("it grows from beyond its cover's start edge: " + first.getPivotX(),
                        first.getPivotX() < first.findViewById(R.id.console_cover_frame).getLeft());
            }

            assertTheStageHasTheArtOf(home, title.getText().toString());
            capture(home, "home-tv-night");
        } finally {
            controller.pause().stop().destroy();
        }
    }
}
