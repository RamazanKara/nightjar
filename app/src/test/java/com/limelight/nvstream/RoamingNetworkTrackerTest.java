package com.limelight.nvstream;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.content.Context;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;

import java.net.InetAddress;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.LongSupplier;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowNetwork;
import org.robolectric.shadows.ShadowNetworkCapabilities;
import org.robolectric.util.ReflectionHelpers;
import org.robolectric.util.ReflectionHelpers.ClassParameter;

import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarPolicyJni;

/** Client roaming: verifies {@code nightjar_policy::roaming::RoamingTracker} and {@link RoamingNetworkWatch} across JNI. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class RoamingNetworkTrackerTest {
    private static final long WIFI = 1L;
    private static final long OTHER_WIFI = 2L;
    private static final long CELLULAR = 3L;

    interface Reporter {
        int networkChanged(boolean metered);
        default void streamNetworkLost() {}
        default void streamNetworkReturned(int change, boolean metered) {}
    }

    private static final class JniTracker implements AutoCloseable {
        private final long handle;
        private final Reporter reporter;
        private final LongSupplier clock;

        JniTracker(Long streamNetwork, Reporter reporter) {
            this(streamNetwork, reporter, () -> System.nanoTime() / 1_000_000L);
        }

        JniTracker(Long streamNetwork, Reporter reporter, LongSupplier clock) {
            this.handle = NightjarPolicyJni.roamingCreate(streamNetwork != null, streamNetwork != null ? streamNetwork : 0L);
            this.reporter = reporter;
            this.clock = clock;
        }

        void capabilities(long network, boolean notMetered) {
            int step = NightjarPolicyJni.roamingOnCapabilities(handle, network, notMetered);
            if (step == NightjarPolicyJni.ROAMING_CAP_IGNORE) return;
            boolean metered = step == NightjarPolicyJni.ROAMING_CAP_ASK_METERED
                    || step == NightjarPolicyJni.ROAMING_CAP_RETURNED_ASK_METERED;
            boolean returned = step == NightjarPolicyJni.ROAMING_CAP_RETURNED_ASK_NOT_METERED
                    || step == NightjarPolicyJni.ROAMING_CAP_RETURNED_ASK_METERED;
            int change = reporter.networkChanged(metered);
            if (NightjarPolicyJni.roamingRecordChange(handle, network, metered, returned, change, clock.getAsLong())) {
                reporter.streamNetworkReturned(change, metered);
            }
        }

        void addresses(long network, String addresses) {
            addresses(network, addresses, false);
        }

        void addresses(long network, String addresses, boolean nat64) {
            int step = NightjarPolicyJni.roamingOnAddresses(handle, network, addresses, nat64, clock.getAsLong());
            if (step == NightjarPolicyJni.ROAMING_ADDR_IGNORE) return;
            boolean metered = step == NightjarPolicyJni.ROAMING_ADDR_ASK_METERED;
            int change = reporter.networkChanged(metered);
            if (NightjarPolicyJni.roamingRecordAddressChange(handle, change)) {
                reporter.streamNetworkReturned(change, metered);
            }
        }

        void lost(long network) {
            if (NightjarPolicyJni.roamingOnLost(handle, network)) {
                reporter.streamNetworkLost();
            }
        }

        void stop() {
            NightjarPolicyJni.roamingStop(handle);
        }

        @Override public void close() {
            NightjarPolicyJni.roamingDestroy(handle);
        }
    }

    private static class Events implements Reporter {
        final List<String> seen = new ArrayList<>();
        int answer = MoonBridge.NETWORK_CARRIES_ON;

        @Override public int networkChanged(boolean metered) {
            seen.add(metered ? "changed metered" : "changed");
            return answer;
        }
        @Override public void streamNetworkLost() { seen.add("lost"); }
        @Override public void streamNetworkReturned(int change, boolean metered) {
            seen.add("returned " + change + (metered ? " metered" : ""));
        }
    }

    @Test public void theStreamsOwnNetworkAndItsCapabilityUpdatesAreNoChange() {
        List<Boolean> reports = new ArrayList<>();
        try (JniTracker tracker = new JniTracker(WIFI, m -> { reports.add(m); return MoonBridge.NETWORK_CARRIES_ON; })) {
            tracker.capabilities(WIFI, true);
            tracker.capabilities(WIFI, true);
            tracker.capabilities(WIFI, false);
            assertEquals(new ArrayList<Boolean>(), reports);
        }
    }

    @Test public void aNewDefaultNetworkIsReportedOnceWithWhetherItIsMetered() {
        List<Boolean> reports = new ArrayList<>();
        try (JniTracker tracker = new JniTracker(WIFI, m -> { reports.add(m); return MoonBridge.NETWORK_CARRIES_ON; })) {
            tracker.capabilities(CELLULAR, false);
            tracker.capabilities(CELLULAR, false);
            tracker.capabilities(OTHER_WIFI, true);
            assertEquals(java.util.Arrays.asList(true, false), reports);
        }
    }

    @Test public void theSameNetworkComingBackAfterItWasLostIsAChange() {
        List<Boolean> reports = new ArrayList<>();
        try (JniTracker tracker = new JniTracker(WIFI, m -> { reports.add(m); return MoonBridge.NETWORK_CARRIES_ON; })) {
            tracker.lost(WIFI);
            tracker.capabilities(WIFI, true);
            tracker.capabilities(WIFI, true);
            assertEquals(java.util.Arrays.asList(false), reports);
            tracker.lost(CELLULAR);
            tracker.capabilities(WIFI, true);
            assertEquals(java.util.Arrays.asList(false), reports);
        }
    }

    @Test public void withoutTheStartNetworkTheFirstOneSeenIsTheBaseline() {
        List<Boolean> reports = new ArrayList<>();
        try (JniTracker tracker = new JniTracker(null, m -> { reports.add(m); return MoonBridge.NETWORK_CARRIES_ON; })) {
            tracker.capabilities(WIFI, true);
            assertEquals(new ArrayList<Boolean>(), reports);
            tracker.capabilities(CELLULAR, false);
            assertEquals(java.util.Arrays.asList(true), reports);
        }
    }

    @Test public void aLostNetworkComingBackIsAReturnWithTheEnginesAnswer() {
        Events events = new Events();
        try (JniTracker tracker = new JniTracker(WIFI, events)) {
            tracker.lost(WIFI);
            tracker.lost(WIFI);
            tracker.capabilities(OTHER_WIFI, true);
            tracker.capabilities(OTHER_WIFI, true);
            assertEquals(java.util.Arrays.asList("lost", "changed", "returned " + MoonBridge.NETWORK_CARRIES_ON), events.seen);

            events.seen.clear();
            events.answer = MoonBridge.NETWORK_CANNOT_CONTINUE;
            tracker.lost(OTHER_WIFI);
            tracker.capabilities(CELLULAR, false);
            assertEquals(java.util.Arrays.asList("lost", "changed metered",
                    "returned " + MoonBridge.NETWORK_CANNOT_CONTINUE + " metered"), events.seen);
        }
    }

    @Test public void aReturnWithoutARouteYetIsAskedAgainAsItsAddressesArrive() {
        Events events = new Events();
        long[] now = {1000};
        try (JniTracker tracker = new JniTracker(WIFI, events, () -> now[0])) {
            tracker.lost(WIFI);
            tracker.capabilities(OTHER_WIFI, true);
            tracker.addresses(OTHER_WIFI, "fe80::1");
            tracker.addresses(OTHER_WIFI, "fe80::1");
            tracker.addresses(CELLULAR, "10.0.0.5");
            now[0] += 7000;
            events.answer = MoonBridge.NETWORK_CANNOT_CONTINUE;
            tracker.addresses(OTHER_WIFI, "fe80::1 192.168.1.21");
            tracker.addresses(OTHER_WIFI, "fe80::1 192.168.1.21 2001:db8::21");
            assertEquals(java.util.Arrays.asList("lost", "changed", "returned " + MoonBridge.NETWORK_CARRIES_ON,
                    "changed", "changed", "returned " + MoonBridge.NETWORK_CANNOT_CONTINUE), events.seen);
        }
    }

    @Test public void aReturnIsAskedAgainOnlyUntilTheConnectionsOwnTimeout() {
        Events events = new Events();
        long[] now = {1000};
        try (JniTracker tracker = new JniTracker(WIFI, events, () -> now[0])) {
            tracker.lost(WIFI);
            tracker.capabilities(OTHER_WIFI, false);
            events.seen.clear();
            tracker.capabilities(OTHER_WIFI, true);
            tracker.capabilities(OTHER_WIFI, false);
            tracker.addresses(OTHER_WIFI, "fe80::1");
            assertEquals(java.util.Arrays.asList("changed metered"), events.seen);
            now[0] += NightjarPolicyJni.recoveryPeerTimeoutMs() + 1;
            events.answer = MoonBridge.NETWORK_CANNOT_CONTINUE;
            tracker.addresses(OTHER_WIFI, "fe80::1 192.168.1.21");
            assertEquals(java.util.Arrays.asList("changed metered"), events.seen);
        }
    }

    @Test public void onlyAReturnTheConnectionCarriedOnOverIsAskedAgain() {
        Events events = new Events();
        try (JniTracker tracker = new JniTracker(WIFI, events, () -> 0)) {
            tracker.addresses(WIFI, "192.168.1.20");
            events.answer = MoonBridge.NETWORK_ROAMED;
            tracker.lost(WIFI);
            tracker.capabilities(OTHER_WIFI, true);
            events.seen.clear();
            tracker.addresses(OTHER_WIFI, "192.168.1.21");
            assertEquals(new ArrayList<String>(), events.seen);
            events.answer = MoonBridge.NETWORK_CANNOT_CONTINUE;
            tracker.lost(OTHER_WIFI);
            tracker.capabilities(CELLULAR, true);
            events.seen.clear();
            tracker.addresses(CELLULAR, "10.0.0.5");
            assertEquals(new ArrayList<String>(), events.seen);
            events.answer = MoonBridge.NETWORK_CARRIES_ON;
            tracker.lost(CELLULAR);
            tracker.capabilities(WIFI, true);
            tracker.lost(WIFI);
            events.seen.clear();
            tracker.addresses(WIFI, "192.168.1.22");
            tracker.capabilities(WIFI, true);
            tracker.stop();
            tracker.addresses(WIFI, "192.168.1.23");
            assertEquals(java.util.Arrays.asList("changed", "returned " + MoonBridge.NETWORK_CARRIES_ON), events.seen);
        }
    }

    @Test public void aNat64PrefixFoundAfterTheAddressesAsksAgain() {
        Events events = new Events();
        try (JniTracker tracker = new JniTracker(WIFI, events, () -> 0)) {
            tracker.lost(WIFI);
            tracker.capabilities(OTHER_WIFI, true);
            events.seen.clear();
            tracker.addresses(OTHER_WIFI, "fe80::1", false);
            tracker.addresses(OTHER_WIFI, "fe80::1", false);
            tracker.addresses(OTHER_WIFI, "fe80::1", true);
            assertEquals(java.util.Arrays.asList("changed", "changed"), events.seen);
        }
    }

    @Test public void anEngineThatTellsKeepsItsOwnAnswer() {
        Events events = new Events();
        try (JniTracker tracker = new JniTracker(WIFI, events, () -> 0)) {
            tracker.addresses(WIFI, "192.168.1.20");
            tracker.lost(WIFI);
            tracker.capabilities(CELLULAR, false);
            tracker.addresses(CELLULAR, "10.0.0.5");
            assertEquals(java.util.Arrays.asList("lost", "changed metered",
                    "returned " + MoonBridge.NETWORK_CARRIES_ON + " metered", "changed metered"), events.seen);
        }
    }

    @Test public void aNewNetworkWhileTheStreamsOwnStaysIsNoReturn() {
        Events events = new Events();
        try (JniTracker tracker = new JniTracker(WIFI, events)) {
            tracker.capabilities(OTHER_WIFI, true);
            tracker.lost(WIFI);
            assertEquals(java.util.Arrays.asList("changed"), events.seen);
        }
    }

    @Test public void nothingIsReportedAfterTheStreamEnds() {
        List<Boolean> reports = new ArrayList<>();
        try (JniTracker tracker = new JniTracker(WIFI, m -> { reports.add(m); return MoonBridge.NETWORK_CARRIES_ON; })) {
            tracker.stop();
            tracker.capabilities(CELLULAR, false);
            tracker.lost(WIFI);
            tracker.capabilities(WIFI, true);
            assertEquals(new ArrayList<Boolean>(), reports);
        }
        Events events = new Events();
        try (JniTracker ended = new JniTracker(WIFI, events)) {
            ended.stop();
            ended.lost(WIFI);
            ended.capabilities(OTHER_WIFI, true);
            assertEquals(new ArrayList<String>(), events.seen);
        }
    }

    @Test public void negativeMonotonicClockExpiresTenSecondRecheck() {
        Events events = new Events();
        long[] now = {-50_000L};
        try (JniTracker tracker = new JniTracker(WIFI, events, () -> now[0])) {
            tracker.lost(WIFI);
            tracker.capabilities(OTHER_WIFI, true);
            events.seen.clear();
            events.answer = MoonBridge.NETWORK_CANNOT_CONTINUE;
            now[0] = -30_000L;
            tracker.addresses(OTHER_WIFI, "192.168.1.21");
            assertEquals(new ArrayList<String>(), events.seen);
        }
    }

    @Test public void crossingZeroMonotonicClockExpiresTenSecondRecheck() {
        Events events = new Events();
        long[] now = {-5_000L};
        try (JniTracker tracker = new JniTracker(WIFI, events, () -> now[0])) {
            tracker.lost(WIFI);
            tracker.capabilities(OTHER_WIFI, true);
            events.seen.clear();
            events.answer = MoonBridge.NETWORK_CANNOT_CONTINUE;
            now[0] = 6_000L;
            tracker.addresses(OTHER_WIFI, "192.168.1.21");
            assertEquals(new ArrayList<String>(), events.seen);
        }
    }

    private static NetworkCapabilities notMeteredCaps() {
        NetworkCapabilities caps = ShadowNetworkCapabilities.newInstance();
        Shadows.shadowOf(caps).addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
        return caps;
    }

    private static LinkProperties ipv4Props(String ip) throws Exception {
        LinkProperties props = new LinkProperties();
        LinkAddress addr = ReflectionHelpers.callConstructor(
                LinkAddress.class,
                ClassParameter.from(InetAddress.class, InetAddress.getByName(ip)),
                ClassParameter.from(int.class, 24));
        ReflectionHelpers.callInstanceMethod(props, "addLinkAddress",
                ClassParameter.from(LinkAddress.class, addr));
        return props;
    }

    private static NvConnectionListener watchListener(Runnable onLost, Runnable onReturned) {
        return (NvConnectionListener) java.lang.reflect.Proxy.newProxyInstance(
                NvConnectionListener.class.getClassLoader(),
                new Class<?>[] {NvConnectionListener.class},
                (proxy, method, args) -> {
                    if ("streamNetworkLost".equals(method.getName()) && onLost != null) {
                        onLost.run();
                    } else if ("streamNetworkReturned".equals(method.getName()) && onReturned != null) {
                        onReturned.run();
                    }
                    return null;
                });
    }

    @Test public void stopBlocksUntilInFlightCallbackCompletesAndDeliversNothingAfterStopReturns() throws Exception {
        Context context = org.robolectric.RuntimeEnvironment.getApplication();
        ConnectivityManager connMgr = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        Network wifi = ShadowNetwork.newInstance(1);
        Network otherWifi = ShadowNetwork.newInstance(2);
        NetworkCapabilities caps = notMeteredCaps();
        LinkProperties props = ipv4Props("192.168.1.21");

        // Deterministic verification: stop() must wait for an in-flight callback holding the lock.
        CountDownLatch inProbe = new CountDownLatch(1);
        CountDownLatch releaseProbe = new CountDownLatch(1);
        AtomicBoolean returnedBeforeStopFinished = new AtomicBoolean();
        AtomicBoolean stopReturned = new AtomicBoolean();
        RoamingNetworkWatch lockedWatch = new RoamingNetworkWatch(connMgr, wifi,
                watchListener(null, () -> returnedBeforeStopFinished.set(!stopReturned.get())),
                metered -> {
                    inProbe.countDown();
                    try {
                        releaseProbe.await(5, TimeUnit.SECONDS);
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                    }
                    return MoonBridge.NETWORK_CARRIES_ON;
                }, () -> {});
        lockedWatch.callback.onLost(wifi);
        Thread cbThread = new Thread(() -> lockedWatch.callback.onCapabilitiesChanged(otherWifi, caps));
        cbThread.start();
        assertTrue(inProbe.await(5, TimeUnit.SECONDS));
        Thread stopThread = new Thread(() -> {
            lockedWatch.stop();
            stopReturned.set(true);
        });
        stopThread.start();
        // Give stopThread a brief moment to block on synchronized (RoamingNetworkWatch.this), then release.
        Thread.sleep(30);
        assertFalse("stop() must not return while callback holds lock", stopReturned.get());
        releaseProbe.countDown();
        cbThread.join(5000);
        stopThread.join(5000);
        assertTrue(stopReturned.get());
        assertTrue(returnedBeforeStopFinished.get());

        // Concurrent race sweep across onLost, onCapabilitiesChanged, and onLinkPropertiesChanged vs stop().
        AtomicInteger lostAfterStop = new AtomicInteger();
        AtomicInteger returnedAfterStop = new AtomicInteger();
        for (int i = 0; i < 300; i++) {
            AtomicBoolean stopped = new AtomicBoolean();
            int iter = i;
            AtomicInteger probeCalls = new AtomicInteger();
            RoamingNetworkWatch watch = new RoamingNetworkWatch(connMgr, wifi,
                    watchListener(
                            () -> { if (stopped.get()) lostAfterStop.incrementAndGet(); },
                            () -> { if (stopped.get()) returnedAfterStop.incrementAndGet(); }),
                    metered -> (iter % 3 == 2 && probeCalls.getAndIncrement() > 0)
                            ? MoonBridge.NETWORK_CANNOT_CONTINUE
                            : MoonBridge.NETWORK_CARRIES_ON, () -> {});
            if (iter % 3 == 2) {
                watch.callback.onLost(wifi);
                watch.callback.onCapabilitiesChanged(otherWifi, caps);
            }
            CountDownLatch ready = new CountDownLatch(2);
            CountDownLatch go = new CountDownLatch(1);
            Thread worker = new Thread(() -> {
                ready.countDown();
                try {
                    go.await(5, TimeUnit.SECONDS);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
                if (iter % 3 == 0) {
                    watch.callback.onLost(wifi);
                } else if (iter % 3 == 1) {
                    watch.callback.onLost(wifi);
                    watch.callback.onCapabilitiesChanged(otherWifi, caps);
                } else {
                    watch.callback.onLinkPropertiesChanged(otherWifi, props);
                }
            });
            worker.start();
            ready.countDown();
            assertTrue(ready.await(5, TimeUnit.SECONDS));
            go.countDown();
            watch.stop();
            stopped.set(true);
            worker.join(5000);
        }
        assertEquals(0, lostAfterStop.get());
        assertEquals(0, returnedAfterStop.get());
    }
}
