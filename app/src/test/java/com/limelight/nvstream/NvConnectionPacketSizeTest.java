/**
 * Regression tests for packet size preflight resolution ({@code STREAM_CFG_AUTO} over
 * tunnel/VPN/cellular -> 1024 vs explicit {@code STREAM_CFG_REMOTE}/local -> 1392) and
 * XML-special-character UUID safety across {@link NightjarSessionJni}.
 */
package com.limelight.nvstream;

import android.app.Application;
import android.content.Context;
import android.net.ConnectivityManager;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.net.NetworkInfo;
import android.preference.PreferenceManager;

import com.limelight.nvstream.av.video.VideoDecoderRenderer;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostHttpResponseException;
import com.limelight.nvstream.http.LimelightCryptoProvider;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarSessionJni;

import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.Implementation;
import org.robolectric.annotation.Implements;
import org.robolectric.shadows.ShadowConnectivityManager;
import org.robolectric.shadows.ShadowNetwork;
import org.robolectric.shadows.ShadowNetworkCapabilities;
import org.robolectric.shadows.ShadowNetworkInfo;

import java.io.IOException;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;

import static org.junit.Assert.*;

@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class,
        shadows = {NvConnectionPacketSizeTest.FakeHost.class, NvConnectionHapticsFlagTest.CapturingBridge.class},
        instrumentedPackages = "com.limelight.nvstream.jni")
