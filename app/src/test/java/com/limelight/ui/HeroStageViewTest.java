package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;
import static org.robolectric.Shadows.shadowOf;

import android.app.Activity;
import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.Rect;
import android.os.Looper;
import android.preference.PreferenceManager;
import android.view.View;

import org.junit.After;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

import java.time.Duration;
import java.util.ArrayList;
import java.util.List;

/**
 * The hero's art and its scrim: where the art goes on each form, how a cover fills that place, and that the words keep WCAG's
 * 4.5:1 over the worst art there could be, measured on the pixels that are drawn.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "w960dp-h540dp-land-night-mdpi")
public class HeroStageViewTest {
    private static final int NIGHT_CANVAS = 0xFF050608;
    private static final int NIGHT_INK = 0xFFF5F7FA;
    private static final int NIGHT_FAINT = 0xFFC7CDD9;
    private static final int DAY_CANVAS = 0xFFF2F4F8;
    private static final int DAY_INK = 0xFF10131A;
    private static final int DAY_FAINT = 0xFF4A5261;

    @After
    public void tearDown() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit().clear().commit();
    }

    private static Bitmap plain(int color) {
        Bitmap cover = Bitmap.createBitmap(200, 300, Bitmap.Config.ARGB_8888);
        cover.eraseColor(color);
        return cover;
    }

    /** A stage of this size in a window, with nothing shown yet. */
    private static HeroStageView stage(Activity activity, int w, int h, float heroShare) {
        HeroStageView stage = new HeroStageView(activity);
        activity.setContentView(stage);
        stage.setHeroShare(heroShare);
        stage.measure(View.MeasureSpec.makeMeasureSpec(w, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(h, View.MeasureSpec.EXACTLY));
        stage.layout(0, 0, w, h);
        return stage;
    }

    /** The stage as it is drawn over a page of {@code under}. */
    private static Bitmap drawn(HeroStageView stage, int under) {
        Bitmap out = Bitmap.createBitmap(stage.getWidth(), stage.getHeight(), Bitmap.Config.ARGB_8888);
        out.eraseColor(under);
        stage.draw(new Canvas(out));
        return out;
    }

    private static double least(Bitmap picture, Rect in, int ink) {
        double least = 21;
        for (int y = in.top; y < in.bottom; y += 3) {
            for (int x = in.left; x < in.right; x += 3) {
                least = Math.min(least, HeroStageView.contrast(ink, picture.getPixel(x, y) | 0xFF000000));
            }
        }
        return least;
    }

    @Test
    public void contrastIsWcags() {
        assertEquals(21.0, HeroStageView.contrast(0xFFFFFFFF, 0xFF000000), .001);
        assertEquals(1.0, HeroStageView.contrast(0xFF336699, 0xFF336699), .001);
        // The grey that just misses 4.5:1 on white, and the one that just makes it.
        assertEquals(4.48, HeroStageView.contrast(0xFF777777, 0xFFFFFFFF), .01);
        assertEquals(4.54, HeroStageView.contrast(0xFF767676, 0xFFFFFFFF), .01);
        assertEquals("either way round", HeroStageView.contrast(0xFF767676, 0xFFFFFFFF), HeroStageView.contrast(0xFFFFFFFF, 0xFF767676), 0);
    }

    @Test
    public void theScrimIsTheLeastThatKeepsTheWordsReadableOverTheWorstArt() {
        int[][] looks = {{NIGHT_INK, NIGHT_CANVAS}, {NIGHT_FAINT, NIGHT_CANVAS}, {DAY_INK, DAY_CANVAS}, {DAY_FAINT, DAY_CANVAS}};
        for (int[] look : looks) {
            int worst = HeroStageView.worstArt(look[0], look[1]);
            float share = HeroStageView.scrimShare(look[0], look[1], HeroStageView.TEXT_CONTRAST);
            String name = Integer.toHexString(look[0]) + " on " + Integer.toHexString(look[1]) + " at " + share;
            assertTrue(name, share > .2f && share < .95f);
            assertTrue(name, HeroStageView.contrast(look[0], HeroStageView.mix(worst, look[1], share)) >= 4.5);
            assertTrue("no more than it takes: " + name, HeroStageView.contrast(look[0], HeroStageView.mix(worst, look[1], share - .03f)) < 4.5);
        }
        assertEquals("white is the worst under light words", 0xFFFFFFFF, HeroStageView.worstArt(NIGHT_INK, NIGHT_CANVAS));
        assertEquals("black under dark ones", 0xFF000000, HeroStageView.worstArt(DAY_INK, DAY_CANVAS));
        assertTrue("fainter words need more", HeroStageView.scrimShare(NIGHT_FAINT, NIGHT_CANVAS, 4.5) > HeroStageView.scrimShare(NIGHT_INK, NIGHT_CANVAS, 4.5));
        assertEquals("words the bare page cannot carry get all of it", 1f, HeroStageView.scrimShare(0xFF404040, NIGHT_CANVAS, 4.5), 0f);
    }

    @Test
    public void theArtTakesTheEndSideOfAWideWindowAndTheTopOfATallOne() {
        Rect rect = new Rect();
        HeroStageView.artRect(1920, 1080, .56f, false, rect);
        assertEquals(new Rect(730, 0, 1920, 605), rect);
        HeroStageView.artRect(1920, 1080, .56f, true, rect);
        assertEquals("mirrored", new Rect(0, 0, 1190, 605), rect);
        HeroStageView.artRect(1080, 2400, .46f, false, rect);
        assertEquals(new Rect(0, 0, 1080, 1104), rect);
        HeroStageView.artRect(1080, 2400, .46f, true, rect);
        assertEquals("a tall window has no sides to swap", new Rect(0, 0, 1080, 1104), rect);
    }

    @Test
    public void aCoverFillsItsPlaceAndKeepsItsUpperPart() {
        Rect rect = new Rect(730, 0, 1920, 605);
        float[] at = HeroStageView.place(600, 900, rect);
        assertEquals("as wide as its place", 1190f, 600 * at[0], .5f);
        assertEquals(730f, at[1], .5f);
        float height = 900 * at[0];
        assertTrue("it starts above its place", at[2] < 0f);
        assertTrue("and ends below it", at[2] + height > 605f);
        assertTrue("more is cut off below than above", -at[2] < at[2] + height - 605f);
        // A wide picture in a tall place is centred across.
        float[] wide = HeroStageView.place(900, 300, new Rect(0, 0, 300, 300));
        assertEquals(1f, wide[0], .001f);
        assertEquals(-300f, wide[1], .5f);
        assertEquals(0f, wide[2], .5f);
    }

    @Test
    public void theMaskLightsAWideWindowFromItsEndEdgeAndATallOneFromTheMiddle() {
        Rect rect = new Rect(730, 0, 1920, 605);
        float[] ltr = HeroStageView.mask(rect, 1920, true, false);
        assertEquals(1920f, ltr[0], 0f);
        assertEquals("it ends where the art does", 605f, ltr[1] + ltr[3], .5f);
        assertEquals("and at the art's inner edge", 730f, ltr[0] - ltr[2], .5f);
        float[] rtl = HeroStageView.mask(new Rect(0, 0, 1190, 605), 1920, true, true);
        assertEquals(0f, rtl[0], 0f);
        float[] tall = HeroStageView.mask(new Rect(0, 0, 1080, 1104), 1080, false, false);
        assertEquals(540f, tall[0], 0f);
        assertEquals(1104f, tall[1] + tall[3], .5f);
        assertTrue("the sides stay whole", 540f / tall[2] < HeroStageView.HOLD);
    }

    @Test
    public void theScrimIsWholeUnderTheWordsAndRunsOutPastThem() {
        // Words from 100 to 200 down, ending 400 across; a ramp of 200 across and an edge of 50 up and down; no top shade.
        assertEquals(1f, HeroStageView.scrimAt(0, 150, 400, 200, 100, 200, 50, 0), 0f);
        assertEquals(1f, HeroStageView.scrimAt(400, 100, 400, 200, 100, 200, 50, 0), 0f);
        assertEquals(.5f, HeroStageView.scrimAt(500, 150, 400, 200, 100, 200, 50, 0), .001f);
        assertEquals(0f, HeroStageView.scrimAt(600, 150, 400, 200, 100, 200, 50, 0), 0f);
        assertEquals(.5f, HeroStageView.scrimAt(0, 225, 400, 200, 100, 200, 50, 0), .001f);
        assertEquals(.5f, HeroStageView.scrimAt(0, 75, 400, 200, 100, 200, 50, 0), .001f);
        assertEquals(0f, HeroStageView.scrimAt(0, 250, 400, 200, 100, 200, 50, 0), 0f);
        // A tall window has no end: the scrim goes all the way across.
        assertEquals(1f, HeroStageView.scrimAt(5000, 150, 0, 0, 100, 200, 50, 0), 0f);
        // The shade under the top edge, with or without words.
        assertEquals(HeroStageView.TOP_SHADE, HeroStageView.scrimAt(900, 0, 400, 200, 100, 200, 50, 60), 0f);
        assertEquals(0f, HeroStageView.scrimAt(900, 60, 400, 200, 100, 200, 50, 60), 0f);
        assertEquals(HeroStageView.TOP_SHADE, HeroStageView.scrimAt(0, 0, 0, 0, 0, 0, 1, 60), 0f);
        assertEquals("never less than the words need", 1f, HeroStageView.scrimAt(0, 0, 400, 200, 0, 200, 50, 60), 0f);
    }

    @Test
    public void theWordsKeepTheirContrastOverWhiteArtOnAWideWindow() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.setCanvasColor(NIGHT_CANVAS);
            stage.setInks(NIGHT_INK, NIGHT_FAINT);
            stage.show(plain(0xFFFFFFFF), false);
            // A short name at the start, then one that runs across into the art.
            for (Rect words : new Rect[] {new Rect(48, 80, 300, 158), new Rect(48, 80, 912, 158)}) {
                stage.setWords(words.left, words.top, words.right, words.bottom);
                Bitmap picture = drawn(stage, 0xFFFFFFFF);
                assertTrue("the name at " + words + ": " + least(picture, words, NIGHT_INK), least(picture, words, NIGHT_INK) >= 4.5);
                assertTrue("the faint line at " + words + ": " + least(picture, words, NIGHT_FAINT), least(picture, words, NIGHT_FAINT) >= 4.5);
            }
        }
    }

    @Test
    @Config(qualifiers = "w412dp-h915dp-port-night-mdpi")
    public void theWordsKeepTheirContrastOverWhiteArtOnATallWindow() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 412, 915, .46f);
            stage.setCanvasColor(NIGHT_CANVAS);
            stage.setInks(NIGHT_INK, NIGHT_FAINT);
            stage.show(plain(0xFFFFFFFF), false);
            Rect words = new Rect(24, 225, 388, 305);
            stage.setWords(words.left, words.top, words.right, words.bottom);
            Bitmap picture = drawn(stage, 0xFFFFFFFF);
            assertTrue("the name: " + least(picture, words, NIGHT_INK), least(picture, words, NIGHT_INK) >= 4.5);
            assertTrue("the faint line: " + least(picture, words, NIGHT_FAINT), least(picture, words, NIGHT_FAINT) >= 4.5);
        }
    }

    @Test
    @Config(qualifiers = "w960dp-h540dp-land-mdpi")
    public void darkWordsKeepTheirContrastOverBlackArtByDay() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.setCanvasColor(DAY_CANVAS);
            stage.setInks(DAY_INK, DAY_FAINT);
            stage.show(plain(0xFF000000), false);
            Rect words = new Rect(48, 80, 912, 158);
            stage.setWords(words.left, words.top, words.right, words.bottom);
            Bitmap picture = drawn(stage, 0xFF000000);
            assertTrue("the name: " + least(picture, words, DAY_INK), least(picture, words, DAY_INK) >= 4.5);
            assertTrue("the faint line: " + least(picture, words, DAY_FAINT), least(picture, words, DAY_FAINT) >= 4.5);
        }
    }

    @Test
    public void theArtIsWholeAtItsEdgeAndGoneWhereItsPlaceEnds() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.setCanvasColor(NIGHT_CANVAS);
            stage.setWords(48, 80, 300, 158);
            assertFalse(stage.hasArt());
            Bitmap empty = drawn(stage, NIGHT_CANVAS);
            assertEquals("nothing is drawn without art, not even the scrim", NIGHT_CANVAS, empty.getPixel(100, 100));
            stage.show(plain(0xFFFF0000), false);
            assertTrue(stage.hasArt());
            Bitmap picture = drawn(stage, NIGHT_CANVAS);
            int whole = picture.getPixel(950, 150);
            assertTrue("the cover itself by the end edge: " + Integer.toHexString(whole), ((whole >> 16) & 0xFF) > 240 && (whole & 0xFF) < 16);
            assertEquals("gone at the foot of the hero", NIGHT_CANVAS, picture.getPixel(950, 320));
            assertEquals("and before the words' side", NIGHT_CANVAS, picture.getPixel(300, 300));
            int mid = (picture.getPixel(950, 250) >> 16) & 0xFF;
            assertTrue("fading in between: " + mid, mid > 30 && mid < 240);
        }
    }

    @Test
    public void aChangeOfGameCrossFadesAndACutIsThereAtOnce() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.setCanvasColor(NIGHT_CANVAS);
            Bitmap first = plain(0xFFFF0000);
            Bitmap second = plain(0xFF0000FF);
            stage.show(first, false);
            assertFalse("a cut has nothing to wait for", stage.isChanging());
            stage.show(second, true);
            assertTrue(stage.isChanging());
            int start = drawn(stage, NIGHT_CANVAS).getPixel(950, 150);
            assertTrue("the old cover is still whole under the new one: " + Integer.toHexString(start), ((start >> 16) & 0xFF) > 240);
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1));
            assertFalse(stage.isChanging());
            int end = drawn(stage, NIGHT_CANVAS).getPixel(950, 150);
            assertTrue("then the new one alone: " + Integer.toHexString(end), (end & 0xFF) > 240 && ((end >> 16) & 0xFF) < 16);
            stage.show(second, true);
            assertFalse("the same cover again changes nothing", stage.isChanging());
            stage.show(null, true);
            assertTrue("the art goes as softly as it came", stage.isChanging());
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1));
            assertFalse(stage.hasArt());
            assertEquals(NIGHT_CANVAS, drawn(stage, NIGHT_CANVAS).getPixel(950, 150));
        }
    }

    @Test
    public void highContrastShowsNeitherArtNorScrim() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putBoolean(AppTheme.HIGH_CONTRAST, true).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.setWords(48, 80, 300, 158);
            stage.show(plain(0xFFFF0000), true);
            assertFalse(stage.isChanging());
            Bitmap picture = drawn(stage, 0xFF000000);
            assertEquals(0xFF000000, picture.getPixel(950, 150));
            assertEquals(0xFF000000, picture.getPixel(100, 100));
        }
    }

    @Test
    public void theBackdropTellsEachCoverAsItsAmbientStarts() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            BackdropView backdrop = new BackdropView(controller.get());
            controller.get().setContentView(backdrop);
            backdrop.setWorker(Runnable::run);
            List<String> keys = new ArrayList<>();
            List<Bitmap> covers = new ArrayList<>();
            backdrop.setCoverListener((key, cover, animate) -> {
                keys.add(key);
                covers.add(cover);
            });
            Bitmap cover = plain(0xFF8040C0);
            backdrop.focus("game:a", cover, false);
            assertTrue("nothing before the selection has settled", keys.isEmpty());
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1));
            assertEquals(List.of("game:a"), keys);
            assertSame(cover, covers.get(0));
            // A PC has no art: the plain canvas, and no cover.
            backdrop.focus("host:tower", null, true, false);
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1));
            assertEquals(List.of("game:a", "host:tower"), keys);
            assertNull(covers.get(1));
            // A cover that is still loading leaves the ambient, but says that no picture is this game's yet.
            backdrop.focus("game:b", null, false, false);
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1));
            assertEquals(List.of("game:a", "host:tower", "game:b"), keys);
            assertNull(covers.get(2));
            backdrop.setCoverListener(null);
            backdrop.focus("game:a", cover, false);
            shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1));
            assertEquals("no one is told any more", 3, keys.size());
        }
    }
}
