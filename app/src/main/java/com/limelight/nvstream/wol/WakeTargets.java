package com.limelight.nvstream.wol;

import android.content.Context;
import android.content.SharedPreferences;

import androidx.annotation.VisibleForTesting;

import com.limelight.LimeLog;
import com.limelight.nativeapi.NativeApiClient;
import com.limelight.nativeapi.NativeApiException;
import com.limelight.nativeapi.NativeCapabilities;
import com.limelight.nativeapi.NativeWake;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.jni.Json;
import com.limelight.nvstream.jni.NightjarDiscoveryJni;

import java.io.File;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

/**
 * The wake targets a host's native API lists besides serverinfo's MAC (GET /wake): the MACs of its other adapters
 * and the subnet broadcasts that reach them. Backed by Rust {@code WakeTargetsStore} ({@code wake_targets.json})
 * via {@link NightjarDiscoveryJni}, with one-time {@link SharedPreferences} migration.
 */
public final class WakeTargets {
    static final String PREFS = "wake_targets";
    /** A host lists one target per physical adapter; more than this is not a PC's worth. */
    static final int MAX_MACS = 8;
    static final int MAX_BROADCASTS = 16;
    static final String MANUAL = "manual:";

    private static final ConcurrentHashMap<String, Long> HANDLES = new ConcurrentHashMap<>();
    private static final Set<String> MIGRATED_DIRS = ConcurrentHashMap.newKeySet();

    /** What is stored for one host. */
    public static final class Targets {
        public static final Targets NONE = new Targets(Collections.emptyList(), Collections.emptyList());

        /** Lowercase, colon-separated. */
        public final List<String> macs;
        /** IPv4 literals. */
        public final List<String> broadcasts;

        Targets(List<String> macs, List<String> broadcasts) {
            this.macs = Collections.unmodifiableList(macs);
            this.broadcasts = Collections.unmodifiableList(broadcasts);
        }
    }

    private final long handle;

    public WakeTargets(Context context) {
        this.handle = openAndMigrate(context.getApplicationContext());
    }

    private static long handleFor(File dir) {
        String key = dir.getAbsolutePath();
        synchronized (HANDLES) {
            return HANDLES.computeIfAbsent(key, NightjarDiscoveryJni::wakeTargetsOpen);
        }
    }

    private static long openAndMigrate(Context app) {
        File dir = app.getFilesDir();
        long h = handleFor(dir);
        String key = dir.getAbsolutePath();
        if (!MIGRATED_DIRS.contains(key) && migrateLegacy(app, h)) {
            MIGRATED_DIRS.add(key);
        }
        return h;
    }

    private static boolean migrateLegacy(Context app, long handle) {
        SharedPreferences prefs = app.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        Map<String, ?> all = prefs.getAll();
        if (all.isEmpty()) {
            return true;
        }
        Json.Obj hosts = Json.obj();
        Json.Obj manual = Json.obj();
        for (Map.Entry<String, ?> entry : all.entrySet()) {
            if (!(entry.getValue() instanceof String)) {
                continue;
            }
            String key = entry.getKey();
            String val = (String) entry.getValue();
            if (key.startsWith(MANUAL)) {
                String uuid = key.substring(MANUAL.length());
                try {
                    manual.put(uuid, Json.parseArray(val));
                } catch (Json.SyntaxException ignored) {
                }
            } else {
                try {
                    hosts.put(key, Json.parseObject(val));
                } catch (Json.SyntaxException ignored) {
                }
            }
        }
        Json.Obj payload = Json.obj().put("hosts", hosts).put("manual", manual);
        if (!NightjarDiscoveryJni.wakeTargetsImportLegacy(handle, payload.toString())) {
            com.limelight.LimeLog.warning("Wake targets legacy import failed; keeping SharedPreferences entries");
            return false;
        }
        prefs.edit().clear().commit();
        return true;
    }

    @VisibleForTesting
    public static void closeForTests(File dir) {
        if (dir == null) return;
        String key = dir.getAbsolutePath();
        synchronized (HANDLES) {
            Long h = HANDLES.remove(key);
            MIGRATED_DIRS.remove(key);
            if (h != null && h != 0L) {
                NightjarDiscoveryJni.wakeTargetsDestroy(h);
            }
        }
    }

    @VisibleForTesting
    public static void resetForTests() {
        synchronized (HANDLES) {
            for (Long h : new ArrayList<>(HANDLES.values())) {
                if (h != null && h != 0L) {
                    NightjarDiscoveryJni.wakeTargetsDestroy(h);
                }
            }
            HANDLES.clear();
            MIGRATED_DIRS.clear();
        }
    }

    /**
     * Whether there is a MAC to wake the PC by: serverinfo's, or one its {@code /wake} listed. Every wake gate asks
     * this, so a PC whose serverinfo has no MAC still wakes through its other adapters.
     */
    public static boolean hasMac(Context context, ComputerDetails computer) {
        return computer != null && NightjarDiscoveryJni.wakeTargetsHasMac(
                new WakeTargets(context).handle,
                computer.uuid == null ? "" : computer.uuid,
                computer.macAddress);
    }

