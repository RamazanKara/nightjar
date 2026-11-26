package com.limelight;

import static com.limelight.PcViewTestSupport.pc;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.os.Looper;
import android.preference.PreferenceManager;
import android.view.View;
import android.widget.AbsListView;
import android.widget.TextView;

import com.limelight.PcViewLifecycleTest.Library;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.PairingManager;
import com.limelight.ui.AppTheme;
import com.limelight.ui.GlassButton;
import com.limelight.ui.Motion;
import com.limelight.ui.ScanPulseView;
import com.limelight.ui.ScreenShots;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

import java.io.File;
import java.time.Duration;
import java.util.concurrent.CountDownLatch;

/**
 * First run on the PCs page: the scan pulse stands over the word while PCs are looked for and rests the moment discovery stops
 * or a PC is found; the first PC takes its place as a card, and the page has one amber control, the first PC's action.
 * With NIGHTJAR_PREVIEW_OUTPUT set, the pictures of both states go to {@code first-run/}.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class PcViewFirstRunTest {
    private static final String PHONE = "w412dp-h915dp-port";
    private static final String HANDHELD = "w960dp-h540dp-land";
    private static final String TV = "w960dp-h540dp-land-television";

    private final PcViewTestSupport support = new PcViewTestSupport();

    private static void set(String key, Object value) {
        var edit = PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit();
        if (value instanceof Boolean) edit.putBoolean(key, (Boolean) value);
        else edit.putString(key, (String) value);
        edit.commit();
    }

    private static void settle() {
        Shadows.shadowOf(Looper.getMainLooper()).idleFor(Duration.ofSeconds(1));
    }

    private static ComputerDetails unpaired(String uuid, String name) {
        return pc(uuid, name, ComputerDetails.State.ONLINE, PairingManager.PairState.NOT_PAIRED);
    }

    private static ComputerDetails ready(String uuid, String name) {
        return pc(uuid, name, ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
    }

    private static ComputerDetails away(String uuid, String name) {
        return pc(uuid, name, ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED);
    }

    private static View card(Library screen, int position) {
        AbsListView list = screen.findViewById(R.id.fragmentView);
        assertNotNull(list);
        return list.getChildAt(position);
    }

    private static String name(View card) {
        return ((TextView) card.findViewById(R.id.grid_text)).getText().toString();
    }

    private static GlassButton amber(View card) {
        return card.findViewById(R.id.grid_resume);
    }

    private static GlassButton quiet(View card) {
        return card.findViewById(R.id.grid_action);
    }

    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void thePulseStandsOverTheWordWhilePcsAreLookedForAndRestsOnceOneIsFound() {
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            Library screen = controller.get();
            ScreenShots.fit(screen, 824, 1830);
            ScanPulseView pulse = screen.findViewById(R.id.first_run_pulse);
            assertEquals(View.VISIBLE, pulse.getVisibility());
            assertTrue("PCs are looked for", pulse.isPulsing());
            assertEquals("its full size where there is room", 464, pulse.getWidth());
            assertEquals(464, pulse.getHeight());
            View word = screen.findViewById(R.id.discovery_message);
            assertTrue("over the word", pulse.getBottom() <= word.getTop());
            View column = screen.findViewById(R.id.first_run);
            assertEquals("in the middle", column.getWidth() / 2, (pulse.getLeft() + pulse.getRight()) / 2, 1);

            support.report(unpaired("a", "Alpha"));
            assertFalse("a PC was found", pulse.isPulsing());
            assertEquals(View.GONE, screen.findViewById(R.id.no_pc_found_layout).getVisibility());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void thePulseRestsWhileDiscoveryIsStopped() {
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            ScanPulseView pulse = controller.get().findViewById(R.id.first_run_pulse);
            ScreenShots.fit(controller.get(), 824, 1830);
            assertTrue(pulse.isPulsing());
            controller.pause();
            assertFalse("the page left the foreground and stopped looking", pulse.isPulsing());
            controller.resume();
            assertTrue(pulse.isPulsing());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void withReducedMotionThePulseStandsStill() {
        set(Motion.REDUCE_MOTION, Motion.REDUCE);
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            ScreenShots.fit(controller.get(), 824, 1830);
            ScanPulseView pulse = controller.get().findViewById(R.id.first_run_pulse);
            assertEquals("the still rings are shown", 464, pulse.getWidth());
            assertFalse(pulse.isPulsing());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /** A phone held sideways has little height: the pulse shrinks or goes, and never pushes the actions out of the window. */
    @Test
    @Config(qualifiers = "w640dp-h360dp-land-night-xhdpi")
    public void aShortWindowKeepsItsActionsInView() {
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            Library screen = controller.get();
            ScreenShots.fit(screen, 1280, 720);
            ScreenShots.fit(screen, 1280, 720);
            View pulse = screen.findViewById(R.id.first_run_pulse);
            View column = screen.findViewById(R.id.first_run);
            View window = screen.findViewById(R.id.no_pc_found_layout);
            assertTrue("nothing to scroll for", column.getHeight() <= window.getHeight());
            if (pulse.getVisibility() == View.VISIBLE) {
                assertTrue("never a mark too small to read", pulse.getWidth() >= 192);
                assertEquals(pulse.getWidth(), pulse.getHeight());
            }
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void theFirstPcsActionIsThePagesOneAmberControl() {
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            Library screen = controller.get();
            support.report(unpaired("p", "Laptop"));
            ScreenShots.fit(screen, 824, 1830);
            View only = card(screen, 0);
            assertEquals(View.VISIBLE, amber(only).getVisibility());
            assertEquals(screen.getString(R.string.pcview_menu_pair_pc), amber(only).getText().toString());
            assertNull("the play mark is Resume's", amber(only).getIcon());
            assertEquals(View.GONE, quiet(only).getVisibility());

            // A paired PC that is up sorts before one that is not paired, and leads.
            support.report(ready("r", "Desk"));
            support.report(away("o", "Attic"));
            ScreenShots.fit(screen, 824, 1830);
            View first = card(screen, 0);
            assertEquals("Desk", name(first));
            assertEquals(View.VISIBLE, amber(first).getVisibility());
            assertEquals(screen.getString(R.string.console_hint_open), amber(first).getText().toString());
            View second = card(screen, 1);
            assertEquals("Laptop", name(second));
            assertEquals(View.GONE, amber(second).getVisibility());
            assertEquals(View.VISIBLE, quiet(second).getVisibility());
            assertEquals(screen.getString(R.string.pcview_menu_pair_pc), quiet(second).getText().toString());
            assertEquals("an away PC is never amber", View.GONE, amber(card(screen, 2)).getVisibility());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = TV + "-night-xhdpi")
    public void onATelevisionTheKeysActAndNoCardShowsAButton() {
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            Library screen = controller.get();
            support.report(unpaired("p", "Laptop"));
            ScreenShots.fit(screen, 1920, 1080);
            View only = card(screen, 0);
            assertEquals(View.GONE, amber(only).getVisibility());
            assertEquals(View.GONE, quiet(only).getVisibility());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /** The footer under the PCs is placed from where the cards lie: they are in their place the moment they are found. */
    @Test
    @Config(qualifiers = PHONE + "-night-xhdpi")
    public void aFoundPcStandsInItsPlaceAtOnce() {
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            Library screen = controller.get();
            ScreenShots.fit(screen, 824, 1830);
            View cards = screen.findViewById(R.id.pcFragmentContainer);
            support.report(unpaired("a", "Alpha"));
            assertEquals(1f, cards.getAlpha(), 0f);
            assertEquals(0f, cards.getTranslationY(), 0f);
        } finally {
            controller.pause().stop().destroy();
        }
    }

    private void picture(String name, int w, int h, ComputerDetails... found) throws Exception {
        File directory = ScreenShots.directory("first-run");
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            ScreenShots.fit(controller.get(), w, h);
            for (ComputerDetails computer : found) support.report(computer);
            settle();
            ScreenShots.capture(controller.get(), w, h, new File(directory, name + ".png"));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void scanPhoneNight() throws Exception {
        picture("scan-phone-night", 824, 1830);
    }

    @Test @Config(qualifiers = PHONE + "-xhdpi")
    public void scanPhoneDay() throws Exception {
        picture("scan-phone-day", 824, 1830);
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void scanPhoneHighContrast() throws Exception {
        set(AppTheme.HIGH_CONTRAST, true);
        picture("scan-phone-high-contrast", 824, 1830);
    }

    @Test @Config(qualifiers = "w640dp-h360dp-land-night-xhdpi")
    public void scanPhoneSideways() throws Exception {
        picture("scan-phone-sideways", 1280, 720);
    }

    @Test @Config(qualifiers = HANDHELD + "-night-xhdpi")
    public void scanHandheld() throws Exception {
        picture("scan-handheld", 1920, 1080);
    }

    @Test @Config(qualifiers = TV + "-night-xhdpi")
    public void scanTv() throws Exception {
        picture("scan-tv", 1920, 1080);
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void foundPhoneNight() throws Exception {
        picture("found-phone-night", 824, 1830, unpaired("a", "Living room PC"));
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void pcsPhoneNight() throws Exception {
        picture("pcs-phone-night", 824, 1830, ready("r", "Desk"), unpaired("p", "Living room PC"), away("o", "Attic"));
    }

    @Test @Config(qualifiers = PHONE + "-xhdpi")
    public void pcsPhoneDay() throws Exception {
        picture("pcs-phone-day", 824, 1830, ready("r", "Desk"), unpaired("p", "Living room PC"), away("o", "Attic"));
    }

    @Test @Config(qualifiers = PHONE + "-night-xhdpi")
    public void pcsPhoneHighContrast() throws Exception {
        set(AppTheme.HIGH_CONTRAST, true);
        picture("pcs-phone-high-contrast", 824, 1830, ready("r", "Desk"), unpaired("p", "Living room PC"), away("o", "Attic"));
    }

    @Test @Config(qualifiers = HANDHELD + "-night-xhdpi")
    public void foundHandheld() throws Exception {
        picture("found-handheld", 1920, 1080, unpaired("a", "Living room PC"));
    }

    @Test @Config(qualifiers = HANDHELD + "-night-xhdpi")
    public void pcsHandheld() throws Exception {
        picture("pcs-handheld", 1920, 1080, ready("r", "Desk"), unpaired("p", "Living room PC"), away("o", "Attic"));
    }

    @Test @Config(qualifiers = TV + "-night-xhdpi")
    public void pcsTv() throws Exception {
        picture("pcs-tv", 1920, 1080, ready("r", "Desk"), unpaired("p", "Living room PC"), away("o", "Attic"));
    }
}
