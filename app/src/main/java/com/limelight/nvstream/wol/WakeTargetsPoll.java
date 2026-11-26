package com.limelight.nvstream.wol;

import com.limelight.LimeLog;
import com.limelight.nativeapi.NativeApiClient;
import com.limelight.nativeapi.KnownCapabilities;
import com.limelight.nativeapi.NativeApiException;
import com.limelight.nativeapi.NativeCapabilities;
import com.limelight.nativeapi.NativePcPower;
import com.limelight.nativeapi.NativeWake;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.nvstream.jni.HandleReaper;
import com.limelight.nvstream.jni.NightjarDiscoveryJni;

import org.json.JSONException;

import java.util.concurrent.Executor;
import java.util.concurrent.RejectedExecutionException;
import java.util.function.LongSupplier;

/**
 * Keeps the wake targets of the PCs the computer manager polls, delegating reservation and
 * 30-second cooldown state to Rust {@code WakeTargetsPollState} via {@link NightjarDiscoveryJni}. With the same ask it
 * keeps the PC's away wake keys ({@link WakeRelayKeys}) while it offers them.
 */
public final class WakeTargetsPoll implements AutoCloseable {
    static final long INTERVAL_MS = 30_000;

    public interface Clients {
        /** A client for the PC's native API, or null when it has no address or pinned certificate yet. */
        NativeApiClient open(ComputerDetails computer);
    }

    private final long handle = NightjarDiscoveryJni.wakePollCreate();
    private final HandleReaper.Token handleToken =
            HandleReaper.register(this, handle, NightjarDiscoveryJni::wakePollDestroy);
    private final WakeTargets targets;
    /** Null: away wake keys are not kept. */
    private final WakeRelayKeys relayKeys;
    private final Clients clients;
    private final Executor requests;
    private final LongSupplier clock;
    private final Object storeLock = new Object();

    /** {@code requests} runs the HTTPS requests, so a poll never waits on them. */
    public WakeTargetsPoll(WakeTargets targets, Clients clients, Executor requests, LongSupplier clock) {
        this(targets, null, clients, requests, clock);
    }

    /** Also keeps each PC's away wake keys in {@code relayKeys} while it offers them, and drops them after. */
    public WakeTargetsPoll(WakeTargets targets, WakeRelayKeys relayKeys, Clients clients, Executor requests,
                           LongSupplier clock) {
        this.targets = targets;
        this.relayKeys = relayKeys;
        this.clients = clients;
        this.requests = requests;
        this.clock = clock;
    }

    @Override
    public void close() {
        handleToken.close();
    }

    /** After each poll of the PC, on the polling thread; an ask that is due goes to the request executor. */
    public void polled(ComputerDetails computer) {
        String uuid = computer.uuid;
        if (uuid == null) {
            return;
        }
        long token = NightjarDiscoveryJni.wakePollReserve(
                handle,
                uuid,
                computer.state == ComputerDetails.State.ONLINE,
                computer.caps.has(HostCapabilities.Feature.NATIVE_API),
                clock.getAsLong());
        if (token == 0) {
            return;
        }
        ComputerDetails asked = new ComputerDetails(computer);
        try {
            requests.execute(() -> ask(uuid, asked, token));
        } catch (RejectedExecutionException e) {
            NightjarDiscoveryJni.wakePollDrop(handle, uuid, token);
        }
    }

    /** A removed PC is asked nothing more, and an ask still in flight for it keeps nothing. */
    public void forget(String uuid) {
        if (uuid != null) {
            NightjarDiscoveryJni.wakePollForget(handle, uuid);
        }
    }

    /** The service is going away: an ask still queued or in flight stores nothing. */
    public void shutdown() {
        NightjarDiscoveryJni.wakePollShutdown(handle);
    }

    boolean hasPendingAsks() {
        return NightjarDiscoveryJni.wakePollPendingCount(handle) > 0;
    }

    private void ask(String uuid, ComputerDetails computer, long token) {
        if (!NightjarDiscoveryJni.wakePollIsActive(handle, uuid, token)) {
            return;
        }
        NativeApiClient client = clients.open(computer);
        if (client == null) {
            NightjarDiscoveryJni.wakePollDrop(handle, uuid, token);
            return;
        }
        try {
            NativeCapabilities capabilities = client.capabilities();
            NativeWake wake = capabilities.has(NativeCapabilities.WAKE) ? client.wake() : null;
            synchronized (storeLock) {
                if (!NightjarDiscoveryJni.wakePollIsActive(handle, uuid, token)) {
                    return;
                }
                if (wake == null) {
                    targets.forget(uuid);
                } else {
                    targets.store(uuid, wake);
                }
            }
            if (relayKeys != null) {
                readRelayKeys(uuid, computer, client, token);
            }
            if (capabilities.has(NativePcPower.FEATURE_UPDATE) && capabilities.hostControl) {
                readUpdate(uuid, client);
            }
        } catch (NativeApiException e) {
            LimeLog.info("Wake targets of " + uuid + " not read: " + e.getMessage());
        }
    }

    /**
     * Keeps the PC's away wake keys while it offers them (serverinfo and the capabilities just read), and drops them
     * once it does not. A failed read keeps what is kept: the relays still take those keys until they change.
     */
    private void readRelayKeys(String uuid, ComputerDetails computer, NativeApiClient client, long token) {
        boolean offered = offersWakeRelay(computer.caps, KnownCapabilities.of(uuid));
        String bundle = null;
        try {
            bundle = offered ? client.wakeRelayBundle() : null;
        } catch (NativeApiException e) {
            LimeLog.info("Wake relay keys of " + uuid + " not read: " + e.getMessage());
            return;
        }
        synchronized (storeLock) {
            if (!NightjarDiscoveryJni.wakePollIsActive(handle, uuid, token)) {
                return;
            }
            if (offered) {
                relayKeys.store(uuid, bundle);
            } else {
                relayKeys.forget(uuid);
            }
        }
    }

    static boolean offersWakeRelay(HostCapabilities caps, String nativeCapabilities) {
        if (caps == null || nativeCapabilities == null) {
            return false;
        }
        try {
            return caps.withNativeCapabilities(nativeCapabilities).has(HostCapabilities.Feature.AWAY_WAKE_RELAY);
        } catch (JSONException e) {
            return false;
        }
    }

    private static void readUpdate(String uuid, NativeApiClient client) {
        try {
            client.hostUpdate();
        } catch (NativeApiException e) {
            LimeLog.info("Host update of " + uuid + " not read: " + e.getMessage());
        }
    }
}
