package com.limelight.nvstream;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.LinkAddress;
import android.net.LinkProperties;
import android.net.Network;
import android.net.NetworkCapabilities;
import android.os.Build;

import com.limelight.LimeLog;
import com.limelight.nvstream.jni.HandleReaper;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarPolicyJni;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * While a stream runs, follows the default network via {@link ConnectivityManager.NetworkCallback}
 * and delegates tracking state transitions to {@code nightjar_policy::roaming::RoamingTracker} via
 * {@link NightjarPolicyJni}.
 */
final class RoamingNetworkWatch {
    interface EngineProbe {
        int networkChanged(boolean metered);
    }

    private final ConnectivityManager connMgr;
    private final long trackerHandle;
    private final HandleReaper.Token reaper;
    final ConnectivityManager.NetworkCallback callback;
    private final EngineProbe engineProbe;
    private boolean stopped;

    /** {@code moved} runs when the stream's network is lost or its route moves. */
    RoamingNetworkWatch(ConnectivityManager connMgr, Network streamNetwork,
                        NvConnectionListener listener, EngineProbe engineProbe, Runnable moved) {
        this.connMgr = connMgr;
        this.engineProbe = noting(engineProbe, moved);
        this.trackerHandle = NightjarPolicyJni.roamingCreate(
                streamNetwork != null, streamNetwork != null ? streamNetwork.getNetworkHandle() : 0L);
        this.reaper = HandleReaper.register(this, trackerHandle, NightjarPolicyJni::roamingDestroy);
        callback = new ConnectivityManager.NetworkCallback() {
            @Override
            public void onAvailable(Network network) {
                if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
                    NetworkCapabilities caps = connMgr.getNetworkCapabilities(network);
                    if (caps != null) {
                        handleCapabilities(network, caps);
                    }
                }
            }

            @Override
            public void onCapabilitiesChanged(Network network, NetworkCapabilities caps) {
                handleCapabilities(network, caps);
            }

            @Override
            public void onLinkPropertiesChanged(Network network, LinkProperties properties) {
                handleAddresses(network, properties);
            }

            @Override
            public void onLost(Network network) {
                synchronized (RoamingNetworkWatch.this) {
                    if (stopped) {
                        return;
                    }
                    if (NightjarPolicyJni.roamingOnLost(trackerHandle, network.getNetworkHandle())) {
                        LimeLog.warning("The stream's network was lost");
                        moved.run();
                        // Under the lock, so stop() returning means no report is still on its way.
                        listener.streamNetworkLost();
                    }
                }
            }

            private void handleCapabilities(Network network, NetworkCapabilities caps) {
                long networkId = network.getNetworkHandle();
                boolean notMetered = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED);
                synchronized (RoamingNetworkWatch.this) {
                    if (stopped) {
                        return;
                    }
                    int step = NightjarPolicyJni.roamingOnCapabilities(trackerHandle, networkId, notMetered);
                    if (step == NightjarPolicyJni.ROAMING_CAP_IGNORE) {
                        return;
                    }
                    boolean metered = step == NightjarPolicyJni.ROAMING_CAP_ASK_METERED
                            || step == NightjarPolicyJni.ROAMING_CAP_RETURNED_ASK_METERED;
                    boolean returned = step == NightjarPolicyJni.ROAMING_CAP_RETURNED_ASK_NOT_METERED
                            || step == NightjarPolicyJni.ROAMING_CAP_RETURNED_ASK_METERED;
                    // Under the lock, so stop() returning means no report is still on its way.
                    int change = engineProbe.networkChanged(metered);
                    if (NightjarPolicyJni.roamingRecordChange(
                            trackerHandle, networkId, metered, returned, change, System.nanoTime() / 1_000_000L)) {
                        listener.streamNetworkReturned(change, metered);
                    }
                }
            }

            private void handleAddresses(Network network, LinkProperties properties) {
                long networkId = network.getNetworkHandle();
                String addressesKey = canonicalAddresses(properties);
                boolean nat64 = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && properties.getNat64Prefix() != null;
                long nowMs = System.nanoTime() / 1_000_000L;
                synchronized (RoamingNetworkWatch.this) {
                    if (stopped) {
                        return;
                    }
                    int step = NightjarPolicyJni.roamingOnAddresses(trackerHandle, networkId, addressesKey, nat64, nowMs);
                    if (step == NightjarPolicyJni.ROAMING_ADDR_IGNORE) {
                        return;
                    }
                    boolean metered = step == NightjarPolicyJni.ROAMING_ADDR_ASK_METERED;
                    // Under the lock, so stop() returning means no report is still on its way.
                    int change = engineProbe.networkChanged(metered);
                    if (NightjarPolicyJni.roamingRecordAddressChange(trackerHandle, change)) {
                        listener.streamNetworkReturned(change, metered);
                    }
                }
            }
        };
    }

    private static int askEngine(boolean metered) {
        int change = MoonBridge.networkChanged(metered);
        LimeLog.info("Roaming: the default network changed (" + (metered ? "metered" : "not metered") +
                "), " + (change == MoonBridge.NETWORK_ROAMED ? "roaming" :
                change == MoonBridge.NETWORK_CANNOT_CONTINUE ? "not roaming, the connection cannot carry on" :
                "not roaming, the connection carries on"));
        return change;
    }

    private static String canonicalAddresses(LinkProperties properties) {
        List<String> list = new ArrayList<>();
        for (LinkAddress address : properties.getLinkAddresses()) {
            if (address.getAddress() != null) {
                String text = address.getAddress().getHostAddress();
                if (text != null && !list.contains(text)) {
                    list.add(text);
                }
            }
        }
        Collections.sort(list);
        return String.join(" ", list);
    }

    static Network defaultNetwork(Context context) {
        ConnectivityManager connMgr = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        return connMgr == null ? null : connMgr.getActiveNetwork();
    }

    /** {@code probe}, running {@code moved} for every change that moved the stream's route, lost first or not. */
    static EngineProbe noting(EngineProbe probe, Runnable moved) {
        return metered -> {
            int change = probe.networkChanged(metered);
            if (change != MoonBridge.NETWORK_CARRIES_ON) moved.run();
            return change;
        };
    }

    static RoamingNetworkWatch start(Context context, Network streamNetwork, NvConnectionListener listener,
                                     Runnable moved) {
        ConnectivityManager connMgr = (ConnectivityManager) context.getSystemService(Context.CONNECTIVITY_SERVICE);
        if (connMgr == null) {
            LimeLog.warning("Roaming: no ConnectivityManager; not roaming");
            return null;
        }
        RoamingNetworkWatch watch = new RoamingNetworkWatch(connMgr, streamNetwork, listener,
                RoamingNetworkWatch::askEngine, moved);
        try {
            connMgr.registerDefaultNetworkCallback(watch.callback);
        } catch (RuntimeException e) {
            LimeLog.warning("Roaming: could not follow network changes, not roaming: " + e);
            synchronized (watch) {
                watch.stopped = true;
                NightjarPolicyJni.roamingStop(watch.trackerHandle);
                watch.reaper.close();
            }
            return null;
        }
        return watch;
    }

    synchronized void stop() {
        if (stopped) {
            return;
        }
        stopped = true;
        NightjarPolicyJni.roamingStop(trackerHandle);
        try {
            connMgr.unregisterNetworkCallback(callback);
        } catch (RuntimeException e) {
            LimeLog.warning("Roaming: could not stop following network changes: " + e);
        } finally {
            reaper.close();
        }
    }
}