public class NvConnectionPacketSizeTest {
    @Before
    public void resetState() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit()
                .remove("jumbo_frames").commit();
        FakeHost.probeUdpFailure = null;
        FakeHost.probeUdpResult = null;
        FakeHost.probedPacketSizes.clear();
    }

    private static void setActiveNetwork(int transport, boolean notVpn) {
        ConnectivityManager connMgr = (ConnectivityManager) RuntimeEnvironment.getApplication()
                .getSystemService(Context.CONNECTIVITY_SERVICE);
        ShadowConnectivityManager shadow = Shadows.shadowOf(connMgr);
        Network active = connMgr.getActiveNetwork();
        assertNotNull(active);
        NetworkCapabilities caps = ShadowNetworkCapabilities.newInstance();
        Shadows.shadowOf(caps).addTransportType(transport);
        if (notVpn) {
            Shadows.shadowOf(caps).addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN);
        }
        shadow.setNetworkCapabilities(active, caps);
    }

    private static NvConnection startStream(String hostAddress, int remoteCfg, int maxPacketSize,
            String serverInfoXml) throws Exception {
        return startStream(hostAddress, remoteCfg, maxPacketSize, serverInfoXml, config -> {});
    }

    private static NvConnection startStream(String hostAddress, int remoteCfg, int maxPacketSize,
            String serverInfoXml, Consumer<StreamConfiguration.Builder> start) throws Exception {
        FakeHost.initial = serverInfoXml;
        FakeHost.resumeFailure = null;
        FakeHost.resumeResult = true;
        FakeHost.requests.clear();
        FakeHost.probedPacketSizes.clear();
        NvConnectionHapticsFlagTest.CapturingBridge.packetSize = -1;
        NvConnectionHapticsFlagTest.CapturingBridge.streamingRemotely = -1;

        NvApp app = new NvApp("Game", 73, false);
        app.setAppUuid("game-a");
        StreamConfiguration.Builder config = new StreamConfiguration.Builder().setApp(app)
                .setResolution(1280, 720).setRefreshRate(60).setRemoteConfiguration(remoteCfg)
                .setMaxPacketSize(maxPacketSize).setSupportedVideoFormats(MoonBridge.VIDEO_FORMAT_H264);
        start.accept(config);
        NvConnection connection = new NvConnection(RuntimeEnvironment.getApplication(),
                new ComputerDetails.AddressTuple(hostAddress, 47989), 47984, "device",
                config.build(), crypto(), null);
        NvConnectionListener listener = (NvConnectionListener) Proxy.newProxyInstance(
                NvConnectionListener.class.getClassLoader(), new Class<?>[] {NvConnectionListener.class},
                (proxy, method, args) -> null);
        connection.start(null, renderer(), listener);
        Thread worker = ConnectionStartup.await(connection);
        assertFalse("Connection startup did not finish", worker.isAlive());
        return connection;
    }

    private static void startStream(String hostAddress, int remoteCfg, int maxPacketSize) throws Exception {
        startStream(hostAddress, remoteCfg, maxPacketSize,
                "<root status_code=\"200\"><appversion>7.1.431.0</appversion>"
                        + "<uniqueid>test-host</uniqueid><HttpsPort>47984</HttpsPort><state>SUNSHINE_SERVER_FREE</state>"
                        + "<currentgame>0</currentgame><currentgameuuid></currentgameuuid><PairStatus>1</PairStatus></root>");
    }

    @Test
    public void autoOverTunnelVpnAndCellularUses1024WhileExplicitRemoteAndLocalKeep1392() throws Exception {
        // Non-VPN Wi-Fi + Auto on non-tunnel route keeps Android's default 1392.
        setActiveNetwork(NetworkCapabilities.TRANSPORT_WIFI, true);
        startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_AUTO, 1392);
        assertEquals(1392, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);

        // Auto + punched tunnel (127.78.x.y) over Wi-Fi with default 1392 -> 1024.
        startStream("127.78.0.1", StreamConfiguration.STREAM_CFG_AUTO, 1392);
        assertEquals(StreamConfiguration.STREAM_CFG_REMOTE, NvConnectionHapticsFlagTest.CapturingBridge.streamingRemotely);
        assertEquals(1024, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);

        // Auto + VPN transport with default 1392 -> 1024.
        setActiveNetwork(NetworkCapabilities.TRANSPORT_VPN, false);
        startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_AUTO, 1392);
        assertEquals(StreamConfiguration.STREAM_CFG_REMOTE, NvConnectionHapticsFlagTest.CapturingBridge.streamingRemotely);
        assertEquals(1024, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);

        // Auto + Cellular transport with default 1392 -> 1024.
        setActiveNetwork(NetworkCapabilities.TRANSPORT_CELLULAR, true);
        startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_AUTO, 1392);
        assertEquals(StreamConfiguration.STREAM_CFG_REMOTE, NvConnectionHapticsFlagTest.CapturingBridge.streamingRemotely);
        assertEquals(1024, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);

        // Explicit STREAM_CFG_REMOTE keeps streamConfig.getMaxPacketSize() (1392).
        startStream("127.78.0.1", StreamConfiguration.STREAM_CFG_REMOTE, 1392);
        assertEquals(StreamConfiguration.STREAM_CFG_REMOTE, NvConnectionHapticsFlagTest.CapturingBridge.streamingRemotely);
        assertEquals(1392, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);

        // Explicit STREAM_CFG_LOCAL keeps streamConfig.getMaxPacketSize() (1392).
        startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_LOCAL, 1392);
        assertEquals(StreamConfiguration.STREAM_CFG_LOCAL, NvConnectionHapticsFlagTest.CapturingBridge.streamingRemotely);
        assertEquals(1392, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);

        // Auto + tunnel with explicit custom packet size preserves the explicit size.
        setActiveNetwork(NetworkCapabilities.TRANSPORT_WIFI, true);
        startStream("127.78.0.1", StreamConfiguration.STREAM_CFG_AUTO, 1200);
        assertEquals(1200, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
    }

    @Test
    public void jumboFramesProbesAndUpgradesOnLanWithDefault1392AndFallsBackOnFailure() throws Exception {
        String jumboHostXml = "<root status_code=\"200\"><appversion>7.1.431.0</appversion>"
                + "<uniqueid>test-host</uniqueid><HttpsPort>47984</HttpsPort><state>SUNSHINE_SERVER_FREE</state>"
                + "<currentgame>0</currentgame><currentgameuuid></currentgameuuid><PairStatus>1</PairStatus>"
                + "<ServerCodecModeSupport>8388609</ServerCodecModeSupport>"
                + "<MaxVideoPacket>8192</MaxVideoPacket><PyroWaveHostLinkMbps>1000</PyroWaveHostLinkMbps>"
                + "<PyroWaveUdpProbeVersion>1</PyroWaveUdpProbeVersion>"
                + "<PyroWaveUdpHandshakeVersion>1</PyroWaveUdpHandshakeVersion>"
                + "<PyroWaveUdpProbeBurstVersion>1</PyroWaveUdpProbeBurstVersion>"
                + "<PyroWaveWireBudgetVersion>1</PyroWaveWireBudgetVersion></root>";
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit()
                .putBoolean("jumbo_frames", true).commit();
        setActiveNetwork(NetworkCapabilities.TRANSPORT_WIFI, true);

        // Auto + LAN + default 1392 + jumboFrames=true probes 8160 and upgrades to 8192.
        NvConnection autoPass = startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_AUTO, 1392, jumboHostXml);
        assertEquals(List.of(8160, 1392), FakeHost.probedPacketSizes);
        assertEquals(8192, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
        assertFalse(autoPass.context.jumboFallback);

        // Explicit STREAM_CFG_LOCAL + default 1392 + jumboFrames=true probes 8160 and upgrades to 8192.
        NvConnection localPass = startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_LOCAL, 1392, jumboHostXml);
        assertEquals(List.of(8160, 1392), FakeHost.probedPacketSizes);
        assertEquals(8192, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
        assertFalse(localPass.context.jumboFallback);

        // Probe failure on LAN skips the redundant 1392 control probe and falls back with jumboFallback=true.
        FakeHost.probeUdpFailure = new IOException("probe timeout");
        try {
            NvConnection failConn = startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_AUTO, 1392, jumboHostXml);
            assertEquals(List.of(8160), FakeHost.probedPacketSizes);
            assertEquals(1392, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
            assertTrue(failConn.context.jumboFallback);
        } finally {
            FakeHost.probeUdpFailure = null;
        }

        // Explicit STREAM_CFG_REMOTE skips jumbo probe and keeps 1392.
        NvConnection remoteConn = startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_REMOTE, 1392, jumboHostXml);
        assertTrue(FakeHost.probedPacketSizes.isEmpty());
        assertEquals(1392, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
        assertFalse(remoteConn.context.jumboFallback);

        // Auto + tunnel with jumboFrames=true skips jumbo probe and keeps 1024.
        NvConnection tunnelConn = startStream("127.78.0.1", StreamConfiguration.STREAM_CFG_AUTO, 1392, jumboHostXml);
        assertTrue(FakeHost.probedPacketSizes.isEmpty());
        assertEquals(1024, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
        assertFalse(tunnelConn.context.jumboFallback);
    }

    private static final String RUNNING_JUMBO_HOST_XML = "<root status_code=\"200\"><appversion>7.1.431.0</appversion>"
            + "<uniqueid>test-host</uniqueid><HttpsPort>47984</HttpsPort><state>SUNSHINE_SERVER_BUSY</state>"
            + "<currentgame>73</currentgame><currentgameuuid>game-a</currentgameuuid><PairStatus>1</PairStatus>"
            + "<ServerCodecModeSupport>8388609</ServerCodecModeSupport>"
            + "<MaxVideoPacket>8192</MaxVideoPacket><PyroWaveHostLinkMbps>1000</PyroWaveHostLinkMbps>"
            + "<PyroWaveUdpProbeVersion>1</PyroWaveUdpProbeVersion>"
            + "<PyroWaveUdpHandshakeVersion>1</PyroWaveUdpHandshakeVersion>"
            + "<PyroWaveUdpProbeBurstVersion>1</PyroWaveUdpProbeBurstVersion>"
            + "<PyroWaveWireBudgetVersion>1</PyroWaveWireBudgetVersion></root>";

    /** Makes a network of {@code type} the default one, on a LAN; the type is its netId here. */
    private static Network useNetwork(int type) {
        ConnectivityManager connMgr = (ConnectivityManager) RuntimeEnvironment.getApplication()
                .getSystemService(Context.CONNECTIVITY_SERVICE);
        ShadowConnectivityManager shadow = Shadows.shadowOf(connMgr);
        NetworkInfo info = ShadowNetworkInfo.newInstance(
                NetworkInfo.DetailedState.CONNECTED, type, 0, true, NetworkInfo.State.CONNECTED);
        shadow.addNetwork(ShadowNetwork.newInstance(type), info);
        shadow.setActiveNetworkInfo(info);
        setActiveNetwork(NetworkCapabilities.TRANSPORT_WIFI, true);
        Network active = connMgr.getActiveNetwork();
        assertNotEquals(0, active.getNetworkHandle());
        return active;
    }

    private static NvConnection resume(int packetSize, Network negotiatedOn) throws Exception {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit()
                .putBoolean("jumbo_frames", true).commit();
        NvConnection resumed = startStream("192.0.2.1", StreamConfiguration.STREAM_CFG_AUTO, 1392,
                RUNNING_JUMBO_HOST_XML, config -> config.setResumeOnly(true)
                        .setResumePacketSize(packetSize, negotiatedOn.getNetworkHandle()));
        assertEquals(List.of("resume"), FakeHost.requests);
        return resumed;
    }

    @Test
    public void aProbeTheBusyPcRefusesCountsAsNotProbed() throws Exception {
        Network wifi = useNetwork(ConnectivityManager.TYPE_WIFI);

        NvConnection resumed = resume(-1, wifi);
        assertEquals(List.of(8160), FakeHost.probedPacketSizes);
        assertEquals(1392, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
        assertFalse("no fallback notice", resumed.context.jumboFallback);
    }

    @Test
    public void aResumeOnTheSameNetworkKeepsTheRunningSessionsPacketSize() throws Exception {
        Network wifi = useNetwork(ConnectivityManager.TYPE_WIFI);

        NvConnection resumed = resume(8192, wifi);
        assertTrue(FakeHost.probedPacketSizes.isEmpty());
        assertEquals(8192, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
        assertEquals(8192, resumed.resumePacketSize());
        assertEquals(wifi.getNetworkHandle(), resumed.resumePacketNetwork());

        Runnable moved = () -> resumed.networkMoved = true;
        RoamingNetworkWatch.noting(metered -> MoonBridge.NETWORK_CARRIES_ON, moved).networkChanged(false);
        assertEquals(8192, resumed.resumePacketSize());
        RoamingNetworkWatch.noting(metered -> MoonBridge.NETWORK_CANNOT_CONTINUE, moved).networkChanged(false);
        assertEquals(0, resumed.resumePacketSize());

        // A start that never got as far as negotiating hands on what it was given.
        NvConnection unstarted = new NvConnection(RuntimeEnvironment.getApplication(),
                new ComputerDetails.AddressTuple("192.0.2.1", 47989), 47984, "device",
                new StreamConfiguration.Builder().setResumePacketSize(8192, 42).build(), crypto(), null);
        assertEquals(8192, unstarted.resumePacketSize());
        assertEquals(42, unstarted.resumePacketNetwork());
    }

    @Test
    public void afterTheStreamsNetworkIsLostAResumeOnAnotherCarriesNoPacketSize() throws Exception {
        Network ethernet = useNetwork(ConnectivityManager.TYPE_ETHERNET);
        NvConnection docked = resume(8192, ethernet);
        assertEquals(8192, docked.resumePacketSize());

        ConnectivityManager connMgr = (ConnectivityManager) RuntimeEnvironment.getApplication()
                .getSystemService(Context.CONNECTIVITY_SERVICE);
        NvConnectionListener listener = (NvConnectionListener) Proxy.newProxyInstance(
                NvConnectionListener.class.getClassLoader(), new Class<?>[] {NvConnectionListener.class},
                (proxy, method, args) -> null);
        RoamingNetworkWatch watch = new RoamingNetworkWatch(connMgr, ethernet, listener,
                metered -> MoonBridge.NETWORK_CARRIES_ON, () -> docked.networkMoved = true);
        watch.callback.onLost(ethernet);
        watch.stop();
        assertEquals(0, docked.resumePacketSize());

        // Even a size handed on before the loss was reported stays with the network that negotiated it.
        useNetwork(ConnectivityManager.TYPE_WIFI);
        NvConnection undocked = resume(8192, ethernet);
        assertEquals(List.of(8160), FakeHost.probedPacketSizes);
        assertEquals(1392, NvConnectionHapticsFlagTest.CapturingBridge.packetSize);
        assertEquals(1392, undocked.resumePacketSize());
        assertNotEquals(ethernet.getNetworkHandle(), undocked.resumePacketNetwork());
    }

    @Test
    public void jniResolvePacketSizePreflightDistinguishesAutoVpnFromExplicitRemote() {
        List<Integer> probedSizes = new ArrayList<>();
        NightjarSessionJni.LinkProbe passingProbe = new NightjarSessionJni.LinkProbe() {
            @Override public String probeLinkMbps() { return "{\"error\":{\"kind\":\"cancelled\",\"message\":\"\"}}"; }
            @Override public String probePace(int kbps, int size, int fps, int pace) {
                probedSizes.add(size);
                return "{\"ok\":{\"expected\":100,\"sent\":100,\"received\":100,\"sender_ms\":2000.0,"
                        + "\"loss\":0.0,\"worst_window_loss\":0.0,\"frames\":120,\"damaged_frames\":0,"
                        + "\"excessive_loss_frames\":0,\"missing_tail_frames\":0,\"worst_frame_loss\":0.0}}";
            }
        };
        NightjarSessionJni.LinkProbe failingProbe = new NightjarSessionJni.LinkProbe() {
            @Override public String probeLinkMbps() { return "{\"error\":{\"kind\":\"cancelled\",\"message\":\"\"}}"; }
            @Override public String probePace(int kbps, int size, int fps, int pace) {
                probedSizes.add(size);
                return "{\"error\":{\"kind\":\"io\",\"message\":\"probe failed\"}}";
            }
        };

        int[] autoVpn = NightjarSessionJni.resolvePacketSizePreflight(
                0, false, true, 8192, true, 1392, true, false, 0, passingProbe);
        assertArrayEquals(new int[] {1024, 0}, autoVpn);
        assertTrue(probedSizes.isEmpty());

        int[] explicitRemote = NightjarSessionJni.resolvePacketSizePreflight(
                2, false, true, 8192, true, 1392, true, false, 0, passingProbe);
        assertArrayEquals(new int[] {1392, 0}, explicitRemote);
        assertTrue(probedSizes.isEmpty());

        int[] autoLanPass = NightjarSessionJni.resolvePacketSizePreflight(
                0, true, false, 8192, true, 1392, true, false, 0, passingProbe);
        assertArrayEquals(new int[] {8192, 0}, autoLanPass);
        assertEquals(List.of(8160, 1392), probedSizes);

        probedSizes.clear();
        int[] localLanPass = NightjarSessionJni.resolvePacketSizePreflight(
                1, true, false, 8192, true, 1392, true, false, 0, passingProbe);
        assertArrayEquals(new int[] {8192, 0}, localLanPass);
        assertEquals(List.of(8160, 1392), probedSizes);

        probedSizes.clear();
        int[] autoLanFail = NightjarSessionJni.resolvePacketSizePreflight(
                0, true, false, 8192, true, 1392, true, false, 0, failingProbe);
        assertArrayEquals(new int[] {1392, 1}, autoLanFail);
        assertEquals(List.of(8160), probedSizes);
    }

    @Implements(value = NvHTTP.class, isInAndroidSdk = false)
    public static class FakeHost {
        static String initial;
        static IOException resumeFailure, probeUdpFailure;
        static PyroWavePace.Result probeUdpResult;
        static boolean resumeResult;
        static final List<String> requests = new ArrayList<>();
        static final List<Integer> probedPacketSizes = new ArrayList<>();

        @Implementation public String getServerInfo(boolean likelyOnline) { return initial; }
        @Implementation public NvApp getAppByUuid(String uuid) {
            NvApp app = new NvApp("Game", 73, false);
            app.setAppUuid(uuid);
            return app;
        }
        @Implementation public PyroWavePace.Result probePyroWaveUdp(int kbps, int packetSize, int fps, int paceKbps)
                throws IOException {
            probedPacketSizes.add(packetSize);
            // As the real host while it holds a session (nvhttp/probe.rs).
            if (initial.contains("SUNSHINE_SERVER_BUSY")) {
                throw new HostHttpResponseException(400, "Stop streaming before testing bandwidth");
            }
            if (probeUdpFailure != null) throw probeUdpFailure;
            if (probeUdpResult != null) return probeUdpResult;
            PyroWavePace.Result r = new PyroWavePace.Result();
            r.expected = 100; r.sent = 100; r.received = 100; r.senderMs = 2000.0;
            r.lossPercent = 0.0; r.worstWindowLossPercent = 0.0; r.frames = 120;
            r.damagedFrames = 0; r.excessiveLossFrames = 0; r.missingTailFrames = 0; r.worstFrameLossPercent = 0.0;
            return r;
        }
        @Implementation public boolean launchApp(ConnectionContext context, String verb, int id, boolean hdr)
                throws IOException {
            requests.add(verb);
            if (resumeFailure != null) throw resumeFailure;
            return resumeResult;
        }
        @Implementation public boolean quitApp() { return true; }
    }

    @Test
    public void xmlSpecialCharactersInCurrentGameUuidDoNotAbortJniLaunchOrServerInfoCheck() {
        NightjarSessionJni.HostCallback noop = new NightjarSessionJni.HostCallback() {
            @Override public String serverInfo(boolean fast) { return "{\"error\":{\"kind\":\"cancelled\",\"message\":\"\"}}"; }
            @Override public boolean hasEndedSession() { return false; }
            @Override public String launch(String verb, int appId, boolean hdr, boolean confirmReplace) {
                return "{\"ok\":{\"started\":false}}";
            }
            @Override public String quitApp() { return "{\"ok\":false}"; }
            @Override public int confirmReplace(boolean terminateOnly) { return 3; }
            @Override public boolean isStopped() { return false; }
        };
        for (String uuid : new String[] {"bad<uuid", "a&b"}) {
            String reply = NightjarSessionJni.launchOrResume(
                    "{\"paired\":true,\"current_game\":1,\"current_game_uuid\":\"" + uuid + "\",\"session_idle\":false}",
                    "{\"app_id\":73,\"app_uuid\":\"game-a\",\"app_name\":\"Game\",\"control\":0,\"role\":0}",
                    "{}",
                    noop);
            assertNotNull(reply);
            assertFalse(NightjarSessionJni.serverinfoConfirmsEndedSession(
                    false, 0, false, true, true, true, 0, uuid));
        }
    }

    private static VideoDecoderRenderer renderer() {
        return new VideoDecoderRenderer() {
            @Override public int setup(int format, int width, int height, int rate) { return -1; }
            @Override public void start() {}
            @Override public void stop() {}
            @Override public void cleanup() {}
            @Override public int getCapabilities() { return 0; }
            @Override public void setHdrMode(boolean enabled, byte[] metadata) {}
            @Override public int submitDecodeUnit(byte[] data, int length, int type, int frame, int frameType,
                    char latency, long received, long queued, long receivedNs, long presentationUs) { return 0; }
        };
    }

    private static LimelightCryptoProvider crypto() {
        return new LimelightCryptoProvider() {
            @Override public java.security.cert.X509Certificate getClientCertificate() { return null; }
            @Override public java.security.PrivateKey getClientPrivateKey() { return null; }
            @Override public byte[] getPemEncodedClientCertificate() { return new byte[0]; }
            @Override public String encodeBase64String(byte[] data) { return ""; }
        };
    }
}
