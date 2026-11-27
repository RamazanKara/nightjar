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
 * The hero's art: where it goes on each form, how a cover fills that place, and that it is held to the stage's light, so the
 * words keep WCAG's 4.5:1 and a control's glyph 3:1 over the worst art there could be, measured on the pixels that are drawn.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "w960dp-h540dp-land-night-mdpi")
public class HeroStageViewTest {
    private static final int NIGHT_CANVAS = 0xFF050608;
    private static final int NIGHT_INK = 0xFFF5F7FA;
    private static final int NIGHT_FAINT = 0xFFC7CDD9;
    private static final int DAY_CANVAS = 0xFFF2F4F8;
    private static final int DAY_INK = 0xFF111318;
    private static final int DAY_FAINT = 0xFF3A404D;
    /** What rounding to eight bits can add to a held luminance. */
    private static final double ROUNDING = .004;

    @After
    public void tearDown() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit().clear().commit();
    }

    private static Bitmap plain(int color) {
        Bitmap cover = Bitmap.createBitmap(200, 300, Bitmap.Config.ARGB_8888);
        cover.eraseColor(color);
        return cover;
    }

    /** A stage of this size in a window, with nothing shown yet; it bakes in the test's thread. */
    private static HeroStageView stage(Activity activity, int w, int h, float heroShare) {
        HeroStageView stage = new HeroStageView(activity);
        stage.setWorker(Runnable::run);
        stage.setInk(NIGHT_INK);
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

    /** The darkest and the brightest pixel of a picture, as luminance. */
    private static double[] range(Bitmap picture) {
        double most = 0;
        double fewest = 1;
        for (int y = 0; y < picture.getHeight(); y += 2) {
            for (int x = 0; x < picture.getWidth(); x += 2) {
                double luminance = HeroStageView.luminance(picture.getPixel(x, y) | 0xFF000000);
                most = Math.max(most, luminance);
                fewest = Math.min(fewest, luminance);
            }
        }
        return new double[] {fewest, most};
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
    public void heldArtKeepsItsColourAndIsNeverBrighterThanTheAmbientMayBe() {
        int[] covers = {0xFFFFFFFF, 0xFFFFE600, 0xFFFF0000, 0xFF00C040, 0xFF0000FF, 0xFF808080, 0xFF000000};
        for (int color : covers) {
            Bitmap cover = plain(color);
            Bitmap held = HeroStageView.held(cover, NIGHT_CANVAS, false);
            String name = Integer.toHexString(color) + " held as " + Integer.toHexString(held.getPixel(10, 10));
            assertTrue("a copy: " + name, held != cover);
            assertEquals("the cover is left as it was", color, cover.getPixel(10, 10));
            assertTrue(name, range(held)[1] <= BackdropBlur.MAX_LUMA + ROUNDING);
            int pixel = held.getPixel(10, 10) | 0xFF000000;
            assertTrue("the name on " + name, HeroStageView.contrast(NIGHT_INK, pixel) >= HeroStageView.TEXT_CONTRAST);
            assertTrue("the faint line on " + name, HeroStageView.contrast(NIGHT_FAINT, pixel) >= HeroStageView.TEXT_CONTRAST);
            assertTrue("a glyph on " + name, HeroStageView.contrast(NIGHT_FAINT, pixel) >= HeroStageView.CONTROL_CONTRAST);
        }
        int red = HeroStageView.held(plain(0xFFFF0000), NIGHT_CANVAS, false).getPixel(10, 10);
        assertTrue("red is still red: " + Integer.toHexString(red), ((red >> 16) & 0xFF) > 140 && ((red >> 8) & 0xFF) < 24 && (red & 0xFF) < 24);
        int blue = HeroStageView.held(plain(0xFF0000FF), NIGHT_CANVAS, false).getPixel(10, 10);
        assertTrue("a colour that is dark enough is left alone: " + Integer.toHexString(blue), (blue & 0xFF) > 240);
        int white = HeroStageView.held(plain(0xFFFFFFFF), NIGHT_CANVAS, false).getPixel(10, 10);
        assertEquals("white is a grey", (white >> 16) & 0xFF, white & 0xFF);
        assertTrue("a dark one: " + Integer.toHexString(white), (white & 0xFF) > 60 && (white & 0xFF) < 96);
        int yellow = HeroStageView.held(plain(0xFFFFE600), NIGHT_CANVAS, false).getPixel(10, 10);
        assertTrue("yellow turns toward orange, away from Play: " + Integer.toHexString(yellow),
                ((yellow >> 16) & 0xFF) > 2 * ((yellow >> 8) & 0xFF));
    }

    @Test
    public void byDayTheArtIsLiftedUnderDarkWords() {
        for (int color : new int[] {0xFF000000, 0xFF202020, 0xFFFF0000, 0xFF0000FF, 0xFFFFFFFF}) {
            Bitmap held = HeroStageView.held(plain(color), DAY_CANVAS, true);
            String name = Integer.toHexString(color) + " held as " + Integer.toHexString(held.getPixel(10, 10));
            assertTrue(name, range(held)[0] >= BackdropBlur.MIN_LUMA_DAY - ROUNDING * 2);
            int pixel = held.getPixel(10, 10) | 0xFF000000;
            assertTrue("the name on " + name, HeroStageView.contrast(DAY_INK, pixel) >= HeroStageView.TEXT_CONTRAST);
            assertTrue("the faint line on " + name, HeroStageView.contrast(DAY_FAINT, pixel) >= HeroStageView.TEXT_CONTRAST);
        }
        int red = HeroStageView.held(plain(0xFFFF0000), DAY_CANVAS, true).getPixel(10, 10);
        assertTrue("red is still red: " + Integer.toHexString(red), ((red >> 16) & 0xFF) > ((red >> 8) & 0xFF) + 40);
        assertTrue(HeroStageView.dark(DAY_INK));
        assertFalse(HeroStageView.dark(NIGHT_INK));
        assertFalse(HeroStageView.dark(NIGHT_FAINT));
    }

    @Test
    public void aLargeCoverIsBakedSmallAndATransparentOneLiesOnTheStage() {
        Bitmap large = Bitmap.createBitmap(1200, 1800, Bitmap.Config.ARGB_8888);
        large.eraseColor(0xFF3060C0);
        Bitmap held = HeroStageView.held(large, NIGHT_CANVAS, false);
        assertEquals(HeroStageView.HELD_SIDE, held.getHeight());
        assertEquals(HeroStageView.HELD_SIDE * 2 / 3, held.getWidth());
        assertFalse("opaque", held.hasAlpha());
        Bitmap clear = Bitmap.createBitmap(20, 30, Bitmap.Config.ARGB_8888);
        Bitmap onStage = HeroStageView.held(clear, NIGHT_CANVAS, false);
        Bitmap stageItself = HeroStageView.held(plain(NIGHT_CANVAS), NIGHT_CANVAS, false);
        assertEquals("where the cover has nothing, the stage's own colour is held", stageItself.getPixel(5, 5), onStage.getPixel(5, 5));
        assertNull("no cover, no copy", HeroStageView.held(null, NIGHT_CANVAS, false));
        Bitmap gone = plain(0xFFFFFFFF);
        gone.recycle();
        assertNull("a cover its cache let go is not read", HeroStageView.held(gone, NIGHT_CANVAS, false));
    }

    @Test
    public void sharpCoverTitleLettersDissolveAndHonorNightAndDayLumaBounds() {
        Bitmap cover = Bitmap.createBitmap(400, 600, Bitmap.Config.ARGB_8888);
        cover.eraseColor(0xFF080A10);
        android.graphics.Paint letterPaint = new android.graphics.Paint();
        letterPaint.setColor(0xFFFFFFFF);
        Canvas coverCanvas = new Canvas(cover);
        // High-contrast vertical letter strokes across the upper third ("RACING GAME" title band).
        for (int x = 40; x < 360; x += 32) {
            coverCanvas.drawRect(x, 80, x + 16, 176, letterPaint);
        }

        Bitmap night = HeroStageView.held(cover, NIGHT_CANVAS, false);
        double[] nightRange = range(night);
        assertTrue("night luma ceiling respected: " + nightRange[1], nightRange[1] <= BackdropBlur.MAX_LUMA + ROUNDING);
        int maxHorizontalJump = 0;
        int titleRow = night.getHeight() / 5;
        for (int x = 1; x < night.getWidth(); x++) {
            int prev = (night.getPixel(x - 1, titleRow) >> 8) & 0xFF;
            int curr = (night.getPixel(x, titleRow) >> 8) & 0xFF;
            maxHorizontalJump = Math.max(maxHorizontalJump, Math.abs(curr - prev));
        }
        assertTrue("cover title strokes dissolved into smooth wash: jump=" + maxHorizontalJump, maxHorizontalJump <= 6);

        Bitmap day = HeroStageView.held(cover, DAY_CANVAS, true);
        double[] dayRange = range(day);
        assertTrue("day luma floor respected: " + dayRange[0], dayRange[0] >= BackdropBlur.MIN_LUMA_DAY - ROUNDING * 2);
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
    public void whiteArtIsNoBrighterThanTheStageAllowsAnywhereOnAWideWindow() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.setCanvasColor(NIGHT_CANVAS);
            stage.show(plain(0xFFFFFFFF), false);
            assertFalse(stage.isDay());
            Bitmap picture = drawn(stage, NIGHT_CANVAS);
            assertTrue("there is art to measure", HeroStageView.luminance(picture.getPixel(950, 150)) > .05);
            assertTrue("the brightest pixel: " + range(picture)[1], range(picture)[1] <= BackdropBlur.MAX_LUMA + ROUNDING);
            // The name where it stands on a TV and a handheld, the top row's buttons at the end, and the whole art.
            Rect words = new Rect(48, 80, 912, 158);
            Rect buttons = new Rect(760, 12, 948, 68);
            Rect all = new Rect(0, 0, 960, 540);
            assertTrue("the name: " + least(picture, words, NIGHT_INK), least(picture, words, NIGHT_INK) >= HeroStageView.TEXT_CONTRAST);
            assertTrue("the faint line: " + least(picture, all, NIGHT_FAINT), least(picture, all, NIGHT_FAINT) >= HeroStageView.TEXT_CONTRAST);
            assertTrue("a button's glyph: " + least(picture, buttons, NIGHT_FAINT), least(picture, buttons, NIGHT_FAINT) >= HeroStageView.CONTROL_CONTRAST);
        }
    }

    @Test
    @Config(qualifiers = "w412dp-h915dp-port-night-mdpi")
    public void whiteArtIsNoBrighterThanTheStageAllowsOnATallWindow() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 412, 915, .46f);
            stage.setCanvasColor(NIGHT_CANVAS);
            stage.show(plain(0xFFFFFFFF), false);
            Bitmap picture = drawn(stage, NIGHT_CANVAS);
            assertTrue("there is art at the top, under the status bar and the top row", HeroStageView.luminance(picture.getPixel(206, 20)) > .05);
            Rect all = new Rect(0, 0, 412, 915);
            assertTrue("the brightest pixel: " + range(picture)[1], range(picture)[1] <= BackdropBlur.MAX_LUMA + ROUNDING);
            assertTrue("the faint line: " + least(picture, all, NIGHT_FAINT), least(picture, all, NIGHT_FAINT) >= HeroStageView.TEXT_CONTRAST);
        }
    }

    @Test
    @Config(qualifiers = "w960dp-h540dp-land-mdpi")
    public void darkWordsKeepTheirContrastOverBlackArtByDay() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.setCanvasColor(DAY_CANVAS);
            stage.setInk(DAY_INK);
            assertTrue(stage.isDay());
            stage.show(plain(0xFF000000), false);
            Bitmap picture = drawn(stage, DAY_CANVAS);
            Rect all = new Rect(0, 0, 960, 540);
            assertTrue("the name: " + least(picture, all, DAY_INK), least(picture, all, DAY_INK) >= HeroStageView.TEXT_CONTRAST);
            assertTrue("the faint line: " + least(picture, all, DAY_FAINT), least(picture, all, DAY_FAINT) >= HeroStageView.TEXT_CONTRAST);
            // A TV writes its hero in the light label colours whatever the theme: the art is the night's again.
            stage.setInk(NIGHT_INK);
            assertFalse(stage.isDay());
            assertTrue("baked again for the new words", range(stage.art())[1] <= BackdropBlur.MAX_LUMA + ROUNDING);
        }
    }

    @Test
    public void theArtIsWholeAtItsEdgeAndGoneWhereItsPlaceEnds() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.setCanvasColor(NIGHT_CANVAS);
            assertFalse(stage.hasArt());
            Bitmap empty = drawn(stage, NIGHT_CANVAS);
            assertEquals("nothing is drawn without art", NIGHT_CANVAS, empty.getPixel(100, 100));
            stage.show(plain(0xFFFF0000), false);
            assertTrue(stage.hasArt());
            int held = (stage.art().getPixel(10, 10) >> 16) & 0xFF;
            Bitmap picture = drawn(stage, NIGHT_CANVAS);
            int whole = picture.getPixel(950, 150);
            assertTrue("the held cover itself by the end edge: " + Integer.toHexString(whole),
                    Math.abs(((whole >> 16) & 0xFF) - held) <= 2 && (whole & 0xFF) < 16);
            assertEquals("gone at the foot of the hero", NIGHT_CANVAS, picture.getPixel(950, 320));
            assertEquals("and before the words' side", NIGHT_CANVAS, picture.getPixel(300, 300));
            int mid = (picture.getPixel(950, 250) >> 16) & 0xFF;
            assertTrue("fading in between: " + mid, mid > 20 && mid < held - 8);
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
            assertTrue("the old cover is still whole under the new one: " + Integer.toHexString(start), ((start >> 16) & 0xFF) > 140);
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
    public void highContrastShowsNoArt() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putBoolean(AppTheme.HIGH_CONTRAST, true).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            HeroStageView stage = stage(controller.get(), 960, 540, .56f);
            stage.show(plain(0xFFFF0000), true);
            assertFalse(stage.isChanging());
            assertNull("nothing is baked for a page that shows none", stage.art());
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
