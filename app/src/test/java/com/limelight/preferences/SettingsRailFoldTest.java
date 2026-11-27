package com.limelight.preferences;

import android.app.Application;
import android.content.Context;
import android.content.Intent;
import android.preference.Preference;
import android.preference.PreferenceScreen;
import android.view.View;
import android.widget.ListView;

import com.limelight.R;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowLooper;
import org.robolectric.util.ReflectionHelpers;
import org.robolectric.util.ReflectionHelpers.ClassParameter;

import java.util.Arrays;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.junit.Assert.*;

/**
 * The list of sections is simple first: Recommended, Controls, Audio and App show, and More settings unfolds the
 * presets, Profiles and the sections of detail in place. Nothing is removed: a link, a search result and the choice
 * itself all open the fold.
 */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class,
        shadows = com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class SettingsRailFoldTest {
    private static final String WIDE = "w1280dp-h720dp-land-mdpi";
    private static final List<String> SIMPLE = Arrays.asList("simple", "input", "audio", "app", "more_sections");
    /** The list unfolded at the defaults: Debug only on request (Show debug settings, SettingsHideTest). */
    private static final List<String> ALL = Arrays.asList("simple", "input", "audio", "app", "more_sections",
            "stream_presets", "stream_profiles", "video", "display", "controller", "streaming", "advanced");

    private static void settle(StreamSettings screen) {
        ShadowLooper.idleMainLooper(300, TimeUnit.MILLISECONDS);
        screen.getFragmentManager().executePendingTransactions();
        View root = screen.findViewById(android.R.id.content);
        int width = screen.getResources().getDisplayMetrics().widthPixels;
        int height = screen.getResources().getDisplayMetrics().heightPixels;
        root.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
        root.layout(0, 0, width, height);
    }

    private static ActivityController<StreamSettings> open(Intent intent) {
        ActivityController<StreamSettings> controller = (intent == null ? Robolectric.buildActivity(StreamSettings.class)
                : Robolectric.buildActivity(StreamSettings.class, intent)).setup().visible();
        settle(controller.get());
        return controller;
    }

    /** The list of sections: the left pane on a wide screen, the whole screen on a phone. */
    private static StreamSettings.RootFragment rail(StreamSettings screen) {
        android.app.Fragment left = screen.getFragmentManager().findFragmentById(R.id.settings_sections);
        return (StreamSettings.RootFragment) (left != null ? left : screen.getFragmentManager().findFragmentById(R.id.stream_settings));
    }

    private static List<String> rows(StreamSettings screen) {
        return SettingsStructureTest.keys(rail(screen).getPreferenceScreen());
    }

    private static void choose(StreamSettings screen, String key) {
        Preference row = rail(screen).findPreference(key);
        row.getOnPreferenceClickListener().onPreferenceClick(row);
        settle(screen);
    }

    private static String shownSection(StreamSettings screen) {
        android.app.Fragment fragment = screen.getFragmentManager().findFragmentById(R.id.stream_settings);
        assertTrue(fragment instanceof StreamSettings.SettingsFragment);
        return fragment.getArguments().getString("section");
    }

    @Test
    public void aFreshOverviewShowsTheSimpleSectionsAndMoreSettings() {
        ActivityController<StreamSettings> controller = open(null);
        try {
            StreamSettings screen = controller.get();
            assertEquals(SIMPLE, rows(screen));
            Preference more = rail(screen).findPreference(StreamSettings.RAIL_MORE_KEY);
            assertEquals(screen.getString(R.string.settings_more), more.getTitle().toString());
            // The rows behind it still answer by key: their summaries are kept current while folded.
            assertNotNull(rail(screen).findPreference("video").getSummary());
        } finally { controller.pause().stop().destroy(); }
    }

    @Test
    public void moreSettingsUnfoldsTheRestInOrderAndFoldsItAgain() {
        ActivityController<StreamSettings> controller = open(null);
        try {
            StreamSettings screen = controller.get();
            choose(screen, StreamSettings.RAIL_MORE_KEY);
            assertEquals(ALL, rows(screen));
            choose(screen, StreamSettings.RAIL_MORE_KEY);
            assertEquals(SIMPLE, rows(screen));
            // And once more: the rows come back to the same places.
            choose(screen, StreamSettings.RAIL_MORE_KEY);
            assertEquals(ALL, rows(screen));
        } finally { controller.pause().stop().destroy(); }
    }

    @Test
    public void theChoiceIsKeptForTheNextVisit() {
        ActivityController<StreamSettings> first = open(null);
        try {
            choose(first.get(), StreamSettings.RAIL_MORE_KEY);
        } finally { first.pause().stop().destroy(); }
        ActivityController<StreamSettings> second = open(null);
        try {
            assertEquals(ALL, rows(second.get()));
        } finally { second.pause().stop().destroy(); }
    }

    @Test @Config(qualifiers = WIDE)
    public void aWideScreenOpensOnRecommended() {
        ActivityController<StreamSettings> controller = open(null);
        try {
            StreamSettings screen = controller.get();
            assertEquals("simple", shownSection(screen));
            assertEquals(SIMPLE, rows(screen));
            ListView list = rail(screen).getView().findViewById(android.R.id.list);
            assertTrue("the section on the right is marked on the left",
                    list.isItemChecked(StreamSettings.RootFragment.positionOf(list, "simple")));
        } finally { controller.pause().stop().destroy(); }
    }

    @Test @Config(qualifiers = WIDE)
    public void aLinkToASectionBehindTheFoldOpensItForTheVisitOnly() {
        Context context = RuntimeEnvironment.getApplication();
        ActivityController<StreamSettings> controller = open(StreamSettings.atStreamSize(context));
        try {
            StreamSettings screen = controller.get();
            assertEquals("video", shownSection(screen));
            assertEquals(ALL, rows(screen));
            ListView list = rail(screen).getView().findViewById(android.R.id.list);
            assertTrue(list.isItemChecked(StreamSettings.RootFragment.positionOf(list, "video")));
        } finally { controller.pause().stop().destroy(); }
        ActivityController<StreamSettings> next = open(null);
        try {
            assertEquals("a link is not the user's choice", SIMPLE, rows(next.get()));
        } finally { next.pause().stop().destroy(); }
    }

    /** The rows More settings unfolds reach the list a pass later: the result's section is marked once they are there. */
    @Test @Config(qualifiers = WIDE)
    public void aSearchResultBehindTheFoldMarksItsSection() {
        ActivityController<StreamSettings> controller = open(null);
        try {
            StreamSettings screen = controller.get();
            assertEquals(SIMPLE, rows(screen));
            ReflectionHelpers.callInstanceMethod(screen, "setSearchQuery", ClassParameter.from(String.class, "bitrate"));
            settle(screen);
            ReflectionHelpers.callInstanceMethod(screen, "openSearchResult",
                    ClassParameter.from(String.class, "video"), ClassParameter.from(String.class, "seekbar_bitrate_kbps"));
            settle(screen);
            assertEquals("video", shownSection(screen));
            assertEquals(ALL, rows(screen));
            ListView list = rail(screen).getView().findViewById(android.R.id.list);
            assertTrue("the result's section is marked on the left",
                    list.isItemChecked(StreamSettings.RootFragment.positionOf(list, "video")));
        } finally { controller.pause().stop().destroy(); }
    }

    /** On a phone a link opens the section alone; Back to the list finds its row there. */
    @Test
    public void onAPhoneALinkedSectionHasItsRowInTheList() {
        ActivityController<StreamSettings> controller = open(StreamSettings.atStreamSize(RuntimeEnvironment.getApplication()));
        try {
            StreamSettings screen = controller.get();
            assertEquals("video", shownSection(screen));
            screen.showSection(null);
            settle(screen);
            assertEquals(ALL, rows(screen));
        } finally { controller.pause().stop().destroy(); }
    }

    @Test @Config(qualifiers = WIDE)
    public void foldingWhileASectionBehindItShowsGoesBackToRecommended() {
        ActivityController<StreamSettings> controller = open(null);
        try {
            StreamSettings screen = controller.get();
            choose(screen, StreamSettings.RAIL_MORE_KEY);
            choose(screen, "display");
            assertEquals("display", shownSection(screen));
            choose(screen, StreamSettings.RAIL_MORE_KEY);
            assertEquals(SIMPLE, rows(screen));
            assertEquals("the pane never shows a section the list hides", "simple", shownSection(screen));
        } finally { controller.pause().stop().destroy(); }
    }

    /** TalkBack hears whether More settings is open: the row offers Expand while folded and Collapse while open, and both work. */
    @Test @Config(qualifiers = WIDE)
    public void aScreenReaderHearsAndChangesWhetherMoreSettingsIsOpen() {
        ActivityController<StreamSettings> controller = open(null);
        try {
            StreamSettings screen = controller.get();
            assertEquals(Arrays.asList(true, false), foldActions(screen));
            assertFalse("collapsing a folded row does nothing", moreRow(screen).performAccessibilityAction(
                    android.view.accessibility.AccessibilityNodeInfo.ACTION_COLLAPSE, null));
            assertTrue(moreRow(screen).performAccessibilityAction(
                    android.view.accessibility.AccessibilityNodeInfo.ACTION_EXPAND, null));
            settle(screen);
            assertEquals(ALL, rows(screen));
            assertEquals(Arrays.asList(false, true), foldActions(screen));
            assertTrue(moreRow(screen).performAccessibilityAction(
                    android.view.accessibility.AccessibilityNodeInfo.ACTION_COLLAPSE, null));
            settle(screen);
            assertEquals(SIMPLE, rows(screen));
            assertEquals(Arrays.asList(true, false), foldActions(screen));
        } finally { controller.pause().stop().destroy(); }
    }

    private static View moreRow(StreamSettings screen) {
        android.widget.ListView list = rail(screen).getView().findViewById(android.R.id.list);
        int at = StreamSettings.RootFragment.positionOf(list, StreamSettings.RAIL_MORE_KEY);
        View row = list.getChildAt(at - list.getFirstVisiblePosition());
        assertNotNull("More settings is row " + at + " of " + list.getCount() + ", " + list.getChildCount() + " on screen", row);
        return row;
    }

    /** Whether the row offers Expand, and whether it offers Collapse. */
    private static List<Boolean> foldActions(StreamSettings screen) {
        android.view.accessibility.AccessibilityNodeInfo info = moreRow(screen).createAccessibilityNodeInfo();
        List<android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction> actions = info.getActionList();
        return Arrays.asList(actions.contains(android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction.ACTION_EXPAND),
                actions.contains(android.view.accessibility.AccessibilityNodeInfo.AccessibilityAction.ACTION_COLLAPSE));
    }

    @Test
    public void everySectionHasARowAndAPlaceInTheChips() {
        assertEquals(StreamSettings.SECTIONS.keySet(), new java.util.LinkedHashSet<>(sorted(StreamSettings.RAIL_SECTIONS)));
        for (String folded : StreamSettings.RAIL_FOLDED) {
            // Debug folds too, though it is listed only on request.
            assertTrue(folded, ALL.contains(folded) || "debug".equals(folded));
        }
    }

    /** {@code RAIL_SECTIONS} in {@code SECTIONS}' order, so the two can be compared as sets with a readable failure. */
    private static List<String> sorted(List<String> sections) {
        List<String> order = new java.util.ArrayList<>(StreamSettings.SECTIONS.keySet());
        List<String> copy = new java.util.ArrayList<>(sections);
        copy.sort(java.util.Comparator.comparingInt(order::indexOf));
        return copy;
    }
}
