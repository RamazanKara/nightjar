package com.limelight.preferences;

import android.app.Application;
import android.content.Context;
import android.content.Intent;
import android.os.Looper;
import android.preference.Preference;

import androidx.appcompat.app.AlertDialog;

import com.limelight.PcViewLifecycleTest;
import com.limelight.R;
import com.limelight.computers.HostCapabilityCache;
import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.nvstream.http.TestHosts;

import org.junit.After;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowDialog;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class,
        shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class HostStreamSettingsTest {
    private final Application app = RuntimeEnvironment.getApplication();

    @After
    public void clear() {
        for (String name : new String[] {StreamProfiles.PREFERENCES, "AppStreamSettings", "HostStreamProfiles"}) {
            app.getSharedPreferences(name, Context.MODE_PRIVATE).edit().clear().commit();
        }
    }

    private static HostCapabilities rubylightWithNvidiaEnhancements() {
        return TestHosts.caps(TestHosts.serverInfo("rubylight-serverinfo.xml")
                .replace("</root>", "<NvidiaEnhancements>3</NvidiaEnhancements></root>"));
    }

    private static String summary(Preference row) {
        assertNotNull(row);
        return row.getSummary() == null ? "" : row.getSummary().toString();
    }

    private static void click(Preference preference) {
        preference.getOnPreferenceClickListener().onPreferenceClick(preference);
        Shadows.shadowOf(Looper.getMainLooper()).idle();
    }

    @Test
    public void gameSettingsOfferNvidiaEnhancementsAndSavePerGameOverrides() {
        HostCapabilityCache.save(app, "nvidia-pc", rubylightWithNvidiaEnhancements());
        HostStreamProfile.deserialize("v2;rtx_hdr=true").save(app, "nvidia-pc");
        Intent intent = HostStreamSettings.intentForGame(app, "nvidia-pc", "Den PC", "game-uuid", 7, "Racer");
        ActivityController<HostStreamSettings> controller = Robolectric.buildActivity(HostStreamSettings.class, intent).setup();
        try {
            HostStreamSettings screen = controller.get();
            HostStreamSettings.SettingsFragment fragment = (HostStreamSettings.SettingsFragment)
                    screen.getFragmentManager().findFragmentById(R.id.host_profile_settings);
            Preference rtxHdr = fragment.findPreference("rtx_hdr");
            Preference rtxVideo = fragment.findPreference("rtx_video");
            assertNotNull(rtxHdr);
            assertNotNull(rtxVideo);

            String expectedFromPc = app.getString(R.string.layers_value_from,
                    app.getString(R.string.host_profile_on), app.getString(R.string.layers_from_pc));
            assertTrue(summary(rtxHdr), summary(rtxHdr).startsWith(expectedFromPc + "\n"));

            String expectedDefault = app.getString(R.string.layers_value_from,
                    app.getString(R.string.fec_choice_host), app.getString(R.string.layers_from_main));
            assertTrue(summary(rtxVideo), summary(rtxVideo).startsWith(expectedDefault + "\n"));

            click(rtxHdr);
            AlertDialog hdrDialog = (AlertDialog) ShadowDialog.getLatestDialog();
            assertEquals(app.getString(R.string.layers_not_set_value, app.getString(R.string.host_profile_on)),
                    hdrDialog.getListView().getAdapter().getItem(0).toString());
            hdrDialog.getListView().performItemClick(null, 2, 2);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertTrue(summary(rtxHdr), summary(rtxHdr).startsWith(app.getString(R.string.host_profile_off) + "\n"));

            click(rtxVideo);
            AlertDialog videoDialog = (AlertDialog) ShadowDialog.getLatestDialog();
            assertEquals(app.getString(R.string.layers_not_set_value, app.getString(R.string.fec_choice_host)),
                    videoDialog.getListView().getAdapter().getItem(0).toString());
            videoDialog.getListView().performItemClick(null, 1, 1);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertTrue(summary(rtxVideo), summary(rtxVideo).startsWith(app.getString(R.string.host_profile_on) + "\n"));

            screen.findViewById(R.id.host_profile_save).performClick();
            HostStreamProfile saved = AppStreamSettings.load(app, "nvidia-pc", "game-uuid", 7);
            assertEquals(Boolean.FALSE, saved.rtxHdr);
            assertEquals(Boolean.TRUE, saved.rtxVideo);
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void liveCapabilityUpdateRebuildsGatedDisplayAndEnhancementRows() {
        HostCapabilityCache.save(app, "upgraded-pc", TestHosts.fixture("sunshine-serverinfo.xml"));
        Intent intent = new Intent(app, HostStreamSettings.class)
                .putExtra("host_uuid", "upgraded-pc")
                .putExtra("host_name", "Desk");
        ActivityController<HostStreamSettings> controller = Robolectric.buildActivity(HostStreamSettings.class, intent).setup();
        try {
            HostStreamSettings screen = controller.get();
            HostStreamSettings.SettingsFragment fragment = (HostStreamSettings.SettingsFragment)
                    screen.getFragmentManager().findFragmentById(R.id.host_profile_settings);
            assertNull(fragment.findPreference("rtx_hdr"));
            assertNull(fragment.findPreference("host_display"));

            com.limelight.nvstream.http.ComputerDetails updated = new com.limelight.nvstream.http.ComputerDetails();
            updated.uuid = "upgraded-pc";
            updated.name = "Desk";
            updated.state = com.limelight.nvstream.http.ComputerDetails.State.ONLINE;
            updated.activeAddress = new com.limelight.nvstream.http.ComputerDetails.AddressTuple("10.0.0.5", 47989);
            updated.caps = TestHosts.caps(TestHosts.serverInfo("rubylight-serverinfo.xml")
                    .replace("</root>", "<NvidiaEnhancements>3</NvidiaEnhancements><DisplayScaleVersion>1</DisplayScaleVersion></root>"));
            screen.showComputer(updated);

            assertNotNull("rtx_hdr rebuilt after live capability upgrade", fragment.findPreference("rtx_hdr"));
            assertNotNull("rtx_video rebuilt after live capability upgrade", fragment.findPreference("rtx_video"));
            assertNotNull("host_display rebuilt after live capability upgrade", fragment.findPreference("host_display"));
            assertNotNull("dpi rebuilt after live capability upgrade", fragment.findPreference("dpi"));
        } finally {
            HostCapabilityCache.forget(app, "upgraded-pc");
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void capabilityRebuildPreservesListScrollPositionAndFocusedPreference() {
        HostCapabilityCache.save(app, "scroll-pc", TestHosts.fixture("sunshine-serverinfo.xml"));
        Intent intent = new Intent(app, HostStreamSettings.class)
                .putExtra("host_uuid", "scroll-pc")
                .putExtra("host_name", "Desk");
        ActivityController<HostStreamSettings> controller = Robolectric.buildActivity(HostStreamSettings.class, intent).setup();
        try {
            HostStreamSettings screen = controller.get();
            HostStreamSettings.SettingsFragment fragment = (HostStreamSettings.SettingsFragment)
                    screen.getFragmentManager().findFragmentById(R.id.host_profile_settings);
            android.widget.ListView listView = fragment.getView().findViewById(android.R.id.list);
            listView.measure(android.view.View.MeasureSpec.makeMeasureSpec(1080, android.view.View.MeasureSpec.EXACTLY),
                    android.view.View.MeasureSpec.makeMeasureSpec(600, android.view.View.MeasureSpec.EXACTLY));
            listView.layout(0, 0, 1080, 600);
            int targetPos = Math.max(1, listView.getCount() - 2);
            listView.setSelectionFromTop(targetPos, -12);
            listView.requestFocus();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            int firstBefore = listView.getFirstVisiblePosition();
            assertTrue("scrolled past row 0 before rebuild", firstBefore > 0);

            com.limelight.nvstream.http.ComputerDetails updated = new com.limelight.nvstream.http.ComputerDetails();
            updated.uuid = "scroll-pc";
            updated.name = "Desk";
            updated.state = com.limelight.nvstream.http.ComputerDetails.State.ONLINE;
            updated.activeAddress = new com.limelight.nvstream.http.ComputerDetails.AddressTuple("10.0.0.5", 47989);
            updated.caps = TestHosts.caps(TestHosts.serverInfo("rubylight-serverinfo.xml")
                    .replace("</root>", "<NvidiaEnhancements>3</NvidiaEnhancements><DisplayScaleVersion>1</DisplayScaleVersion></root>"));
            screen.showComputer(updated);
            Shadows.shadowOf(Looper.getMainLooper()).idle();

            android.widget.ListView rebuiltList = fragment.getView().findViewById(android.R.id.list);
            assertTrue("scroll position kept near prior row across capability rebuild: was " + firstBefore
                    + ", now " + rebuiltList.getFirstVisiblePosition(),
                    rebuiltList.getFirstVisiblePosition() >= firstBefore - 1);
            assertTrue("list focus kept after rebuild", rebuiltList.hasFocus());
        } finally {
            HostCapabilityCache.forget(app, "scroll-pc");
            controller.pause().stop().destroy();
        }
    }
}
