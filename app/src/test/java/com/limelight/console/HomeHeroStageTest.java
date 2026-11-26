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
import android.graphics.LinearGradient;
import android.graphics.Paint;
import android.graphics.Rect;
import android.graphics.Shader;
import android.os.Looper;
import android.preference.PreferenceManager;
import android.view.View;
import android.view.ViewGroup;
import android.widget.TextView;

import androidx.recyclerview.widget.RecyclerView;

import com.limelight.PcViewLifecycleTest;
import com.limelight.R;
import com.limelight.computers.ComputerManagerListener;
import com.limelight.computers.ComputerManagerService;
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
import java.util.Collections;
import java.util.HashSet;

/**
 * Home's hero art on each form: on a phone the hero's words stand under the art at the foot of the window's top third, on a
 * handheld the scrim lies under the whole hero, on a TV under the words of the card that has focus; the art moves with the
 * list and goes with the hero. With {@code NIGHTJAR_PREVIEW_OUTPUT} set, each form is also drawn to a file.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni", qualifiers = "w412dp-h915dp-port-night-xhdpi")
public class HomeHeroStageTest {
    private static final String[] TITLES = {"Hollow Knight", "Outer Wilds", "Celeste", "Hades", "Disco Elysium", "Tunic",
            "Return of the Obra Dinn", "Stardew Valley", "Slay the Spire", "Dead Cells", "Inside", "Journey"};

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

    @Before
    public void setUp() throws Exception {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit().clear().commit();
        rig = new ConsoleRig(temporary.newFolder());
    }

    @After
    public void tearDown() {
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
        ConsoleHomeAdapter shelves = ReflectionHelpers.getField(home, "shelves");
        shelves.submit(HomeModel.of(snapshot, new HashSet<>(Collections.singletonList("tower"))));
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

    private static int topOf(View view) {
        int[] at = new int[2];
        view.getLocationInWindow(at);
        return at[1];
    }

    private static int leftOf(View view) {
        int[] at = new int[2];
        view.getLocationInWindow(at);
        return at[0];
    }

    /** A cover with a picture in its upper part, so a preview shows what the crop and the mask keep of it. */
    private static Bitmap cover() {
        Bitmap cover = Bitmap.createBitmap(400, 600, Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(cover);
        Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        paint.setShader(new LinearGradient(0, 0, 400, 600, new int[] {0xFFF2A65A, 0xFFB5446E, 0xFF2B2D6E}, null, Shader.TileMode.CLAMP));
        canvas.drawRect(0, 0, 400, 600, paint);
        paint.setShader(null);
        paint.setColor(0xFFFFF3D6);
        canvas.drawCircle(250, 190, 70, paint);
        paint.setColor(0xFF1B1B3A);
        for (int i = 0; i < 6; i++) canvas.drawRect(i * 80 - 20, 330 - i * 14, i * 80 + 40, 600, paint);
        return cover;
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
            assertTrue("the art has room over the words: " + words.getPaddingTop(), words.getPaddingTop() > 0);
            assertTrue("the words end by the window's third: " + foot + " of " + window, foot <= window / 3 + 2);
            View cards = item.findViewById(R.id.shelf_cards);
            assertTrue("the covers follow the words", topOf(cards) >= foot);
            View play = item.findViewById(R.id.hero_play);
            assertTrue("Play is still above the tab bar's room",
                    topOf(play) + play.getHeight() <= topOf(list) + list.getHeight() - list.getPaddingBottom());

            HeroStageView stage = stage(home);
            assertEquals(View.VISIBLE, stage.getVisibility());
            assertEquals("the backdrop lies in the window's corner", 0, topOf(home.findViewById(R.id.home_backdrop)));
            Rect scrim = new Rect();
            stage.getWords(scrim);
            assertEquals("the scrim starts at the name", topOf(title), scrim.top);
            assertEquals("and ends with the words", foot, scrim.bottom);

            // The art is part of the hero: it goes up with it, and is gone when the hero is.
            list.scrollBy(0, 120);
            assertEquals(-120f, stage.getTranslationY(), 0f);
            stage.getWords(scrim);
            assertEquals("the words have not moved in the art", foot, scrim.bottom);
            // Search hides the list: the art goes with it, and is back with it.
            list.setVisibility(View.INVISIBLE);
            settle(home);
            assertEquals(View.INVISIBLE, stage.getVisibility());
            list.setVisibility(View.VISIBLE);
            list.scrollToPosition(0);
            settle(home);
            assertEquals(View.VISIBLE, stage.getVisibility());
            assertEquals(0f, stage.getTranslationY(), 0f);

            stage.show(cover(), false);
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
            show(home);
            HeroStageView stage = stage(home);
            assertEquals(View.VISIBLE, stage.getVisibility());
            stage.show(cover(), false);
            capture(home, "home-phone-day");
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
            stage(home).show(cover(), false);
            capture(home, "home-small-phone-night");
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = "w891dp-h411dp-land-night-xxhdpi")
    public void onAHandheldTheScrimLiesUnderTheWholeHero() throws Exception {
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
            Rect scrim = new Rect();
            stage.getWords(scrim);
            assertEquals(topOf(item), scrim.top);
            assertEquals(leftOf(item) + item.getPaddingLeft(), scrim.left);
            assertEquals(leftOf(item) + item.getWidth() - item.getPaddingRight(), scrim.right);
            assertTrue("down to the buttons", scrim.bottom >= topOf(item.findViewById(R.id.hero_play)) + item.findViewById(R.id.hero_play).getHeight());
            stage.show(cover(), false);
            capture(home, "home-handheld-night");
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = "w960dp-h540dp-land-television-night-xhdpi")
    public void onATvTheScrimIsAsWideAsTheFocusedCardsWords() throws Exception {
        ActivityController<Home> controller = Robolectric.buildActivity(Home.class).setup().visible();
        try {
            Home home = controller.get();
            RecyclerView list = show(home);
            HeroStageView stage = stage(home);
            assertEquals("a TV's art is the focused card's, wherever the list is", View.VISIBLE, stage.getVisibility());
            assertFalse(stage.hasArt());

            // A key was used: the first card takes focus, and the hero says its name.
            Object root = ReflectionHelpers.callInstanceMethod(home.getWindow().getDecorView(), "getViewRootImpl");
            ReflectionHelpers.callInstanceMethod(root, "ensureTouchMode", ReflectionHelpers.ClassParameter.from(boolean.class, false));
            ViewGroup row = (ViewGroup) list.getChildAt(0);
            RecyclerView cards = row.findViewById(R.id.shelf_cards);
            assertNotNull("a shelf of cards", cards);
            assertTrue(cards.getChildAt(0).requestFocus());
            settle(home);
            ViewGroup hero = home.findViewById(R.id.home_hero);
            TextView title = home.findViewById(R.id.home_hero_title);
            assertTrue("the hero names the card", title.getText().length() > 0);
            Rect scrim = new Rect();
            stage.getWords(scrim);
            assertFalse("the scrim has the words' place", scrim.isEmpty());
            assertEquals(leftOf(hero) - Math.round(hero.getTranslationX()) + hero.getPaddingLeft(), scrim.left);
            assertEquals(topOf(title), scrim.top);
            assertTrue("no wider than the words: " + scrim.width(),
                    scrim.width() <= hero.getWidth() - hero.getPaddingLeft() - hero.getPaddingRight());
            assertTrue("and no narrower than the name", scrim.width() >= (int) title.getPaint().measureText(title.getText(), 0, title.getText().length()));

            stage.show(cover(), false);
            capture(home, "home-tv-night");
        } finally {
            controller.pause().stop().destroy();
        }
    }
}
