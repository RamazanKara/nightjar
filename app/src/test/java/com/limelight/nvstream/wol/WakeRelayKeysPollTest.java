package com.limelight.nvstream.wol;

import static com.limelight.nativeapi.NativeApiFixtures.ok;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import android.app.Application;

import com.limelight.nativeapi.HostEndpoint;
import com.limelight.nativeapi.NativeApiClient;
import com.limelight.nativeapi.NativeApiFixtures;
import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.nvstream.http.TestHosts;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

import java.util.Collections;

/** The poll that keeps a PC's wake targets keeps its away wake keys too, only while the PC offers them. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class WakeRelayKeysPollTest {
    private static final String SERVERINFO = "<root status_code=\"200\"><RustHostVersion>7.0.0</RustHostVersion>"
            + "<PairStatus>1</PairStatus><NativeApiVersion>1</NativeApiVersion>";
    private static final HostCapabilities OFFERS = TestHosts.caps(SERVERINFO
            + "<AwayWakeRelayVersion>1</AwayWakeRelayVersion></root>");
    private static final HostCapabilities OLDER = TestHosts.caps(SERVERINFO + "</root>");
    private static final String WITH_RELAY = capabilities("\"wake\",\"wake.relay\"");
    private static final String WITHOUT_RELAY = capabilities("\"wake\"");
    private static final String BUNDLE = "{\"target_id\":\"PC\",\"epoch\":1,\"device\":\"ab\","
            + "\"relays\":[{\"id\":\"r\",\"name\":\"Living Room NUC\",\"secret\":\"00\"}]}";

    private final NativeApiFixtures.FakeTransport transport = new NativeApiFixtures.FakeTransport();
    private WakeRelayKeys keys;
    private WakeTargetsPoll poll;
    private long now;

    private static String capabilities(String features) {
        return ok("{\"api_version\":1,\"host\":{\"name\":\"PC\",\"version\":\"7.0.0\",\"unique_id\":\"pc\"},"
                + "\"features\":[" + features + "],"
                + "\"permissions\":{\"list\":true,\"view\":true,\"launch\":true,\"host_control\":false}}");
    }

    @Before
    public void setUp() {
        Application app = RuntimeEnvironment.getApplication();
        keys = new WakeRelayKeys(app);
        poll = new WakeTargetsPoll(new WakeTargets(app), keys,
                computer -> new NativeApiClient(transport, HostEndpoint.of(computer, "unique", NativeApiFixtures.crypto())),
                Runnable::run, () -> now);
        transport.reply("pc", "wake", ok(WakeTargetsTest.WAKE));
        transport.reply("pc", "wake_relay", ok("{\"enabled\":true,\"target_id\":\"PC\",\"epoch\":1,\"relays\":[],"
                + "\"bundle\":" + BUNDLE + "}"));
    }

    @After
    public void tearDown() {
        WakeTargets.closeForTests(RuntimeEnvironment.getApplication().getFilesDir());
    }

    private void poll(HostCapabilities caps) {
        poll.polled(NativeApiFixtures.host("pc", "PC", caps, true));
        now += WakeTargetsPoll.INTERVAL_MS;
    }

    private int keyReads() {
        return Collections.frequency(transport.calls, "pc|wake_relay");
    }

    @Test
    public void aPcThatOffersItHandsItsKeysAndTheyGoWhenItStops() {
        transport.reply("pc", "capabilities", WITH_RELAY);
        poll(OFFERS);
        WakeRelayKeys.Kept kept = keys.load("pc");
        assertNotNull(kept);
        assertEquals(Collections.singletonList("Living Room NUC"), kept.relays);
        assertTrue(kept.hasRelay());
        assertEquals(1, keyReads());

        transport.reply("pc", "capabilities", WITHOUT_RELAY);
        poll(OFFERS);
        assertNull("turned off on the PC", keys.load("pc"));
        assertEquals("nothing asked of a PC that does not offer it", 1, keyReads());
    }

    @Test
    public void anAnswerWithoutKeysKeepsOnlyThatThePcOffersIt() {
        transport.reply("pc", "capabilities", WITH_RELAY);
        transport.reply("pc", "wake_relay", ok("{\"enabled\":true,\"target_id\":\"PC\",\"epoch\":1,\"relays\":[]}"));
        poll(OFFERS);
        assertNotNull(keys.load("pc"));
        assertFalse(keys.load("pc").hasRelay());
    }

    @Test
    public void aFailedReadKeepsTheKeysAndAnOlderHostIsNeverAsked() {
        transport.reply("pc", "capabilities", WITH_RELAY);
        poll(OFFERS);
        transport.reply("pc", "wake_relay", NativeApiFixtures.error("network", "connection reset"));
        poll(OFFERS);
        assertTrue(keys.load("pc").hasRelay());

        keys.forget("pc");
        transport.calls.clear();
        poll(OLDER);
        assertEquals(0, keyReads());
        assertNull(keys.load("pc"));
    }
}