    /** The host's targets with the user's own MACs after them. */
    public Targets load(String hostUuid) {
        if (hostUuid == null) {
            return Targets.NONE;
        }
        String raw = NightjarDiscoveryJni.wakeTargetsLoad(handle, hostUuid);
        if (raw == null) {
            return Targets.NONE;
        }
        Json.Obj json = Json.parseObject(raw);
        List<String> macs = strings(json.optArray("macs"));
        List<String> broadcasts = strings(json.optArray("broadcasts"));
        if (macs.isEmpty() && broadcasts.isEmpty()) {
            return Targets.NONE;
        }
        return new Targets(macs, broadcasts);
    }

    /** The MACs the user added for the PC, lowercase and colon-separated. */
    public List<String> manual(String hostUuid) {
        if (hostUuid == null) {
            return new ArrayList<>();
        }
        return strings(Json.parseArray(NightjarDiscoveryJni.wakeTargetsManual(handle, hostUuid)));
    }

    /** Keeps the user's MACs for the PC (already checked by {@link #parseMacs}); an empty list removes them. */
    public void storeManual(String hostUuid, List<String> macs) {
        if (hostUuid == null) {
            return;
        }
        Json.Arr arr = Json.arr();
        if (macs != null) {
            for (String mac : macs) {
                arr.add(mac);
            }
        }
        NightjarDiscoveryJni.wakeTargetsStoreManual(handle, hostUuid, arr.toString());
    }

    /**
     * The MACs typed into the PC's wake addresses, separated by commas, spaces or lines, as {@code 00:11:22:33:44:55},
     * {@code 00-11-22-33-44-55} or {@code 001122334455}; each once. Null when one of them is not a MAC address, is all
     * zeros, or there are more than {@link #MAX_MACS}.
     */
    public static List<String> parseMacs(String text) {
        if (text == null) {
            return null;
        }
        String raw = NightjarDiscoveryJni.wolParseMacs(text);
        if (raw == null) {
            return null;
        }
        return strings(Json.parseArray(raw));
    }

    /** Keeps the host's targets in place of the stored ones; MACs and broadcasts that are not well formed are left out. */
    public void store(String hostUuid, NativeWake wake) {
        if (hostUuid == null || wake == null) {
            return;
        }
        Json.Arr targetsJson = Json.arr();
        for (NativeWake.Target target : wake.targets) {
            Json.Arr bcasts = Json.arr();
            for (String b : target.broadcasts) {
                bcasts.add(b);
            }
            targetsJson.add(Json.obj().put("mac", target.mac).put("broadcasts", bcasts));
        }
        NightjarDiscoveryJni.wakeTargetsStoreWake(handle, hostUuid, wake.primary, targetsJson.toString());
    }

    /** Drops what the host listed; the user's own MACs stay (see {@link #forgetHost}). */
    public void forget(String hostUuid) {
        if (hostUuid != null) {
            NightjarDiscoveryJni.wakeTargetsForget(handle, hostUuid);
        }
    }

    /** Drops everything kept for a removed PC, the user's own MACs too. */
    public void forgetHost(String hostUuid) {
        if (hostUuid != null) {
            NightjarDiscoveryJni.wakeTargetsForgetHost(handle, hostUuid);
        }
    }

    /**
     * Reads the targets of a host that lists {@link NativeCapabilities#WAKE}, and forgets them for one that does not
     * (blocks on HTTPS). A failed read keeps what is stored: those adapters still belong to the PC.
     */
    public void refresh(String hostUuid, NativeCapabilities capabilities, NativeApiClient client) {
        refresh(hostUuid, capabilities != null && capabilities.has(NativeCapabilities.WAKE), client);
    }

    /**
     * Reads the targets of a host when {@code hasWake} is true, and forgets them otherwise (blocks on HTTPS).
     * A failed read keeps what is stored: those adapters still belong to the PC.
     */
    public void refresh(String hostUuid, boolean hasWake, NativeApiClient client) {
        if (hostUuid == null) {
            return;
        }
        if (!hasWake) {
            forget(hostUuid);
            return;
        }
        try {
            store(hostUuid, client.wake());
        } catch (NativeApiException e) {
            LimeLog.info("Wake targets of " + hostUuid + " not read: " + e.getMessage());
        }
    }

    /** The four bytes of a dotted-quad IPv4 literal, or null: a name here would need a DNS lookup. */
    static byte[] ipv4(String literal) {
        return literal == null ? null : NightjarDiscoveryJni.wolIpv4Literal(literal);
    }

    private static List<String> strings(Json.Arr array) {
        List<String> values = new ArrayList<>();
        for (int i = 0; array != null && i < array.length(); i++) {
            String item = array.optString(i, null);
            if (item != null) {
                values.add(item);
            }
        }
        return values;
    }
}
