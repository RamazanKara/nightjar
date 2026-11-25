package com.limelight.nvstream.http;

import android.app.Application;

import com.limelight.nvstream.jni.NightjarHttpJni;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import java.io.StringReader;
import java.util.LinkedList;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class NvHTTPTest {
    @Test
    public void appListStripsLegacyOrderZeroWidthPrefixFromAppTitle() throws Exception {
        LinkedList<NvApp> apps = NvHTTP.getAppListByReader(new StringReader(
                "<root status_code=\"200\">" +
                        "<App><ID>1</ID><AppTitle>&#x200b;&#x200c;MoonDeckStream</AppTitle></App>" +
                        "<App><ID>2</ID><AppTitle>Desktop</AppTitle></App>" +
                        "</root>"));
        assertEquals(2, apps.size());
        assertEquals("MoonDeckStream", apps.get(0).getAppName());
        assertEquals("Desktop", apps.get(1).getAppName());
        assertEquals("", NvHTTP.stripLegacyOrderPrefix(null));
        assertEquals("", NvHTTP.stripLegacyOrderPrefix("\u200B\u200C"));
    }

    @Test
    public void serverInfoGettersReuseParsedSnapshotUntilXmlChanges() throws Exception {
        String xmlA = "<root status_code=\"200\">" +
                "<appversion>7.1.431.-1</appversion>" +
                "<GfeVersion>3.23.0.74</GfeVersion>" +
                "<PairStatus>1</PairStatus>" +
                "<MaxLumaPixelsHEVC>1869449984</MaxLumaPixelsHEVC>" +
                "<ServerCodecModeSupport>259</ServerCodecModeSupport>" +
                "<gputype>AMD Radeon RX</gputype>" +
                "<currentgame>42</currentgame>" +
                "<HttpsPort>47984</HttpsPort>" +
                "<ExternalPort>47989</ExternalPort>" +
                "<state>SUNSHINE_SERVER_BUSY</state>" +
                "</root>";
        String xmlB = "<root status_code=\"200\">" +
                "<appversion>7.1.432.-1</appversion>" +
                "<PairStatus>0</PairStatus>" +
                "<MaxLumaPixelsHEVC>0</MaxLumaPixelsHEVC>" +
                "<ServerCodecModeSupport>3</ServerCodecModeSupport>" +
                "<gputype>NVIDIA</gputype>" +
                "<currentgame>0</currentgame>" +
                "<HttpsPort>47985</HttpsPort>" +
                "<ExternalPort>47990</ExternalPort>" +
                "</root>";

        AtomicInteger parseCalls = new AtomicInteger();
        try (NvHTTP http = new NvHTTP(new ComputerDetails.AddressTuple("127.0.0.1", 47989), 47984,
                "0123456789abcdef", null, null, (raw, port, strict) -> {
            parseCalls.incrementAndGet();
            return NightjarHttpJni.parseServerInfoXml(raw, port, strict);
        })) {
            assertEquals("7.1.431.-1", http.getServerVersion(xmlA));
            assertEquals(PairingManager.PairState.PAIRED, http.getPairState(xmlA));
            assertEquals(0L, http.getMaxLumaPixelsH264(xmlA));
            assertEquals(1869449984L, http.getMaxLumaPixelsHEVC(xmlA));
            assertEquals(259L, http.getServerCodecModeSupport(xmlA));
            assertEquals("AMD Radeon RX", http.getGpuType(xmlA));
            assertTrue(http.supports4K(xmlA));
            assertEquals(42, http.getCurrentGame(xmlA));
            assertEquals(47984, http.getHttpsPort(xmlA));
            assertEquals(47989, http.getExternalPort(xmlA));
            assertEquals(1, parseCalls.get());

            assertEquals("7.1.432.-1", http.getServerVersion(xmlB));
            assertEquals(PairingManager.PairState.NOT_PAIRED, http.getPairState(xmlB));
            assertFalse(http.supports4K(xmlB));
            assertEquals(47985, http.getHttpsPort(xmlB));
            assertEquals(47990, http.getExternalPort(xmlB));
            assertEquals(2, parseCalls.get());
        }
    }
}
